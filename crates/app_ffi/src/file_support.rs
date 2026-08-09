use std::fs;
use std::path::{Component, Path, PathBuf};

use domain::ArtworkImage;
use errors::{PlayerError, PlayerResult};
use library_service::{ALBUM_ARTWORK_STEMS, ARTWORK_EXTENSIONS, LYRICS_EXTENSIONS};
use store_sqlite::LibraryStore;

use crate::dto::{cache_key_for_view_id, track_view_id};
use crate::LIBRARY_PACKAGE_MUSIC_DIRECTORY;

pub(super) fn remove_library_storage(db_path: &Path, media_root: &Path) -> PlayerResult<()> {
    let mut first_error = None;

    for path in sqlite_database_files(db_path) {
        if let Err(source) = fs::remove_file(&path) {
            if source.kind() != std::io::ErrorKind::NotFound && first_error.is_none() {
                first_error = Some(PlayerError::io(path, source));
            }
        }
    }

    for path in [media_root.to_path_buf(), artwork_cache_root(db_path)] {
        if let Err(source) = fs::remove_dir_all(&path) {
            if source.kind() != std::io::ErrorKind::NotFound && first_error.is_none() {
                first_error = Some(PlayerError::io(path, source));
            }
        }
    }

    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

pub(super) fn delete_managed_track_files(
    media_root: &Path,
    track_path: &Path,
) -> PlayerResult<usize> {
    if !is_managed_track_path(media_root, track_path) {
        return Ok(0);
    }

    let mut candidates = vec![track_path.to_path_buf()];
    if let (Some(parent), Some(stem)) = (
        track_path.parent(),
        track_path.file_stem().and_then(|value| value.to_str()),
    ) {
        for extension in LYRICS_EXTENSIONS.iter().chain(ARTWORK_EXTENSIONS.iter()) {
            candidates.push(parent.join(format!("{stem}.{extension}")));
        }
    }

    let mut deleted = 0;
    for candidate in candidates {
        match fs::remove_file(&candidate) {
            Ok(()) => deleted += 1,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(PlayerError::io(candidate, source)),
        }
    }
    Ok(deleted)
}

pub(super) fn is_managed_track_path(media_root: &Path, track_path: &Path) -> bool {
    match (media_root.canonicalize(), track_path.canonicalize()) {
        (Ok(root), Ok(track)) => track.starts_with(root),
        _ => track_path.starts_with(media_root),
    }
}

pub(super) fn sqlite_database_files(db_path: &Path) -> [PathBuf; 4] {
    [
        db_path.to_path_buf(),
        sqlite_companion_path(db_path, "-wal"),
        sqlite_companion_path(db_path, "-shm"),
        sqlite_companion_path(db_path, "-journal"),
    ]
}

pub(super) fn sqlite_companion_path(db_path: &Path, suffix: &str) -> PathBuf {
    let mut path = db_path.as_os_str().to_os_string();
    path.push(suffix);
    path.into()
}

pub(super) fn validated_package_file(
    package_root: &Path,
    relative_path: &str,
    kind: &str,
) -> PlayerResult<PathBuf> {
    let relative_path = Path::new(relative_path);
    if relative_path.as_os_str().is_empty()
        || relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(PlayerError::store(format!(
            "library package {kind} path must be relative and normalized: {}",
            relative_path.display()
        )));
    }
    let path = package_root.join(relative_path);
    let canonical = path
        .canonicalize()
        .map_err(|source| PlayerError::io(&path, source))?;
    if !canonical.starts_with(package_root) || !canonical.is_file() {
        return Err(PlayerError::store(format!(
            "library package {kind} path escapes the package or is not a file: {}",
            relative_path.display()
        )));
    }
    Ok(canonical)
}

pub(super) fn library_package_audio_path(index: usize, source_path: &Path) -> PathBuf {
    let file_name = source_path
        .file_name()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| std::ffi::OsStr::new("track.audio"));
    PathBuf::from(LIBRARY_PACKAGE_MUSIC_DIRECTORY)
        .join(format!("{index:08}"))
        .join(file_name)
}

pub(super) fn materialized_primary_view_id(
    audio_hash: &str,
    created_at_unix_nanos: u128,
) -> String {
    format!(
        "audio:{}:materialized:{created_at_unix_nanos:x}",
        audio_hash.trim()
    )
}

pub(super) fn cached_artwork_path(
    store: &LibraryStore,
    db_path: &Path,
    track_path: &Path,
) -> PlayerResult<Option<PathBuf>> {
    let Some(image) = store
        .artwork_for_path(track_path)?
        .into_iter()
        .find(|image| !image.data.is_empty())
    else {
        return Ok(None);
    };

    let track = store
        .track_by_path(track_path)?
        .ok_or_else(|| PlayerError::store(format!("track not found: {}", track_path.display())))?;
    let view_cache_key = cache_key_for_view_id(track_view_id(&track)?);
    let cache_root = artwork_cache_root(db_path);
    fs::create_dir_all(&cache_root)
        .map_err(|source| PlayerError::io(cache_root.clone(), source))?;

    let extension = artwork_extension(&image);
    let cache_path = cache_root.join(format!(
        "{}-{}.{}",
        view_cache_key, image.picture_index, extension
    ));

    if cached_file_needs_write(&cache_path, &image.data) {
        fs::write(&cache_path, &image.data)
            .map_err(|source| PlayerError::io(cache_path.clone(), source))?;
    }

    Ok(Some(cache_path))
}

pub(super) fn resolved_artwork_path(
    store: &LibraryStore,
    db_path: &Path,
    track_path: &Path,
) -> PlayerResult<Option<(PathBuf, &'static str)>> {
    if let Some(reference) = store.track_artwork_reference(track_path)? {
        if let Some(path) =
            cached_artwork_asset_path(db_path, &reference.asset_id, &reference.image)?
        {
            return Ok(Some((path, "track")));
        }
    }
    if let Some(path) = cached_artwork_path(store, db_path, track_path)? {
        return Ok(Some((path, "embedded")));
    }
    if let Some(path) = sidecar_artwork_path(track_path)? {
        return Ok(Some((path, "sidecar")));
    }
    if let Some(reference) = store.album_artwork_reference(track_path)? {
        if let Some(path) =
            cached_artwork_asset_path(db_path, &reference.asset_id, &reference.image)?
        {
            return Ok(Some((path, "album")));
        }
    }
    Ok(None)
}

pub(super) fn playlist_artwork_path(
    store: &LibraryStore,
    db_path: &Path,
    playlist_id: i64,
    playlist_name: &str,
) -> PlayerResult<Option<(PathBuf, &'static str)>> {
    if let Some(image) = store.playlist_artwork(playlist_name)? {
        return Ok(cached_playlist_artwork_path(db_path, playlist_id, &image)?
            .map(|path| (path, "playlist")));
    }

    let Some(first_entry) = store.playlist_tracks(playlist_name)?.into_iter().next() else {
        return Ok(None);
    };

    resolved_artwork_path(store, db_path, &first_entry.track.path)
}

pub(super) fn cached_artwork_asset_path(
    db_path: &Path,
    asset_id: &str,
    image: &ArtworkImage,
) -> PlayerResult<Option<PathBuf>> {
    if image.data.is_empty() {
        return Ok(None);
    }
    let cache_root = artwork_cache_root(db_path).join("Assets");
    fs::create_dir_all(&cache_root)
        .map_err(|source| PlayerError::io(cache_root.clone(), source))?;
    let extension = artwork_extension(image);
    let cache_path = cache_root.join(format!("{}.{}", cache_key_for_view_id(asset_id), extension));
    if cached_file_needs_write(&cache_path, &image.data) {
        fs::write(&cache_path, &image.data)
            .map_err(|source| PlayerError::io(cache_path.clone(), source))?;
    }
    Ok(Some(cache_path))
}

pub(super) fn cached_playlist_artwork_path(
    db_path: &Path,
    playlist_id: i64,
    image: &ArtworkImage,
) -> PlayerResult<Option<PathBuf>> {
    if image.data.is_empty() {
        return Ok(None);
    }
    let cache_root = artwork_cache_root(db_path).join("Playlists");
    fs::create_dir_all(&cache_root)
        .map_err(|source| PlayerError::io(cache_root.clone(), source))?;
    let extension = artwork_extension(image);
    let cache_path = cache_root.join(format!("{playlist_id}.{extension}"));
    if cached_file_needs_write(&cache_path, &image.data) {
        fs::write(&cache_path, &image.data)
            .map_err(|source| PlayerError::io(cache_path.clone(), source))?;
    }
    Ok(Some(cache_path))
}

pub(super) fn cached_file_needs_write(path: &Path, data: &[u8]) -> bool {
    match fs::read(path) {
        Ok(existing) => existing != data,
        Err(_) => true,
    }
}

pub(super) fn artwork_cache_root(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .map(|parent| parent.join("Artwork"))
        .unwrap_or_else(|| PathBuf::from("Artwork"))
}

pub(super) fn read_artwork_image(path: &Path) -> PlayerResult<ArtworkImage> {
    let data = fs::read(path).map_err(|source| PlayerError::io(path.to_path_buf(), source))?;
    if data.is_empty() {
        return Err(PlayerError::metadata(format!(
            "empty artwork file: {}",
            path.display()
        )));
    }
    Ok(ArtworkImage {
        picture_index: 0,
        mime_type: image_mime_type(path, &data),
        picture_type: "CoverFront".to_owned(),
        description: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned()),
        data,
    })
}

pub(super) fn image_mime_type(path: &Path, data: &[u8]) -> Option<String> {
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg".to_owned());
    }
    if data.starts_with(b"\x89PNG\r\n\x1A\n") {
        return Some("image/png".to_owned());
    }
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return Some("image/gif".to_owned());
    }
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return Some("image/webp".to_owned());
    }
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => Some("image/jpeg".to_owned()),
        Some("png") => Some("image/png".to_owned()),
        Some("gif") => Some("image/gif".to_owned()),
        Some("webp") => Some("image/webp".to_owned()),
        _ => None,
    }
}

pub(super) fn artwork_extension(image: &ArtworkImage) -> &'static str {
    if let Some(mime_type) = image.mime_type.as_deref().map(str::to_ascii_lowercase) {
        match mime_type.as_str() {
            "image/jpeg" | "image/jpg" => return "jpg",
            "image/png" => return "png",
            "image/webp" => return "webp",
            "image/gif" => return "gif",
            _ => {}
        }
    }

    if image.data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "jpg"
    } else if image.data.starts_with(b"\x89PNG\r\n\x1A\n") {
        "png"
    } else if image.data.starts_with(b"GIF87a") || image.data.starts_with(b"GIF89a") {
        "gif"
    } else if image.data.len() >= 12
        && &image.data[0..4] == b"RIFF"
        && &image.data[8..12] == b"WEBP"
    {
        "webp"
    } else {
        "bin"
    }
}

pub(super) fn sidecar_artwork_path(track_path: &Path) -> PlayerResult<Option<PathBuf>> {
    let Some(dir) = track_path.parent() else {
        return Ok(None);
    };
    let Some(stem) = track_path.file_stem().and_then(|value| value.to_str()) else {
        return Ok(None);
    };
    let stems = std::iter::once(stem)
        .chain(ALBUM_ARTWORK_STEMS.iter().copied())
        .collect::<Vec<_>>();
    find_sidecar_file(dir, &stems, ARTWORK_EXTENSIONS)
}

pub(super) fn find_sidecar_file(
    dir: &Path,
    stems: &[&str],
    extensions: &[&str],
) -> PlayerResult<Option<PathBuf>> {
    let mut lower_names = Vec::new();
    for stem in stems {
        for extension in extensions {
            let file_name = format!("{stem}.{extension}");
            let exact = dir.join(&file_name);
            if exact.is_file() {
                return Ok(Some(exact));
            }
            lower_names.push(file_name.to_ascii_lowercase());
        }
    }

    let entries = fs::read_dir(dir).map_err(|source| PlayerError::io(dir, source))?;
    for entry in entries {
        let entry = entry.map_err(|source| PlayerError::io(dir, source))?;
        let file_name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if lower_names.iter().any(|candidate| candidate == &file_name) {
            let path = entry.path();
            if path.is_file() {
                return Ok(Some(path));
            }
        }
    }
    Ok(None)
}
