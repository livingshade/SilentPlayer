use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::mpsc;
use std::thread;

use domain::{FileFingerprint, Track};
use errors::{PlayerError, PlayerResult};
use fingerprint::{audio_hash, file_hash};
use library_fs::fingerprint_from_metadata;
use library_service::{import_folder as import_folder_service, ImportProgress};
use serde::Serialize;
use store_sqlite::LibraryStore;

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
    let summary = import_folder_service(db_path, media_root, folder, |progress| match progress {
        ImportProgress::Started { total } => emit(&LibraryEvent::Started {
            operation: "import".to_owned(),
            total,
        }),
        ImportProgress::TrackFinished {
            index,
            total,
            path,
            title,
            summary,
        } => emit(&LibraryEvent::TrackFinished {
            operation: "import".to_owned(),
            index,
            total,
            path,
            title,
            imported: summary.imported,
            copied: summary.copied,
            duplicates_skipped: summary.duplicates_skipped,
            artwork_cached: summary.artwork_cached,
            metadata_warnings: summary.metadata_warnings,
            failures: summary.failures,
        }),
        ImportProgress::TrackSkipped {
            index,
            total,
            path,
            title,
            reason,
            summary,
        } => emit(&LibraryEvent::TrackSkipped {
            operation: "import".to_owned(),
            index,
            total,
            path,
            title,
            reason: reason.to_owned(),
            duplicates_skipped: summary.duplicates_skipped,
            failures: summary.failures,
        }),
        ImportProgress::TrackFailed {
            index,
            total,
            path,
            title,
            error,
            summary,
        } => emit(&LibraryEvent::TrackFailed {
            operation: "import".to_owned(),
            index,
            total,
            path: Some(path),
            title: Some(title),
            error,
            failures: summary.failures,
        }),
    })?;
    emit(&LibraryEvent::Finished {
        operation: "import".to_owned(),
        total: summary.total,
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
    println!(
        "  library_worker import --db <library.sqlite3> --media-root <dir> --folder <music-dir>"
    );
    println!("  library_worker audit --db <library.sqlite3>");
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

struct AuditWorkResult {
    source_index: usize,
    file_hash: Option<String>,
    audio_hash: Option<String>,
    fingerprint: Option<FileFingerprint>,
    failures: usize,
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
