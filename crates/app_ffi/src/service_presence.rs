use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use discord_presence::{
    discord_desktop_available, DiscordPresence, ListeningActivity, PresenceTrack,
};
use errors::{PlayerError, PlayerResult};
use serde::Serialize;

use crate::PlayerApp;

#[derive(Serialize)]
pub(crate) struct DiscordPresenceStatus {
    enabled: bool,
    discord_running: bool,
    sharing_track: bool,
}

impl PlayerApp {
    pub(crate) fn service_configure_discord_presence(
        &mut self,
        application_id: Option<&str>,
    ) -> PlayerResult<DiscordPresenceStatus> {
        self.discord_presence = application_id
            .map(DiscordPresence::new)
            .transpose()
            .map_err(discord_error)?;
        Ok(self.discord_presence_status())
    }

    pub(crate) fn service_sync_discord_presence(&mut self) -> PlayerResult<DiscordPresenceStatus> {
        let activity = self.discord_listening_activity()?;
        let Some(presence) = self.discord_presence.as_mut() else {
            return Ok(self.discord_presence_status());
        };
        match activity {
            Some(activity) => presence.update(activity).map_err(discord_error)?,
            None => presence.clear().map_err(discord_error)?,
        }
        Ok(self.discord_presence_status())
    }

    pub(crate) fn service_test_discord_presence(&mut self) -> PlayerResult<DiscordPresenceStatus> {
        let Some(presence) = self.discord_presence.as_mut() else {
            return Err(PlayerError::invalid_input(
                "Discord Rich Presence is not configured",
            ));
        };
        presence.connect().map_err(discord_error)?;
        Ok(self.discord_presence_status())
    }

    fn discord_listening_activity(&self) -> PlayerResult<Option<ListeningActivity>> {
        if !self.is_playing {
            return Ok(None);
        }
        let Some(track) = self.current_track.as_ref() else {
            return Ok(None);
        };
        let artwork_public_url = self
            .store()?
            .track_artwork_public_url(Path::new(&track.path))?;
        Ok(Some(ListeningActivity::from_track(
            PresenceTrack {
                title: &track.title,
                artist: track.artist.as_deref(),
                album: track.album.as_deref(),
                duration_ms: track.duration_ms,
                artwork_public_url: artwork_public_url.as_deref(),
            },
            self.position_ms,
            unix_time_ms(),
        )))
    }

    fn discord_presence_status(&self) -> DiscordPresenceStatus {
        DiscordPresenceStatus {
            enabled: self.discord_presence.is_some(),
            discord_running: discord_desktop_available(),
            sharing_track: self.discord_presence.is_some()
                && self.is_playing
                && self.current_track.is_some(),
        }
    }
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn discord_error(error: impl std::fmt::Display) -> PlayerError {
    PlayerError::engine(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn disabled_presence_is_a_noop() {
        let (mut app, root) = temporary_app("disabled");
        let status = app.service_sync_discord_presence().unwrap();
        assert!(!status.enabled);
        assert!(!status.sharing_track);
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn empty_application_id_is_rejected() {
        let (mut app, root) = temporary_app("empty_id");
        let error = app
            .service_configure_discord_presence(Some("  "))
            .err()
            .expect("empty ID should fail");
        assert!(error.to_string().contains("application ID is empty"));
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    fn temporary_app(name: &str) -> (PlayerApp, PathBuf) {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("silent_presence_{name}_{nonce}"));
        fs::create_dir_all(root.join("media")).unwrap();
        let app = PlayerApp::new(root.join("library.sqlite3"), root.join("media"));
        (app, root)
    }
}
