use std::path::Path;

use errors::PlayerResult;

use crate::dto::{ActivePlaybackSession, LocalUserProfile, TrackDto};
use crate::support::{new_session_id, now_unix_seconds};
use crate::PlayerApp;

impl PlayerApp {
    pub(crate) fn local_user(&mut self) -> PlayerResult<&LocalUserProfile> {
        if self.local_user.is_none() {
            self.local_user = Some(self.activity_store.load_or_create_profile()?);
        }
        Ok(self
            .local_user
            .as_ref()
            .expect("local user just initialized"))
    }

    pub(crate) fn start_active_session(&mut self, track: TrackDto, position_ms: u64) {
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

    pub(crate) fn observe_active_position(&mut self, position_ms: u64) {
        if let Some(session) = &mut self.active_session {
            session.observe_position(position_ms, self.is_playing);
        }
    }

    pub(crate) fn finish_active_session(&mut self, finish_reason: &str) -> PlayerResult<()> {
        self.observe_active_position(self.position_ms);
        let Some(session) = self.active_session.take() else {
            return Ok(());
        };
        let user = self.local_user()?.clone();
        let record = session.into_record(&user.user_id, finish_reason, now_unix_seconds());
        self.activity_store.append_playback(&record)?;
        self.store()?.record_playback(
            Path::new(&record.track.path),
            record.end_position_ms,
            record.completed,
        )?;
        Ok(())
    }
}
