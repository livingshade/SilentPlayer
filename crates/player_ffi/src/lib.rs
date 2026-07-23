#![allow(clippy::missing_safety_doc, clippy::not_unsafe_ptr_arg_deref)]

mod client;

pub use client::{SilentAppClient, SilentAppClientError};

use std::collections::{BTreeMap, HashSet};
use std::ffi::{c_char, CStr, CString};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Component, Path, PathBuf};
use std::ptr;
use std::time::{SystemTime, UNIX_EPOCH};

use player_analysis_ebur128::{
    analyze_album_loudness, analyze_pending, AlbumAnalysisOptions, BatchAnalysisOptions,
};
use player_audio_rodio::RodioBackend;
use player_core::{
    gain_for_track, ArtworkImage, LoudnessStatus, NormalizationSettings, PlaybackLifecycle,
    PlaybackLifecycleAction, RepeatMode, Track, TrackId, TrackViewId, TrackViewKind,
};
use player_engine::{PlaybackEvent, PlayerEngine};
use player_error::{PlayerError, PlayerResult};
use player_fingerprint::{audio_hash, file_hash};
use player_library_fs::{
    fingerprint_from_metadata, is_supported_audio_file, LibraryScanner, ScanOptions,
};
use player_metadata_lofty::{enrich_track, read_track_artwork};
use player_store_sqlite::{LibraryStore, PlaylistSort, PlaylistSummary};
use serde::{Deserialize, Serialize};

pub struct PlayerApp {
    db_path: PathBuf,
    media_root: PathBuf,
    activity_store: UserActivityStore,
    local_user: Option<LocalUserProfile>,
    active_session: Option<ActivePlaybackSession>,
    pending_session_end_reason: Option<String>,
    engine: Option<PlayerEngine>,
    current_track: Option<TrackDto>,
    queue_tracks: Vec<TrackDto>,
    queue_current_index: Option<usize>,
    repeat_mode: RepeatMode,
    shuffle_enabled: bool,
    is_playing: bool,
    position_ms: u64,
    gain_db: Option<f32>,
    loudness_status: Option<String>,
    last_error: Option<String>,
    playback_lifecycle: PlaybackLifecycle,
}

#[derive(Serialize)]
struct Response<T: Serialize> {
    ok: bool,
    data: Option<T>,
    error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct TrackDto {
    id: String,
    view_id: String,
    primary_view_id: String,
    is_primary_view: bool,
    view_kind: String,
    view_name: Option<String>,
    rating: Option<u8>,
    title: String,
    artist: Option<String>,
    album: Option<String>,
    duration_ms: Option<u64>,
    artwork_count: u32,
    artwork_path: Option<String>,
    artwork_source: Option<String>,
    has_album_identity: bool,
    path: String,
    quality_profile: Option<String>,
    format_name: Option<String>,
    gain_db: Option<f32>,
    loudness_status: String,
}

#[derive(Serialize)]
struct ImportSummary {
    imported: usize,
    copied: usize,
    duplicates_skipped: usize,
    artwork_cached: usize,
    metadata_warnings: usize,
}

#[derive(Deserialize, Serialize)]
struct LibraryPackageManifest {
    format_version: u32,
    database_file: String,
    tracks: Vec<LibraryPackageTrack>,
}

#[derive(Deserialize, Serialize)]
struct LibraryPackageTrack {
    database_path: String,
    audio_file: String,
}

#[derive(Serialize)]
struct LibraryPackageSummary {
    tracks: usize,
    audio_files: usize,
    sidecar_files: usize,
}

struct PendingImportTrack {
    source_root: PathBuf,
    track: Track,
}

#[derive(Serialize)]
struct AnalysisSummary {
    tracks_analyzed: usize,
    track_failures: usize,
    albums_analyzed: usize,
    album_tracks_updated: usize,
    album_skipped: usize,
}

#[derive(Serialize)]
struct PlaybackSnapshot {
    is_playing: bool,
    position_ms: u64,
    current_track: Option<TrackDto>,
    queue_len: usize,
    queue_position: Option<usize>,
    repeat_mode: String,
    shuffle_enabled: bool,
    gain_db: Option<f32>,
    loudness_status: Option<String>,
    error: Option<String>,
    interruption_active: bool,
    resume_after_interruption: bool,
}

#[derive(Serialize)]
struct PlaybackQueueDto {
    tracks: Vec<TrackDto>,
    current_index: Option<usize>,
    repeat_mode: String,
    shuffle_enabled: bool,
}

#[derive(Serialize)]
struct TrackDetailsDto {
    view_id: String,
    primary_view_id: String,
    is_primary_view: bool,
    view_kind: String,
    view_name: Option<String>,
    rating: Option<u8>,
    transform_spec: Option<String>,
    quality_profile: Option<String>,
    format_name: Option<String>,
    artwork_path: Option<String>,
    artwork_source: Option<String>,
    lyrics_path: Option<String>,
    lyrics_text: Option<String>,
    notes: Option<String>,
    audio_hash: String,
    original_title: String,
    original_artist: Option<String>,
    original_album: Option<String>,
    display_title: String,
    display_artist: Option<String>,
    display_album: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TrackViewEditRequest {
    view_name: Option<String>,
    title: String,
    artist: Option<String>,
    album: Option<String>,
    notes: Option<String>,
    artwork_path: Option<String>,
    lyrics_path: Option<String>,
}

#[derive(Serialize)]
struct AlbumArtworkSummary {
    tracks_updated: usize,
}

#[derive(Serialize)]
struct AuditSummary {
    tracks_scanned: usize,
    hashes_updated: usize,
    duplicate_groups: usize,
    tracks_merged: usize,
    failures: usize,
}

#[derive(Serialize)]
struct PlaylistDto {
    id: i64,
    name: String,
    track_count: usize,
    artwork_path: Option<String>,
    artwork_source: Option<String>,
}

#[derive(Clone, Debug)]
struct UserActivityStore {
    root: PathBuf,
    profile_path: PathBuf,
    history_path: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LocalUserProfile {
    schema_version: u32,
    user_id: String,
    display_name: String,
    sync_enabled: bool,
    created_at_unix_seconds: i64,
    updated_at_unix_seconds: i64,
}

#[derive(Clone, Debug)]
struct ActivePlaybackSession {
    session_id: String,
    track: TrackDto,
    started_at_unix_seconds: i64,
    start_position_ms: u64,
    last_position_ms: u64,
    listened_ms: u64,
    seek_count: u32,
}

#[derive(Serialize)]
struct UserDataDto {
    user_id: String,
    display_name: String,
    sync_enabled: bool,
    profile_path: String,
    history_path: String,
    created_at_unix_seconds: i64,
}

#[derive(Serialize)]
struct PlaybackHistoryRecord {
    schema_version: u32,
    record_type: String,
    user_id: String,
    session_id: String,
    started_at_unix_seconds: i64,
    ended_at_unix_seconds: i64,
    start_position_ms: u64,
    end_position_ms: u64,
    listened_ms: u64,
    track_duration_ms: Option<u64>,
    completion_ratio: Option<f32>,
    completed: bool,
    finish_reason: String,
    seek_count: u32,
    track: PlaybackTrackRecord,
}

#[derive(Serialize)]
struct PlaybackTrackRecord {
    id: String,
    title: String,
    artist: Option<String>,
    album: Option<String>,
    path: String,
    gain_db: Option<f32>,
    loudness_status: String,
}

#[derive(Serialize)]
struct Empty {}

const LIBRARY_PACKAGE_FORMAT_VERSION: u32 = 1;
const LIBRARY_PACKAGE_DATABASE_FILE: &str = "player_library.sqlite3";
const LIBRARY_PACKAGE_MANIFEST_FILE: &str = "manifest.json";
const LIBRARY_PACKAGE_MUSIC_DIRECTORY: &str = "Music";

#[no_mangle]
pub unsafe extern "C" fn player_app_create(
    db_path: *const c_char,
    media_root: *const c_char,
) -> *mut PlayerApp {
    let Ok(db_path) = (unsafe { c_string(db_path) }) else {
        return ptr::null_mut();
    };
    let Ok(media_root) = (unsafe { c_string(media_root) }) else {
        return ptr::null_mut();
    };
    create_app(PathBuf::from(db_path), PathBuf::from(media_root))
}

fn create_app(db_path: PathBuf, media_root: PathBuf) -> *mut PlayerApp {
    let activity_store = UserActivityStore::for_db(&db_path);
    let local_user = activity_store.load_or_create_profile().ok();
    Box::into_raw(Box::new(PlayerApp {
        db_path,
        media_root,
        activity_store,
        local_user,
        active_session: None,
        pending_session_end_reason: None,
        engine: None,
        current_track: None,
        queue_tracks: Vec::new(),
        queue_current_index: None,
        repeat_mode: RepeatMode::Off,
        shuffle_enabled: false,
        is_playing: false,
        position_ms: 0,
        gain_db: None,
        loudness_status: None,
        last_error: None,
        playback_lifecycle: PlaybackLifecycle::default(),
    }))
}

#[no_mangle]
pub unsafe extern "C" fn player_app_destroy(app: *mut PlayerApp) {
    if app.is_null() {
        return;
    }
    let mut app = Box::from_raw(app);
    app.poll_events();
    app.finish_active_session("app_destroy").ok();
    drop(app);
}

#[no_mangle]
pub unsafe extern "C" fn player_string_free(value: *mut c_char) {
    if value.is_null() {
        return;
    }
    drop(CString::from_raw(value));
}

#[no_mangle]
pub unsafe extern "C" fn player_app_export_library(
    app: *mut PlayerApp,
    package_path: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let package_path = PathBuf::from(c_string(package_path)?);
        app.export_library(&package_path)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_import_library(
    app: *mut PlayerApp,
    package_path: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let package_path = PathBuf::from(c_string(package_path)?);
        app.import_library(&package_path)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_zero_out_library(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.zero_out_library()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_import_folder(
    app: *mut PlayerApp,
    folder: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let folder = PathBuf::from(c_string(folder)?);
        let scanner = LibraryScanner::new(ScanOptions::default());
        let pending_tracks = scanner
            .scan(&folder)?
            .into_iter()
            .map(|track| PendingImportTrack {
                source_root: folder.clone(),
                track,
            })
            .collect();

        import_pending_tracks(app, pending_tracks, 0)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_import_files(
    app: *mut PlayerApp,
    paths_json: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let paths: Vec<String> = serde_json::from_str(&c_string(paths_json)?)
            .map_err(|error| PlayerError::metadata(format!("invalid import file list: {error}")))?;
        if paths.is_empty() {
            return Err(PlayerError::metadata("no import files selected"));
        }

        let mut pending_tracks = Vec::with_capacity(paths.len());
        let mut metadata_warnings = 0_usize;
        for path in paths {
            let path = PathBuf::from(path);
            if !is_supported_audio_file(&path) {
                metadata_warnings += 1;
                continue;
            }
            let metadata =
                fs::metadata(&path).map_err(|source| PlayerError::io(path.clone(), source))?;
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
            pending_tracks.push(PendingImportTrack { source_root, track });
        }

        import_pending_tracks(app, pending_tracks, metadata_warnings)
    })
}

fn import_pending_tracks(
    app: &mut PlayerApp,
    pending_tracks: Vec<PendingImportTrack>,
    initial_metadata_warnings: usize,
) -> PlayerResult<ImportSummary> {
    let media_root = app.media_root.clone();
    fs::create_dir_all(&media_root)
        .map_err(|source| PlayerError::io(media_root.clone(), source))?;

    let mut store = app.store()?;
    let mut tracks = Vec::with_capacity(pending_tracks.len());
    let mut copied = 0_usize;
    let mut duplicates_skipped = 0_usize;
    let mut metadata_warnings = initial_metadata_warnings;
    let mut seen_file_hashes = HashSet::new();
    let mut seen_audio_hashes = HashSet::new();
    for pending in pending_tracks {
        let source_track = pending.track;
        let source_file_hash = file_hash(&source_track.path)?;
        if seen_file_hashes.contains(&source_file_hash)
            || store.track_by_file_hash(&source_file_hash)?.is_some()
        {
            duplicates_skipped += 1;
            continue;
        }

        let source_audio_hash = match audio_hash(&source_track.path) {
            Ok(fingerprint) => fingerprint.hash,
            Err(_) => {
                metadata_warnings += 1;
                continue;
            }
        };
        if seen_audio_hashes.contains(&source_audio_hash)
            || store.track_by_audio_hash(&source_audio_hash)?.is_some()
        {
            duplicates_skipped += 1;
            continue;
        }

        let destination =
            managed_import_path(&pending.source_root, &source_track.path, &media_root);
        if copy_into_media_library(&source_track.path, &destination)? {
            copied += 1;
        }
        copy_related_sidecars(&source_track.path, &destination)?;
        let mut track = Track::from_path(destination.clone());
        track.fingerprint = fs::metadata(&destination)
            .ok()
            .map(|metadata| fingerprint_from_metadata(&metadata));
        track.file_hash = Some(source_file_hash.clone());
        track.set_primary_audio_hash(source_audio_hash.clone());
        tracks.push(track);
        seen_file_hashes.insert(source_file_hash);
        seen_audio_hashes.insert(source_audio_hash);
    }

    let mut artwork_cache = Vec::new();

    for track in &mut tracks {
        if enrich_track(track).is_err() {
            metadata_warnings += 1;
        }
        match read_track_artwork(&track.path) {
            Ok(images) if !images.is_empty() => {
                artwork_cache.push((track.path.clone(), images));
            }
            Ok(_) => {}
            Err(_) => metadata_warnings += 1,
        }
    }

    store.upsert_tracks(&tracks)?;
    let mut artwork_cached = 0_usize;
    for (path, images) in artwork_cache {
        artwork_cached += store.save_artwork(path, &images)?;
    }

    Ok(ImportSummary {
        imported: tracks.len(),
        copied,
        duplicates_skipped,
        artwork_cached,
        metadata_warnings,
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_library(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let store = app.store()?;
        let tracks = store.tracks()?;
        track_dtos_with_artwork(&tracks, &store, &app.db_path)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_search(
    app: *mut PlayerApp,
    query: *const c_char,
    limit: usize,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let query = c_string(query)?;
        let store = app.store()?;
        let tracks = store.search_tracks(&query, limit.max(1))?;
        track_dtos_with_artwork(&tracks, &store, &app.db_path)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_analyze(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let mut store = app.store()?;
        let track_summary = analyze_pending(&mut store, BatchAnalysisOptions::default())?;
        let album_summary = analyze_album_loudness(&mut store, AlbumAnalysisOptions::default())?;
        Ok(AnalysisSummary {
            tracks_analyzed: track_summary.analyzed,
            track_failures: track_summary.failed,
            albums_analyzed: album_summary.albums_analyzed,
            album_tracks_updated: album_summary.tracks_updated,
            album_skipped: album_summary.skipped,
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_audit_database(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.audit_database()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_user_data(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let profile = app.local_user()?.clone();
        Ok(UserDataDto {
            user_id: profile.user_id.clone(),
            display_name: profile.display_name.clone(),
            sync_enabled: profile.sync_enabled,
            profile_path: path_to_string_lossy(&app.activity_store.profile_path),
            history_path: path_to_string_lossy(&app.activity_store.history_path),
            created_at_unix_seconds: profile.created_at_unix_seconds,
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_play_path(
    app: *mut PlayerApp,
    path: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let path = PathBuf::from(c_string(path)?);
        let tracks = app.store()?.tracks()?;
        let start_index = tracks
            .iter()
            .position(|track| track.path == path)
            .ok_or_else(|| {
                PlayerError::store(format!("track is not in library: {}", path.display()))
            })?;
        app.play_queue_tracks(tracks, start_index)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_play_queue(
    app: *mut PlayerApp,
    paths_json: *const c_char,
    start_path: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let paths: Vec<String> = serde_json::from_str(&c_string(paths_json)?)
            .map_err(|error| PlayerError::metadata(format!("invalid queue path list: {error}")))?;
        if paths.is_empty() {
            return Err(PlayerError::invalid_input("queue is empty"));
        }
        let start_path = PathBuf::from(c_string(start_path)?);
        let store = app.store()?;
        let mut tracks = Vec::with_capacity(paths.len());
        for path in paths {
            let path = PathBuf::from(path);
            let track = store.track_by_path(&path)?.ok_or_else(|| {
                PlayerError::store(format!("track is not in library: {}", path.display()))
            })?;
            tracks.push(track);
        }
        let start_index = tracks
            .iter()
            .position(|track| track.path == start_path)
            .ok_or_else(|| {
                PlayerError::store(format!(
                    "queue start track is not in queue: {}",
                    start_path.display()
                ))
            })?;
        app.play_queue_tracks(tracks, start_index)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_pause(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.playback_lifecycle.user_stopped_playback();
        if let Some(engine) = &app.engine {
            engine.pause()?;
            app.poll_events();
        } else {
            app.is_playing = false;
        }
        Ok(app.snapshot())
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_resume(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.ensure_playback_can_start()?;
        app.engine()?.play()?;
        app.poll_events();
        Ok(app.snapshot())
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_audio_interruption_began(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.poll_events();
        let action = app.playback_lifecycle.begin_interruption(app.is_playing);
        app.apply_playback_lifecycle_action(action)?;
        Ok(app.snapshot())
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_audio_interruption_ended(
    app: *mut PlayerApp,
    system_should_resume: bool,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.poll_events();
        let action = app
            .playback_lifecycle
            .end_interruption(system_should_resume, app.current_track.is_some());
        app.apply_playback_lifecycle_action(action)?;
        Ok(app.snapshot())
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_audio_output_disconnected(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.poll_events();
        let action = app.playback_lifecycle.output_disconnected(app.is_playing);
        app.apply_playback_lifecycle_action(action)?;
        Ok(app.snapshot())
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_stop(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.playback_lifecycle.user_stopped_playback();
        if let Some(engine) = app.engine.as_ref() {
            engine.pause()?;
        }
        if app.engine.is_some() {
            app.poll_events();
            app.finish_active_session("stopped").ok();
        }
        if let Some(engine) = app.engine.as_ref() {
            engine.load_queue(Vec::new(), 0)?;
            app.poll_events();
        }
        app.is_playing = false;
        app.position_ms = 0;
        app.current_track = None;
        app.queue_tracks.clear();
        app.queue_current_index = None;
        Ok(app.snapshot())
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_next(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.pending_session_end_reason = Some("next".to_owned());
        app.engine()?.next()?;
        app.poll_events();
        Ok(app.snapshot())
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_previous(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.pending_session_end_reason = Some("previous".to_owned());
        app.engine()?.previous()?;
        app.poll_events();
        Ok(app.snapshot())
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_seek(app: *mut PlayerApp, position_ms: u64) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.observe_active_position(app.position_ms);
        app.engine()?.seek_to(position_ms)?;
        app.position_ms = position_ms;
        if let Some(session) = &mut app.active_session {
            session.seek_count = session.seek_count.saturating_add(1);
            session.last_position_ms = position_ms;
        }
        app.poll_events();
        Ok(app.snapshot())
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_poll(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.poll_events();
        Ok(app.snapshot())
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_set_repeat_mode(
    app: *mut PlayerApp,
    repeat_mode: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let repeat_mode = parse_repeat_mode(&c_string(repeat_mode)?)?;
        app.repeat_mode = repeat_mode;
        if let Some(engine) = app.engine.as_ref() {
            engine.set_repeat_mode(repeat_mode)?;
            app.poll_events();
        }
        Ok(app.snapshot())
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_set_shuffle(app: *mut PlayerApp, enabled: bool) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.shuffle_enabled = enabled;
        if let Some(engine) = app.engine.as_ref() {
            engine.set_shuffle(enabled)?;
            app.poll_events();
        }
        Ok(app.snapshot())
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_queue(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.poll_events();
        Ok(PlaybackQueueDto {
            tracks: app.queue_tracks.clone(),
            current_index: app.queue_current_index,
            repeat_mode: repeat_mode_name(app.repeat_mode).to_owned(),
            shuffle_enabled: app.shuffle_enabled,
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_track_details(
    app: *mut PlayerApp,
    path: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let path = PathBuf::from(c_string(path)?);
        app.track_details(&path)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_edit_track_view(
    app: *mut PlayerApp,
    path: *const c_char,
    edit_json: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let path = PathBuf::from(c_string(path)?);
        let request: TrackViewEditRequest =
            serde_json::from_str(&c_string(edit_json)?).map_err(|error| {
                PlayerError::metadata(format!("invalid track view edit request: {error}"))
            })?;
        if request.title.trim().is_empty() {
            return Err(PlayerError::metadata("track title cannot be empty"));
        }

        let artwork_image = request
            .artwork_path
            .as_deref()
            .map(|path| read_artwork_image(Path::new(path)))
            .transpose()?;
        let lyrics_path = request.lyrics_path.as_ref().map(PathBuf::from);
        if let Some(lyrics_path) = &lyrics_path {
            if !lyrics_path.is_file() {
                return Err(PlayerError::metadata(format!(
                    "lyrics file not found: {}",
                    lyrics_path.display()
                )));
            }
        }

        let derived = app.create_derived_view_for_edit(&path, "view_edit")?;
        {
            let mut store = app.store()?;
            store.set_track_display_metadata(
                &derived.path,
                &request.title,
                request.artist.as_deref(),
                request.album.as_deref(),
            )?;
            store.set_track_view_name(&derived.path, request.view_name.as_deref())?;
            if let Some(notes) = request.notes.as_deref() {
                store.set_track_notes(&derived.path, notes)?;
            }
            if let Some(artwork_image) = artwork_image.as_ref() {
                store.set_track_artwork_reference(&derived.path, artwork_image)?;
            }
        }
        if let Some(lyrics_path) = lyrics_path {
            copy_track_lyrics_file(&derived.path, &lyrics_path)?;
        }

        let derived = app
            .store()?
            .track_by_path(&derived.path)?
            .ok_or_else(|| PlayerError::store("derived view edit disappeared"))?;
        app.track_to_dto_with_artwork(&derived)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_set_track_notes(
    app: *mut PlayerApp,
    path: *const c_char,
    notes: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let path = PathBuf::from(c_string(path)?);
        let notes = c_string(notes)?;
        let derived = app.create_derived_view_for_edit(&path, "notes")?;
        app.store()?.set_track_notes(&derived.path, &notes)?;
        let derived = app
            .store()?
            .track_by_path(&derived.path)?
            .ok_or_else(|| PlayerError::store("derived notes view disappeared"))?;
        app.track_to_dto_with_artwork(&derived)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_set_track_rating(
    app: *mut PlayerApp,
    path: *const c_char,
    rating: i32,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let path = PathBuf::from(c_string(path)?);
        let rating = match rating {
            0 => None,
            1..=10 => Some(rating as u8),
            _ => {
                return Err(PlayerError::store(
                    "rating must be 0 to clear or between 1 and 10",
                ));
            }
        };
        let updated = {
            let mut store = app.store()?;
            store.set_track_rating(&path, rating)?;
            store
                .track_by_path(&path)?
                .ok_or_else(|| PlayerError::store(format!("track not found: {}", path.display())))?
        };
        let dto = app.track_to_dto_with_artwork(&updated)?;
        app.replace_cached_track(dto.clone());
        Ok(dto)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_set_track_metadata(
    app: *mut PlayerApp,
    path: *const c_char,
    title: *const c_char,
    artist: *const c_char,
    album: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let path = PathBuf::from(c_string(path)?);
        let title = c_string(title)?;
        let artist = c_string(artist)?;
        let album = c_string(album)?;
        let derived = app.create_derived_view_for_edit(&path, "metadata")?;
        app.store()?.set_track_display_metadata(
            &derived.path,
            &title,
            Some(&artist),
            Some(&album),
        )?;
        let derived = app
            .store()?
            .track_by_path(&derived.path)?
            .ok_or_else(|| PlayerError::store("derived metadata view disappeared"))?;
        app.track_to_dto_with_artwork(&derived)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_set_track_artwork(
    app: *mut PlayerApp,
    path: *const c_char,
    image_path: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let path = PathBuf::from(c_string(path)?);
        let image_path = PathBuf::from(c_string(image_path)?);
        let image = read_artwork_image(&image_path)?;
        let derived = app.create_derived_view_for_edit(&path, "artwork")?;
        app.store()?
            .set_track_artwork_reference(&derived.path, &image)?;
        let derived = app
            .store()?
            .track_by_path(&derived.path)?
            .ok_or_else(|| PlayerError::store("derived artwork view disappeared"))?;
        app.track_to_dto_with_artwork(&derived)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_set_album_artwork(
    app: *mut PlayerApp,
    path: *const c_char,
    image_path: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let path = PathBuf::from(c_string(path)?);
        let image_path = PathBuf::from(c_string(image_path)?);
        let image = read_artwork_image(&image_path)?;
        let tracks_updated = app
            .store()?
            .set_album_artwork_reference_for_track(&path, &image)?;
        Ok(AlbumArtworkSummary { tracks_updated })
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_set_track_lyrics(
    app: *mut PlayerApp,
    path: *const c_char,
    lyrics_path: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let path = PathBuf::from(c_string(path)?);
        let lyrics_path = PathBuf::from(c_string(lyrics_path)?);
        let derived = app.create_derived_view_for_edit(&path, "lyrics")?;
        copy_track_lyrics_file(&derived.path, &lyrics_path)?;
        let derived = app
            .store()?
            .track_by_path(&derived.path)?
            .ok_or_else(|| PlayerError::store("derived lyrics view disappeared"))?;
        app.track_to_dto_with_artwork(&derived)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_export_track_view(
    app: *mut PlayerApp,
    path: *const c_char,
    destination: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let path = PathBuf::from(c_string(path)?);
        let destination = PathBuf::from(c_string(destination)?);
        let track = app.materialize_track_view(&path, &destination)?;
        app.track_to_dto_with_artwork(&track)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_set_favorite(
    app: *mut PlayerApp,
    path: *const c_char,
    enabled: bool,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let path = c_string(path)?;
        app.store()?.set_favorite(path, enabled)?;
        Ok(Empty {})
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_favorites(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let store = app.store()?;
        let tracks = store.favorite_tracks()?;
        track_dtos_with_artwork(&tracks, &store, &app.db_path)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_history(app: *mut PlayerApp, limit: usize) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let store = app.store()?;
        let tracks = store
            .play_history(limit.max(1))?
            .into_iter()
            .map(|entry| entry.track)
            .collect::<Vec<_>>();
        track_dtos_with_artwork(&tracks, &store, &app.db_path)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_playlists(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let store = app.store()?;
        store
            .playlists()?
            .into_iter()
            .map(|playlist| app.playlist_to_dto(&store, playlist))
            .collect::<PlayerResult<Vec<_>>>()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_create_playlist(
    app: *mut PlayerApp,
    name: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let name = c_string(name)?;
        app.store()?.create_playlist(&name)?;
        Ok(Empty {})
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_rename_playlist(
    app: *mut PlayerApp,
    old_name: *const c_char,
    new_name: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let old_name = c_string(old_name)?;
        let new_name = c_string(new_name)?;
        app.store()?.rename_playlist(&old_name, &new_name)?;
        Ok(Empty {})
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_set_playlist_artwork(
    app: *mut PlayerApp,
    name: *const c_char,
    image_path: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let name = c_string(name)?;
        let image_path = PathBuf::from(c_string(image_path)?);
        let image = read_artwork_image(&image_path)?;
        app.store()?.save_playlist_artwork(&name, &image)?;
        Ok(Empty {})
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_delete_playlist(
    app: *mut PlayerApp,
    name: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let name = c_string(name)?;
        app.store()?.delete_playlist(&name)?;
        Ok(Empty {})
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_clear_playlist(
    app: *mut PlayerApp,
    name: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let name = c_string(name)?;
        app.store()?.clear_playlist(&name)?;
        Ok(Empty {})
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_add_to_playlist(
    app: *mut PlayerApp,
    name: *const c_char,
    path: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let name = c_string(name)?;
        let path = c_string(path)?;
        app.store()?.add_playlist_track(&name, path)?;
        Ok(Empty {})
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_remove_from_playlist(
    app: *mut PlayerApp,
    name: *const c_char,
    path: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let name = c_string(name)?;
        let path = c_string(path)?;
        app.store()?.remove_playlist_track(&name, path)?;
        Ok(Empty {})
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_move_playlist_track(
    app: *mut PlayerApp,
    name: *const c_char,
    path: *const c_char,
    delta: i32,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let name = c_string(name)?;
        let path = c_string(path)?;
        app.store()?.move_playlist_track(&name, path, delta)?;
        Ok(Empty {})
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_sort_playlist(
    app: *mut PlayerApp,
    name: *const c_char,
    sort: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let name = c_string(name)?;
        let sort = PlaylistSort::parse(&c_string(sort)?)?;
        app.store()?.sort_playlist(&name, sort)?;
        Ok(Empty {})
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_playlist_tracks(
    app: *mut PlayerApp,
    name: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let name = c_string(name)?;
        let store = app.store()?;
        let tracks = store
            .playlist_tracks(&name)?
            .into_iter()
            .map(|entry| entry.track)
            .collect::<Vec<_>>();
        track_dtos_with_artwork(&tracks, &store, &app.db_path)
    })
}

impl UserActivityStore {
    fn for_db(db_path: &Path) -> Self {
        let root = db_path
            .parent()
            .map(|parent| parent.join("UserData"))
            .unwrap_or_else(|| PathBuf::from("UserData"));
        Self {
            profile_path: root.join("user.json"),
            history_path: root.join("play_history.jsonl"),
            root,
        }
    }

    fn load_or_create_profile(&self) -> PlayerResult<LocalUserProfile> {
        if self.profile_path.exists() {
            let bytes = fs::read(&self.profile_path)
                .map_err(|source| PlayerError::io(self.profile_path.clone(), source))?;
            return serde_json::from_slice(&bytes)
                .map_err(|error| PlayerError::store(error.to_string()));
        }

        fs::create_dir_all(&self.root).map_err(|source| PlayerError::io(&self.root, source))?;
        let now = now_unix_seconds();
        let profile = LocalUserProfile {
            schema_version: 1,
            user_id: new_local_user_id(),
            display_name: "Local User".to_owned(),
            sync_enabled: false,
            created_at_unix_seconds: now,
            updated_at_unix_seconds: now,
        };
        let json = serde_json::to_vec_pretty(&profile)
            .map_err(|error| PlayerError::store(error.to_string()))?;
        fs::write(&self.profile_path, json)
            .map_err(|source| PlayerError::io(&self.profile_path, source))?;
        Ok(profile)
    }

    fn append_playback(&self, record: &PlaybackHistoryRecord) -> PlayerResult<()> {
        fs::create_dir_all(&self.root).map_err(|source| PlayerError::io(&self.root, source))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.history_path)
            .map_err(|source| PlayerError::io(&self.history_path, source))?;
        serde_json::to_writer(&mut file, record)
            .map_err(|error| PlayerError::store(error.to_string()))?;
        file.write_all(b"\n")
            .map_err(|source| PlayerError::io(&self.history_path, source))?;
        Ok(())
    }
}

impl ActivePlaybackSession {
    fn observe_position(&mut self, position_ms: u64, is_playing: bool) {
        if is_playing && position_ms >= self.last_position_ms {
            self.listened_ms = self
                .listened_ms
                .saturating_add(position_ms - self.last_position_ms);
        }
        self.last_position_ms = position_ms;
    }

    fn into_record(
        self,
        user_id: &str,
        finish_reason: &str,
        ended_at_unix_seconds: i64,
    ) -> PlaybackHistoryRecord {
        let track_duration_ms = self.track.duration_ms;
        let completion_ratio = track_duration_ms
            .filter(|duration| *duration > 0)
            .map(|duration| {
                let progress = self.last_position_ms.max(self.listened_ms) as f32 / duration as f32;
                progress.min(1.0)
            });
        let completed = completion_ratio.map(|ratio| ratio >= 0.95).unwrap_or(false);

        PlaybackHistoryRecord {
            schema_version: 1,
            record_type: "playback_session".to_owned(),
            user_id: user_id.to_owned(),
            session_id: self.session_id,
            started_at_unix_seconds: self.started_at_unix_seconds,
            ended_at_unix_seconds,
            start_position_ms: self.start_position_ms,
            end_position_ms: self.last_position_ms,
            listened_ms: self.listened_ms,
            track_duration_ms,
            completion_ratio,
            completed,
            finish_reason: finish_reason.to_owned(),
            seek_count: self.seek_count,
            track: PlaybackTrackRecord {
                id: self.track.id,
                title: self.track.title,
                artist: self.track.artist,
                album: self.track.album,
                path: self.track.path,
                gain_db: self.track.gain_db,
                loudness_status: self.track.loudness_status,
            },
        }
    }
}

impl PlayerApp {
    fn export_library(&self, package_path: &Path) -> PlayerResult<LibraryPackageSummary> {
        let tracks = self.store()?.tracks()?;
        let package_music_root = package_path.join(LIBRARY_PACKAGE_MUSIC_DIRECTORY);
        fs::create_dir_all(&package_music_root)
            .map_err(|source| PlayerError::io(&package_music_root, source))?;

        let mut manifest_tracks = Vec::with_capacity(tracks.len());
        let mut sidecar_files = 0_usize;
        for (index, track) in tracks.iter().enumerate() {
            let audio_file = library_package_audio_path(index, &track.path);
            let destination = package_path.join(&audio_file);
            copy_into_media_library(&track.path, &destination)?;
            sidecar_files += copy_related_sidecars(&track.path, &destination)?;
            manifest_tracks.push(LibraryPackageTrack {
                database_path: path_to_string_lossy(&track.path),
                audio_file: path_to_string_lossy(&audio_file),
            });
        }

        let package_database = package_path.join(LIBRARY_PACKAGE_DATABASE_FILE);
        fs::copy(&self.db_path, &package_database)
            .map_err(|source| PlayerError::io(&package_database, source))?;
        let manifest = LibraryPackageManifest {
            format_version: LIBRARY_PACKAGE_FORMAT_VERSION,
            database_file: LIBRARY_PACKAGE_DATABASE_FILE.to_owned(),
            tracks: manifest_tracks,
        };
        let manifest_data = serde_json::to_vec_pretty(&manifest)
            .map_err(|error| PlayerError::store(error.to_string()))?;
        let manifest_path = package_path.join(LIBRARY_PACKAGE_MANIFEST_FILE);
        fs::write(&manifest_path, manifest_data)
            .map_err(|source| PlayerError::io(&manifest_path, source))?;

        Ok(LibraryPackageSummary {
            tracks: tracks.len(),
            audio_files: tracks.len(),
            sidecar_files,
        })
    }

    fn import_library(&mut self, package_path: &Path) -> PlayerResult<LibraryPackageSummary> {
        let manifest_path = package_path.join(LIBRARY_PACKAGE_MANIFEST_FILE);
        let manifest_data =
            fs::read(&manifest_path).map_err(|source| PlayerError::io(&manifest_path, source))?;
        let manifest: LibraryPackageManifest = serde_json::from_slice(&manifest_data)
            .map_err(|error| PlayerError::store(error.to_string()))?;
        if manifest.format_version != LIBRARY_PACKAGE_FORMAT_VERSION {
            return Err(PlayerError::store(format!(
                "unsupported library package version: {}",
                manifest.format_version
            )));
        }
        if manifest.database_file != LIBRARY_PACKAGE_DATABASE_FILE {
            return Err(PlayerError::store(format!(
                "library package database must be `{LIBRARY_PACKAGE_DATABASE_FILE}`"
            )));
        }

        let package_root = package_path
            .canonicalize()
            .map_err(|source| PlayerError::io(package_path, source))?;
        let package_database =
            validated_package_file(&package_root, &manifest.database_file, "database")?;
        let database_track_paths = LibraryStore::open(&package_database)?
            .tracks()?
            .into_iter()
            .map(|track| track.path)
            .collect::<HashSet<_>>();
        let manifest_database_paths = manifest
            .tracks
            .iter()
            .map(|track| PathBuf::from(&track.database_path))
            .collect::<HashSet<_>>();
        if manifest_database_paths.len() != manifest.tracks.len()
            || manifest_database_paths != database_track_paths
        {
            return Err(PlayerError::store(
                "library package manifest tracks do not match its database",
            ));
        }
        let mut validated_audio_files = Vec::with_capacity(manifest.tracks.len());
        let mut unique_audio_files = HashSet::with_capacity(manifest.tracks.len());
        for track in &manifest.tracks {
            let audio_file = Path::new(&track.audio_file);
            if audio_file
                .strip_prefix(LIBRARY_PACKAGE_MUSIC_DIRECTORY)
                .ok()
                .is_none_or(|relative| relative.as_os_str().is_empty())
            {
                return Err(PlayerError::store(format!(
                    "library package audio path must be below `{LIBRARY_PACKAGE_MUSIC_DIRECTORY}`: {}",
                    track.audio_file
                )));
            }
            if !unique_audio_files.insert(audio_file.to_path_buf()) {
                return Err(PlayerError::store(format!(
                    "library package contains duplicate audio path: {}",
                    track.audio_file
                )));
            }
            validated_audio_files.push(validated_package_file(
                &package_root,
                &track.audio_file,
                "audio",
            )?);
        }

        self.reset_library_runtime_state();
        if let Some(parent) = self.db_path.parent() {
            fs::create_dir_all(parent).map_err(|source| PlayerError::io(parent, source))?;
        }
        fs::copy(&package_database, &self.db_path)
            .map_err(|source| PlayerError::io(&self.db_path, source))?;
        fs::create_dir_all(&self.media_root)
            .map_err(|source| PlayerError::io(&self.media_root, source))?;

        let mut replacements = Vec::with_capacity(manifest.tracks.len());
        let mut sidecar_files = 0_usize;
        for (track, source) in manifest.tracks.iter().zip(validated_audio_files) {
            let audio_file = PathBuf::from(&track.audio_file);
            let relative_audio_path = audio_file
                .strip_prefix(LIBRARY_PACKAGE_MUSIC_DIRECTORY)
                .map_err(|_| PlayerError::store("validated package audio path lost its prefix"))?;
            let destination = self.media_root.join(relative_audio_path);
            copy_into_media_library(&source, &destination)?;
            sidecar_files += copy_related_sidecars(&source, &destination)?;
            replacements.push((PathBuf::from(&track.database_path), destination));
        }

        self.store()?.replace_track_paths(&replacements)?;
        Ok(LibraryPackageSummary {
            tracks: manifest.tracks.len(),
            audio_files: manifest.tracks.len(),
            sidecar_files,
        })
    }

    fn zero_out_library(&mut self) -> PlayerResult<Empty> {
        self.finish_active_session("library_zero_out")?;
        self.reset_library_runtime_state();
        self.store()?.zero_out()?;
        if self.media_root.exists() {
            fs::remove_dir_all(&self.media_root)
                .map_err(|source| PlayerError::io(&self.media_root, source))?;
        }
        fs::create_dir_all(&self.media_root)
            .map_err(|source| PlayerError::io(&self.media_root, source))?;
        Ok(Empty {})
    }

    fn reset_library_runtime_state(&mut self) {
        self.engine = None;
        self.active_session = None;
        self.pending_session_end_reason = None;
        self.current_track = None;
        self.queue_tracks.clear();
        self.queue_current_index = None;
        self.is_playing = false;
        self.position_ms = 0;
        self.gain_db = None;
        self.loudness_status = None;
        self.last_error = None;
        self.playback_lifecycle = PlaybackLifecycle::default();
    }

    fn local_user(&mut self) -> PlayerResult<&LocalUserProfile> {
        if self.local_user.is_none() {
            self.local_user = Some(self.activity_store.load_or_create_profile()?);
        }
        Ok(self
            .local_user
            .as_ref()
            .expect("local user just initialized"))
    }

    fn start_active_session(&mut self, track: TrackDto, position_ms: u64) {
        self.active_session = Some(ActivePlaybackSession {
            session_id: new_session_id(),
            track,
            started_at_unix_seconds: now_unix_seconds(),
            start_position_ms: position_ms,
            last_position_ms: position_ms,
            listened_ms: 0,
            seek_count: 0,
        });
    }

    fn observe_active_position(&mut self, position_ms: u64) {
        if let Some(session) = &mut self.active_session {
            session.observe_position(position_ms, self.is_playing);
        }
    }

    fn finish_active_session(&mut self, finish_reason: &str) -> PlayerResult<()> {
        self.observe_active_position(self.position_ms);
        let Some(session) = self.active_session.take() else {
            return Ok(());
        };
        let user = self.local_user()?.clone();
        let record = session.into_record(&user.user_id, finish_reason, now_unix_seconds());
        self.activity_store.append_playback(&record)?;
        self.store()?
            .record_playback(
                Path::new(&record.track.path),
                record.end_position_ms,
                record.completed,
            )
            .ok();
        Ok(())
    }

    fn ensure_audio_hash_for_path(&self, path: &Path) -> PlayerResult<String> {
        let store = self.store()?;
        if let Some(track) = store.track_by_path(path)? {
            if let Some(audio_hash) = track.audio_hash.filter(|hash| !hash.trim().is_empty()) {
                return Ok(audio_hash);
            }
        }
        drop(store);

        let fingerprint = audio_hash(path)?;
        let file_hash = file_hash(path).ok();
        let metadata = fs::metadata(path)
            .ok()
            .map(|metadata| fingerprint_from_metadata(&metadata));
        self.store()?.update_track_hashes(
            path,
            file_hash.as_deref(),
            Some(&fingerprint.hash),
            metadata,
        )?;
        Ok(fingerprint.hash)
    }

    fn create_derived_view_for_edit(
        &self,
        path: &Path,
        transform_kind: &str,
    ) -> PlayerResult<Track> {
        self.ensure_audio_hash_for_path(path)?;
        let source = self
            .store()?
            .track_by_path(path)?
            .ok_or_else(|| PlayerError::store(format!("track not found: {}", path.display())))?;
        let primary_view_id = source.primary_view_id.value().to_owned();
        let created_at = now_unix_nanos();
        let view_id = derived_view_id(&primary_view_id, transform_kind, created_at);
        let destination =
            derived_view_audio_path(&self.media_root, &source.path, &primary_view_id, &view_id);
        copy_into_media_library(&source.path, &destination)?;
        copy_related_sidecars(&source.path, &destination)?;
        let transform_spec =
            format!(r#"{{"kind":"{transform_kind}","created_at_unix_nanos":{created_at}}}"#);
        self.store()?
            .create_derived_view(&source.path, &destination, &view_id, &transform_spec)
    }

    fn materialize_track_view(&self, path: &Path, destination: &Path) -> PlayerResult<Track> {
        let mut store = self.store()?;
        let source = store
            .track_by_path(path)?
            .ok_or_else(|| PlayerError::store(format!("track not found: {}", path.display())))?;
        if store.track_by_path(destination)?.is_some() {
            return Err(PlayerError::store(format!(
                "destination is already in the library: {}",
                destination.display()
            )));
        }

        copy_into_media_library(&source.path, destination)?;
        copy_related_sidecars(&source.path, destination)?;

        let audio_fingerprint = audio_hash(destination)?;
        let materialized_file_hash = file_hash(destination)?;
        let fingerprint = fs::metadata(destination)
            .ok()
            .map(|metadata| fingerprint_from_metadata(&metadata));
        let created_at = now_unix_nanos();
        let view_id = materialized_primary_view_id(&audio_fingerprint.hash, created_at);
        let mut materialized = source.clone();
        materialized.id = TrackId::from_path(destination);
        materialized.path = destination.to_path_buf();
        materialized.view_id = TrackViewId::from_value(view_id.clone());
        materialized.primary_view_id = TrackViewId::from_value(view_id);
        materialized.view_kind = TrackViewKind::Primary;
        materialized.transform_spec = None;
        materialized.format_name = destination
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase());
        materialized.file_hash = Some(materialized_file_hash);
        materialized.audio_hash = Some(audio_fingerprint.hash);
        materialized.fingerprint = fingerprint;

        store.upsert_track(&materialized)?;
        if let Some(notes) = store.track_notes(&source.path)? {
            store.set_track_notes(destination, &notes)?;
        }
        store.copy_artwork_references(&source.path, destination)?;
        let artwork = store.artwork_for_path(&source.path)?;
        if !artwork.is_empty() {
            store.save_artwork(destination, &artwork)?;
        }
        store.track_by_path(destination)?.ok_or_else(|| {
            PlayerError::store(format!(
                "materialized view not found after export: {}",
                destination.display()
            ))
        })
    }

    fn play_queue_tracks(
        &mut self,
        tracks: Vec<Track>,
        start_index: usize,
    ) -> PlayerResult<PlaybackSnapshot> {
        if tracks.is_empty() {
            return Err(PlayerError::invalid_input("queue is empty"));
        }
        if start_index >= tracks.len() {
            return Err(PlayerError::invalid_input(format!(
                "invalid queue index {start_index} for queue length {}",
                tracks.len()
            )));
        }

        self.poll_events();
        self.ensure_playback_can_start()?;
        self.finish_active_session("played_other_track").ok();

        let store = self.store()?;
        let queue_tracks = track_dtos_with_artwork(&tracks, &store, &self.db_path)?;
        let repeat_mode = self.repeat_mode;
        let shuffle_enabled = self.shuffle_enabled;
        {
            let engine = self.engine()?;
            engine.play_queue(tracks, start_index, repeat_mode, shuffle_enabled)?;
        }
        self.queue_tracks = queue_tracks;
        self.last_error = None;
        self.poll_events();
        Ok(self.snapshot())
    }

    fn replace_cached_track(&mut self, updated: TrackDto) {
        if self
            .current_track
            .as_ref()
            .is_some_and(|track| track.path == updated.path)
        {
            self.current_track = Some(updated.clone());
        }

        for track in &mut self.queue_tracks {
            if track.path == updated.path {
                *track = updated.clone();
            }
        }
    }

    fn track_to_dto_with_artwork(&self, track: &Track) -> PlayerResult<TrackDto> {
        let store = self.store()?;
        track_to_dto_with_artwork(track, &store, &self.db_path)
    }

    fn store(&self) -> PlayerResult<LibraryStore> {
        LibraryStore::open(&self.db_path)
    }

    fn engine(&mut self) -> PlayerResult<&PlayerEngine> {
        if self.engine.is_none() {
            self.engine = Some(PlayerEngine::spawn(
                NormalizationSettings::default(),
                RodioBackend::open_default,
            )?);
        }
        Ok(self.engine.as_ref().expect("engine just initialized"))
    }

    fn ensure_playback_can_start(&mut self) -> PlayerResult<()> {
        if self.playback_lifecycle.request_playback_start() {
            Ok(())
        } else {
            Err(PlayerError::audio(
                "playback cannot start while an audio interruption is active",
            ))
        }
    }

    fn apply_playback_lifecycle_action(
        &mut self,
        action: PlaybackLifecycleAction,
    ) -> PlayerResult<()> {
        match action {
            PlaybackLifecycleAction::None => {}
            PlaybackLifecycleAction::Pause => {
                if let Some(engine) = &self.engine {
                    engine.pause()?;
                    self.poll_events();
                }
                self.is_playing = false;
            }
            PlaybackLifecycleAction::Resume => {
                if self.current_track.is_some() {
                    self.engine()?.play()?;
                    self.poll_events();
                }
            }
        }
        Ok(())
    }

    fn poll_events(&mut self) {
        let Some(engine) = &self.engine else {
            return;
        };

        let mut events = Vec::new();
        while let Some(event) = engine.try_recv_event() {
            events.push(event);
        }
        for event in events {
            self.apply_event(event);
        }
    }

    fn apply_event(&mut self, event: PlaybackEvent) {
        match event {
            PlaybackEvent::StateChanged(state) => {
                self.observe_active_position(state.position_ms);
                self.is_playing = state.is_playing;
                self.position_ms = state.position_ms;
                self.repeat_mode = state.repeat_mode;
                self.shuffle_enabled = state.shuffle;
            }
            PlaybackEvent::TrackChanged(track) => {
                let next_track = match track
                    .as_deref()
                    .map(|track| self.track_to_dto_with_artwork(track))
                    .transpose()
                {
                    Ok(track) => track,
                    Err(error) => {
                        self.last_error = Some(error.to_string());
                        None
                    }
                };
                let old_path = self.current_track.as_ref().map(|track| track.path.as_str());
                let next_path = next_track.as_ref().map(|track| track.path.as_str());
                if old_path != next_path {
                    let reason = self
                        .pending_session_end_reason
                        .take()
                        .unwrap_or_else(|| "track_changed".to_owned());
                    self.finish_active_session(&reason).ok();
                    if self.is_playing {
                        if let Some(track) = next_track.clone() {
                            self.start_active_session(track, self.position_ms);
                        }
                    }
                } else {
                    self.pending_session_end_reason = None;
                    if self.is_playing && self.active_session.is_none() {
                        if let Some(track) = next_track.clone() {
                            self.start_active_session(track, self.position_ms);
                        }
                    }
                }
                self.queue_current_index = next_track.as_ref().and_then(|next_track| {
                    self.queue_tracks
                        .iter()
                        .position(|track| track.path == next_track.path)
                });
                self.current_track = next_track;
            }
            PlaybackEvent::GainChanged(gain) => {
                self.gain_db = gain.as_ref().map(|gain| gain.gain_db);
                self.loudness_status = gain.map(|gain| format!("{:?}", gain.status));
            }
            PlaybackEvent::PositionChanged(position_ms) => {
                self.observe_active_position(position_ms);
                self.position_ms = position_ms;
            }
            PlaybackEvent::Error(error) => {
                self.finish_active_session("error").ok();
                self.last_error = Some(error);
                self.is_playing = false;
            }
            PlaybackEvent::Stopped => {
                let reason = self
                    .pending_session_end_reason
                    .take()
                    .unwrap_or_else(|| "stopped".to_owned());
                self.finish_active_session(&reason).ok();
                self.is_playing = false;
            }
        }
    }

    fn snapshot(&self) -> PlaybackSnapshot {
        PlaybackSnapshot {
            is_playing: self.is_playing,
            position_ms: self.position_ms,
            current_track: self.current_track.clone(),
            queue_len: self.queue_tracks.len(),
            queue_position: self.queue_current_index,
            repeat_mode: repeat_mode_name(self.repeat_mode).to_owned(),
            shuffle_enabled: self.shuffle_enabled,
            gain_db: self.gain_db,
            loudness_status: self.loudness_status.clone(),
            error: self.last_error.clone(),
            interruption_active: self.playback_lifecycle.interruption_active(),
            resume_after_interruption: self.playback_lifecycle.resume_after_interruption(),
        }
    }

    fn track_details(&self, path: &Path) -> PlayerResult<TrackDetailsDto> {
        let store = self.store()?;
        let artwork = resolved_artwork_path(&store, &self.db_path, path)?;
        let lyrics = sidecar_lyrics(path)?;
        let notes = store.track_notes(path)?;
        let metadata = store
            .track_metadata(path)?
            .ok_or_else(|| PlayerError::store(format!("track not found: {}", path.display())))?;
        let audio_hash = required_audio_hash(metadata.audio_hash, path)?;

        Ok(TrackDetailsDto {
            view_id: metadata.view_id.clone(),
            primary_view_id: metadata.primary_view_id.clone(),
            is_primary_view: metadata.view_id == metadata.primary_view_id,
            view_kind: metadata.view_kind,
            view_name: metadata.view_name,
            rating: metadata.user_rating,
            transform_spec: metadata.transform_spec,
            quality_profile: metadata.quality_profile,
            format_name: metadata.format_name,
            artwork_path: artwork.as_ref().map(|(path, _)| path_to_string_lossy(path)),
            artwork_source: artwork.map(|(_, source)| source.to_owned()),
            lyrics_path: lyrics.as_ref().map(|(path, _)| path_to_string_lossy(path)),
            lyrics_text: lyrics.map(|(_, text)| text),
            notes,
            audio_hash,
            original_title: metadata.original_title,
            original_artist: metadata.original_artist,
            original_album: metadata.original_album,
            display_title: metadata.display_title,
            display_artist: metadata.display_artist,
            display_album: metadata.display_album,
        })
    }

    fn playlist_to_dto(
        &self,
        store: &LibraryStore,
        playlist: PlaylistSummary,
    ) -> PlayerResult<PlaylistDto> {
        let (artwork_path, artwork_source) =
            playlist_artwork_path(store, &self.db_path, playlist.id, &playlist.name)?
                .map(|(path, source)| (Some(path_to_string_lossy(path)), Some(source.to_owned())))
                .unwrap_or((None, None));

        Ok(PlaylistDto {
            id: playlist.id,
            name: playlist.name,
            track_count: playlist.track_count,
            artwork_path,
            artwork_source,
        })
    }

    fn audit_database(&self) -> PlayerResult<AuditSummary> {
        let mut store = self.store()?;
        let mut tracks = store.tracks()?;
        let tracks_scanned = tracks.len();
        let mut hashes_updated = 0_usize;
        let mut failures = 0_usize;

        for track in &mut tracks {
            let mut changed = false;
            if track.file_hash.is_none() {
                match file_hash(&track.path) {
                    Ok(hash) => {
                        track.file_hash = Some(hash);
                        changed = true;
                    }
                    Err(_) => failures += 1,
                }
            }
            match audio_hash(&track.path) {
                Ok(fingerprint) => {
                    if track.audio_hash.as_deref() != Some(fingerprint.hash.as_str()) {
                        track.set_primary_audio_hash(fingerprint.hash);
                        changed = true;
                    }
                }
                Err(_) => failures += 1,
            }
            if changed {
                let fingerprint = fs::metadata(&track.path)
                    .ok()
                    .map(|metadata| fingerprint_from_metadata(&metadata));
                store.update_track_hashes(
                    &track.path,
                    track.file_hash.as_deref(),
                    track.audio_hash.as_deref(),
                    fingerprint,
                )?;
                hashes_updated += 1;
            }
        }

        let mut groups: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
        for track in tracks {
            if let Some(audio_hash) = track.audio_hash {
                groups
                    .entry(format!("audio:{audio_hash}"))
                    .or_default()
                    .push(track.path);
            }
        }

        let mut duplicate_groups = 0_usize;
        let mut tracks_merged = 0_usize;
        for mut paths in groups.into_values().filter(|paths| paths.len() > 1) {
            duplicate_groups += 1;
            paths.sort();
            let canonical = paths[0].clone();
            for duplicate in paths.into_iter().skip(1) {
                if store.merge_duplicate_track(&canonical, &duplicate)? {
                    tracks_merged += 1;
                }
            }
        }

        Ok(AuditSummary {
            tracks_scanned,
            hashes_updated,
            duplicate_groups,
            tracks_merged,
            failures,
        })
    }
}

fn ffi_result<T, F>(operation: F) -> *mut c_char
where
    T: Serialize,
    F: FnOnce() -> PlayerResult<T>,
{
    let response = match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(data)) => Response {
            ok: true,
            data: Some(data),
            error: None,
        },
        Ok(Err(error)) => Response::<T> {
            ok: false,
            data: None,
            error: Some(error.to_string()),
        },
        Err(_) => Response::<T> {
            ok: false,
            data: None,
            error: Some("panic across FFI boundary".to_owned()),
        },
    };
    json_to_c_string(&response)
}

fn json_to_c_string<T: Serialize>(value: &T) -> *mut c_char {
    let json = serde_json::to_string(value).unwrap_or_else(|error| {
        format!(
            r#"{{"ok":false,"data":null,"error":"serialization failed: {}"}}"#,
            error
        )
    });
    CString::new(json)
        .unwrap_or_else(|_| {
            CString::new(r#"{"ok":false,"data":null,"error":"invalid json string"}"#).unwrap()
        })
        .into_raw()
}

unsafe fn app_mut<'a>(app: *mut PlayerApp) -> PlayerResult<&'a mut PlayerApp> {
    app.as_mut()
        .ok_or_else(|| PlayerError::engine("PlayerApp handle is null"))
}

unsafe fn c_string(value: *const c_char) -> PlayerResult<String> {
    if value.is_null() {
        return Err(PlayerError::engine("string pointer is null"));
    }
    CStr::from_ptr(value)
        .to_str()
        .map(ToOwned::to_owned)
        .map_err(|error| PlayerError::engine(error.to_string()))
}

fn validated_package_file(
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

fn library_package_audio_path(index: usize, source_path: &Path) -> PathBuf {
    let file_name = source_path
        .file_name()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| std::ffi::OsStr::new("track.audio"));
    PathBuf::from(LIBRARY_PACKAGE_MUSIC_DIRECTORY)
        .join(format!("{index:08}"))
        .join(file_name)
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

fn derived_view_id(
    primary_view_id: &str,
    transform_kind: &str,
    created_at_unix_nanos: u128,
) -> String {
    format!("{primary_view_id}:view:{transform_kind}:{created_at_unix_nanos:x}")
}

fn materialized_primary_view_id(audio_hash: &str, created_at_unix_nanos: u128) -> String {
    format!(
        "audio:{}:materialized:{created_at_unix_nanos:x}",
        audio_hash.trim()
    )
}

fn parse_repeat_mode(value: &str) -> PlayerResult<RepeatMode> {
    match value {
        "off" => Ok(RepeatMode::Off),
        "one" => Ok(RepeatMode::One),
        "all" => Ok(RepeatMode::All),
        other => Err(PlayerError::metadata(format!(
            "unknown repeat mode: {other}"
        ))),
    }
}

fn repeat_mode_name(repeat_mode: RepeatMode) -> &'static str {
    match repeat_mode {
        RepeatMode::Off => "off",
        RepeatMode::One => "one",
        RepeatMode::All => "all",
    }
}

fn derived_view_audio_path(
    media_root: &Path,
    source_path: &Path,
    primary_view_id: &str,
    view_id: &str,
) -> PathBuf {
    let extension = source_path
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("audio")
        .to_ascii_lowercase();
    media_root
        .join("Views")
        .join(cache_key_for_view_id(primary_view_id))
        .join(format!("{}.{}", cache_key_for_view_id(view_id), extension))
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

fn cached_artwork_path(
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

fn resolved_artwork_path(
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
    if let Some(path) = sidecar_artwork_path(track_path) {
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

fn playlist_artwork_path(
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

fn cached_artwork_asset_path(
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

fn cached_playlist_artwork_path(
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

fn cached_file_needs_write(path: &Path, data: &[u8]) -> bool {
    match fs::read(path) {
        Ok(existing) => existing != data,
        Err(_) => true,
    }
}

fn artwork_cache_root(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .map(|parent| parent.join("Artwork"))
        .unwrap_or_else(|| PathBuf::from("Artwork"))
}

fn read_artwork_image(path: &Path) -> PlayerResult<ArtworkImage> {
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

fn image_mime_type(path: &Path, data: &[u8]) -> Option<String> {
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

fn artwork_extension(image: &ArtworkImage) -> &'static str {
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

fn sidecar_artwork_path(track_path: &Path) -> Option<PathBuf> {
    let dir = track_path.parent()?;
    let stem = track_path.file_stem()?.to_str()?;
    let stems = std::iter::once(stem)
        .chain(ALBUM_ARTWORK_STEMS.iter().copied())
        .collect::<Vec<_>>();
    find_sidecar_file(dir, &stems, ARTWORK_EXTENSIONS)
}

fn sidecar_lyrics(track_path: &Path) -> PlayerResult<Option<(PathBuf, String)>> {
    let Some(dir) = track_path.parent() else {
        return Ok(None);
    };
    let Some(stem) = track_path.file_stem().and_then(|value| value.to_str()) else {
        return Ok(None);
    };
    let Some(path) = find_sidecar_file(dir, &[stem], LYRICS_EXTENSIONS) else {
        return Ok(None);
    };
    let bytes = fs::read(&path).map_err(|source| PlayerError::io(path.clone(), source))?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Ok(Some((path, text)))
}

fn copy_track_lyrics_file(track_path: &Path, lyrics_path: &Path) -> PlayerResult<PathBuf> {
    let Some(dir) = track_path.parent() else {
        return Err(PlayerError::metadata(format!(
            "track has no parent directory: {}",
            track_path.display()
        )));
    };
    let stem = track_path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| PlayerError::metadata("track has no file stem"))?;
    let extension = lyrics_path
        .extension()
        .and_then(|value| value.to_str())
        .filter(|extension| {
            LYRICS_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
        .unwrap_or("lrc")
        .to_ascii_lowercase();
    let destination = dir.join(format!("{stem}.{extension}"));
    let source_canonical = lyrics_path.canonicalize().ok();
    let destination_canonical = destination.canonicalize().ok();

    for old_extension in LYRICS_EXTENSIONS {
        let candidate = dir.join(format!("{stem}.{old_extension}"));
        if candidate == destination {
            continue;
        }
        if source_canonical.is_some() && candidate.canonicalize().ok() == source_canonical {
            continue;
        }
        fs::remove_file(candidate).ok();
    }

    if source_canonical.is_some() && source_canonical == destination_canonical {
        return Ok(destination);
    }
    fs::copy(lyrics_path, &destination)
        .map_err(|source| PlayerError::io(destination.clone(), source))?;
    Ok(destination)
}

fn find_sidecar_file(dir: &Path, stems: &[&str], extensions: &[&str]) -> Option<PathBuf> {
    let mut lower_names = Vec::new();
    for stem in stems {
        for extension in extensions {
            let file_name = format!("{stem}.{extension}");
            let exact = dir.join(&file_name);
            if exact.is_file() {
                return Some(exact);
            }
            lower_names.push(file_name.to_ascii_lowercase());
        }
    }

    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if lower_names.iter().any(|candidate| candidate == &file_name) {
            let path = entry.path();
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

fn path_to_string_lossy(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().into_owned()
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn now_unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

fn new_local_user_id() -> String {
    format!("local-{:x}-{:x}", now_unix_nanos(), std::process::id())
}

fn new_session_id() -> String {
    format!("session-{:x}", now_unix_nanos())
}

#[cfg(test)]
fn track_dtos(tracks: &[Track]) -> PlayerResult<Vec<TrackDto>> {
    tracks.iter().map(track_to_dto).collect()
}

fn track_to_dto(track: &Track) -> PlayerResult<TrackDto> {
    let gain = gain_for_track(track, NormalizationSettings::default());
    let view_id = track_view_id(track)?;
    Ok(TrackDto {
        id: view_id.to_owned(),
        view_id: view_id.to_owned(),
        primary_view_id: track.primary_view_id.value().to_owned(),
        is_primary_view: track.view_id == track.primary_view_id,
        view_kind: track.view_kind.as_str().to_owned(),
        view_name: track.view_name.clone(),
        rating: track.user_rating,
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        duration_ms: track.duration_ms,
        artwork_count: track.artwork_count,
        artwork_path: None,
        artwork_source: None,
        has_album_identity: track_has_album_identity(track),
        path: track.path.to_string_lossy().into_owned(),
        quality_profile: track.quality_profile.clone(),
        format_name: track.format_name.clone(),
        gain_db: if gain.status == LoudnessStatus::Ready {
            Some(gain.gain_db)
        } else {
            None
        },
        loudness_status: format!("{:?}", gain.status),
    })
}

fn track_dtos_with_artwork(
    tracks: &[Track],
    store: &LibraryStore,
    db_path: &Path,
) -> PlayerResult<Vec<TrackDto>> {
    tracks
        .iter()
        .map(|track| track_to_dto_with_artwork(track, store, db_path))
        .collect()
}

fn track_to_dto_with_artwork(
    track: &Track,
    store: &LibraryStore,
    db_path: &Path,
) -> PlayerResult<TrackDto> {
    let mut dto = track_to_dto(track)?;
    if let Some((path, source)) = resolved_artwork_path(store, db_path, &track.path)? {
        dto.artwork_path = Some(path_to_string_lossy(&path));
        dto.artwork_source = Some(source.to_owned());
    }
    Ok(dto)
}

fn track_has_album_identity(track: &Track) -> bool {
    track
        .album
        .as_deref()
        .is_some_and(|album| !album.trim().is_empty())
}

fn track_view_id(track: &Track) -> PlayerResult<&str> {
    let view_id = track.view_id.value();
    if view_id.trim().is_empty() {
        return Err(PlayerError::store(format!(
            "track is missing view id: {}",
            track.path.display()
        )));
    }
    Ok(view_id)
}

fn required_audio_hash(audio_hash: Option<String>, path: &Path) -> PlayerResult<String> {
    audio_hash
        .filter(|hash| !hash.trim().is_empty())
        .ok_or_else(|| {
            PlayerError::store(format!("track is missing audio hash: {}", path.display()))
        })
}

fn cache_key_for_view_id(view_id: &str) -> String {
    view_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

const LYRICS_EXTENSIONS: &[&str] = &["lrc", "txt", "lyrics"];
const ARTWORK_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif"];
const ALBUM_ARTWORK_STEMS: &[&str] = &["cover", "folder", "front", "album"];

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn app_import_search_collections_and_history_roundtrip() {
        let db_path = temp_db_path("roundtrip");
        let media_root = temp_dir("media");
        let audio_root = workspace_root().join("test-assets").join("audio");
        let source_fixture = audio_root.join("into_the_oceans_chorus.ogg");
        let app = create_app(&db_path, &media_root);

        let import = unsafe {
            call_json(player_app_import_folder(
                app,
                c_string_arg(&audio_root).as_ptr(),
            ))
        };
        assert_ok(&import);
        assert_eq!(import["data"]["imported"], 3);
        assert_eq!(import["data"]["copied"], 3);
        assert!(source_fixture.exists());

        let library = unsafe { call_json(player_app_library(app)) };
        assert_ok(&library);
        assert_eq!(library["data"].as_array().unwrap().len(), 3);
        for track in library["data"].as_array().unwrap() {
            let path = track["path"].as_str().unwrap();
            assert!(Path::new(path).starts_with(&media_root), "{path}");
            assert!(Path::new(path).exists(), "{path}");
        }

        let search =
            unsafe { call_json(player_app_search(app, c_string_arg("oceans").as_ptr(), 25)) };
        assert_ok(&search);
        assert!(search["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|track| track["path"].as_str().unwrap().contains("into_the_oceans")));
        let managed_fixture = search["data"][0]["path"].as_str().unwrap().to_owned();

        let favorite = unsafe {
            call_json(player_app_set_favorite(
                app,
                c_string_arg(&managed_fixture).as_ptr(),
                true,
            ))
        };
        assert_ok(&favorite);
        let favorites = unsafe { call_json(player_app_favorites(app)) };
        assert_ok(&favorites);
        assert_eq!(favorites["data"].as_array().unwrap().len(), 1);

        let playlist_name = c_string_arg("Mix");
        let playlist =
            unsafe { call_json(player_app_create_playlist(app, playlist_name.as_ptr())) };
        assert_ok(&playlist);
        let add = unsafe {
            call_json(player_app_add_to_playlist(
                app,
                playlist_name.as_ptr(),
                c_string_arg(&managed_fixture).as_ptr(),
            ))
        };
        assert_ok(&add);
        let playlists = unsafe { call_json(player_app_playlists(app)) };
        assert_ok(&playlists);
        assert_eq!(playlists["data"][0]["name"], "Mix");
        assert_eq!(playlists["data"][0]["track_count"], 1);
        let playlist_tracks =
            unsafe { call_json(player_app_playlist_tracks(app, playlist_name.as_ptr())) };
        assert_ok(&playlist_tracks);
        assert_eq!(playlist_tracks["data"].as_array().unwrap().len(), 1);

        LibraryStore::open(&db_path)
            .unwrap()
            .record_playback(&managed_fixture, 123, true)
            .unwrap();
        let history = unsafe { call_json(player_app_history(app, 10)) };
        assert_ok(&history);
        assert_eq!(history["data"].as_array().unwrap().len(), 1);

        unsafe { player_app_destroy(app) };
        fs::remove_file(db_path).ok();
        fs::remove_dir_all(media_root).ok();
    }

    #[test]
    fn app_exports_zeroes_and_imports_a_complete_library_package() {
        let source_root = temp_dir("library_package_source");
        let source_db = source_root.join("player_library.sqlite3");
        let source_media = source_root.join("Music");
        let source_audio = source_media.join("Portable Album").join("song.wav");
        fs::create_dir_all(source_audio.parent().unwrap()).unwrap();
        write_test_wav(&source_audio, b"Portable Song").unwrap();
        fs::write(
            source_audio.with_extension("lrc"),
            b"[00:00]Portable lyrics",
        )
        .unwrap();
        fs::write(source_audio.parent().unwrap().join("cover.jpg"), b"cover").unwrap();

        let image = ArtworkImage {
            picture_index: 0,
            mime_type: Some("image/png".to_owned()),
            picture_type: "CoverFront".to_owned(),
            description: Some("portable artwork".to_owned()),
            data: vec![1, 3, 5, 7],
        };
        {
            let mut store = LibraryStore::open(&source_db).unwrap();
            let mut track = Track::from_path(source_audio.clone());
            track.title = "Portable Song".to_owned();
            track.artist = Some("Portable Artist".to_owned());
            track.album = Some("Portable Album".to_owned());
            track.view_name = Some("Portable View".to_owned());
            track.user_rating = Some(9);
            track.set_primary_audio_hash("portable-audio-hash");
            store.upsert_track(&track).unwrap();
            store
                .set_track_notes(&source_audio, "portable note")
                .unwrap();
            store.set_favorite(&source_audio, true).unwrap();
            store.record_playback(&source_audio, 4321, true).unwrap();
            store.create_playlist("Portable Playlist").unwrap();
            store
                .add_playlist_track("Portable Playlist", &source_audio)
                .unwrap();
            store
                .save_artwork(&source_audio, std::slice::from_ref(&image))
                .unwrap();
            store
                .set_track_artwork_reference(&source_audio, &image)
                .unwrap();
            store
                .set_album_artwork_reference_for_track(&source_audio, &image)
                .unwrap();
            store
                .save_playlist_artwork("Portable Playlist", &image)
                .unwrap();
        }

        let package_root = temp_dir("library_package");
        let source_app = create_app(&source_db, &source_media);
        let exported = unsafe {
            call_json(player_app_export_library(
                source_app,
                c_string_arg(&package_root).as_ptr(),
            ))
        };
        assert_ok(&exported);
        assert_eq!(exported["data"]["tracks"], 1);
        assert_eq!(exported["data"]["audio_files"], 1);
        assert_eq!(exported["data"]["sidecar_files"], 2);
        assert!(package_root.join(LIBRARY_PACKAGE_DATABASE_FILE).is_file());
        assert!(package_root.join(LIBRARY_PACKAGE_MANIFEST_FILE).is_file());
        unsafe { player_app_destroy(source_app) };

        let target_root = temp_dir("library_package_target");
        let target_db = target_root.join("player_library.sqlite3");
        let target_media = target_root.join("Music");
        let old_audio = target_media.join("old.wav");
        fs::create_dir_all(&target_media).unwrap();
        write_test_wav(&old_audio, b"Old Song").unwrap();
        {
            let mut store = LibraryStore::open(&target_db).unwrap();
            store
                .upsert_track(&Track::from_path(old_audio.clone()))
                .unwrap();
        }
        let target_app = create_app(&target_db, &target_media);
        let zeroed = unsafe { call_json(player_app_zero_out_library(target_app)) };
        assert_ok(&zeroed);
        assert!(LibraryStore::open(&target_db)
            .unwrap()
            .tracks()
            .unwrap()
            .is_empty());
        assert!(!old_audio.exists());

        let imported = unsafe {
            call_json(player_app_import_library(
                target_app,
                c_string_arg(&package_root).as_ptr(),
            ))
        };
        assert_ok(&imported);
        assert_eq!(imported["data"]["tracks"], 1);
        assert_eq!(imported["data"]["audio_files"], 1);
        assert_eq!(imported["data"]["sidecar_files"], 2);

        let store = LibraryStore::open(&target_db).unwrap();
        let tracks = store.tracks().unwrap();
        assert_eq!(tracks.len(), 1);
        let imported_track = &tracks[0];
        assert_eq!(imported_track.title, "Portable Song");
        assert_eq!(imported_track.artist.as_deref(), Some("Portable Artist"));
        assert_eq!(imported_track.view_name.as_deref(), Some("Portable View"));
        assert_eq!(imported_track.user_rating, Some(9));
        assert!(imported_track.path.starts_with(&target_media));
        assert_eq!(
            fs::read(&imported_track.path).unwrap(),
            fs::read(&source_audio).unwrap()
        );
        assert_eq!(
            fs::read(imported_track.path.with_extension("lrc")).unwrap(),
            b"[00:00]Portable lyrics"
        );
        assert_eq!(
            fs::read(imported_track.path.parent().unwrap().join("cover.jpg")).unwrap(),
            b"cover"
        );
        assert_eq!(
            store.playlist_tracks("Portable Playlist").unwrap()[0]
                .track
                .path,
            imported_track.path
        );
        assert_eq!(
            store.favorite_tracks().unwrap()[0].path,
            imported_track.path
        );
        let history = store.play_history(10).unwrap();
        assert_eq!(history[0].track.path, imported_track.path);
        assert_eq!(history[0].position_ms, 4321);
        assert!(history[0].completed);
        assert_eq!(
            store.track_notes(&imported_track.path).unwrap().as_deref(),
            Some("portable note")
        );
        assert_eq!(
            store.artwork_for_path(&imported_track.path).unwrap()[0].data,
            image.data
        );
        assert!(store
            .track_artwork_reference(&imported_track.path)
            .unwrap()
            .is_some());
        assert!(store
            .album_artwork_reference(&imported_track.path)
            .unwrap()
            .is_some());
        assert_eq!(
            store
                .playlist_artwork("Portable Playlist")
                .unwrap()
                .unwrap()
                .data,
            image.data
        );

        unsafe { player_app_destroy(target_app) };
        fs::remove_dir_all(source_root).ok();
        fs::remove_dir_all(package_root).ok();
        fs::remove_dir_all(target_root).ok();
    }

    #[test]
    fn library_package_import_rejects_escaping_paths_before_replacing_state() {
        let target_root = temp_dir("unsafe_library_package_target");
        let target_db = target_root.join("player_library.sqlite3");
        let target_media = target_root.join("Music");
        let existing_audio = target_media.join("existing.wav");
        fs::create_dir_all(&target_media).unwrap();
        write_test_wav(&existing_audio, b"Existing").unwrap();
        {
            let mut store = LibraryStore::open(&target_db).unwrap();
            store
                .upsert_track(&Track::from_path(existing_audio.clone()))
                .unwrap();
        }

        let package_root = temp_dir("unsafe_library_package");
        fs::create_dir_all(&package_root).unwrap();
        fs::copy(&target_db, package_root.join(LIBRARY_PACKAGE_DATABASE_FILE)).unwrap();
        fs::write(
            package_root.join(LIBRARY_PACKAGE_MANIFEST_FILE),
            serde_json::to_vec(&LibraryPackageManifest {
                format_version: LIBRARY_PACKAGE_FORMAT_VERSION,
                database_file: LIBRARY_PACKAGE_DATABASE_FILE.to_owned(),
                tracks: vec![LibraryPackageTrack {
                    database_path: existing_audio.to_string_lossy().into_owned(),
                    audio_file: "../outside.wav".to_owned(),
                }],
            })
            .unwrap(),
        )
        .unwrap();

        let app = create_app(&target_db, &target_media);
        let imported = unsafe {
            call_json(player_app_import_library(
                app,
                c_string_arg(&package_root).as_ptr(),
            ))
        };
        assert_eq!(imported["ok"], false);
        let library = unsafe { call_json(player_app_library(app)) };
        assert_ok(&library);
        assert_eq!(library["data"].as_array().unwrap().len(), 1);
        assert_eq!(
            library["data"][0]["path"],
            existing_audio.to_string_lossy().as_ref()
        );

        unsafe { player_app_destroy(app) };
        fs::remove_dir_all(package_root).ok();
        fs::remove_dir_all(target_root).ok();
    }

    #[test]
    fn track_details_find_imported_sidecar_artwork_and_lyrics() {
        let source_dir = temp_dir("detail_source");
        fs::create_dir_all(&source_dir).unwrap();
        let source_audio = source_dir.join("song.ogg");
        fs::copy(
            workspace_root()
                .join("test-assets")
                .join("audio")
                .join("into_the_oceans_chorus.ogg"),
            &source_audio,
        )
        .unwrap();
        fs::write(
            source_dir.join("song.lrc"),
            "[00:01.00]hello normal player\n",
        )
        .unwrap();
        fs::write(source_dir.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xD9]).unwrap();

        let db_dir = temp_dir("details_db");
        fs::create_dir_all(&db_dir).unwrap();
        let db_path = db_dir.join("library.sqlite3");
        let media_root = temp_dir("details_media");
        let app = create_app(&db_path, &media_root);

        let import = unsafe {
            call_json(player_app_import_folder(
                app,
                c_string_arg(&source_dir).as_ptr(),
            ))
        };
        assert_ok(&import);
        assert_eq!(import["data"]["imported"], 1);

        let library = unsafe { call_json(player_app_library(app)) };
        assert_ok(&library);
        let managed_path = library["data"][0]["path"].as_str().unwrap().to_owned();
        let details = unsafe {
            call_json(player_app_track_details(
                app,
                c_string_arg(&managed_path).as_ptr(),
            ))
        };
        assert_ok(&details);
        let data = &details["data"];

        let lyrics_path = PathBuf::from(data["lyrics_path"].as_str().unwrap());
        assert!(
            lyrics_path.starts_with(&media_root),
            "{}",
            lyrics_path.display()
        );
        assert!(lyrics_path.exists(), "{}", lyrics_path.display());
        assert!(data["lyrics_text"]
            .as_str()
            .unwrap()
            .contains("hello normal player"));

        let artwork_path = PathBuf::from(data["artwork_path"].as_str().unwrap());
        assert!(
            artwork_path.starts_with(&media_root),
            "{}",
            artwork_path.display()
        );
        assert_eq!(artwork_path.file_name().unwrap(), "cover.jpg");

        let notes = unsafe {
            call_json(player_app_set_track_notes(
                app,
                c_string_arg(&managed_path).as_ptr(),
                c_string_arg("listen again").as_ptr(),
            ))
        };
        assert_ok(&notes);
        let notes_path = notes["data"]["path"].as_str().unwrap();
        assert_ne!(notes_path, managed_path);
        assert_eq!(notes["data"]["primary_view_id"], data["primary_view_id"]);
        assert_eq!(notes["data"]["view_kind"], "derived");

        let original_details = unsafe {
            call_json(player_app_track_details(
                app,
                c_string_arg(&managed_path).as_ptr(),
            ))
        };
        assert!(original_details["data"]["notes"].is_null());
        let details = unsafe {
            call_json(player_app_track_details(
                app,
                c_string_arg(notes_path).as_ptr(),
            ))
        };
        assert_eq!(details["data"]["notes"], "listen again");
        assert!(details["data"]["lyrics_text"]
            .as_str()
            .unwrap()
            .contains("hello normal player"));

        unsafe { player_app_destroy(app) };
        fs::remove_dir_all(source_dir).ok();
        fs::remove_dir_all(db_dir).ok();
        fs::remove_dir_all(media_root).ok();
    }

    #[test]
    fn app_sorts_playlist_via_ffi() {
        let db_path = temp_db_path("sort_playlist");
        let media_root = temp_dir("sort_playlist_media");
        fs::create_dir_all(&media_root).unwrap();
        let first_path = media_root.join("a.ogg");
        let second_path = media_root.join("b.ogg");
        let third_path = media_root.join("c.ogg");
        let app = create_app(&db_path, &media_root);

        {
            let mut first = Track::from_path(first_path.clone());
            first.title = "Delta".to_owned();
            first.artist = Some("Beta".to_owned());
            first.album = Some("Second".to_owned());
            first.track_number = Some(2);
            first.user_rating = Some(8);
            first.set_primary_audio_hash("audio-a");

            let mut second = Track::from_path(second_path.clone());
            second.title = "Alpha".to_owned();
            second.artist = Some("Gamma".to_owned());
            second.album = Some("First".to_owned());
            second.track_number = Some(2);
            second.set_primary_audio_hash("audio-b");

            let mut third = Track::from_path(third_path.clone());
            third.title = "Charlie".to_owned();
            third.artist = Some("Alpha".to_owned());
            third.album = Some("First".to_owned());
            third.track_number = Some(1);
            third.user_rating = Some(10);
            third.set_primary_audio_hash("audio-c");

            let mut store = LibraryStore::open(&db_path).unwrap();
            store.upsert_tracks(&[first, second, third]).unwrap();
            store.add_playlist_track("Road", &second_path).unwrap();
            store.add_playlist_track("Road", &first_path).unwrap();
            store.add_playlist_track("Road", &third_path).unwrap();
        }

        let sort = unsafe {
            call_json(player_app_sort_playlist(
                app,
                c_string_arg("Road").as_ptr(),
                c_string_arg("title").as_ptr(),
            ))
        };
        assert_ok(&sort);
        let sorted = unsafe {
            call_json(player_app_playlist_tracks(
                app,
                c_string_arg("Road").as_ptr(),
            ))
        };
        assert_ok(&sorted);
        assert_eq!(
            playlist_paths(&sorted),
            vec![
                second_path.to_string_lossy().into_owned(),
                third_path.to_string_lossy().into_owned(),
                first_path.to_string_lossy().into_owned()
            ]
        );

        let sort_rating = unsafe {
            call_json(player_app_sort_playlist(
                app,
                c_string_arg("Road").as_ptr(),
                c_string_arg("rating").as_ptr(),
            ))
        };
        assert_ok(&sort_rating);
        let sorted = unsafe {
            call_json(player_app_playlist_tracks(
                app,
                c_string_arg("Road").as_ptr(),
            ))
        };
        assert_eq!(
            playlist_paths(&sorted),
            vec![
                third_path.to_string_lossy().into_owned(),
                first_path.to_string_lossy().into_owned(),
                second_path.to_string_lossy().into_owned()
            ]
        );

        let reset = unsafe {
            call_json(player_app_sort_playlist(
                app,
                c_string_arg("Road").as_ptr(),
                c_string_arg("manual").as_ptr(),
            ))
        };
        assert_ok(&reset);
        let sorted = unsafe {
            call_json(player_app_playlist_tracks(
                app,
                c_string_arg("Road").as_ptr(),
            ))
        };
        assert_eq!(
            playlist_paths(&sorted),
            vec![
                second_path.to_string_lossy().into_owned(),
                first_path.to_string_lossy().into_owned(),
                third_path.to_string_lossy().into_owned()
            ]
        );

        unsafe { player_app_destroy(app) };
        fs::remove_file(db_path).ok();
        fs::remove_dir_all(media_root).ok();
    }

    #[test]
    fn app_edits_metadata_by_creating_a_derived_view() {
        let db_path = temp_db_path("metadata_edit");
        let media_root = temp_dir("metadata_edit_media");
        fs::create_dir_all(&media_root).unwrap();
        let source_path = media_root.join("first.ogg");
        fs::write(&source_path, b"not decoded by this test").unwrap();
        let app = create_app(&db_path, &media_root);

        {
            let mut first = Track::from_path(source_path.clone());
            first.title = "Original Title".to_owned();
            first.artist = Some("Original Artist".to_owned());
            first.album = Some("Original Album".to_owned());
            first.set_primary_audio_hash("same-audio");

            LibraryStore::open(&db_path)
                .unwrap()
                .upsert_track(&first)
                .unwrap();
        }

        let edit = unsafe {
            call_json(player_app_set_track_metadata(
                app,
                c_string_arg(&source_path).as_ptr(),
                c_string_arg("Display Title").as_ptr(),
                c_string_arg("Display Artist").as_ptr(),
                c_string_arg("Display Album").as_ptr(),
            ))
        };
        assert_ok(&edit);
        let derived_path = edit["data"]["path"].as_str().unwrap();
        assert_ne!(derived_path, source_path.to_string_lossy());
        assert!(Path::new(derived_path).starts_with(media_root.join("Views")));
        assert_eq!(edit["data"]["primary_view_id"], "audio:same-audio");
        assert_eq!(edit["data"]["view_kind"], "derived");
        assert_eq!(edit["data"]["title"], "Display Title");

        let library = unsafe { call_json(player_app_library(app)) };
        assert_ok(&library);
        let tracks = library["data"].as_array().unwrap();
        assert_eq!(tracks.len(), 2);
        assert!(tracks.iter().any(|track| {
            track["path"] == source_path.to_string_lossy().as_ref()
                && track["view_kind"] == "primary"
                && track["title"] == "Original Title"
        }));
        assert!(tracks.iter().any(|track| {
            track["path"] == derived_path
                && track["view_kind"] == "derived"
                && track["title"] == "Display Title"
        }));

        let original_details = unsafe {
            call_json(player_app_track_details(
                app,
                c_string_arg(&source_path).as_ptr(),
            ))
        };
        assert_ok(&original_details);
        assert_eq!(original_details["data"]["audio_hash"], "same-audio");
        assert_eq!(original_details["data"]["original_title"], "Original Title");
        assert_eq!(original_details["data"]["display_title"], "Original Title");

        let derived_details = unsafe {
            call_json(player_app_track_details(
                app,
                c_string_arg(derived_path).as_ptr(),
            ))
        };
        assert_ok(&derived_details);
        assert_eq!(derived_details["data"]["audio_hash"], "same-audio");
        assert_eq!(derived_details["data"]["original_title"], "Original Title");
        assert_eq!(
            derived_details["data"]["original_artist"],
            "Original Artist"
        );
        assert_eq!(derived_details["data"]["original_album"], "Original Album");
        assert_eq!(derived_details["data"]["display_title"], "Display Title");
        assert_eq!(derived_details["data"]["display_artist"], "Display Artist");
        assert_eq!(derived_details["data"]["display_album"], "Display Album");

        unsafe { player_app_destroy(app) };
        fs::remove_file(db_path).ok();
        fs::remove_dir_all(media_root).ok();
    }

    #[test]
    fn app_sets_track_rating_via_ffi_and_reports_invalid_values() {
        let db_path = temp_db_path("rating");
        let media_root = temp_dir("rating_media");
        fs::create_dir_all(&media_root).unwrap();
        let track_path = media_root.join("rated.ogg");
        let app = create_app(&db_path, &media_root);

        {
            let mut track = Track::from_path(track_path.clone());
            track.title = "Rated Song".to_owned();
            track.set_primary_audio_hash("rating-audio");
            LibraryStore::open(&db_path)
                .unwrap()
                .upsert_track(&track)
                .unwrap();
        }

        let rated = unsafe {
            call_json(player_app_set_track_rating(
                app,
                c_string_arg(&track_path).as_ptr(),
                8,
            ))
        };
        assert_ok(&rated);
        assert_eq!(rated["data"]["rating"], 8);

        let details = unsafe {
            call_json(player_app_track_details(
                app,
                c_string_arg(&track_path).as_ptr(),
            ))
        };
        assert_ok(&details);
        assert_eq!(details["data"]["rating"], 8);

        let cleared = unsafe {
            call_json(player_app_set_track_rating(
                app,
                c_string_arg(&track_path).as_ptr(),
                0,
            ))
        };
        assert_ok(&cleared);
        assert!(cleared["data"]["rating"].is_null());

        let invalid_high = unsafe {
            call_json(player_app_set_track_rating(
                app,
                c_string_arg(&track_path).as_ptr(),
                11,
            ))
        };
        assert_eq!(invalid_high["ok"], false);
        assert!(invalid_high["error"]
            .as_str()
            .unwrap()
            .contains("between 1 and 10"));

        let invalid_negative = unsafe {
            call_json(player_app_set_track_rating(
                app,
                c_string_arg(&track_path).as_ptr(),
                -1,
            ))
        };
        assert_eq!(invalid_negative["ok"], false);

        unsafe { player_app_destroy(app) };
        fs::remove_file(db_path).ok();
        fs::remove_dir_all(media_root).ok();
    }

    #[test]
    fn app_edits_full_view_payload_by_creating_one_derived_view() {
        let db_path = temp_db_path("view_edit");
        let media_root = temp_dir("view_edit_media");
        fs::create_dir_all(&media_root).unwrap();
        let source_path = media_root.join("source.ogg");
        fs::write(&source_path, b"not decoded by this test").unwrap();
        let artwork_path = media_root.join("cover.png");
        fs::write(&artwork_path, b"\x89PNG\r\n\x1A\npayload").unwrap();
        let lyrics_path = media_root.join("words.lrc");
        fs::write(&lyrics_path, "[00:00.00]new words\n").unwrap();
        let app = create_app(&db_path, &media_root);

        {
            let mut source = Track::from_path(source_path.clone());
            source.title = "Source Title".to_owned();
            source.artist = Some("Source Artist".to_owned());
            source.album = Some("Source Album".to_owned());
            source.view_name = Some("Source view".to_owned());
            source.set_primary_audio_hash("view-edit-audio");
            let mut store = LibraryStore::open(&db_path).unwrap();
            store.upsert_track(&source).unwrap();
            store.set_track_notes(&source.path, "source note").unwrap();
        }

        let edit_payload = serde_json::json!({
            "view_name": "Evening edit",
            "title": "Edited Title",
            "artist": "Edited Artist",
            "album": "Edited Album",
            "notes": "edited note",
            "artwork_path": artwork_path.to_string_lossy(),
            "lyrics_path": lyrics_path.to_string_lossy()
        })
        .to_string();
        let edit = unsafe {
            call_json(player_app_edit_track_view(
                app,
                c_string_arg(&source_path).as_ptr(),
                c_string_arg(edit_payload.as_str()).as_ptr(),
            ))
        };
        assert_ok(&edit);
        assert_eq!(edit["data"]["view_name"], "Evening edit");
        assert_eq!(edit["data"]["title"], "Edited Title");
        assert_eq!(edit["data"]["view_kind"], "derived");
        let derived_path = edit["data"]["path"].as_str().unwrap();
        assert_ne!(derived_path, source_path.to_string_lossy());

        let library = unsafe { call_json(player_app_library(app)) };
        assert_ok(&library);
        let tracks = library["data"].as_array().unwrap();
        assert_eq!(tracks.len(), 2);
        assert_eq!(
            tracks
                .iter()
                .filter(|track| track["view_kind"] == "derived")
                .count(),
            1
        );

        let source_details = unsafe {
            call_json(player_app_track_details(
                app,
                c_string_arg(&source_path).as_ptr(),
            ))
        };
        assert_ok(&source_details);
        assert_eq!(source_details["data"]["view_name"], "Source view");
        assert_eq!(source_details["data"]["display_title"], "Source Title");
        assert_eq!(source_details["data"]["notes"], "source note");

        let details = unsafe {
            call_json(player_app_track_details(
                app,
                c_string_arg(derived_path).as_ptr(),
            ))
        };
        assert_ok(&details);
        assert_eq!(details["data"]["view_name"], "Evening edit");
        assert_eq!(details["data"]["display_title"], "Edited Title");
        assert_eq!(details["data"]["display_artist"], "Edited Artist");
        assert_eq!(details["data"]["display_album"], "Edited Album");
        assert_eq!(details["data"]["notes"], "edited note");
        assert!(details["data"]["lyrics_text"]
            .as_str()
            .unwrap()
            .contains("new words"));
        assert!(Path::new(details["data"]["artwork_path"].as_str().unwrap()).exists());

        unsafe { player_app_destroy(app) };
        fs::remove_file(db_path).ok();
        fs::remove_dir_all(media_root).ok();
    }

    #[test]
    fn app_sets_album_artwork_asset_for_album_tracks_persistently() {
        let db_dir = temp_dir("album_artwork_db");
        fs::create_dir_all(&db_dir).unwrap();
        let db_path = db_dir.join("library.sqlite");
        let media_root = temp_dir("album_artwork_media");
        fs::create_dir_all(&media_root).unwrap();
        let cover_path = media_root.join("album-cover.png");
        fs::write(&cover_path, b"\x89PNG\r\n\x1A\nalbum").unwrap();
        let first_path = media_root.join("01.ogg");
        let second_path = media_root.join("02.ogg");
        let other_path = media_root.join("other.ogg");
        let app = create_app(&db_path, &media_root);

        {
            let mut first = Track::from_path(first_path.clone());
            first.title = "First".to_owned();
            first.album = Some("Shared".to_owned());
            first.album_artist = Some("Band".to_owned());
            first.set_primary_audio_hash("album-artwork-a");

            let mut second = Track::from_path(second_path.clone());
            second.title = "Second".to_owned();
            second.album = Some("Shared".to_owned());
            second.artist = Some("Band".to_owned());
            second.set_primary_audio_hash("album-artwork-b");

            let mut other = Track::from_path(other_path.clone());
            other.title = "Other".to_owned();
            other.album = Some("Shared".to_owned());
            other.artist = Some("Other Band".to_owned());
            other.set_primary_audio_hash("album-artwork-c");

            LibraryStore::open(&db_path)
                .unwrap()
                .upsert_tracks(&[first, second, other])
                .unwrap();
        }

        let updated = unsafe {
            call_json(player_app_set_album_artwork(
                app,
                c_string_arg(&first_path).as_ptr(),
                c_string_arg(&cover_path).as_ptr(),
            ))
        };
        assert_ok(&updated);
        assert_eq!(updated["data"]["tracks_updated"], 2);

        let library = unsafe { call_json(player_app_library(app)) };
        assert_ok(&library);
        let album_artwork_tracks = library["data"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|track| track["artwork_source"] == "album")
            .collect::<Vec<_>>();
        assert_eq!(album_artwork_tracks.len(), 2);
        assert!(album_artwork_tracks.iter().all(|track| {
            Path::new(track["artwork_path"].as_str().unwrap())
                .starts_with(db_dir.join("Artwork").join("Assets"))
        }));
        assert!(album_artwork_tracks
            .iter()
            .all(|track| track["has_album_identity"] == true));
        fs::remove_file(&cover_path).unwrap();
        for path in [&first_path, &second_path] {
            let details =
                unsafe { call_json(player_app_track_details(app, c_string_arg(path).as_ptr())) };
            assert_ok(&details);
            let artwork_path = PathBuf::from(details["data"]["artwork_path"].as_str().unwrap());
            assert!(artwork_path.starts_with(db_dir.join("Artwork").join("Assets")));
            assert_eq!(fs::read(&artwork_path).unwrap(), b"\x89PNG\r\n\x1A\nalbum");
            assert_eq!(details["data"]["artwork_source"], "album");
        }

        let other_details = unsafe {
            call_json(player_app_track_details(
                app,
                c_string_arg(&other_path).as_ptr(),
            ))
        };
        assert_ok(&other_details);
        assert!(other_details["data"]["artwork_path"].is_null());

        unsafe { player_app_destroy(app) };
        fs::remove_dir_all(db_dir).ok();
        fs::remove_dir_all(media_root).ok();
    }

    #[test]
    fn app_track_artwork_reference_overrides_album_artwork_reference() {
        let db_path = temp_db_path("track_over_album_artwork");
        let media_root = temp_dir("track_over_album_artwork_media");
        fs::create_dir_all(&media_root).unwrap();
        let album_cover = media_root.join("album-reference.png");
        let track_cover = media_root.join("track-reference.png");
        fs::write(&album_cover, b"\x89PNG\r\n\x1A\nalbum").unwrap();
        fs::write(&track_cover, b"\x89PNG\r\n\x1A\ntrack").unwrap();
        let source_path = media_root.join("song.ogg");
        fs::write(&source_path, b"not decoded by this test").unwrap();
        let app = create_app(&db_path, &media_root);

        {
            let mut track = Track::from_path(source_path.clone());
            track.title = "Song".to_owned();
            track.album = Some("Album".to_owned());
            track.artist = Some("Artist".to_owned());
            track.set_primary_audio_hash("track-over-album-artwork");
            LibraryStore::open(&db_path)
                .unwrap()
                .upsert_track(&track)
                .unwrap();
        }

        let album = unsafe {
            call_json(player_app_set_album_artwork(
                app,
                c_string_arg(&source_path).as_ptr(),
                c_string_arg(&album_cover).as_ptr(),
            ))
        };
        assert_ok(&album);

        let track_edit = unsafe {
            call_json(player_app_set_track_artwork(
                app,
                c_string_arg(&source_path).as_ptr(),
                c_string_arg(&track_cover).as_ptr(),
            ))
        };
        assert_ok(&track_edit);
        assert_eq!(track_edit["data"]["view_kind"], "derived");
        let track_edit_artwork_path =
            PathBuf::from(track_edit["data"]["artwork_path"].as_str().unwrap());
        assert!(track_edit_artwork_path
            .starts_with(db_path.parent().unwrap().join("Artwork").join("Assets")));
        assert_eq!(
            fs::read(&track_edit_artwork_path).unwrap(),
            b"\x89PNG\r\n\x1A\ntrack"
        );
        assert_eq!(track_edit["data"]["artwork_source"], "track");
        let derived_path = PathBuf::from(track_edit["data"]["path"].as_str().unwrap());
        fs::remove_file(&track_cover).unwrap();
        fs::remove_file(&album_cover).unwrap();

        let details = unsafe {
            call_json(player_app_track_details(
                app,
                c_string_arg(&derived_path).as_ptr(),
            ))
        };
        assert_ok(&details);
        let details_artwork_path = PathBuf::from(details["data"]["artwork_path"].as_str().unwrap());
        assert!(details_artwork_path
            .starts_with(db_path.parent().unwrap().join("Artwork").join("Assets")));
        assert_eq!(
            fs::read(&details_artwork_path).unwrap(),
            b"\x89PNG\r\n\x1A\ntrack"
        );
        assert_eq!(details["data"]["artwork_source"], "track");

        let source_details = unsafe {
            call_json(player_app_track_details(
                app,
                c_string_arg(&source_path).as_ptr(),
            ))
        };
        let source_artwork_path =
            PathBuf::from(source_details["data"]["artwork_path"].as_str().unwrap());
        assert!(source_artwork_path
            .starts_with(db_path.parent().unwrap().join("Artwork").join("Assets")));
        assert_eq!(
            fs::read(&source_artwork_path).unwrap(),
            b"\x89PNG\r\n\x1A\nalbum"
        );
        assert_eq!(source_details["data"]["artwork_source"], "album");

        unsafe { player_app_destroy(app) };
        fs::remove_file(db_path).ok();
        fs::remove_dir_all(media_root).ok();
    }

    #[test]
    fn app_playlist_artwork_defaults_to_first_track_and_custom_overrides() {
        let db_dir = temp_dir("playlist_artwork_db");
        fs::create_dir_all(&db_dir).unwrap();
        let db_path = db_dir.join("library.sqlite3");
        let media_root = temp_dir("playlist_artwork_media");
        fs::create_dir_all(&media_root).unwrap();
        let first_path = media_root.join("first.ogg");
        let second_path = media_root.join("second.ogg");
        let first_cover = media_root
            .join("first-cover.png")
            .canonicalize()
            .unwrap_or_else(|_| media_root.join("first-cover.png"));
        let custom_cover = media_root.join("custom-playlist.png");
        let custom_cover_two = media_root.join("custom-playlist-two.png");
        fs::write(&first_path, b"not decoded by this test").unwrap();
        fs::write(&second_path, b"not decoded by this test").unwrap();
        fs::write(&first_cover, b"\x89PNG\r\n\x1A\nfirst").unwrap();
        fs::write(&custom_cover, b"\x89PNG\r\n\x1A\ncustom").unwrap();
        fs::write(&custom_cover_two, b"\x89PNG\r\n\x1A\nsecond").unwrap();
        let app = create_app(&db_path, &media_root);

        {
            let mut first = Track::from_path(first_path.clone());
            first.title = "First".to_owned();
            first.set_primary_audio_hash("playlist-artwork-first");

            let mut second = Track::from_path(second_path.clone());
            second.title = "Second".to_owned();
            second.set_primary_audio_hash("playlist-artwork-second");

            let mut store = LibraryStore::open(&db_path).unwrap();
            store
                .upsert_tracks(&[first.clone(), second.clone()])
                .unwrap();
            let first_image = read_artwork_image(&first_cover).unwrap();
            store
                .set_track_artwork_reference(&first.path, &first_image)
                .unwrap();
            store.add_playlist_track("Mix", &first.path).unwrap();
            store.add_playlist_track("Mix", &second.path).unwrap();
        }

        let playlists = unsafe { call_json(player_app_playlists(app)) };
        assert_ok(&playlists);
        assert_eq!(playlists["data"][0]["name"], "Mix");
        assert_eq!(playlists["data"][0]["track_count"], 2);
        let first_cached_path =
            PathBuf::from(playlists["data"][0]["artwork_path"].as_str().unwrap());
        assert!(first_cached_path.starts_with(db_dir.join("Artwork").join("Assets")));
        assert_eq!(
            fs::read(&first_cached_path).unwrap(),
            b"\x89PNG\r\n\x1A\nfirst"
        );
        assert_eq!(playlists["data"][0]["artwork_source"], "track");

        let updated = unsafe {
            call_json(player_app_set_playlist_artwork(
                app,
                c_string_arg("Mix").as_ptr(),
                c_string_arg(&custom_cover).as_ptr(),
            ))
        };
        assert_ok(&updated);
        fs::remove_file(&custom_cover).unwrap();

        let playlists = unsafe { call_json(player_app_playlists(app)) };
        assert_ok(&playlists);
        let custom_path = PathBuf::from(playlists["data"][0]["artwork_path"].as_str().unwrap());
        assert_eq!(playlists["data"][0]["artwork_source"], "playlist");
        assert!(custom_path.starts_with(db_dir.join("Artwork").join("Playlists")));
        assert_eq!(fs::read(&custom_path).unwrap(), b"\x89PNG\r\n\x1A\ncustom");

        let updated = unsafe {
            call_json(player_app_set_playlist_artwork(
                app,
                c_string_arg("Mix").as_ptr(),
                c_string_arg(&custom_cover_two).as_ptr(),
            ))
        };
        assert_ok(&updated);
        fs::remove_file(&custom_cover_two).unwrap();
        let playlists = unsafe { call_json(player_app_playlists(app)) };
        assert_ok(&playlists);
        let rewritten_custom_path =
            PathBuf::from(playlists["data"][0]["artwork_path"].as_str().unwrap());
        assert_eq!(rewritten_custom_path, custom_path);
        assert_eq!(
            fs::read(&rewritten_custom_path).unwrap(),
            b"\x89PNG\r\n\x1A\nsecond"
        );

        unsafe { player_app_destroy(app) };
        fs::remove_dir_all(db_dir).ok();
        fs::remove_dir_all(media_root).ok();
    }

    #[test]
    fn app_writes_local_user_profile_and_playback_history_file() {
        let db_dir = temp_dir("user_data_db");
        fs::create_dir_all(&db_dir).unwrap();
        let db_path = db_dir.join("library.sqlite3");
        let media_root = temp_dir("user_data_media");
        fs::create_dir_all(&media_root).unwrap();
        let track_path = media_root.join("summary_song.ogg");
        let app = create_app(&db_path, &media_root);

        let user_data = unsafe { call_json(player_app_user_data(app)) };
        assert_ok(&user_data);
        assert_eq!(user_data["data"]["display_name"], "Local User");
        assert_eq!(user_data["data"]["sync_enabled"], false);
        let profile_path = PathBuf::from(user_data["data"]["profile_path"].as_str().unwrap());
        let history_path = PathBuf::from(user_data["data"]["history_path"].as_str().unwrap());
        assert!(profile_path.exists(), "{}", profile_path.display());

        let mut track = Track::from_path(track_path.clone());
        track.title = "Summary Song".to_owned();
        track.artist = Some("Normal Artist".to_owned());
        track.album = Some("Midyear Mix".to_owned());
        track.duration_ms = Some(120_000);
        track.set_primary_audio_hash("summary-audio");
        LibraryStore::open(&db_path)
            .unwrap()
            .upsert_track(&track)
            .unwrap();

        unsafe {
            let app = &mut *app;
            let dto = track_to_dto(&track).unwrap();
            app.current_track = Some(dto.clone());
            app.is_playing = true;
            app.position_ms = 0;
            app.start_active_session(dto, 0);
            app.position_ms = 90_000;
            app.observe_active_position(90_000);
            app.finish_active_session("stopped").unwrap();
        }

        let history_text = fs::read_to_string(&history_path).unwrap();
        let history_lines = history_text.lines().collect::<Vec<_>>();
        assert_eq!(history_lines.len(), 1);
        let event: Value = serde_json::from_str(history_lines[0]).unwrap();
        assert_eq!(event["record_type"], "playback_session");
        assert_eq!(event["track"]["title"], "Summary Song");
        assert_eq!(event["track"]["artist"], "Normal Artist");
        assert_eq!(event["listened_ms"], 90_000);
        assert_eq!(event["end_position_ms"], 90_000);
        assert_eq!(event["track_duration_ms"], 120_000);
        assert_eq!(event["completed"], false);
        assert_eq!(event["finish_reason"], "stopped");

        let sqlite_history = LibraryStore::open(&db_path)
            .unwrap()
            .play_history(10)
            .unwrap();
        assert_eq!(sqlite_history.len(), 1);
        assert_eq!(sqlite_history[0].track.title, "Summary Song");
        assert_eq!(sqlite_history[0].position_ms, 90_000);
        assert!(!sqlite_history[0].completed);

        unsafe { player_app_destroy(app) };
        fs::remove_dir_all(db_dir).ok();
        fs::remove_dir_all(media_root).ok();
    }

    #[test]
    fn app_exposes_repeat_shuffle_and_empty_queue_snapshot_without_opening_audio() {
        let db_path = temp_db_path("queue_modes");
        let media_root = temp_dir("queue_modes_media");
        fs::create_dir_all(&media_root).unwrap();
        let app = create_app(&db_path, &media_root);

        let repeat = unsafe {
            call_json(player_app_set_repeat_mode(
                app,
                c_string_arg("one").as_ptr(),
            ))
        };
        assert_ok(&repeat);
        assert_eq!(repeat["data"]["repeat_mode"], "one");
        assert_eq!(repeat["data"]["shuffle_enabled"], false);
        assert_eq!(repeat["data"]["queue_len"], 0);
        assert!(repeat["data"]["queue_position"].is_null());

        let shuffle = unsafe { call_json(player_app_set_shuffle(app, true)) };
        assert_ok(&shuffle);
        assert_eq!(shuffle["data"]["repeat_mode"], "one");
        assert_eq!(shuffle["data"]["shuffle_enabled"], true);

        let queue = unsafe { call_json(player_app_queue(app)) };
        assert_ok(&queue);
        assert_eq!(queue["data"]["tracks"].as_array().unwrap().len(), 0);
        assert_eq!(queue["data"]["repeat_mode"], "one");
        assert_eq!(queue["data"]["shuffle_enabled"], true);

        let invalid = unsafe {
            call_json(player_app_set_repeat_mode(
                app,
                c_string_arg("sideways").as_ptr(),
            ))
        };
        assert!(!invalid["ok"].as_bool().unwrap());

        unsafe { player_app_destroy(app) };
        fs::remove_file(db_path).ok();
        fs::remove_dir_all(media_root).ok();
    }

    #[test]
    fn app_exposes_audio_lifecycle_state_without_opening_audio() {
        let db_path = temp_db_path("audio_lifecycle");
        let media_root = temp_dir("audio_lifecycle_media");
        fs::create_dir_all(&media_root).unwrap();
        let app = create_app(&db_path, &media_root);

        unsafe {
            (*app).is_playing = true;
        }

        let began = unsafe { call_json(player_app_audio_interruption_began(app)) };
        assert_ok(&began);
        assert_eq!(began["data"]["interruption_active"], true);
        assert_eq!(began["data"]["resume_after_interruption"], true);
        assert_eq!(began["data"]["is_playing"], false);

        let blocked_resume = unsafe { call_json(player_app_resume(app)) };
        assert!(!blocked_resume["ok"].as_bool().unwrap());
        assert!(blocked_resume["error"]
            .as_str()
            .unwrap()
            .contains("audio interruption is active"));

        let ended = unsafe { call_json(player_app_audio_interruption_ended(app, false)) };
        assert_ok(&ended);
        assert_eq!(ended["data"]["interruption_active"], false);
        assert_eq!(ended["data"]["resume_after_interruption"], false);
        assert_eq!(ended["data"]["is_playing"], false);

        let disconnected = unsafe { call_json(player_app_audio_output_disconnected(app)) };
        assert_ok(&disconnected);
        assert_eq!(disconnected["data"]["is_playing"], false);
        assert_eq!(disconnected["data"]["resume_after_interruption"], false);

        unsafe { player_app_destroy(app) };
        fs::remove_file(db_path).ok();
        fs::remove_dir_all(media_root).ok();
    }

    #[test]
    fn interrupted_app_rejects_a_new_queue_before_opening_audio() {
        let db_path = temp_db_path("interrupted_queue");
        let media_root = temp_dir("interrupted_queue_media");
        fs::create_dir_all(&media_root).unwrap();
        let app = create_app(&db_path, &media_root);
        let track = Track::from_path(media_root.join("blocked.ogg"));

        unsafe {
            (*app).playback_lifecycle.begin_interruption(false);
            let error = match (*app).play_queue_tracks(vec![track], 0) {
                Ok(_) => panic!("playback unexpectedly started during an interruption"),
                Err(error) => error,
            };
            assert!(error.to_string().contains("audio interruption is active"));
            assert!((*app).engine.is_none());
            assert!((*app).current_track.is_none());
        }

        unsafe { player_app_destroy(app) };
        fs::remove_file(db_path).ok();
        fs::remove_dir_all(media_root).ok();
    }

    #[test]
    fn app_queue_snapshot_tracks_current_event_index_and_modes() {
        let db_path = temp_db_path("queue_events");
        let media_root = temp_dir("queue_events_media");
        fs::create_dir_all(&media_root).unwrap();
        let first_path = media_root.join("first.ogg");
        let second_path = media_root.join("second.ogg");
        fs::write(&first_path, b"not decoded by this test").unwrap();
        fs::write(&second_path, b"not decoded by this test").unwrap();
        let app = create_app(&db_path, &media_root);

        let first = {
            let mut track = Track::from_path(first_path.clone());
            track.title = "First".to_owned();
            track.set_primary_audio_hash("queue-first");
            track
        };
        let second = {
            let mut track = Track::from_path(second_path.clone());
            track.title = "Second".to_owned();
            track.set_primary_audio_hash("queue-second");
            track
        };

        unsafe {
            let app_ref = &mut *app;
            app_ref.queue_tracks = track_dtos(&[first.clone(), second.clone()]).unwrap();
            app_ref.apply_event(PlaybackEvent::StateChanged(player_core::PlaybackState {
                is_playing: true,
                current_index: Some(1),
                position_ms: 1_234,
                repeat_mode: RepeatMode::All,
                shuffle: true,
            }));
            app_ref.apply_event(PlaybackEvent::TrackChanged(Some(Box::new(second))));
        }

        let snapshot = unsafe { call_json(player_app_poll(app)) };
        assert_ok(&snapshot);
        assert_eq!(snapshot["data"]["queue_len"], 2);
        assert_eq!(snapshot["data"]["queue_position"], 1);
        assert_eq!(snapshot["data"]["repeat_mode"], "all");
        assert_eq!(snapshot["data"]["shuffle_enabled"], true);
        assert_eq!(snapshot["data"]["current_track"]["title"], "Second");

        unsafe { player_app_destroy(app) };
        fs::remove_file(db_path).ok();
        fs::remove_dir_all(media_root).ok();
    }

    #[test]
    fn app_materializes_current_music_view_as_independent_primary() {
        let db_path = temp_db_path("export_view");
        let media_root = temp_dir("export_view_media");
        fs::create_dir_all(&media_root).unwrap();
        let track_path = media_root.join("exportable.wav");
        let export_path = temp_dir("export_view_out").join("portable.wav");
        write_test_wav(&track_path, b"portable source").unwrap();
        fs::write(
            media_root.join("exportable.lrc"),
            "[00:00.00]portable lyric\n",
        )
        .unwrap();
        let audio = audio_hash(&track_path).unwrap().hash;
        let app = create_app(&db_path, &media_root);

        {
            let mut store = LibraryStore::open(&db_path).unwrap();
            let mut track = Track::from_path(track_path.clone());
            track.title = "Portable Title".to_owned();
            track.artist = Some("Portable Artist".to_owned());
            track.album = Some("Portable Album".to_owned());
            track.set_primary_audio_hash(audio.clone());
            store.upsert_track(&track).unwrap();
            store
                .set_track_notes(&track.path, "portable notes")
                .unwrap();
            store
                .save_artwork(
                    &track.path,
                    &[ArtworkImage {
                        picture_index: 0,
                        mime_type: Some("image/png".to_owned()),
                        picture_type: "CoverFront".to_owned(),
                        description: None,
                        data: vec![1, 2, 3, 4],
                    }],
                )
                .unwrap();
        }

        let export = unsafe {
            call_json(player_app_export_track_view(
                app,
                c_string_arg(&track_path).as_ptr(),
                c_string_arg(&export_path).as_ptr(),
            ))
        };
        assert_ok(&export);
        assert_eq!(
            fs::read(&export_path).unwrap(),
            fs::read(&track_path).unwrap()
        );
        assert!(export_path.with_extension("lrc").exists());
        assert_eq!(
            export["data"]["path"],
            export_path.to_string_lossy().as_ref()
        );
        assert_eq!(export["data"]["title"], "Portable Title");
        assert_eq!(export["data"]["view_kind"], "primary");
        assert_eq!(export["data"]["is_primary_view"], true);
        assert_eq!(export["data"]["view_id"], export["data"]["primary_view_id"]);
        assert_ne!(export["data"]["primary_view_id"], format!("audio:{audio}"));
        assert!(export["data"]["primary_view_id"]
            .as_str()
            .unwrap()
            .starts_with(&format!("audio:{audio}:materialized:")));

        let details = unsafe {
            call_json(player_app_track_details(
                app,
                c_string_arg(&export_path).as_ptr(),
            ))
        };
        assert_ok(&details);
        assert_eq!(details["data"]["audio_hash"], audio);
        assert_eq!(details["data"]["is_primary_view"], true);
        assert_eq!(details["data"]["original_title"], "Portable Title");
        assert_eq!(details["data"]["display_title"], "Portable Title");
        assert_eq!(details["data"]["notes"], "portable notes");
        assert!(details["data"]["lyrics_text"]
            .as_str()
            .unwrap()
            .contains("portable lyric"));
        assert!(Path::new(details["data"]["artwork_path"].as_str().unwrap()).exists());

        let library = unsafe { call_json(player_app_library(app)) };
        assert_ok(&library);
        let tracks = library["data"].as_array().unwrap();
        assert_eq!(tracks.len(), 2);
        assert_eq!(
            tracks
                .iter()
                .filter(|track| track["view_kind"] == "primary")
                .count(),
            2
        );

        unsafe { player_app_destroy(app) };
        fs::remove_file(db_path).ok();
        fs::remove_dir_all(media_root).ok();
        fs::remove_dir_all(export_path.parent().unwrap()).ok();
    }

    #[test]
    fn track_details_exports_cached_artwork_to_persistent_file() {
        let db_dir = temp_dir("artwork_db");
        fs::create_dir_all(&db_dir).unwrap();
        let db_path = db_dir.join("library.sqlite3");
        let media_root = temp_dir("artwork_media");
        let app = create_app(&db_path, &media_root);
        let track_path = media_root.join("song.ogg");
        let image_data = vec![0xFF, 0xD8, 0xFF, 0xD9];

        {
            let mut store = LibraryStore::open(&db_path).unwrap();
            let mut track = Track::from_path(track_path.clone());
            track.set_primary_audio_hash("artwork-audio-hash");
            store.upsert_track(&track).unwrap();
            store
                .save_artwork(
                    &track_path,
                    &[ArtworkImage {
                        picture_index: 0,
                        mime_type: Some("image/jpeg".to_owned()),
                        picture_type: "CoverFront".to_owned(),
                        description: None,
                        data: image_data.clone(),
                    }],
                )
                .unwrap();
        }

        let details = unsafe {
            call_json(player_app_track_details(
                app,
                c_string_arg(&track_path).as_ptr(),
            ))
        };
        assert_ok(&details);
        let artwork_path = PathBuf::from(details["data"]["artwork_path"].as_str().unwrap());
        assert!(artwork_path.starts_with(db_dir.join("Artwork")));
        assert!(artwork_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("audio-artwork-audio-hash-"));
        assert_eq!(artwork_path.extension().unwrap(), "jpg");
        assert_eq!(fs::read(artwork_path).unwrap(), image_data);
        assert!(details["data"]["lyrics_path"].is_null());
        assert!(details["data"]["lyrics_text"].is_null());
        assert_eq!(details["data"]["audio_hash"], "artwork-audio-hash");
        assert_eq!(details["data"]["view_id"], "audio:artwork-audio-hash");
        assert_eq!(
            details["data"]["primary_view_id"],
            "audio:artwork-audio-hash"
        );
        assert_eq!(details["data"]["is_primary_view"], true);
        assert_eq!(details["data"]["view_kind"], "primary");
        assert_eq!(details["data"]["format_name"], "ogg");
        assert!(details["data"]["quality_profile"].is_null());

        unsafe { player_app_destroy(app) };
        fs::remove_dir_all(db_dir).ok();
        fs::remove_dir_all(media_root).ok();
    }

    #[test]
    fn import_skips_duplicate_audio_even_when_file_hash_differs() {
        let source_dir = temp_dir("duplicate_source");
        fs::create_dir_all(&source_dir).unwrap();
        let first = source_dir.join("first title.wav");
        let second = source_dir.join("second title.wav");
        write_test_wav(&first, b"first title").unwrap();
        write_test_wav(&second, b"second title").unwrap();
        assert_ne!(file_hash(&first).unwrap(), file_hash(&second).unwrap());
        assert_eq!(
            audio_hash(&first).unwrap().hash,
            audio_hash(&second).unwrap().hash
        );

        let db_path = temp_db_path("duplicate_audio");
        let media_root = temp_dir("duplicate_media");
        let app = create_app(&db_path, &media_root);

        let import = unsafe {
            call_json(player_app_import_folder(
                app,
                c_string_arg(&source_dir).as_ptr(),
            ))
        };
        assert_ok(&import);
        assert_eq!(import["data"]["imported"], 1);
        assert_eq!(import["data"]["copied"], 1);
        assert_eq!(import["data"]["duplicates_skipped"], 1);

        let library = unsafe { call_json(player_app_library(app)) };
        assert_ok(&library);
        assert_eq!(library["data"].as_array().unwrap().len(), 1);
        let track = &library["data"][0];
        assert_eq!(track["id"], track["view_id"]);
        assert!(track["id"].as_str().unwrap().starts_with("audio:"));
        assert_eq!(track["primary_view_id"], track["view_id"]);
        assert_eq!(track["is_primary_view"], true);
        assert_eq!(track["view_kind"], "primary");
        assert_eq!(track["format_name"], "wav");

        unsafe { player_app_destroy(app) };
        fs::remove_dir_all(source_dir).ok();
        fs::remove_file(db_path).ok();
        fs::remove_dir_all(media_root).ok();
    }

    #[test]
    fn import_files_imports_selected_audio_without_requiring_folder_selection() {
        let db_path = temp_db_path("import_files");
        let media_root = temp_dir("import_files_media");
        let audio_root = workspace_root().join("test-assets").join("audio");
        let selected = [
            audio_root.join("into_the_oceans_chorus.ogg"),
            audio_root.join("funk_room_reverb.ogg"),
            audio_root.join("SOURCES.md"),
        ];
        let paths_json = serde_json::to_string(
            &selected
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let paths_arg = CString::new(paths_json).unwrap();
        let app = create_app(&db_path, &media_root);

        let import = unsafe { call_json(player_app_import_files(app, paths_arg.as_ptr())) };
        assert_ok(&import);
        assert_eq!(import["data"]["imported"], 2);
        assert_eq!(import["data"]["copied"], 2);
        assert_eq!(import["data"]["duplicates_skipped"], 0);
        assert_eq!(import["data"]["metadata_warnings"], 1);

        let library = unsafe { call_json(player_app_library(app)) };
        assert_ok(&library);
        let tracks = library["data"].as_array().unwrap();
        assert_eq!(tracks.len(), 2);
        let imported_file_names: HashSet<_> = tracks
            .iter()
            .map(|track| {
                Path::new(track["path"].as_str().unwrap())
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert!(imported_file_names.contains("into_the_oceans_chorus.ogg"));
        assert!(imported_file_names.contains("funk_room_reverb.ogg"));
        for track in tracks {
            let path = Path::new(track["path"].as_str().unwrap());
            assert!(path.starts_with(&media_root), "{}", path.display());
            assert!(path.exists(), "{}", path.display());
            assert_eq!(
                path.parent().and_then(Path::file_name).unwrap(),
                std::ffi::OsStr::new("audio")
            );
        }

        unsafe { player_app_destroy(app) };
        fs::remove_file(db_path).ok();
        fs::remove_dir_all(media_root).ok();
    }

    #[test]
    fn audit_database_merges_existing_duplicate_tracks() {
        let db_path = temp_db_path("audit");
        let media_root = temp_dir("audit_media");
        fs::create_dir_all(&media_root).unwrap();
        let first = media_root.join("a.wav");
        let second = media_root.join("b.wav");
        write_test_wav(&first, b"same audio first").unwrap();
        write_test_wav(&second, b"same audio second").unwrap();
        let app = create_app(&db_path, &media_root);

        {
            let mut store = LibraryStore::open(&db_path).unwrap();
            store
                .upsert_tracks(&[
                    Track::from_path(first.clone()),
                    Track::from_path(second.clone()),
                ])
                .unwrap();
            store.create_playlist("Audit").unwrap();
            store.add_playlist_track("Audit", &second).unwrap();
            store.set_track_notes(&second, "duplicate note").unwrap();
        }

        let audit = unsafe { call_json(player_app_audit_database(app)) };
        assert_ok(&audit);
        assert_eq!(audit["data"]["tracks_scanned"], 2);
        assert_eq!(audit["data"]["duplicate_groups"], 1);
        assert_eq!(audit["data"]["tracks_merged"], 1);

        let library = unsafe { call_json(player_app_library(app)) };
        assert_ok(&library);
        assert_eq!(library["data"].as_array().unwrap().len(), 1);
        assert_eq!(library["data"][0]["path"], first.to_string_lossy().as_ref());

        let playlist = unsafe {
            call_json(player_app_playlist_tracks(
                app,
                c_string_arg("Audit").as_ptr(),
            ))
        };
        assert_eq!(
            playlist["data"][0]["path"],
            first.to_string_lossy().as_ref()
        );
        let details =
            unsafe { call_json(player_app_track_details(app, c_string_arg(&first).as_ptr())) };
        assert_eq!(details["data"]["notes"], "duplicate note");

        unsafe { player_app_destroy(app) };
        fs::remove_file(db_path).ok();
        fs::remove_dir_all(media_root).ok();
    }

    #[test]
    fn app_reports_json_errors_for_bad_inputs() {
        let null_response = unsafe { call_json(player_app_library(ptr::null_mut())) };
        assert_eq!(null_response["ok"], false);
        assert!(null_response["error"]
            .as_str()
            .unwrap()
            .contains("PlayerApp handle is null"));

        let db_path = temp_db_path("errors");
        let media_root = temp_dir("errors_media");
        let app = create_app(&db_path, &media_root);
        let bad_playlist =
            unsafe { call_json(player_app_create_playlist(app, c_string_arg("").as_ptr())) };
        assert_eq!(bad_playlist["ok"], false);

        let repeat_alias = unsafe {
            call_json(player_app_set_repeat_mode(
                app,
                c_string_arg("loop").as_ptr(),
            ))
        };
        assert_eq!(repeat_alias["ok"], false);

        let poll = unsafe { call_json(player_app_poll(app)) };
        assert_ok(&poll);
        assert_eq!(poll["data"]["is_playing"], false);
        assert!(poll["data"]["current_track"].is_null());

        unsafe { player_app_destroy(app) };
        fs::remove_file(db_path).ok();
        fs::remove_dir_all(media_root).ok();
    }

    fn create_app(db_path: &Path, media_root: &Path) -> *mut PlayerApp {
        let db_path = c_string_arg(db_path);
        let media_root = c_string_arg(media_root);
        let app = unsafe { player_app_create(db_path.as_ptr(), media_root.as_ptr()) };
        assert!(!app.is_null());
        app
    }

    unsafe fn call_json(response: *mut c_char) -> Value {
        assert!(!response.is_null());
        let text = CStr::from_ptr(response).to_string_lossy().into_owned();
        player_string_free(response);
        serde_json::from_str(&text).unwrap_or_else(|error| panic!("{error}: {text}"))
    }

    fn assert_ok(response: &Value) {
        assert_eq!(response["ok"], true, "{response}");
    }

    fn playlist_paths(response: &Value) -> Vec<String> {
        response["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|track| track["path"].as_str().unwrap().to_owned())
            .collect()
    }

    fn c_string_arg(value: impl AsRef<Path>) -> CString {
        CString::new(value.as_ref().to_string_lossy().into_owned()).unwrap()
    }

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
    }

    fn temp_db_path(prefix: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("player_ffi_{prefix}_{nonce}.sqlite3"))
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("player_ffi_{prefix}_{nonce}"))
    }

    fn write_test_wav(path: &Path, title: &[u8]) -> std::io::Result<()> {
        use std::io::Write;

        let sample_rate = 8_000_u32;
        let channels = 1_u16;
        let bits_per_sample = 16_u16;
        let sample_count = 800_u32;
        let block_align = channels * bits_per_sample / 8;
        let byte_rate = sample_rate * u32::from(block_align);
        let data_size = sample_count * u32::from(block_align);
        let title_padding = title.len() % 2;
        let list_payload_size = 4 + 8 + title.len() + title_padding;
        let list_padding = list_payload_size % 2;
        let list_size_with_padding = list_payload_size + list_padding;
        let riff_size = 4 + (8 + 16) + (8 + list_size_with_padding as u32) + (8 + data_size);

        let mut file = fs::File::create(path)?;
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
        file.write_all(b"LIST")?;
        file.write_all(&(list_payload_size as u32).to_le_bytes())?;
        file.write_all(b"INFO")?;
        file.write_all(b"INAM")?;
        file.write_all(&(title.len() as u32).to_le_bytes())?;
        file.write_all(title)?;
        if title_padding == 1 {
            file.write_all(&[0])?;
        }
        if list_padding == 1 {
            file.write_all(&[0])?;
        }
        file.write_all(b"data")?;
        file.write_all(&data_size.to_le_bytes())?;
        for index in 0..sample_count {
            let sample = if index % 2 == 0 { 900_i16 } else { -900_i16 };
            file.write_all(&sample.to_le_bytes())?;
        }
        Ok(())
    }
}
