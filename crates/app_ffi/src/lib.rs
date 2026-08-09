mod activity_runtime;
mod app_runtime;
mod audit;
mod client;
mod dto;
mod ffi;
mod ffi_support;
mod file_support;
mod library_runtime;
mod playback_helpers;
mod playback_runtime;
mod service_library;
mod service_playback;
mod service_playlists;
mod service_tracks;
mod support;
mod track_runtime;
mod user_activity;

pub use client::{SilentAppClient, SilentAppClientError};

use std::path::PathBuf;

use domain::{PlaybackLifecycle, RepeatMode};
use engine::PlayerEngine;

use dto::{ActivePlaybackSession, LocalUserProfile, TrackDto, UserActivityStore};

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
    queue_playback_order: Vec<usize>,
    queue_playback_position: Option<usize>,
    repeat_mode: RepeatMode,
    shuffle_enabled: bool,
    is_playing: bool,
    position_ms: u64,
    last_persisted_queue_position_ms: u64,
    last_persisted_queue_index: Option<usize>,
    gain_db: Option<f32>,
    loudness_status: Option<String>,
    last_error: Option<String>,
    playback_lifecycle: PlaybackLifecycle,
}

const LIBRARY_PACKAGE_FORMAT_VERSION: u32 = 1;
const LIBRARY_PACKAGE_DATABASE_FILE: &str = "player_library.sqlite3";
const LIBRARY_PACKAGE_MANIFEST_FILE: &str = "manifest.json";
const LIBRARY_PACKAGE_MUSIC_DIRECTORY: &str = "Music";

#[cfg(test)]
mod tests;
