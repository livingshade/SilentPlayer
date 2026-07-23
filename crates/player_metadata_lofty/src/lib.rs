use std::path::Path;

use lofty::file::{AudioFile, TaggedFileExt};
use lofty::tag::{Accessor, ItemKey, Tag};
use player_core::{ArtworkImage, Track, TrackMetadata};
use player_error::{PlayerError, PlayerResult};

pub fn read_track_metadata(path: impl AsRef<Path>) -> PlayerResult<TrackMetadata> {
    let path = path.as_ref();
    let tagged_file = lofty::read_from_path(path)
        .map_err(|error| PlayerError::metadata(format!("{}: {error}", path.display())))?;

    let primary_tag = tagged_file.primary_tag();
    let best_tag = primary_tag.or_else(|| tagged_file.tags().first());
    let duration_ms = u64::try_from(tagged_file.properties().duration().as_millis()).ok();

    let Some(tag) = best_tag else {
        return Ok(TrackMetadata {
            duration_ms,
            ..TrackMetadata::default()
        });
    };

    Ok(TrackMetadata {
        title: tag.title().map(|value| value.into_owned()),
        artist: tag.artist().map(|value| value.into_owned()),
        album: tag.album().map(|value| value.into_owned()),
        album_artist: string_value(tag, ItemKey::AlbumArtist),
        genre: tag.genre().map(|value| value.into_owned()),
        track_number: tag.track(),
        disc_number: tag.disk(),
        year: tag.date().map(|timestamp| i32::from(timestamp.year)),
        duration_ms,
        artwork_count: tag.pictures().len().min(u32::MAX as usize) as u32,
    })
}

pub fn enrich_track(track: &mut Track) -> PlayerResult<()> {
    let metadata = read_track_metadata(&track.path)?;
    track.apply_metadata(metadata);
    Ok(())
}

pub fn read_track_artwork(path: impl AsRef<Path>) -> PlayerResult<Vec<ArtworkImage>> {
    let path = path.as_ref();
    let tagged_file = lofty::read_from_path(path)
        .map_err(|error| PlayerError::metadata(format!("{}: {error}", path.display())))?;
    let primary_tag = tagged_file.primary_tag();
    let best_tag = primary_tag.or_else(|| tagged_file.tags().first());

    let Some(tag) = best_tag else {
        return Ok(Vec::new());
    };

    Ok(tag
        .pictures()
        .iter()
        .enumerate()
        .map(|(index, picture)| ArtworkImage {
            picture_index: index.min(u32::MAX as usize) as u32,
            mime_type: picture
                .mime_type()
                .map(|mime_type| mime_type.as_str().to_owned()),
            picture_type: format!("{:?}", picture.pic_type()),
            description: picture.description().map(ToOwned::to_owned),
            data: picture.data().to_vec(),
        })
        .collect())
}

fn string_value(tag: &Tag, key: ItemKey) -> Option<String> {
    tag.get_string(key).map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn reads_duration_from_downloaded_ogg() {
        let metadata = read_track_metadata(fixture("into_the_oceans_chorus.ogg")).unwrap();
        assert!(metadata.duration_ms.unwrap_or_default() > 30_000);
    }

    #[test]
    fn enriches_track_without_losing_path_identity() {
        let path = fixture("funk_room_reverb.ogg");
        let mut track = Track::from_path(path.clone());
        let original_id = track.id;

        enrich_track(&mut track).unwrap();

        assert_eq!(track.id, original_id);
        assert_eq!(track.path, path);
        assert!(track.duration_ms.unwrap_or_default() > 20_000);
    }

    #[test]
    fn reads_artwork_list_from_downloaded_ogg_without_error() {
        let artwork = read_track_artwork(fixture("into_the_oceans_chorus.ogg")).unwrap();
        assert_eq!(artwork.len(), 0);
    }

    fn fixture(name: &str) -> PathBuf {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("test-assets")
            .join("audio")
            .join(name);
        assert!(path.exists(), "missing fixture: {}", path.display());
        path
    }
}
