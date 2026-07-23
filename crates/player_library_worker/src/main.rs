use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::process;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use player_core::{FileFingerprint, Track};
use player_error::{PlayerError, PlayerResult};
use player_fingerprint::{audio_hash, file_hash};
use player_library_fs::{fingerprint_from_metadata, LibraryScanner, ScanOptions};
use player_metadata_lofty::{enrich_track, read_track_artwork};
use player_store_sqlite::LibraryStore;
use serde::Serialize;

fn main() {
    if let Err(error) = run() {
        let _ = emit(&LibraryEvent::Fatal {
            operation: "library".to_owned(),
            error: error.to_string(),
        });
        process::exit(1);
    }
}

fn run() -> PlayerResult<()> {
    match Args::parse(env::args().skip(1).collect())? {
        Args::Import {
            db_path,
            media_root,
            folder,
        } => import_folder(&db_path, &media_root, &folder),
        Args::Audit { db_path } => audit_database(&db_path),
    }
}

fn import_folder(db_path: &Path, media_root: &Path, folder: &Path) -> PlayerResult<()> {
    let scanner = LibraryScanner::new(ScanOptions::default());
    let source_tracks = scanner.scan(folder)?;
    let total = source_tracks.len();
    let mut store = LibraryStore::open(db_path)?;
    fs::create_dir_all(media_root).map_err(|source| PlayerError::io(media_root, source))?;
    let existing_tracks = store.tracks()?;

    let mut summary = ImportSummary::default();
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

    emit(&LibraryEvent::Started {
        operation: "import".to_owned(),
        total,
    })?;

    let jobs = source_tracks
        .into_iter()
        .enumerate()
        .map(|(source_index, track)| IndexedTrack {
            source_index,
            track,
        })
        .collect::<Vec<_>>();
    let worker_count = worker_count(total);
    let chunks = distribute_jobs(jobs, worker_count);
    let (tx, rx) = mpsc::channel();

    thread::scope(|scope| -> PlayerResult<()> {
        for chunk in chunks {
            let tx = tx.clone();
            let source_root = folder.to_path_buf();
            let media_root = media_root.to_path_buf();
            let seen_file_hashes = Arc::clone(&seen_file_hashes);
            let seen_audio_hashes = Arc::clone(&seen_audio_hashes);
            scope.spawn(move || {
                for job in chunk {
                    let path = job.track.path.clone();
                    let title = job.track.title.clone();
                    let decision = import_one(
                        &source_root,
                        &media_root,
                        job.track,
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

        for completed in 1..=total {
            let result = rx
                .recv()
                .map_err(|error| PlayerError::engine(error.to_string()))?;
            match result.decision {
                Ok(ImportDecision::Imported {
                    track,
                    artwork,
                    copied,
                    metadata_warnings,
                }) => {
                    summary.imported += 1;
                    if copied {
                        summary.copied += 1;
                    }
                    summary.metadata_warnings += metadata_warnings;
                    let path = track.path.clone();
                    let title = track.title.clone();
                    if !artwork.is_empty() {
                        pending_artwork.push((path.clone(), artwork));
                    }
                    pending_tracks.push(*track);
                    emit(&LibraryEvent::TrackFinished {
                        operation: "import".to_owned(),
                        index: completed,
                        total,
                        path,
                        title,
                        imported: summary.imported,
                        copied: summary.copied,
                        duplicates_skipped: summary.duplicates_skipped,
                        artwork_cached: summary.artwork_cached,
                        metadata_warnings: summary.metadata_warnings,
                        failures: summary.failures,
                    })?;
                }
                Ok(ImportDecision::SkippedDuplicate { path, title }) => {
                    summary.duplicates_skipped += 1;
                    emit(&LibraryEvent::TrackSkipped {
                        operation: "import".to_owned(),
                        index: completed,
                        total,
                        path,
                        title,
                        reason: "duplicate".to_owned(),
                        duplicates_skipped: summary.duplicates_skipped,
                        failures: summary.failures,
                    })?;
                }
                Ok(ImportDecision::SkippedUnidentified {
                    path,
                    title,
                    metadata_warnings,
                }) => {
                    summary.metadata_warnings += metadata_warnings;
                    emit(&LibraryEvent::TrackSkipped {
                        operation: "import".to_owned(),
                        index: completed,
                        total,
                        path,
                        title,
                        reason: "missing_audio_hash".to_owned(),
                        duplicates_skipped: summary.duplicates_skipped,
                        failures: summary.failures,
                    })?;
                }
                Err(error) => {
                    summary.failures += 1;
                    emit(&LibraryEvent::TrackFailed {
                        operation: "import".to_owned(),
                        index: completed,
                        total,
                        path: Some(result.path),
                        title: Some(result.title),
                        error,
                        failures: summary.failures,
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

    emit(&LibraryEvent::Finished {
        operation: "import".to_owned(),
        total,
        imported: summary.imported,
        copied: summary.copied,
        duplicates_skipped: summary.duplicates_skipped,
        artwork_cached: summary.artwork_cached,
        metadata_warnings: summary.metadata_warnings,
        tracks_scanned: None,
        hashes_updated: None,
        duplicate_groups: None,
        tracks_merged: None,
        failures: summary.failures,
    })
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
    track.file_hash = Some(source_file_hash.clone());
    track.set_primary_audio_hash(source_audio_hash.clone());

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

fn audit_one(source_index: usize, track: Track) -> AuditWorkResult {
    let mut failures = 0_usize;
    let file_hash = if track.file_hash.is_none() {
        match file_hash(&track.path) {
            Ok(hash) => Some(hash),
            Err(_) => {
                failures += 1;
                None
            }
        }
    } else {
        None
    };
    let audio_hash = match audio_hash(&track.path) {
        Ok(fingerprint) => Some(fingerprint.hash),
        Err(_) => {
            failures += 1;
            None
        }
    };
    let fingerprint = fs::metadata(&track.path)
        .ok()
        .map(|metadata| fingerprint_from_metadata(&metadata));

    AuditWorkResult {
        source_index,
        file_hash,
        audio_hash,
        fingerprint,
        failures,
    }
}

fn audit_database(db_path: &Path) -> PlayerResult<()> {
    let mut store = LibraryStore::open(db_path)?;
    let mut tracks = store.tracks()?;
    let total = tracks.len();
    let mut summary = AuditSummary {
        tracks_scanned: total,
        ..AuditSummary::default()
    };

    emit(&LibraryEvent::Started {
        operation: "audit".to_owned(),
        total,
    })?;

    let jobs = tracks
        .iter()
        .cloned()
        .enumerate()
        .map(|(source_index, track)| IndexedTrack {
            source_index,
            track,
        })
        .collect::<Vec<_>>();
    let worker_count = worker_count(total);
    let chunks = distribute_jobs(jobs, worker_count);
    let (tx, rx) = mpsc::channel();

    thread::scope(|scope| -> PlayerResult<()> {
        for chunk in chunks {
            let tx = tx.clone();
            scope.spawn(move || {
                for job in chunk {
                    let source_index = job.source_index;
                    let result = audit_one(source_index, job.track);
                    if tx.send(result).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);

        for completed in 1..=total {
            let result = rx
                .recv()
                .map_err(|error| PlayerError::engine(error.to_string()))?;
            let track = tracks.get_mut(result.source_index).ok_or_else(|| {
                PlayerError::engine(format!(
                    "invalid audit result index {}",
                    result.source_index
                ))
            })?;
            summary.failures += result.failures;

            let mut changed = false;
            if let Some(hash) = result.file_hash {
                if track.file_hash.as_deref() != Some(hash.as_str()) {
                    track.file_hash = Some(hash);
                    changed = true;
                }
            }
            if let Some(hash) = result.audio_hash {
                if track.audio_hash.as_deref() != Some(hash.as_str()) {
                    track.set_primary_audio_hash(hash);
                    changed = true;
                }
            }
            if changed {
                store.update_track_hashes(
                    &track.path,
                    track.file_hash.as_deref(),
                    track.audio_hash.as_deref(),
                    result.fingerprint,
                )?;
                summary.hashes_updated += 1;
            }

            emit(&LibraryEvent::TrackFinished {
                operation: "audit".to_owned(),
                index: completed,
                total,
                path: track.path.clone(),
                title: track.title.clone(),
                imported: 0,
                copied: 0,
                duplicates_skipped: 0,
                artwork_cached: 0,
                metadata_warnings: 0,
                failures: summary.failures,
            })?;
        }
        Ok(())
    })?;

    let mut groups: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for track in tracks {
        if let Some(audio_hash) = track.audio_hash {
            groups
                .entry(format!("audio:{audio_hash}"))
                .or_default()
                .push(track.path);
        }
    }

    for mut paths in groups.into_values().filter(|paths| paths.len() > 1) {
        summary.duplicate_groups += 1;
        paths.sort();
        let canonical = paths[0].clone();
        for duplicate in paths.into_iter().skip(1) {
            if store.merge_duplicate_track(&canonical, &duplicate)? {
                summary.tracks_merged += 1;
            }
        }
    }

    emit(&LibraryEvent::MergeFinished {
        operation: "audit".to_owned(),
        duplicate_groups: summary.duplicate_groups,
        tracks_merged: summary.tracks_merged,
        failures: summary.failures,
    })?;

    emit(&LibraryEvent::Finished {
        operation: "audit".to_owned(),
        total,
        imported: 0,
        copied: 0,
        duplicates_skipped: 0,
        artwork_cached: 0,
        metadata_warnings: 0,
        tracks_scanned: Some(summary.tracks_scanned),
        hashes_updated: Some(summary.hashes_updated),
        duplicate_groups: Some(summary.duplicate_groups),
        tracks_merged: Some(summary.tracks_merged),
        failures: summary.failures,
    })
}

fn emit(event: &LibraryEvent) -> PlayerResult<()> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer(&mut stdout, event)
        .map_err(|error| PlayerError::engine(error.to_string()))?;
    stdout
        .write_all(b"\n")
        .map_err(|error| PlayerError::engine(error.to_string()))?;
    stdout
        .flush()
        .map_err(|error| PlayerError::engine(error.to_string()))
}

fn managed_import_path(source_root: &Path, source_path: &Path, media_root: &Path) -> PathBuf {
    let mut relative = PathBuf::new();
    relative.push(
        source_root
            .file_name()
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| std::ffi::OsStr::new("Imported")),
    );

    if let Ok(stripped) = source_path.strip_prefix(source_root) {
        push_normal_components(&mut relative, stripped);
    } else if let Some(file_name) = source_path.file_name() {
        relative.push(file_name);
    }

    media_root.join(relative)
}

fn push_normal_components(target: &mut PathBuf, path: &Path) {
    for component in path.components() {
        if let Component::Normal(value) = component {
            target.push(value);
        }
    }
}

fn copy_into_media_library(source: &Path, destination: &Path) -> PlayerResult<bool> {
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

fn copy_related_sidecars(source_audio: &Path, destination_audio: &Path) -> PlayerResult<usize> {
    let mut copied = 0_usize;
    for (source, destination) in sidecar_copy_candidates(source_audio, destination_audio) {
        if source.exists() && copy_optional_file(&source, &destination)? {
            copied += 1;
        }
    }
    Ok(copied)
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

fn distribute_jobs(jobs: Vec<IndexedTrack>, worker_count: usize) -> Vec<Vec<IndexedTrack>> {
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

#[derive(Debug)]
enum Args {
    Import {
        db_path: PathBuf,
        media_root: PathBuf,
        folder: PathBuf,
    },
    Audit {
        db_path: PathBuf,
    },
}

impl Args {
    fn parse(args: Vec<String>) -> PlayerResult<Self> {
        let Some(operation) = args.first().map(String::as_str) else {
            print_usage();
            return Err(PlayerError::engine("missing operation"));
        };
        match operation {
            "import" => {
                let mut db_path = None;
                let mut media_root = None;
                let mut folder = None;
                let mut args = args.into_iter().skip(1);
                while let Some(flag) = args.next() {
                    match flag.as_str() {
                        "--db" => {
                            db_path = Some(PathBuf::from(required_value(&flag, args.next())?))
                        }
                        "--media-root" => {
                            media_root = Some(PathBuf::from(required_value(&flag, args.next())?));
                        }
                        "--folder" => {
                            folder = Some(PathBuf::from(required_value(&flag, args.next())?))
                        }
                        "--help" | "-h" => {
                            print_usage();
                            process::exit(0);
                        }
                        _ => return Err(PlayerError::engine(format!("unknown option: {flag}"))),
                    }
                }
                Ok(Self::Import {
                    db_path: db_path.ok_or_else(|| PlayerError::engine("missing --db <path>"))?,
                    media_root: media_root
                        .ok_or_else(|| PlayerError::engine("missing --media-root <path>"))?,
                    folder: folder.ok_or_else(|| PlayerError::engine("missing --folder <path>"))?,
                })
            }
            "audit" => {
                let mut db_path = None;
                let mut args = args.into_iter().skip(1);
                while let Some(flag) = args.next() {
                    match flag.as_str() {
                        "--db" => {
                            db_path = Some(PathBuf::from(required_value(&flag, args.next())?))
                        }
                        "--help" | "-h" => {
                            print_usage();
                            process::exit(0);
                        }
                        _ => return Err(PlayerError::engine(format!("unknown option: {flag}"))),
                    }
                }
                Ok(Self::Audit {
                    db_path: db_path.ok_or_else(|| PlayerError::engine("missing --db <path>"))?,
                })
            }
            "--help" | "-h" => {
                print_usage();
                process::exit(0);
            }
            _ => Err(PlayerError::engine(format!(
                "unknown operation: {operation}"
            ))),
        }
    }
}

fn required_value(flag: &str, value: Option<String>) -> PlayerResult<String> {
    value.ok_or_else(|| PlayerError::engine(format!("{flag} requires a value")))
}

fn print_usage() {
    println!("usage:");
    println!("  player_library_worker import --db <library.sqlite3> --media-root <dir> --folder <music-dir>");
    println!("  player_library_worker audit --db <library.sqlite3>");
}

#[derive(Default)]
struct ImportSummary {
    imported: usize,
    copied: usize,
    duplicates_skipped: usize,
    artwork_cached: usize,
    metadata_warnings: usize,
    failures: usize,
}

#[derive(Default)]
struct AuditSummary {
    tracks_scanned: usize,
    hashes_updated: usize,
    duplicate_groups: usize,
    tracks_merged: usize,
    failures: usize,
}

struct IndexedTrack {
    source_index: usize,
    track: Track,
}

struct ImportWorkResult {
    path: PathBuf,
    title: String,
    decision: Result<ImportDecision, String>,
}

struct AuditWorkResult {
    source_index: usize,
    file_hash: Option<String>,
    audio_hash: Option<String>,
    fingerprint: Option<FileFingerprint>,
    failures: usize,
}

enum ImportDecision {
    Imported {
        track: Box<Track>,
        artwork: Vec<player_core::ArtworkImage>,
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

#[derive(Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum LibraryEvent {
    Started {
        operation: String,
        total: usize,
    },
    TrackFinished {
        operation: String,
        index: usize,
        total: usize,
        #[serde(serialize_with = "serialize_path")]
        path: PathBuf,
        title: String,
        imported: usize,
        copied: usize,
        duplicates_skipped: usize,
        artwork_cached: usize,
        metadata_warnings: usize,
        failures: usize,
    },
    TrackSkipped {
        operation: String,
        index: usize,
        total: usize,
        #[serde(serialize_with = "serialize_path")]
        path: PathBuf,
        title: String,
        reason: String,
        duplicates_skipped: usize,
        failures: usize,
    },
    TrackFailed {
        operation: String,
        index: usize,
        total: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(serialize_with = "serialize_optional_path")]
        path: Option<PathBuf>,
        title: Option<String>,
        error: String,
        failures: usize,
    },
    MergeFinished {
        operation: String,
        duplicate_groups: usize,
        tracks_merged: usize,
        failures: usize,
    },
    Finished {
        operation: String,
        total: usize,
        imported: usize,
        copied: usize,
        duplicates_skipped: usize,
        artwork_cached: usize,
        metadata_warnings: usize,
        tracks_scanned: Option<usize>,
        hashes_updated: Option<usize>,
        duplicate_groups: Option<usize>,
        tracks_merged: Option<usize>,
        failures: usize,
    },
    Fatal {
        operation: String,
        error: String,
    },
}

fn serialize_path<S>(path: &Path, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&path.to_string_lossy())
}

fn serialize_optional_path<S>(path: &Option<PathBuf>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match path {
        Some(path) => serializer.serialize_some(&path.to_string_lossy()),
        None => serializer.serialize_none(),
    }
}

const LYRICS_EXTENSIONS: &[&str] = &["lrc", "txt", "lyrics"];
const ARTWORK_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif"];
const ALBUM_ARTWORK_STEMS: &[&str] = &["cover", "folder", "front", "album"];
