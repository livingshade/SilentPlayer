use std::path::PathBuf;

use domain::{PlaybackLifecycle, RepeatMode};

use crate::dto::UserActivityStore;
use crate::PlayerApp;

impl PlayerApp {
    pub(crate) fn new(db_path: PathBuf, media_root: PathBuf) -> Self {
        let activity_store = UserActivityStore::for_db(&db_path);
        let (local_user, startup_error) = match activity_store.load_or_create_profile() {
            Ok(profile) => (Some(profile), None),
            Err(error) => (None, Some(error.to_string())),
        };
        let mut app = PlayerApp {
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
            queue_playback_order: Vec::new(),
            queue_playback_position: None,
            repeat_mode: RepeatMode::Off,
            shuffle_enabled: false,
            is_playing: false,
            position_ms: 0,
            last_persisted_queue_position_ms: 0,
            last_persisted_queue_index: None,
            gain_db: None,
            loudness_status: None,
            last_error: startup_error,
            playback_lifecycle: PlaybackLifecycle::default(),
        };
        if let Err(error) = app.restore_persisted_queue() {
            app.record_nonfatal_error(error);
        }
        app
    }

    pub(crate) fn close(&mut self) {
        self.poll_events();
        if let Err(error) = self.persist_queue_state() {
            eprintln!("failed to persist playback queue during shutdown: {error}");
        }
        if let Err(error) = self.finish_active_session("app_destroy") {
            eprintln!("failed to finish playback session during shutdown: {error}");
        }
    }

    pub(crate) fn record_nonfatal_error(&mut self, error: impl std::fmt::Display) {
        self.last_error = Some(error.to_string());
    }

    pub(crate) fn finish_active_session_best_effort(&mut self, finish_reason: &str) {
        if let Err(error) = self.finish_active_session(finish_reason) {
            self.record_nonfatal_error(error);
        }
    }
}
