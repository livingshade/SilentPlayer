mod lyrics;

use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use domain::{ArtworkImage, Track};
use errors::{PlayerError, PlayerResult};
use fingerprint::{audio_hash, file_hash};
use library_fs::{fingerprint_from_metadata, is_supported_audio_file, LibraryScanner, ScanOptions};
use metadata_lofty::{enrich_track, read_track_artwork};
use store_sqlite::LibraryStore;

pub use lyrics::{
    load_lyrics_file, load_track_lyrics, parse_lyrics_text, remove_track_lyrics, set_track_lyrics,
    LyricsAsset, LyricsContent, LyricsDiagnostic, LyricsDiagnosticSeverity, LyricsDocument,
    LyricsFormat, LyricsMetadata, LyricsRemoval, TimedLyricsLine,
};

pub const LYRICS_EXTENSIONS: &[&str] = &["lrc", "txt", "lyrics"];
pub const ARTWORK_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif"];
pub const ALBUM_ARTWORK_STEMS: &[&str] = &["cover", "folder", "front", "album"];

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ImportSummary {
    pub total: usize,
    pub imported: usize,
    pub copied: usize,
    pub duplicates_skipped: usize,
    pub artwork_cached: usize,
    pub metadata_warnings: usize,
    pub failures: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImportProgress {
    Started {
        total: usize,
    },
    TrackFinished {
        index: usize,
        total: usize,
        path: PathBuf,
        title: String,
        summary: ImportSummary,
    },
    TrackSkipped {
        index: usize,
        total: usize,
        path: PathBuf,
        title: String,
        reason: &'static str,
        summary: ImportSummary,
    },
    TrackFailed {
        index: usize,
        total: usize,
        path: PathBuf,
        title: String,
        error: String,
        summary: ImportSummary,
    },
}

struct PendingImportTrack {
    source_root: PathBuf,
    track: Track,
}

pub fn import_folder<F>(
    db_path: &Path,
    media_root: &Path,
    folder: &Path,
    progress: F,
) -> PlayerResult<ImportSummary>
where
    F: FnMut(ImportProgress) -> PlayerResult<()>,
{
    let scanner = LibraryScanner::new(ScanOptions::default());
    let pending = scanner
        .scan(folder)?
        .into_iter()
        .map(|track| PendingImportTrack {
            source_root: folder.to_path_buf(),
            track,
        })
        .collect();
    import_pending(db_path, media_root, pending, 0, progress)
}

pub fn import_files<F>(
    db_path: &Path,
    media_root: &Path,
    paths: &[PathBuf],
    progress: F,
) -> PlayerResult<ImportSummary>
where
    F: FnMut(ImportProgress) -> PlayerResult<()>,
{
    if paths.is_empty() {
        return Err(PlayerError::metadata("no import files selected"));
    }

    let mut pending = Vec::with_capacity(paths.len());
    let mut metadata_warnings = 0_usize;
    for path in paths {
        if !is_supported_audio_file(path) {
            metadata_warnings += 1;
            continue;
        }
        let metadata =
            fs::metadata(path).map_err(|source| PlayerError::io(path.clone(), source))?;
        if !metadata.is_file() {
            metadata_warnings += 1;
            continue;
        }
        let mut track = Track::from_path(path.clone());
        track.fingerprint = Some(fingerprint_from_metadata(&metadata));
        let source_root = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("Imported"));
        pending.push(PendingImportTrack { source_root, track });
    }

    import_pending(db_path, media_root, pending, metadata_warnings, progress)
}

fn import_pending<F>(
    db_path: &Path,
    media_root: &Path,
    pending: Vec<PendingImportTrack>,
    initial_metadata_warnings: usize,
    mut progress: F,
) -> PlayerResult<ImportSummary>
where
    F: FnMut(ImportProgress) -> PlayerResult<()>,
{
    fs::create_dir_all(media_root).map_err(|source| PlayerError::io(media_root, source))?;
    let mut store = LibraryStore::open(db_path)?;
    let existing_tracks = store.tracks()?;
    let total = pending.len();
    let mut summary = ImportSummary {
        total,
        metadata_warnings: initial_metadata_warnings,
        ..ImportSummary::default()
    };

    let seen_file_hashes = Arc::new(Mutex::new(
        existing_tracks
            .iter()
            .filter_map(|track| track.file_hash.clone())
            .collect::<HashSet<_>>(),
    ));
    let seen_audio_hashes = Arc::new(Mutex::new(
        existing_tracks
            .iter()
            .filter_map(|track| track.audio_hash.clone())
            .collect::<HashSet<_>>(),
    ));
    let mut pending_tracks = Vec::new();
    let mut pending_artwork = Vec::new();

    progress(ImportProgress::Started { total })?;

    let worker_count = worker_count(total);
    let chunks = distribute_jobs(pending, worker_count);
    let (tx, rx) = mpsc::channel();

    thread::scope(|scope| -> PlayerResult<()> {
        for chunk in chunks {
            let tx = tx.clone();
            let media_root = media_root.to_path_buf();
            let seen_file_hashes = Arc::clone(&seen_file_hashes);
            let seen_audio_hashes = Arc::clone(&seen_audio_hashes);
            scope.spawn(move || {
                for pending in chunk {
                    let path = pending.track.path.clone();
                    let title = pending.track.title.clone();
                    let decision = import_one(
                        &pending.source_root,
                        &media_root,
                        pending.track,
                        &seen_file_hashes,
                        &seen_audio_hashes,
                    )
                    .map_err(|error| error.to_string());
                    if tx
                        .send(ImportWorkResult {
                            path,
                            title,
                            decision,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
        drop(tx);

        for (offset, result) in rx.into_iter().enumerate() {
            let index = offset + 1;
            match result.decision {
                Ok(ImportDecision::Imported {
                    track,
                    artwork,
                    copied,
                    metadata_warnings,
                }) => {
                    summary.imported += 1;
                    summary.copied += usize::from(copied);
                    summary.metadata_warnings += metadata_warnings;
                    let path = track.path.clone();
                    let title = track.title.clone();
                    if !artwork.is_empty() {
                        pending_artwork.push((path.clone(), artwork));
                    }
                    pending_tracks.push(*track);
                    progress(ImportProgress::TrackFinished {
                        index,
                        total,
                        path,
                        title,
                        summary: summary.clone(),
                    })?;
                }
                Ok(ImportDecision::SkippedDuplicate { path, title }) => {
                    summary.duplicates_skipped += 1;
                    progress(ImportProgress::TrackSkipped {
                        index,
                        total,
                        path,
                        title,
                        reason: "duplicate",
                        summary: summary.clone(),
                    })?;
                }
                Ok(ImportDecision::SkippedUnidentified {
                    path,
                    title,
                    metadata_warnings,
                }) => {
                    summary.metadata_warnings += metadata_warnings;
                    progress(ImportProgress::TrackSkipped {
                        index,
                        total,
                        path,
                        title,
                        reason: "missing_audio_hash",
                        summary: summary.clone(),
                    })?;
                }
                Err(error) => {
                    summary.failures += 1;
                    progress(ImportProgress::TrackFailed {
                        index,
                        total,
                        path: result.path,
                        title: result.title,
                        error,
                        summary: summary.clone(),
                    })?;
                }
            }
        }
        Ok(())
    })?;

    store.upsert_tracks(&pending_tracks)?;
    for (path, artwork) in pending_artwork {
        summary.artwork_cached += store.save_artwork(path, &artwork)?;
    }
    Ok(summary)
}

fn import_one(
    source_root: &Path,
    media_root: &Path,
    source_track: Track,
    seen_file_hashes: &Arc<Mutex<HashSet<String>>>,
    seen_audio_hashes: &Arc<Mutex<HashSet<String>>>,
) -> PlayerResult<ImportDecision> {
    let source_file_hash = file_hash(&source_track.path)?;
    if !insert_unique_hash(seen_file_hashes, &source_file_hash)? {
        return Ok(ImportDecision::SkippedDuplicate {
            path: source_track.path,
            title: source_track.title,
        });
    }

    let source_audio_hash = match audio_hash(&source_track.path) {
        Ok(fingerprint) => fingerprint.hash,
        Err(_) => {
            return Ok(ImportDecision::SkippedUnidentified {
                path: source_track.path,
                title: source_track.title,
                metadata_warnings: 1,
            });
        }
    };
    if !insert_unique_hash(seen_audio_hashes, &source_audio_hash)? {
        return Ok(ImportDecision::SkippedDuplicate {
            path: source_track.path,
            title: source_track.title,
        });
    }

    let destination = managed_import_path(source_root, &source_track.path, media_root);
    let copied = copy_into_media_library(&source_track.path, &destination)?;
    copy_related_sidecars(&source_track.path, &destination)?;

    let mut track = Track::from_path(destination.clone());
    track.fingerprint = fs::metadata(&destination)
        .ok()
        .map(|metadata| fingerprint_from_metadata(&metadata));
    track.file_hash = Some(source_file_hash);
    track.set_primary_audio_hash(source_audio_hash);

    let mut metadata_warnings = 0_usize;
    if enrich_track(&mut track).is_err() {
        metadata_warnings += 1;
    }
    let artwork = match read_track_artwork(&track.path) {
        Ok(images) => images,
        Err(_) => {
            metadata_warnings += 1;
            Vec::new()
        }
    };

    Ok(ImportDecision::Imported {
        track: Box::new(track),
        artwork,
        copied,
        metadata_warnings,
    })
}

pub fn managed_import_path(source_root: &Path, source_path: &Path, media_root: &Path) -> PathBuf {
    let mut relative = PathBuf::new();
    relative.push(
        source_root
            .file_name()
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| OsStr::new("Imported")),
    );

    if let Ok(stripped) = source_path.strip_prefix(source_root) {
        push_normal_components(&mut relative, stripped);
    } else if let Some(file_name) = source_path.file_name() {
        relative.push(file_name);
    }

    media_root.join(relative)
}

pub fn copy_into_media_library(source: &Path, destination: &Path) -> PlayerResult<bool> {
    let source_canonical = source.canonicalize().ok();
    let destination_canonical = destination.canonicalize().ok();
    if source_canonical.is_some() && source_canonical == destination_canonical {
        return Ok(false);
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|source| PlayerError::io(parent, source))?;
    }
    fs::copy(source, destination).map_err(|source| PlayerError::io(destination, source))?;
    Ok(true)
}

pub fn copy_related_sidecars(source_audio: &Path, destination_audio: &Path) -> PlayerResult<usize> {
    let mut copied = 0_usize;
    for (source, destination) in sidecar_copy_candidates(source_audio, destination_audio) {
        if source.exists() && copy_optional_file(&source, &destination)? {
            copied += 1;
        }
    }
    Ok(copied)
}

fn push_normal_components(target: &mut PathBuf, path: &Path) {
    for component in path.components() {
        if let Component::Normal(value) = component {
            target.push(value);
        }
    }
}

fn sidecar_copy_candidates(
    source_audio: &Path,
    destination_audio: &Path,
) -> Vec<(PathBuf, PathBuf)> {
    let mut candidates = Vec::new();
    let Some(source_dir) = source_audio.parent() else {
        return candidates;
    };
    let Some(destination_dir) = destination_audio.parent() else {
        return candidates;
    };

    if let (Some(source_stem), Some(destination_stem)) = (
        source_audio.file_stem().and_then(|value| value.to_str()),
        destination_audio
            .file_stem()
            .and_then(|value| value.to_str()),
    ) {
        for extension in LYRICS_EXTENSIONS {
            candidates.push((
                source_dir.join(format!("{source_stem}.{extension}")),
                destination_dir.join(format!("{destination_stem}.{extension}")),
            ));
        }
        for extension in ARTWORK_EXTENSIONS {
            candidates.push((
                source_dir.join(format!("{source_stem}.{extension}")),
                destination_dir.join(format!("{destination_stem}.{extension}")),
            ));
        }
    }

    for stem in ALBUM_ARTWORK_STEMS {
        for extension in ARTWORK_EXTENSIONS {
            let file_name = format!("{stem}.{extension}");
            candidates.push((source_dir.join(&file_name), destination_dir.join(file_name)));
        }
    }
    candidates
}

fn copy_optional_file(source: &Path, destination: &Path) -> PlayerResult<bool> {
    let source_canonical = source.canonicalize().ok();
    let destination_canonical = destination.canonicalize().ok();
    if source_canonical.is_some() && source_canonical == destination_canonical {
        return Ok(false);
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|source| PlayerError::io(parent, source))?;
    }
    fs::copy(source, destination).map_err(|source| PlayerError::io(destination, source))?;
    Ok(true)
}

fn worker_count(total: usize) -> usize {
    if total == 0 {
        return 0;
    }
    thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(2)
        .clamp(1, total)
}

fn distribute_jobs(
    jobs: Vec<PendingImportTrack>,
    worker_count: usize,
) -> Vec<Vec<PendingImportTrack>> {
    if worker_count == 0 {
        return Vec::new();
    }
    let mut chunks = (0..worker_count).map(|_| Vec::new()).collect::<Vec<_>>();
    for (offset, job) in jobs.into_iter().enumerate() {
        chunks[offset % worker_count].push(job);
    }
    chunks
        .into_iter()
        .filter(|chunk| !chunk.is_empty())
        .collect()
}

fn insert_unique_hash(hashes: &Arc<Mutex<HashSet<String>>>, hash: &str) -> PlayerResult<bool> {
    let mut hashes = hashes
        .lock()
        .map_err(|_| PlayerError::engine("import hash set lock poisoned"))?;
    Ok(hashes.insert(hash.to_owned()))
}

struct ImportWorkResult {
    path: PathBuf,
    title: String,
    decision: Result<ImportDecision, String>,
}

enum ImportDecision {
    Imported {
        track: Box<Track>,
        artwork: Vec<ArtworkImage>,
        copied: bool,
        metadata_warnings: usize,
    },
    SkippedDuplicate {
        path: PathBuf,
        title: String,
    },
    SkippedUnidentified {
        path: PathBuf,
        title: String,
        metadata_warnings: usize,
    },
}
