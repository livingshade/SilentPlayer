use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use errors::{PlayerError, PlayerResult};
use serde::Serialize;

use crate::file_support::resolved_artwork_path;
use crate::PlayerApp;

#[derive(Serialize)]
pub(crate) struct PublicArtworkMappingSummary {
    tracks_scanned: usize,
    tracks_mapped: usize,
    tracks_without_artwork: usize,
    unique_images: usize,
    public_url_prefix: String,
    export_directory: String,
}

struct CollectedPublicArtwork {
    tracks_scanned: usize,
    mappings: Vec<(PathBuf, String)>,
    images: BTreeMap<String, PathBuf>,
}

impl PlayerApp {
    pub(crate) fn service_map_public_artwork_urls(
        &mut self,
        public_url_prefix: &str,
        export_directory: &Path,
    ) -> PlayerResult<PublicArtworkMappingSummary> {
        let public_url_prefix = normalized_public_url_prefix(public_url_prefix)?;
        let collected = self.collect_public_artwork_mappings(&public_url_prefix)?;
        export_public_artwork_images(export_directory, &collected.images)?;
        self.store()?
            .replace_track_artwork_public_urls(&collected.mappings)?;

        Ok(PublicArtworkMappingSummary {
            tracks_scanned: collected.tracks_scanned,
            tracks_mapped: collected.mappings.len(),
            tracks_without_artwork: collected
                .tracks_scanned
                .saturating_sub(collected.mappings.len()),
            unique_images: collected.images.len(),
            public_url_prefix,
            export_directory: export_directory.to_string_lossy().into_owned(),
        })
    }

    fn collect_public_artwork_mappings(
        &self,
        public_url_prefix: &str,
    ) -> PlayerResult<CollectedPublicArtwork> {
        let store = self.store()?;
        let tracks = store.tracks()?;
        let mut mappings = Vec::new();
        let mut images = BTreeMap::new();

        for track in &tracks {
            let Some((artwork_path, _)) =
                resolved_artwork_path(&store, &self.db_path, &track.path)?
            else {
                continue;
            };
            let file_name = public_artwork_file_name(&artwork_path)?;
            let public_url = format!("{public_url_prefix}/{file_name}");
            mappings.push((track.path.clone(), public_url));
            images.entry(file_name).or_insert(artwork_path);
        }

        Ok(CollectedPublicArtwork {
            tracks_scanned: tracks.len(),
            mappings,
            images,
        })
    }
}

fn normalized_public_url_prefix(value: &str) -> PlayerResult<String> {
    let value = value.trim().trim_end_matches('/');
    if value.is_empty() {
        return Err(PlayerError::invalid_input(
            "public artwork URL prefix is empty",
        ));
    }
    if !value.starts_with("https://") && !value.starts_with("http://") {
        return Err(PlayerError::invalid_input(
            "public artwork URL prefix must start with http:// or https://",
        ));
    }
    Ok(value.to_owned())
}

fn public_artwork_file_name(path: &Path) -> PlayerResult<String> {
    let extension = public_artwork_extension(path)?;
    let hash = fingerprint::file_hash(path)?;
    Ok(format!("{hash}.{extension}"))
}

fn public_artwork_extension(path: &Path) -> PlayerResult<&'static str> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => Ok("jpg"),
        Some("png") => Ok("png"),
        Some("webp") => Ok("webp"),
        Some("gif") => Ok("gif"),
        _ => Err(PlayerError::metadata(format!(
            "unsupported artwork file extension: {}",
            path.display()
        ))),
    }
}

fn export_public_artwork_images(
    export_directory: &Path,
    images: &BTreeMap<String, PathBuf>,
) -> PlayerResult<()> {
    fs::create_dir_all(export_directory)
        .map_err(|source| PlayerError::io(export_directory.to_path_buf(), source))?;
    for (file_name, source_path) in images {
        let destination = export_directory.join(file_name);
        fs::copy(source_path, &destination)
            .map_err(|source| PlayerError::io(destination, source))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{ArtworkImage, Track};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn prefix_is_normalized_without_changing_scheme() {
        assert_eq!(
            normalized_public_url_prefix(" https://livingshade.github.io/silent/// ").unwrap(),
            "https://livingshade.github.io/silent"
        );
        assert!(normalized_public_url_prefix("livingshade.github.io/silent").is_err());
    }

    #[test]
    fn extension_is_normalized_for_public_assets() {
        assert_eq!(
            public_artwork_extension(Path::new("cover.JPEG")).unwrap(),
            "jpg"
        );
        assert_eq!(
            public_artwork_extension(Path::new("cover.webp")).unwrap(),
            "webp"
        );
        assert!(public_artwork_extension(Path::new("cover.bin")).is_err());
    }

    #[test]
    fn migration_writes_urls_per_track_while_reusing_the_same_cover_file() {
        let root = temporary_root("per_track");
        let media_root = root.join("Music");
        fs::create_dir_all(&media_root).unwrap();
        let mut app = PlayerApp::new(root.join("library.sqlite3"), media_root);
        let mut first = Track::from_path(root.join("01.ogg"));
        first.album = Some("Album".to_owned());
        let mut second = Track::from_path(root.join("02.ogg"));
        second.album = Some("Album".to_owned());
        let artwork = ArtworkImage {
            picture_index: 0,
            mime_type: Some("image/png".to_owned()),
            picture_type: "CoverFront".to_owned(),
            description: None,
            data: b"\x89PNG\r\n\x1A\nshared".to_vec(),
        };
        {
            let mut store = app.store().unwrap();
            store
                .upsert_tracks(&[first.clone(), second.clone()])
                .unwrap();
            store
                .save_artwork(&first.path, std::slice::from_ref(&artwork))
                .unwrap();
            store.save_artwork(&second.path, &[artwork]).unwrap();
        }

        let export_directory = root.join("site").join("silent");
        let summary = app
            .service_map_public_artwork_urls(
                "https://livingshade.github.io/silent/",
                &export_directory,
            )
            .unwrap();
        assert_eq!(summary.tracks_mapped, 2);
        assert_eq!(summary.unique_images, 1);
        let store = app.store().unwrap();
        let first_url = store
            .track_artwork_public_url(&first.path)
            .unwrap()
            .unwrap();
        let second_url = store
            .track_artwork_public_url(&second.path)
            .unwrap()
            .unwrap();
        assert_eq!(first_url, second_url);
        assert!(first_url.starts_with("https://livingshade.github.io/silent/"));
        assert_eq!(fs::read_dir(export_directory).unwrap().count(), 1);

        drop(store);
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    fn temporary_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("silent_public_artwork_{name}_{nonce}"))
    }
}
