use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

use errors::{PlayerError, PlayerResult};
use serde::Serialize;
use serde_json::Value;

use crate::{PlayerApp, TrackViewEditRequest};

/// Safe Rust access to the application service shared by the CLI and Apple FFI.
///
/// JSON values are retained at this public boundary because the CLI prints the same
/// response shapes consumed by the Apple clients. No C ABI allocation or string
/// conversion is involved.
pub struct SilentAppClient {
    app: Box<PlayerApp>,
}

#[derive(Debug)]
pub struct SilentAppClientError {
    message: String,
}

impl SilentAppClientError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SilentAppClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for SilentAppClientError {}

impl From<PlayerError> for SilentAppClientError {
    fn from(error: PlayerError) -> Self {
        Self::new(error.to_string())
    }
}

impl SilentAppClient {
    pub fn open(
        db_path: impl AsRef<Path>,
        media_root: impl AsRef<Path>,
    ) -> Result<Self, SilentAppClientError> {
        Ok(Self {
            app: Box::new(PlayerApp::new(
                db_path.as_ref().to_path_buf(),
                media_root.as_ref().to_path_buf(),
            )),
        })
    }

    pub fn export_library(&mut self, path: impl AsRef<Path>) -> ClientResult {
        self.call(|app| app.service_export_library(path.as_ref()))
    }

    pub fn import_library(&mut self, path: impl AsRef<Path>) -> ClientResult {
        self.call(|app| app.service_import_library(path.as_ref()))
    }

    pub fn zero_out_library(&mut self) -> ClientResult {
        self.call(PlayerApp::service_zero_out_library)
    }

    pub fn delete_from_library(&mut self, path: impl AsRef<Path>) -> ClientResult {
        self.call(|app| app.service_delete_from_library(path.as_ref()))
    }

    pub fn import_folder(&mut self, path: impl AsRef<Path>) -> ClientResult {
        self.call(|app| app.service_import_folder(path.as_ref()))
    }

    pub fn import_files(&mut self, paths: &[PathBuf]) -> ClientResult {
        self.call(|app| app.service_import_files(paths))
    }

    pub fn library(&mut self) -> ClientResult {
        self.call(PlayerApp::service_library)
    }

    pub fn library_page(&mut self, offset: usize, limit: usize) -> ClientResult {
        self.call(|app| app.service_library_page(offset, limit))
    }

    pub fn search(&mut self, query: &str, limit: usize) -> ClientResult {
        self.call(|app| app.service_search(query, limit))
    }

    pub fn search_playlist(&mut self, name: &str, query: &str, limit: usize) -> ClientResult {
        self.call(|app| app.service_search_playlist(name, query, limit))
    }

    pub fn analyze(&mut self) -> ClientResult {
        self.call(PlayerApp::service_analyze)
    }

    pub fn audit_database(&mut self) -> ClientResult {
        self.call(PlayerApp::service_audit_database)
    }

    pub fn user_data(&mut self) -> ClientResult {
        self.call(PlayerApp::service_user_data)
    }

    pub fn play_path(&mut self, path: impl AsRef<Path>) -> ClientResult {
        self.call(|app| app.service_play_path(path.as_ref()))
    }

    pub fn play_library(&mut self) -> ClientResult {
        self.call(PlayerApp::service_play_library)
    }

    pub fn play_queue(&mut self, paths: &[PathBuf], start_path: impl AsRef<Path>) -> ClientResult {
        self.call(|app| app.service_play_queue(paths, start_path.as_ref()))
    }

    pub fn play_playlist(
        &mut self,
        name: &str,
        start_path: Option<&Path>,
        shuffle: bool,
    ) -> ClientResult {
        self.call(|app| app.service_play_playlist(name, start_path, shuffle))
    }

    pub fn pause(&mut self) -> ClientResult {
        self.call(PlayerApp::service_pause)
    }

    pub fn resume(&mut self) -> ClientResult {
        self.call(PlayerApp::service_resume)
    }

    pub fn audio_interruption_began(&mut self) -> ClientResult {
        self.call(PlayerApp::service_audio_interruption_began)
    }

    pub fn audio_interruption_ended(&mut self, system_should_resume: bool) -> ClientResult {
        self.call(|app| app.service_audio_interruption_ended(system_should_resume))
    }

    pub fn audio_output_disconnected(&mut self) -> ClientResult {
        self.call(PlayerApp::service_audio_output_disconnected)
    }

    pub fn stop(&mut self) -> ClientResult {
        self.call(PlayerApp::service_stop)
    }

    pub fn next_track(&mut self) -> ClientResult {
        self.call(PlayerApp::service_next)
    }

    pub fn previous_track(&mut self) -> ClientResult {
        self.call(PlayerApp::service_previous)
    }

    pub fn seek(&mut self, position_ms: u64) -> ClientResult {
        self.call(|app| app.service_seek(position_ms))
    }

    pub fn poll(&mut self) -> ClientResult {
        self.call(PlayerApp::service_poll)
    }

    pub fn set_repeat_mode(&mut self, mode: &str) -> ClientResult {
        self.call(|app| app.service_set_repeat_mode(mode))
    }

    pub fn set_shuffle(&mut self, enabled: bool) -> ClientResult {
        self.call(|app| app.service_set_shuffle(enabled))
    }

    pub fn queue(&mut self) -> ClientResult {
        self.call(PlayerApp::service_queue)
    }

    pub fn queue_play_next(&mut self, path: impl AsRef<Path>) -> ClientResult {
        self.call(|app| app.service_queue_play_next(path.as_ref()))
    }

    pub fn queue_add(&mut self, path: impl AsRef<Path>) -> ClientResult {
        self.call(|app| app.service_queue_add(path.as_ref()))
    }

    pub fn queue_move(&mut self, from: usize, to: usize) -> ClientResult {
        self.call(|app| app.service_queue_move(from, to))
    }

    pub fn queue_remove(&mut self, index: usize) -> ClientResult {
        self.call(|app| app.service_queue_remove(index))
    }

    pub fn queue_clear(&mut self) -> ClientResult {
        self.call(PlayerApp::service_queue_clear)
    }

    pub fn track_details(&mut self, path: impl AsRef<Path>) -> ClientResult {
        self.call(|app| app.service_track_details(path.as_ref()))
    }

    pub fn edit_track_view(&mut self, path: impl AsRef<Path>, edit: &Value) -> ClientResult {
        let request: TrackViewEditRequest =
            serde_json::from_value(edit.clone()).map_err(|error| {
                SilentAppClientError::new(format!("invalid track view edit request: {error}"))
            })?;
        self.call(|app| app.service_edit_track_view(path.as_ref(), request))
    }

    pub fn set_track_notes(&mut self, path: impl AsRef<Path>, notes: &str) -> ClientResult {
        self.call(|app| app.service_set_track_notes(path.as_ref(), notes))
    }

    pub fn set_track_rating(&mut self, path: impl AsRef<Path>, rating: i32) -> ClientResult {
        self.call(|app| app.service_set_track_rating(path.as_ref(), rating))
    }

    pub fn set_track_metadata(
        &mut self,
        path: impl AsRef<Path>,
        title: &str,
        artist: &str,
        album: &str,
    ) -> ClientResult {
        self.call(|app| app.service_set_track_metadata(path.as_ref(), title, artist, album))
    }

    pub fn set_track_artwork(
        &mut self,
        path: impl AsRef<Path>,
        image_path: impl AsRef<Path>,
    ) -> ClientResult {
        self.call(|app| app.service_set_track_artwork(path.as_ref(), image_path.as_ref()))
    }

    pub fn set_album_artwork(
        &mut self,
        path: impl AsRef<Path>,
        image_path: impl AsRef<Path>,
    ) -> ClientResult {
        self.call(|app| app.service_set_album_artwork(path.as_ref(), image_path.as_ref()))
    }

    pub fn set_track_lyrics(
        &mut self,
        path: impl AsRef<Path>,
        lyrics_path: impl AsRef<Path>,
    ) -> ClientResult {
        self.call(|app| app.service_set_track_lyrics(path.as_ref(), lyrics_path.as_ref()))
    }

    pub fn export_track_view(
        &mut self,
        path: impl AsRef<Path>,
        destination: impl AsRef<Path>,
    ) -> ClientResult {
        self.call(|app| app.service_export_track_view(path.as_ref(), destination.as_ref()))
    }

    pub fn set_favorite(&mut self, path: impl AsRef<Path>, enabled: bool) -> ClientResult {
        self.call(|app| app.service_set_favorite(path.as_ref(), enabled))
    }

    pub fn favorites(&mut self) -> ClientResult {
        self.call(PlayerApp::service_favorites)
    }

    pub fn history(&mut self, limit: usize) -> ClientResult {
        self.call(|app| app.service_history(limit))
    }

    pub fn playlists(&mut self) -> ClientResult {
        self.call(PlayerApp::service_playlists)
    }

    pub fn recent_playlists(&mut self, limit: usize) -> ClientResult {
        self.call(|app| app.service_recent_playlists(limit))
    }

    pub fn create_playlist(&mut self, name: &str) -> ClientResult {
        self.call(|app| app.service_create_playlist(name))
    }

    pub fn rename_playlist(&mut self, old_name: &str, new_name: &str) -> ClientResult {
        self.call(|app| app.service_rename_playlist(old_name, new_name))
    }

    pub fn set_playlist_artwork(
        &mut self,
        name: &str,
        image_path: impl AsRef<Path>,
    ) -> ClientResult {
        self.call(|app| app.service_set_playlist_artwork(name, image_path.as_ref()))
    }

    pub fn delete_playlist(&mut self, name: &str) -> ClientResult {
        self.call(|app| app.service_delete_playlist(name))
    }

    pub fn clear_playlist(&mut self, name: &str) -> ClientResult {
        self.call(|app| app.service_clear_playlist(name))
    }

    pub fn add_to_playlist(&mut self, name: &str, path: impl AsRef<Path>) -> ClientResult {
        self.call(|app| app.service_add_to_playlist(name, path.as_ref()))
    }

    pub fn remove_from_playlist(&mut self, name: &str, path: impl AsRef<Path>) -> ClientResult {
        self.call(|app| app.service_remove_from_playlist(name, path.as_ref()))
    }

    pub fn move_playlist_track(
        &mut self,
        name: &str,
        path: impl AsRef<Path>,
        delta: i32,
    ) -> ClientResult {
        self.call(|app| app.service_move_playlist_track(name, path.as_ref(), delta))
    }

    pub fn sort_playlist(&mut self, name: &str, sort: &str) -> ClientResult {
        self.call(|app| app.service_sort_playlist(name, sort))
    }

    pub fn playlist_tracks(&mut self, name: &str) -> ClientResult {
        self.call(|app| app.service_playlist_tracks(name))
    }

    fn call<T: Serialize>(
        &mut self,
        operation: impl FnOnce(&mut PlayerApp) -> PlayerResult<T>,
    ) -> ClientResult {
        let data = operation(&mut self.app)?;
        serde_json::to_value(data).map_err(|error| SilentAppClientError::new(error.to_string()))
    }
}

impl Drop for SilentAppClient {
    fn drop(&mut self) {
        self.app.close();
    }
}

type ClientResult = Result<Value, SilentAppClientError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_client_returns_service_values_without_ffi_envelopes() {
        let root = std::env::temp_dir().join(format!("silent-app-client-{}", std::process::id()));
        let mut client = SilentAppClient::open(root.join("library.sqlite"), root.join("Music"))
            .expect("client should open");

        assert_eq!(
            client.library().expect("library response"),
            Value::Array(vec![])
        );
        drop(client);
        std::fs::remove_dir_all(root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn safe_client_accepts_non_utf8_paths() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let root =
            std::env::temp_dir().join(format!("silent-app-client-non-utf8-{}", std::process::id()));
        let db_path = root.join(OsString::from_vec(b"library-\xff.sqlite".to_vec()));
        let media_root = root.join(OsString::from_vec(b"music-\xfe".to_vec()));
        let client = SilentAppClient::open(db_path, media_root);

        assert!(client.is_ok());
        drop(client);
        std::fs::remove_dir_all(root).ok();
    }
}
