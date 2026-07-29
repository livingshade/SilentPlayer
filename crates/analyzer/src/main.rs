use std::env;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;

use analysis_ebur128::{
    analyze_album_loudness, analyze_pending_with_progress, AlbumAnalysisOptions,
    BatchAnalysisOptions, BatchAnalysisProgress, ANALYSIS_VERSION,
};
use errors::{PlayerError, PlayerResult};
use serde::Serialize;
use store_sqlite::LibraryStore;

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
    let track_summary = analyze_pending_with_progress(
        &mut store,
        BatchAnalysisOptions {
            analysis_version: ANALYSIS_VERSION,
            limit: args.limit,
        },
        |progress| match progress {
            BatchAnalysisProgress::Started { total } => emit(&AnalyzerEvent::Started { total }),
            BatchAnalysisProgress::TrackFinished {
                index,
                total,
                path,
                title,
                integrated_lufs,
                true_peak_dbtp,
                duration_ms,
                analyzed,
                failed,
            } => emit(&AnalyzerEvent::TrackFinished {
                index,
                total,
                path,
                title,
                integrated_lufs,
                true_peak_dbtp,
                duration_ms,
                analyzed,
                failed,
            }),
            BatchAnalysisProgress::TrackFailed {
                index,
                total,
                path,
                title,
                error,
                analyzed,
                failed,
            } => emit(&AnalyzerEvent::TrackFailed {
                index,
                total,
                path,
                title,
                error,
                analyzed,
                failed,
            }),
        },
    )?;

    let album_summary = analyze_album_loudness(&mut store, AlbumAnalysisOptions::default())?;
    emit(&AnalyzerEvent::AlbumFinished {
        albums_analyzed: album_summary.albums_analyzed,
        album_tracks_updated: album_summary.tracks_updated,
        album_skipped: album_summary.skipped,
    })?;

    emit(&AnalyzerEvent::Finished {
        total: track_summary.analyzed + track_summary.failed,
        analyzed: track_summary.analyzed,
        failed: track_summary.failed,
        albums_analyzed: album_summary.albums_analyzed,
        album_tracks_updated: album_summary.tracks_updated,
        album_skipped: album_summary.skipped,
    })?;

    Ok(())
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
    println!("usage: analyzer --db <library.sqlite3> [--limit <n>]");
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
