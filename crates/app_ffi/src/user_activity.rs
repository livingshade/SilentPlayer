use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use errors::{PlayerError, PlayerResult};

use crate::dto::{
    ActivePlaybackSession, LocalUserProfile, PlaybackHistoryRecord, PlaybackTrackRecord,
    UserActivityStore,
};
use crate::support::{new_local_user_id, now_unix_seconds};

impl UserActivityStore {
    pub(crate) fn for_db(db_path: &Path) -> Self {
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

    pub(crate) fn load_or_create_profile(&self) -> PlayerResult<LocalUserProfile> {
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

    pub(crate) fn append_playback(&self, record: &PlaybackHistoryRecord) -> PlayerResult<()> {
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
    pub(crate) fn observe_position(&mut self, position_ms: u64, is_playing: bool) {
        if is_playing && position_ms >= self.last_position_ms {
            self.listened_ms = self
                .listened_ms
                .saturating_add(position_ms - self.last_position_ms);
        }
        self.last_position_ms = position_ms;
    }

    pub(crate) fn into_record(
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
