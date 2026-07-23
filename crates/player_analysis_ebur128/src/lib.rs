use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

use ebur128_stream::{AnalyzerBuilder, Channel, Mode};
use player_core::{FileFingerprint, LoudnessInfo, Track};
use player_error::{PlayerError, PlayerResult};
use player_library_fs::fingerprint_from_metadata;
use player_store_sqlite::LibraryStore;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub const ANALYSIS_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq)]
pub struct AnalysisReport {
    pub path: PathBuf,
    pub sample_rate_hz: u32,
    pub channels: usize,
    pub duration_seconds: f64,
    pub integrated_lufs: Option<f32>,
    pub true_peak_dbtp: Option<f32>,
}

impl AnalysisReport {
    pub fn loudness_info(&self) -> Option<LoudnessInfo> {
        Some(LoudnessInfo {
            integrated_lufs: self.integrated_lufs?,
            true_peak_dbtp: self.true_peak_dbtp?,
            album_integrated_lufs: None,
            album_true_peak_dbtp: None,
            analysis_version: ANALYSIS_VERSION,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchAnalysisOptions {
    pub analysis_version: u32,
    pub limit: Option<usize>,
}

impl Default for BatchAnalysisOptions {
    fn default() -> Self {
        Self {
            analysis_version: ANALYSIS_VERSION,
            limit: None,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BatchAnalysisSummary {
    pub analyzed: usize,
    pub failed: usize,
    pub errors: Vec<BatchAnalysisError>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchAnalysisError {
    pub path: PathBuf,
    pub message: String,
}

struct BatchAnalysisWorkResult {
    track: Track,
    result: Result<BatchAnalysisWorkPayload, String>,
}

struct BatchAnalysisWorkPayload {
    loudness: LoudnessInfo,
    fingerprint: Option<FileFingerprint>,
    duration_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AlbumAnalysisOptions {
    pub analysis_version: u32,
    pub min_tracks: usize,
}

impl Default for AlbumAnalysisOptions {
    fn default() -> Self {
        Self {
            analysis_version: ANALYSIS_VERSION,
            min_tracks: 1,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AlbumAnalysisSummary {
    pub albums_analyzed: usize,
    pub tracks_updated: usize,
    pub skipped: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlbumLoudness {
    pub integrated_lufs: f32,
    pub true_peak_dbtp: f32,
}

pub fn analyze_pending(
    store: &mut LibraryStore,
    options: BatchAnalysisOptions,
) -> PlayerResult<BatchAnalysisSummary> {
    let pending = store.pending_analysis(options.analysis_version, options.limit)?;
    let mut summary = BatchAnalysisSummary::default();

    let worker_count = worker_count(pending.len());
    let chunks = distribute_tracks(pending, worker_count);
    let (tx, rx) = mpsc::channel();

    thread::scope(|scope| {
        for chunk in chunks {
            let tx = tx.clone();
            scope.spawn(move || {
                for track in chunk {
                    let result = analyze_track_payload(&track, options.analysis_version)
                        .map_err(|error| error.to_string());
                    if tx.send(BatchAnalysisWorkResult { track, result }).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);

        for result in rx {
            match result.result {
                Ok(analysis) => match store.save_loudness_with_duration(
                    &result.track.path,
                    analysis.fingerprint,
                    analysis.duration_ms,
                    analysis.loudness,
                ) {
                    Ok(()) => summary.analyzed += 1,
                    Err(error) => {
                        summary.failed += 1;
                        summary.errors.push(BatchAnalysisError {
                            path: result.track.path,
                            message: error.to_string(),
                        });
                    }
                },
                Err(error) => {
                    summary.failed += 1;
                    summary.errors.push(BatchAnalysisError {
                        path: result.track.path,
                        message: error,
                    });
                }
            }
        }
    });

    Ok(summary)
}

fn analyze_track_payload(
    track: &Track,
    analysis_version: u32,
) -> PlayerResult<BatchAnalysisWorkPayload> {
    let report = analyze_path(&track.path)?;
    let mut loudness = report
        .loudness_info()
        .ok_or_else(|| PlayerError::audio("analysis produced no loudness result"))?;
    loudness.analysis_version = analysis_version;

    let fingerprint = std::fs::metadata(&track.path)
        .ok()
        .map(|metadata| fingerprint_from_metadata(&metadata));
    let duration_ms = duration_ms_from_seconds(report.duration_seconds);

    Ok(BatchAnalysisWorkPayload {
        loudness,
        fingerprint,
        duration_ms,
    })
}

pub fn analyze_album_loudness(
    store: &mut LibraryStore,
    options: AlbumAnalysisOptions,
) -> PlayerResult<AlbumAnalysisSummary> {
    let mut summary = AlbumAnalysisSummary::default();

    for group in store.album_groups()? {
        if group.tracks.len() < options.min_tracks {
            summary.skipped += 1;
            continue;
        }

        if group
            .tracks
            .iter()
            .all(|track| album_loudness_is_current(track, options.analysis_version))
        {
            summary.skipped += 1;
            continue;
        }

        let Some(album_loudness) =
            album_loudness_from_tracks(&group.tracks, options.analysis_version)
        else {
            summary.skipped += 1;
            continue;
        };

        let paths = group
            .tracks
            .iter()
            .map(|track| track.path.clone())
            .collect::<Vec<_>>();
        let updated = store.save_album_loudness_for_paths(
            &paths,
            album_loudness.integrated_lufs,
            album_loudness.true_peak_dbtp,
            options.analysis_version,
        )?;

        summary.albums_analyzed += 1;
        summary.tracks_updated += updated;
    }

    Ok(summary)
}

pub fn album_loudness_from_tracks(
    tracks: &[Track],
    analysis_version: u32,
) -> Option<AlbumLoudness> {
    let mut weighted_power = 0.0_f64;
    let mut duration_sum = 0.0_f64;
    let mut true_peak_dbtp = f32::NEG_INFINITY;

    for track in tracks {
        let loudness = track.loudness.as_ref()?;
        if loudness.analysis_version != analysis_version {
            return None;
        }

        let duration_seconds = track.duration_ms? as f64 / 1000.0;
        if duration_seconds <= 0.0 {
            return None;
        }

        weighted_power += lufs_to_power(loudness.integrated_lufs) * duration_seconds;
        duration_sum += duration_seconds;
        true_peak_dbtp = true_peak_dbtp.max(loudness.true_peak_dbtp);
    }

    if duration_sum <= 0.0 || weighted_power <= 0.0 || !true_peak_dbtp.is_finite() {
        return None;
    }

    Some(AlbumLoudness {
        integrated_lufs: power_to_lufs(weighted_power / duration_sum),
        true_peak_dbtp,
    })
}

pub fn analyze_path(path: impl AsRef<Path>) -> PlayerResult<AnalysisReport> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|source| PlayerError::io(path.to_path_buf(), source))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
        hint.with_extension(extension);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|error| PlayerError::audio(format!("failed to probe audio: {error}")))?;

    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| PlayerError::audio("no default audio track found"))?;
    let track_id = track.id;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|error| PlayerError::audio(format!("failed to create decoder: {error}")))?;

    let mut analyzer = None;
    let mut sample_rate_hz = 0;
    let mut channels = 0_usize;
    let mut total_frames = 0_u64;

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(SymphoniaError::ResetRequired) => {
                return Err(PlayerError::audio(
                    "stream reset required while analyzing; dynamic tracks are not supported yet",
                ));
            }
            Err(error) => {
                return Err(PlayerError::audio(format!(
                    "failed to read audio packet: {error}"
                )));
            }
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(error) => {
                return Err(PlayerError::audio(format!(
                    "failed to decode audio packet: {error}"
                )));
            }
        };

        let spec = *decoded.spec();
        let packet_channels = spec.channels.count();
        if packet_channels == 0 {
            continue;
        }

        if analyzer.is_none() {
            sample_rate_hz = spec.rate;
            channels = packet_channels;
            let layout = channel_layout(packet_channels);
            analyzer = Some(
                AnalyzerBuilder::new()
                    .sample_rate(sample_rate_hz)
                    .channels(&layout)
                    .modes(Mode::Integrated | Mode::TruePeak)
                    .build()
                    .map_err(|error| {
                        PlayerError::audio(format!("failed to build loudness analyzer: {error}"))
                    })?,
            );
        }

        if packet_channels != channels || spec.rate != sample_rate_hz {
            return Err(PlayerError::audio(
                "sample rate or channel count changed during analysis",
            ));
        }

        let mut samples = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        samples.copy_interleaved_ref(decoded);

        if let Some(analyzer) = analyzer.as_mut() {
            analyzer
                .push_interleaved::<f32>(samples.samples())
                .map_err(|error| {
                    PlayerError::audio(format!("loudness analysis failed: {error}"))
                })?;
        }

        total_frames += (samples.samples().len() / channels) as u64;
    }

    let analyzer =
        analyzer.ok_or_else(|| PlayerError::audio("no decodable audio samples found"))?;
    let report = analyzer.finalize();
    let duration_seconds = if sample_rate_hz == 0 {
        0.0
    } else {
        total_frames as f64 / f64::from(sample_rate_hz)
    };

    Ok(AnalysisReport {
        path: path.to_path_buf(),
        sample_rate_hz,
        channels,
        duration_seconds,
        integrated_lufs: report.integrated_lufs().map(|value| value as f32),
        true_peak_dbtp: report.true_peak_dbtp().map(|value| value as f32),
    })
}

fn channel_layout(channels: usize) -> Vec<Channel> {
    match channels {
        1 => vec![Channel::Center],
        2 => vec![Channel::Left, Channel::Right],
        6 => vec![
            Channel::Left,
            Channel::Right,
            Channel::Center,
            Channel::Lfe,
            Channel::LeftSurround,
            Channel::RightSurround,
        ],
        count => vec![Channel::Other; count],
    }
}

fn album_loudness_is_current(track: &Track, analysis_version: u32) -> bool {
    track.loudness.as_ref().is_some_and(|loudness| {
        loudness.analysis_version == analysis_version
            && loudness.album_integrated_lufs.is_some()
            && loudness.album_true_peak_dbtp.is_some()
    })
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

fn distribute_tracks(tracks: Vec<Track>, worker_count: usize) -> Vec<Vec<Track>> {
    if worker_count == 0 {
        return Vec::new();
    }
    let mut chunks = (0..worker_count).map(|_| Vec::new()).collect::<Vec<_>>();
    for (offset, track) in tracks.into_iter().enumerate() {
        chunks[offset % worker_count].push(track);
    }
    chunks
        .into_iter()
        .filter(|chunk| !chunk.is_empty())
        .collect()
}

fn lufs_to_power(lufs: f32) -> f64 {
    10.0_f64.powf(f64::from(lufs) / 10.0)
}

fn power_to_lufs(power: f64) -> f32 {
    (10.0 * power.log10()) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn analyzes_generated_wav() {
        let path = std::env::temp_dir().join(format!(
            "player_analysis_{}.wav",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        write_test_tone_wav(&path, 1_200).unwrap();

        let report = analyze_path(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(report.sample_rate_hz, 44_100);
        assert_eq!(report.channels, 1);
        assert!(report.duration_seconds > 1.1);
        assert!(report.integrated_lufs.is_some());
        assert!(report.true_peak_dbtp.is_some());
        assert!(report.loudness_info().is_some());
    }

    #[test]
    fn analyzes_downloaded_ogg_fixtures() {
        for path in downloaded_fixture_paths() {
            let report = analyze_path(&path).unwrap();
            assert_eq!(report.sample_rate_hz, 44_100, "{}", path.display());
            assert_eq!(report.channels, 2, "{}", path.display());
            assert!(report.duration_seconds > 1.0, "{}", path.display());
            assert!(report.integrated_lufs.is_some(), "{}", path.display());
            assert!(report.true_peak_dbtp.is_some(), "{}", path.display());
            assert!(report.loudness_info().is_some(), "{}", path.display());
        }
    }

    #[test]
    fn returns_error_for_non_audio_file() {
        let path = std::env::temp_dir().join(format!(
            "player_analysis_not_audio_{}.txt",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, "not audio").unwrap();

        let err = analyze_path(&path).unwrap_err();
        std::fs::remove_file(&path).ok();

        assert!(err.to_string().contains("audio"));
    }

    #[test]
    fn batch_analyzes_pending_tracks_and_caches_results() {
        let mut store = LibraryStore::in_memory().unwrap();
        let track = player_core::Track::from_path(fixture("funk_room_reverb.ogg"));
        store.upsert_track(&track).unwrap();

        let summary = analyze_pending(&mut store, BatchAnalysisOptions::default()).unwrap();

        assert_eq!(summary.analyzed, 1);
        assert_eq!(summary.failed, 0);
        assert_eq!(
            store
                .pending_analysis(ANALYSIS_VERSION, None)
                .unwrap()
                .len(),
            0
        );
        let loaded = store.track_by_path(&track.path).unwrap().unwrap();
        assert!(loaded.loudness.is_some());
        assert!(loaded.duration_ms.unwrap_or_default() > 1000);
    }

    #[test]
    fn computes_weighted_album_loudness_from_cached_tracks() {
        let tracks = vec![
            album_track("/music/a.ogg", -20.0, -3.0, 10_000),
            album_track("/music/b.ogg", -10.0, -2.0, 30_000),
        ];

        let album = album_loudness_from_tracks(&tracks, ANALYSIS_VERSION).unwrap();

        assert!((album.integrated_lufs - -11.106).abs() < 0.01);
        assert_eq!(album.true_peak_dbtp, -2.0);
    }

    #[test]
    fn album_analysis_updates_album_gain_cache() {
        let mut store = LibraryStore::in_memory().unwrap();
        let tracks = vec![
            album_track("/music/a.ogg", -20.0, -3.0, 10_000),
            album_track("/music/b.ogg", -10.0, -2.0, 30_000),
        ];
        store.upsert_tracks(&tracks).unwrap();

        let summary = analyze_album_loudness(&mut store, AlbumAnalysisOptions::default()).unwrap();

        assert_eq!(
            summary,
            AlbumAnalysisSummary {
                albums_analyzed: 1,
                tracks_updated: 2,
                skipped: 0,
            }
        );
        let loaded = store.track_by_path("/music/a.ogg").unwrap().unwrap();
        let loudness = loaded.loudness.unwrap();
        assert!(loudness.album_integrated_lufs.is_some());
        assert_eq!(loudness.album_true_peak_dbtp, Some(-2.0));

        let summary = analyze_album_loudness(&mut store, AlbumAnalysisOptions::default()).unwrap();
        assert_eq!(summary.albums_analyzed, 0);
        assert_eq!(summary.skipped, 1);
    }

    fn downloaded_fixture_paths() -> Vec<PathBuf> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("test-assets")
            .join("audio");
        vec![
            root.join("into_the_oceans_chorus.ogg"),
            root.join("into_the_oceans_instrumental.ogg"),
            root.join("funk_room_reverb.ogg"),
        ]
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

    fn album_track(
        path: &str,
        integrated_lufs: f32,
        true_peak_dbtp: f32,
        duration_ms: u64,
    ) -> Track {
        let mut track = Track::from_path(path.into());
        track.album = Some("Album".to_owned());
        track.album_artist = Some("Band".to_owned());
        track.duration_ms = Some(duration_ms);
        track.loudness = Some(LoudnessInfo {
            integrated_lufs,
            true_peak_dbtp,
            album_integrated_lufs: None,
            album_true_peak_dbtp: None,
            analysis_version: ANALYSIS_VERSION,
        });
        track
    }

    fn write_test_tone_wav(path: &Path, duration_ms: u64) -> std::io::Result<()> {
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
            let sample = (t * 1_000.0 * std::f32::consts::TAU).sin() * 0.20;
            let pcm = (sample * f32::from(i16::MAX)) as i16;
            file.write_all(&pcm.to_le_bytes())?;
        }

        Ok(())
    }
}
