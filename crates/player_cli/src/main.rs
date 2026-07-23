use std::collections::HashSet;
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process;
use std::time::Duration;

use player_analysis_ebur128::{
    analyze_album_loudness, analyze_path, analyze_pending, AlbumAnalysisOptions, AnalysisReport,
    BatchAnalysisOptions,
};
use player_audio_rodio::RodioBackend;
use player_core::{gain_for_track, GainDecision, LoudnessInfo, NormalizationSettings, Track};
use player_engine::AudioRenderSettings;
use player_fingerprint::{audio_hash, file_hash};
use player_library_fs::{LibraryScanner, ScanOptions};
use player_metadata_lofty::{enrich_track, read_track_artwork};
use player_store_sqlite::LibraryStore;

const DEFAULT_DB_PATH: &str = "player_library.sqlite3";

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        print_usage();
        return Ok(());
    };

    match command.as_str() {
        "scan" => {
            let Some(root) = args.next() else {
                print_usage();
                return Ok(());
            };

            let scanner = LibraryScanner::new(ScanOptions::default());
            let tracks = scanner.scan(Path::new(&root))?;
            println!("found {} tracks", tracks.len());
            for track in tracks.iter().take(50) {
                println!("{} | {}", track.id.value(), track.path.display());
            }
            if tracks.len() > 50 {
                println!("... {} more", tracks.len() - 50);
            }
        }
        "play" => {
            let play_args = PlayArgs::parse(args.collect())?;
            let (track, gain, analysis) = build_track_and_gain(&play_args)?;

            println!("playing {}", track.path.display());
            if let Some(analysis) = &analysis {
                print_analysis(analysis);
            }
            println!(
                "normalize status={:?} gain_db={:.2} linear_gain={:.4}",
                gain.status, gain.gain_db, gain.linear_gain
            );

            let mut backend = RodioBackend::open_default()?;
            backend.play_track_blocking(
                &track,
                AudioRenderSettings::new(0, gain),
                play_args.stop_after_ms.map(Duration::from_millis),
            )?;
        }
        "self-test" => {
            let args = SelfTestArgs::parse(args.collect())?;
            let path = env::temp_dir().join("player_cli_self_test.wav");
            write_test_tone_wav(&path, args.tone_duration_ms)?;

            let track = Track::from_path(path.clone());
            let gain = GainDecision::ready(args.gain_db, false);

            println!("playing generated test tone {}", path.display());
            println!(
                "normalize status={:?} gain_db={:.2} linear_gain={:.4}",
                gain.status, gain.gain_db, gain.linear_gain
            );

            let mut backend = RodioBackend::open_default()?;
            backend.play_track_blocking(
                &track,
                AudioRenderSettings::new(0, gain),
                Some(Duration::from_millis(args.stop_after_ms)),
            )?;
        }
        "analyze" => {
            let Some(path) = args.next() else {
                print_usage();
                return Ok(());
            };

            let report = analyze_path(path)?;
            print_analysis(&report);
        }
        "import" => {
            let import_args = ImportArgs::parse(args.collect())?;
            let scanner = LibraryScanner::new(ScanOptions::default());
            let scanned_tracks = scanner.scan(&import_args.root)?;
            let mut metadata_errors = 0_usize;
            let mut duplicates_skipped = 0_usize;
            let mut seen_file_hashes = HashSet::new();
            let mut seen_audio_hashes = HashSet::new();
            let mut artwork_cache = Vec::new();
            let mut store = LibraryStore::open(&import_args.db_path)?;
            let mut tracks = Vec::with_capacity(scanned_tracks.len());

            for mut track in scanned_tracks {
                let track_file_hash = file_hash(&track.path)?;
                if seen_file_hashes.contains(&track_file_hash)
                    || store.track_by_file_hash(&track_file_hash)?.is_some()
                {
                    duplicates_skipped += 1;
                    continue;
                }

                let track_audio_hash = match audio_hash(&track.path) {
                    Ok(fingerprint) => fingerprint.hash,
                    Err(error) => {
                        metadata_errors += 1;
                        eprintln!("fingerprint warning: {}: {error}", track.path.display());
                        continue;
                    }
                };
                if seen_audio_hashes.contains(&track_audio_hash)
                    || store.track_by_audio_hash(&track_audio_hash)?.is_some()
                {
                    duplicates_skipped += 1;
                    continue;
                }

                track.file_hash = Some(track_file_hash.clone());
                track.set_primary_audio_hash(track_audio_hash.clone());

                if let Err(error) = enrich_track(&mut track) {
                    metadata_errors += 1;
                    eprintln!("metadata warning: {}: {error}", track.path.display());
                }
                match read_track_artwork(&track.path) {
                    Ok(images) if !images.is_empty() => {
                        artwork_cache.push((track.path.clone(), images));
                    }
                    Ok(_) => {}
                    Err(error) => {
                        metadata_errors += 1;
                        eprintln!("artwork warning: {}: {error}", track.path.display());
                    }
                }
                tracks.push(track);
                seen_file_hashes.insert(track_file_hash);
                seen_audio_hashes.insert(track_audio_hash);
            }

            store.upsert_tracks(&tracks)?;
            let mut artwork_cached = 0_usize;
            for (path, images) in artwork_cache {
                artwork_cached += store.save_artwork(path, &images)?;
            }
            println!(
                "imported {} tracks into {}",
                tracks.len(),
                import_args.db_path.display()
            );
            println!("duplicates skipped={duplicates_skipped}");
            println!("artwork cached={artwork_cached}");
            if metadata_errors > 0 {
                println!("metadata warnings={metadata_errors}");
            }
        }
        "library" => {
            let args = DbArgs::parse(args.collect())?;
            let store = LibraryStore::open(&args.db_path)?;
            let tracks = store.tracks()?;
            println!("library {} tracks", tracks.len());
            for track in tracks {
                print_track_line(&track);
            }
        }
        "search" => {
            let args = SearchArgs::parse(args.collect())?;
            let store = LibraryStore::open(&args.db_path)?;
            let tracks = store.search_tracks(&args.query, args.limit)?;
            println!("search {} tracks", tracks.len());
            for track in tracks {
                print_track_line(&track);
            }
        }
        "playlist-create" => {
            let args = PlaylistNameArgs::parse(args.collect())?;
            let mut store = LibraryStore::open(&args.db_path)?;
            let playlist_id = store.create_playlist(&args.name)?;
            println!("playlist id={} name={}", playlist_id, args.name);
        }
        "playlists" => {
            let args = DbArgs::parse(args.collect())?;
            let store = LibraryStore::open(&args.db_path)?;
            let playlists = store.playlists()?;
            println!("playlists {}", playlists.len());
            for playlist in playlists {
                println!(
                    "{} | {} | {} tracks",
                    playlist.id, playlist.name, playlist.track_count
                );
            }
        }
        "playlist-add" => {
            let args = PlaylistAddArgs::parse(args.collect())?;
            let mut store = LibraryStore::open(&args.db_path)?;
            let item_id = store.add_playlist_track(&args.name, &args.path)?;
            println!(
                "playlist-add item={} playlist={} path={}",
                item_id,
                args.name,
                args.path.display()
            );
        }
        "playlist" => {
            let args = PlaylistNameArgs::parse(args.collect())?;
            let store = LibraryStore::open(&args.db_path)?;
            let entries = store.playlist_tracks(&args.name)?;
            println!("playlist {} tracks={}", args.name, entries.len());
            for entry in entries {
                println!(
                    "{} | {} | {} | {}",
                    entry.item_id,
                    entry.position,
                    entry.track.title,
                    entry.track.path.display()
                );
            }
        }
        "favorite" => {
            let args = FavoriteArgs::parse(args.collect())?;
            let mut store = LibraryStore::open(&args.db_path)?;
            store.set_favorite(&args.path, !args.unset)?;
            println!(
                "favorite path={} enabled={}",
                args.path.display(),
                !args.unset
            );
        }
        "favorites" => {
            let args = DbArgs::parse(args.collect())?;
            let store = LibraryStore::open(&args.db_path)?;
            let tracks = store.favorite_tracks()?;
            println!("favorites {} tracks", tracks.len());
            for track in tracks {
                print_track_line(&track);
            }
        }
        "history-add" => {
            let args = HistoryAddArgs::parse(args.collect())?;
            let mut store = LibraryStore::open(&args.db_path)?;
            let id = store.record_playback(&args.path, args.position_ms, args.completed)?;
            println!(
                "history id={} path={} position_ms={} completed={}",
                id,
                args.path.display(),
                args.position_ms,
                args.completed
            );
        }
        "history" => {
            let args = HistoryArgs::parse(args.collect())?;
            let store = LibraryStore::open(&args.db_path)?;
            let entries = store.play_history(args.limit)?;
            println!("history {} entries", entries.len());
            for entry in entries {
                println!(
                    "{} | {} | {}ms | completed={} | {}",
                    entry.id,
                    entry.track.title,
                    entry.position_ms,
                    entry.completed,
                    entry.track.path.display()
                );
            }
        }
        "extract-artwork" => {
            let args = ArtworkArgs::parse(args.collect())?;
            let images = read_track_artwork(&args.path)?;
            let mut store = LibraryStore::open(&args.db_path)?;
            let saved = store.save_artwork(&args.path, &images)?;
            let bytes = images.iter().map(|image| image.data.len()).sum::<usize>();
            println!(
                "artwork saved={} bytes={} path={}",
                saved,
                bytes,
                args.path.display()
            );
        }
        "artwork" => {
            let args = ArtworkArgs::parse(args.collect())?;
            let store = LibraryStore::open(&args.db_path)?;
            let images = store.artwork_for_path(&args.path)?;
            println!("artwork {} images", images.len());
            for image in images {
                println!(
                    "{} | {} | {} bytes",
                    image.picture_index,
                    image.mime_type.as_deref().unwrap_or("unknown"),
                    image.data.len()
                );
            }
        }
        "analyze-library" => {
            let args = AnalyzeLibraryArgs::parse(args.collect())?;
            let mut store = LibraryStore::open(&args.db_path)?;
            let summary = analyze_pending(
                &mut store,
                BatchAnalysisOptions {
                    limit: args.limit,
                    ..BatchAnalysisOptions::default()
                },
            )?;
            println!(
                "analysis analyzed={} failed={} db={}",
                summary.analyzed,
                summary.failed,
                args.db_path.display()
            );
            for error in summary.errors {
                eprintln!(
                    "analysis error: {}: {}",
                    error.path.display(),
                    error.message
                );
            }
        }
        "analyze-albums" => {
            let args = AnalyzeAlbumsArgs::parse(args.collect())?;
            let mut store = LibraryStore::open(&args.db_path)?;
            let summary = analyze_album_loudness(
                &mut store,
                AlbumAnalysisOptions {
                    min_tracks: args.min_tracks,
                    ..AlbumAnalysisOptions::default()
                },
            )?;
            println!(
                "album-analysis albums={} tracks={} skipped={} db={}",
                summary.albums_analyzed,
                summary.tracks_updated,
                summary.skipped,
                args.db_path.display()
            );
        }
        _ => print_usage(),
    }

    Ok(())
}

#[derive(Debug)]
struct DbArgs {
    db_path: PathBuf,
}

impl DbArgs {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut db_path = PathBuf::from(DEFAULT_DB_PATH);
        let mut args = args.into_iter();

        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--db" => db_path = PathBuf::from(required_value(&flag, args.next())?),
                _ => return Err(format!("unknown option: {flag}")),
            }
        }

        Ok(Self { db_path })
    }
}

#[derive(Debug)]
struct ImportArgs {
    root: PathBuf,
    db_path: PathBuf,
}

impl ImportArgs {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut args = args.into_iter();
        let root = args
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| "missing music folder path".to_owned())?;
        let db = DbArgs::parse(args.collect())?;
        Ok(Self {
            root,
            db_path: db.db_path,
        })
    }
}

#[derive(Debug)]
struct SearchArgs {
    query: String,
    db_path: PathBuf,
    limit: usize,
}

impl SearchArgs {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut args = args.into_iter();
        let query = args
            .next()
            .ok_or_else(|| "missing search query".to_owned())?;
        let mut db_path = PathBuf::from(DEFAULT_DB_PATH);
        let mut limit = 25_usize;

        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--db" => db_path = PathBuf::from(required_value(&flag, args.next())?),
                "--limit" => limit = parse_value(&flag, args.next())?,
                _ => return Err(format!("unknown search option: {flag}")),
            }
        }

        Ok(Self {
            query,
            db_path,
            limit,
        })
    }
}

#[derive(Debug)]
struct PlaylistNameArgs {
    name: String,
    db_path: PathBuf,
}

impl PlaylistNameArgs {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut args = args.into_iter();
        let name = args
            .next()
            .ok_or_else(|| "missing playlist name".to_owned())?;
        let db = DbArgs::parse(args.collect())?;
        Ok(Self {
            name,
            db_path: db.db_path,
        })
    }
}

#[derive(Debug)]
struct PlaylistAddArgs {
    name: String,
    path: PathBuf,
    db_path: PathBuf,
}

impl PlaylistAddArgs {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut args = args.into_iter();
        let name = args
            .next()
            .ok_or_else(|| "missing playlist name".to_owned())?;
        let path = args
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| "missing music file path".to_owned())?;
        let db = DbArgs::parse(args.collect())?;
        Ok(Self {
            name,
            path,
            db_path: db.db_path,
        })
    }
}

#[derive(Debug)]
struct FavoriteArgs {
    path: PathBuf,
    db_path: PathBuf,
    unset: bool,
}

impl FavoriteArgs {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut args = args.into_iter();
        let path = args
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| "missing music file path".to_owned())?;
        let mut db_path = PathBuf::from(DEFAULT_DB_PATH);
        let mut unset = false;

        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--db" => db_path = PathBuf::from(required_value(&flag, args.next())?),
                "--unset" => unset = true,
                _ => return Err(format!("unknown favorite option: {flag}")),
            }
        }

        Ok(Self {
            path,
            db_path,
            unset,
        })
    }
}

#[derive(Debug)]
struct HistoryAddArgs {
    path: PathBuf,
    db_path: PathBuf,
    position_ms: u64,
    completed: bool,
}

impl HistoryAddArgs {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut args = args.into_iter();
        let path = args
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| "missing music file path".to_owned())?;
        let mut db_path = PathBuf::from(DEFAULT_DB_PATH);
        let mut position_ms = 0_u64;
        let mut completed = false;

        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--db" => db_path = PathBuf::from(required_value(&flag, args.next())?),
                "--position-ms" => position_ms = parse_value(&flag, args.next())?,
                "--completed" => completed = true,
                _ => return Err(format!("unknown history-add option: {flag}")),
            }
        }

        Ok(Self {
            path,
            db_path,
            position_ms,
            completed,
        })
    }
}

#[derive(Debug)]
struct HistoryArgs {
    db_path: PathBuf,
    limit: usize,
}

impl HistoryArgs {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut db_path = PathBuf::from(DEFAULT_DB_PATH);
        let mut limit = 25_usize;
        let mut args = args.into_iter();

        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--db" => db_path = PathBuf::from(required_value(&flag, args.next())?),
                "--limit" => limit = parse_value(&flag, args.next())?,
                _ => return Err(format!("unknown history option: {flag}")),
            }
        }

        Ok(Self { db_path, limit })
    }
}

#[derive(Debug)]
struct ArtworkArgs {
    path: PathBuf,
    db_path: PathBuf,
}

impl ArtworkArgs {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut args = args.into_iter();
        let path = args
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| "missing music file path".to_owned())?;
        let db = DbArgs::parse(args.collect())?;
        Ok(Self {
            path,
            db_path: db.db_path,
        })
    }
}

#[derive(Debug)]
struct AnalyzeLibraryArgs {
    db_path: PathBuf,
    limit: Option<usize>,
}

impl AnalyzeLibraryArgs {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut db_path = PathBuf::from(DEFAULT_DB_PATH);
        let mut limit = None;
        let mut args = args.into_iter();

        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--db" => db_path = PathBuf::from(required_value(&flag, args.next())?),
                "--limit" => limit = Some(parse_value(&flag, args.next())?),
                _ => return Err(format!("unknown analyze-library option: {flag}")),
            }
        }

        Ok(Self { db_path, limit })
    }
}

#[derive(Debug)]
struct AnalyzeAlbumsArgs {
    db_path: PathBuf,
    min_tracks: usize,
}

impl AnalyzeAlbumsArgs {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut db_path = PathBuf::from(DEFAULT_DB_PATH);
        let mut min_tracks = AlbumAnalysisOptions::default().min_tracks;
        let mut args = args.into_iter();

        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--db" => db_path = PathBuf::from(required_value(&flag, args.next())?),
                "--min-tracks" => min_tracks = parse_value(&flag, args.next())?,
                _ => return Err(format!("unknown analyze-albums option: {flag}")),
            }
        }

        Ok(Self {
            db_path,
            min_tracks,
        })
    }
}

#[derive(Debug)]
struct PlayArgs {
    path: PathBuf,
    gain_db: Option<f32>,
    measured_lufs: Option<f32>,
    true_peak_dbtp: Option<f32>,
    target_lufs: f32,
    peak_ceiling_dbtp: f32,
    stop_after_ms: Option<u64>,
    analyze: bool,
}

impl PlayArgs {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut args = args.into_iter();
        let path = args
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| "missing music file path".to_owned())?;

        let mut parsed = Self {
            path,
            gain_db: None,
            measured_lufs: None,
            true_peak_dbtp: None,
            target_lufs: NormalizationSettings::default().target_lufs,
            peak_ceiling_dbtp: NormalizationSettings::default().true_peak_ceiling_dbtp,
            stop_after_ms: None,
            analyze: false,
        };

        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--gain-db" => parsed.gain_db = Some(parse_value(&flag, args.next())?),
                "--measured-lufs" => parsed.measured_lufs = Some(parse_value(&flag, args.next())?),
                "--true-peak-dbtp" => {
                    parsed.true_peak_dbtp = Some(parse_value(&flag, args.next())?)
                }
                "--target-lufs" => parsed.target_lufs = parse_value(&flag, args.next())?,
                "--peak-ceiling-dbtp" => {
                    parsed.peak_ceiling_dbtp = parse_value(&flag, args.next())?
                }
                "--stop-after-ms" => parsed.stop_after_ms = Some(parse_value(&flag, args.next())?),
                "--analyze" => parsed.analyze = true,
                _ => return Err(format!("unknown play option: {flag}")),
            }
        }

        Ok(parsed)
    }
}

#[derive(Debug)]
struct SelfTestArgs {
    gain_db: f32,
    stop_after_ms: u64,
    tone_duration_ms: u64,
}

impl SelfTestArgs {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut parsed = Self {
            gain_db: -24.0,
            stop_after_ms: 250,
            tone_duration_ms: 500,
        };

        let mut args = args.into_iter();
        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--gain-db" => parsed.gain_db = parse_value(&flag, args.next())?,
                "--stop-after-ms" => parsed.stop_after_ms = parse_value(&flag, args.next())?,
                "--tone-duration-ms" => parsed.tone_duration_ms = parse_value(&flag, args.next())?,
                _ => return Err(format!("unknown self-test option: {flag}")),
            }
        }

        Ok(parsed)
    }
}

fn parse_value<T>(flag: &str, value: Option<String>) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let value = required_value(flag, value)?;
    value
        .parse()
        .map_err(|error| format!("invalid value for {flag}: {error}"))
}

fn required_value(flag: &str, value: Option<String>) -> Result<String, String> {
    value.ok_or_else(|| format!("{flag} requires a value"))
}

fn build_track_and_gain(
    args: &PlayArgs,
) -> Result<(Track, GainDecision, Option<AnalysisReport>), Box<dyn std::error::Error>> {
    let mut track = Track::from_path(args.path.clone());
    let mut analysis = None;

    let gain = if let Some(gain_db) = args.gain_db {
        GainDecision::ready(gain_db, false)
    } else {
        if let Some(lufs) = args.measured_lufs {
            track.loudness = Some(LoudnessInfo::track(
                lufs,
                args.true_peak_dbtp.unwrap_or(-6.0),
            ));
        } else if args.analyze {
            let report = analyze_path(&args.path)?;
            track.loudness = report.loudness_info();
            analysis = Some(report);
        }

        let settings = NormalizationSettings {
            target_lufs: args.target_lufs,
            true_peak_ceiling_dbtp: args.peak_ceiling_dbtp,
            ..NormalizationSettings::default()
        };
        gain_for_track(&track, settings)
    };

    Ok((track, gain, analysis))
}

fn print_track_line(track: &Track) {
    println!(
        "{} | {} | {} | {} | {}",
        track.id.value(),
        track.title,
        track.artist.as_deref().unwrap_or(""),
        track
            .duration_ms
            .map(|duration| duration.to_string())
            .unwrap_or_default(),
        track.path.display()
    );
}

fn print_analysis(report: &AnalysisReport) {
    println!("analysis file={}", report.path.display());
    println!(
        "analysis sample_rate={}Hz channels={} duration={:.2}s",
        report.sample_rate_hz, report.channels, report.duration_seconds
    );
    println!(
        "analysis integrated_lufs={} true_peak_dbtp={}",
        format_optional_db(report.integrated_lufs),
        format_optional_db(report.true_peak_dbtp)
    );
}

fn format_optional_db(value: Option<f32>) -> String {
    value
        .map(|value| format!("{value:.2}"))
        .unwrap_or_else(|| "n/a".to_owned())
}

fn write_test_tone_wav(path: &Path, duration_ms: u64) -> Result<(), Box<dyn std::error::Error>> {
    let sample_rate = 44_100_u32;
    let channels = 1_u16;
    let bits_per_sample = 16_u16;
    let sample_count = (u64::from(sample_rate) * duration_ms / 1000) as u32;
    let byte_rate = sample_rate * u32::from(channels) * u32::from(bits_per_sample) / 8;
    let block_align = channels * bits_per_sample / 8;
    let data_size = sample_count * u32::from(block_align);
    let riff_size = 36 + data_size;

    let mut file = File::create(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&riff_size.to_le_bytes())?;
    file.write_all(b"WAVE")?;
    file.write_all(b"fmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&channels.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&block_align.to_le_bytes())?;
    file.write_all(&bits_per_sample.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_size.to_le_bytes())?;

    for index in 0..sample_count {
        let t = index as f32 / sample_rate as f32;
        let sample = (t * 440.0 * std::f32::consts::TAU).sin() * 0.20;
        let pcm = (sample * f32::from(i16::MAX)) as i16;
        file.write_all(&pcm.to_le_bytes())?;
    }

    Ok(())
}

fn print_usage() {
    eprintln!("usage:");
    eprintln!("  cargo run -p player_cli -- scan <music-folder>");
    eprintln!("  cargo run -p player_cli -- import <music-folder> [--db <library.sqlite3>]");
    eprintln!("  cargo run -p player_cli -- library [--db <library.sqlite3>]");
    eprintln!("  cargo run -p player_cli -- search <query> [--db <library.sqlite3>] [--limit <n>]");
    eprintln!("  cargo run -p player_cli -- playlist-create <name> [--db <library.sqlite3>]");
    eprintln!(
        "  cargo run -p player_cli -- playlist-add <name> <music-file> [--db <library.sqlite3>]"
    );
    eprintln!("  cargo run -p player_cli -- playlist <name> [--db <library.sqlite3>]");
    eprintln!("  cargo run -p player_cli -- playlists [--db <library.sqlite3>]");
    eprintln!(
        "  cargo run -p player_cli -- favorite <music-file> [--db <library.sqlite3>] [--unset]"
    );
    eprintln!("  cargo run -p player_cli -- favorites [--db <library.sqlite3>]");
    eprintln!("  cargo run -p player_cli -- history-add <music-file> [--db <library.sqlite3>] [--position-ms <ms>] [--completed]");
    eprintln!("  cargo run -p player_cli -- history [--db <library.sqlite3>] [--limit <n>]");
    eprintln!("  cargo run -p player_cli -- extract-artwork <music-file> [--db <library.sqlite3>]");
    eprintln!("  cargo run -p player_cli -- artwork <music-file> [--db <library.sqlite3>]");
    eprintln!(
        "  cargo run -p player_cli -- analyze-library [--db <library.sqlite3>] [--limit <n>]"
    );
    eprintln!(
        "  cargo run -p player_cli -- analyze-albums [--db <library.sqlite3>] [--min-tracks <n>]"
    );
    eprintln!("  cargo run -p player_cli -- analyze <music-file>");
    eprintln!("  cargo run -p player_cli -- play <music-file> [--gain-db <db>]");
    eprintln!("  cargo run -p player_cli -- play <music-file> --analyze");
    eprintln!("  cargo run -p player_cli -- play <music-file> --measured-lufs <lufs> [--true-peak-dbtp <dbtp>]");
    eprintln!("  cargo run -p player_cli -- self-test");
}
