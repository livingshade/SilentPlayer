use std::env;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::mpsc;
use std::thread;

use player_analysis_ebur128::{
    analyze_album_loudness, analyze_path, AlbumAnalysisOptions, ANALYSIS_VERSION,
};
use player_core::{FileFingerprint, LoudnessInfo, Track};
use player_error::{PlayerError, PlayerResult};
use player_library_fs::fingerprint_from_metadata;
use player_store_sqlite::LibraryStore;
use serde::Serialize;

fn main() {
    if let Err(error) = run() {
        let _ = emit(&AnalyzerEvent::Fatal {
            error: error.to_string(),
        });
        process::exit(1);
    }
}

fn run() -> PlayerResult<()> {
    let args = Args::parse(env::args().skip(1).collect())?;
    let mut store = LibraryStore::open(&args.db_path)?;
    let pending = store.pending_analysis(ANALYSIS_VERSION, args.limit)?;
    let total = pending.len();
    let mut summary = AnalysisSummary::default();

    emit(&AnalyzerEvent::Started { total })?;

    let jobs = pending
        .into_iter()
        .map(|track| IndexedTrack { track })
        .collect::<Vec<_>>();
    let worker_count = worker_count(total);
    let chunks = distribute_jobs(jobs, worker_count);
    let (tx, rx) = mpsc::channel();

    thread::scope(|scope| -> PlayerResult<()> {
        for chunk in chunks {
            let tx = tx.clone();
            scope.spawn(move || {
                for job in chunk {
                    let path = job.track.path.clone();
                    let title = job.track.title.clone();
                    let result =
                        analyze_track_payload(&job.track).map_err(|error| error.to_string());
                    if tx
                        .send(AnalysisWorkResult {
                            path,
                            title,
                            result,
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
            match result.result {
                Ok(analysis) => match store.save_loudness_with_duration(
                    &result.path,
                    analysis.fingerprint,
                    analysis.duration_ms,
                    analysis.loudness,
                ) {
                    Ok(()) => {
                        summary.analyzed += 1;
                        emit(&AnalyzerEvent::TrackFinished {
                            index: completed,
                            total,
                            path: result.path,
                            title: result.title,
                            integrated_lufs: analysis.integrated_lufs,
                            true_peak_dbtp: analysis.true_peak_dbtp,
                            duration_ms: analysis.duration_ms,
                            analyzed: summary.analyzed,
                            failed: summary.failed,
                        })?;
                    }
                    Err(error) => {
                        summary.failed += 1;
                        emit(&AnalyzerEvent::TrackFailed {
                            index: completed,
                            total,
                            path: result.path,
                            title: result.title,
                            error: error.to_string(),
                            analyzed: summary.analyzed,
                            failed: summary.failed,
                        })?;
                    }
                },
                Err(error) => {
                    summary.failed += 1;
                    emit(&AnalyzerEvent::TrackFailed {
                        index: completed,
                        total,
                        path: result.path,
                        title: result.title,
                        error,
                        analyzed: summary.analyzed,
                        failed: summary.failed,
                    })?;
                }
            }
        }
        Ok(())
    })?;

    let album_summary = analyze_album_loudness(&mut store, AlbumAnalysisOptions::default())?;
    emit(&AnalyzerEvent::AlbumFinished {
        albums_analyzed: album_summary.albums_analyzed,
        album_tracks_updated: album_summary.tracks_updated,
        album_skipped: album_summary.skipped,
    })?;

    emit(&AnalyzerEvent::Finished {
        total,
        analyzed: summary.analyzed,
        failed: summary.failed,
        albums_analyzed: album_summary.albums_analyzed,
        album_tracks_updated: album_summary.tracks_updated,
        album_skipped: album_summary.skipped,
    })?;

    Ok(())
}

fn analyze_track_payload(track: &Track) -> PlayerResult<TrackAnalysisResult> {
    let report = analyze_path(&track.path)?;
    let mut loudness = report
        .loudness_info()
        .ok_or_else(|| PlayerError::audio("analysis produced no loudness result"))?;
    loudness.analysis_version = ANALYSIS_VERSION;

    let fingerprint = std::fs::metadata(&track.path)
        .ok()
        .map(|metadata| fingerprint_from_metadata(&metadata));
    let duration_ms = duration_ms_from_seconds(report.duration_seconds);

    Ok(TrackAnalysisResult {
        loudness,
        fingerprint,
        integrated_lufs: report.integrated_lufs.unwrap_or_default(),
        true_peak_dbtp: report.true_peak_dbtp.unwrap_or_default(),
        duration_ms,
    })
}

fn emit(event: &AnalyzerEvent) -> PlayerResult<()> {
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

fn duration_ms_from_seconds(seconds: f64) -> Option<u64> {
    if !seconds.is_finite() || seconds <= 0.0 {
        return None;
    }

    Some((seconds * 1000.0).round().min(u64::MAX as f64) as u64)
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
struct Args {
    db_path: PathBuf,
    limit: Option<usize>,
}

impl Args {
    fn parse(args: Vec<String>) -> PlayerResult<Self> {
        let mut db_path = None;
        let mut limit = None;
        let mut args = args.into_iter();

        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--db" => {
                    db_path = Some(PathBuf::from(required_value(&flag, args.next())?));
                }
                "--limit" => {
                    let value = required_value(&flag, args.next())?;
                    limit =
                        Some(value.parse::<usize>().map_err(|_| {
                            PlayerError::engine(format!("invalid --limit: {value}"))
                        })?);
                }
                "--help" | "-h" => {
                    print_usage();
                    process::exit(0);
                }
                _ => return Err(PlayerError::engine(format!("unknown option: {flag}"))),
            }
        }

        let db_path = db_path.ok_or_else(|| PlayerError::engine("missing --db <path>"))?;
        Ok(Self { db_path, limit })
    }
}

fn required_value(flag: &str, value: Option<String>) -> PlayerResult<String> {
    value.ok_or_else(|| PlayerError::engine(format!("{flag} requires a value")))
}

fn print_usage() {
    println!("usage: player_analyzer --db <library.sqlite3> [--limit <n>]");
}

#[derive(Default)]
struct AnalysisSummary {
    analyzed: usize,
    failed: usize,
}

struct IndexedTrack {
    track: Track,
}

struct AnalysisWorkResult {
    path: PathBuf,
    title: String,
    result: Result<TrackAnalysisResult, String>,
}

struct TrackAnalysisResult {
    loudness: LoudnessInfo,
    fingerprint: Option<FileFingerprint>,
    integrated_lufs: f32,
    true_peak_dbtp: f32,
    duration_ms: Option<u64>,
}

#[derive(Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum AnalyzerEvent {
    Started {
        total: usize,
    },
    TrackFinished {
        index: usize,
        total: usize,
        #[serde(serialize_with = "serialize_path")]
        path: PathBuf,
        title: String,
        integrated_lufs: f32,
        true_peak_dbtp: f32,
        duration_ms: Option<u64>,
        analyzed: usize,
        failed: usize,
    },
    TrackFailed {
        index: usize,
        total: usize,
        #[serde(serialize_with = "serialize_path")]
        path: PathBuf,
        title: String,
        error: String,
        analyzed: usize,
        failed: usize,
    },
    AlbumFinished {
        albums_analyzed: usize,
        album_tracks_updated: usize,
        album_skipped: usize,
    },
    Finished {
        total: usize,
        analyzed: usize,
        failed: usize,
        albums_analyzed: usize,
        album_tracks_updated: usize,
        album_skipped: usize,
    },
    Fatal {
        error: String,
    },
}

fn serialize_path<S>(path: &Path, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&path.to_string_lossy())
}
