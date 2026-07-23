use std::error::Error;
use std::ffi::{CStr, CString, NulError};
use std::fmt;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;

use serde_json::Value;

use crate::{
    player_app_add_to_playlist, player_app_analyze, player_app_audio_interruption_began,
    player_app_audio_interruption_ended, player_app_audio_output_disconnected,
    player_app_audit_database, player_app_clear_playlist, player_app_create,
    player_app_create_playlist, player_app_delete_playlist, player_app_destroy,
    player_app_edit_track_view, player_app_export_library, player_app_export_track_view,
    player_app_favorites, player_app_history, player_app_import_files, player_app_import_folder,
    player_app_import_library, player_app_library, player_app_move_playlist_track, player_app_next,
    player_app_pause, player_app_play_path, player_app_play_queue, player_app_playlist_tracks,
    player_app_playlists, player_app_poll, player_app_previous, player_app_queue,
    player_app_remove_from_playlist, player_app_rename_playlist, player_app_resume,
    player_app_search, player_app_seek, player_app_set_album_artwork, player_app_set_favorite,
    player_app_set_playlist_artwork, player_app_set_repeat_mode, player_app_set_shuffle,
    player_app_set_track_artwork, player_app_set_track_lyrics, player_app_set_track_metadata,
    player_app_set_track_notes, player_app_set_track_rating, player_app_sort_playlist,
    player_app_stop, player_app_track_details, player_app_user_data, player_app_zero_out_library,
    player_string_free, PlayerApp,
};

/// Safe, typed-lifetime owner for the application service exposed by this crate.
///
/// The Apple FFI and the generic CLI both use the same `PlayerApp` implementation. The
/// client intentionally returns JSON values because those values are also the stable wire
/// contract consumed by Swift.
pub struct SilentAppClient {
    app: NonNull<PlayerApp>,
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

impl From<NulError> for SilentAppClientError {
    fn from(error: NulError) -> Self {
        Self::new(format!("path or argument contains a NUL byte: {error}"))
    }
}

impl SilentAppClient {
    pub fn open(
        db_path: impl AsRef<Path>,
        media_root: impl AsRef<Path>,
    ) -> Result<Self, SilentAppClientError> {
        let db_path = c_path(db_path.as_ref())?;
        let media_root = c_path(media_root.as_ref())?;
        let app = unsafe { player_app_create(db_path.as_ptr(), media_root.as_ptr()) };
        let app = NonNull::new(app)
            .ok_or_else(|| SilentAppClientError::new("unable to create Silent application"))?;
        Ok(Self { app })
    }

    pub fn export_library(&mut self, path: impl AsRef<Path>) -> ClientResult {
        let path = c_path(path.as_ref())?;
        self.call(|app| unsafe { player_app_export_library(app, path.as_ptr()) })
    }

    pub fn import_library(&mut self, path: impl AsRef<Path>) -> ClientResult {
        let path = c_path(path.as_ref())?;
        self.call(|app| unsafe { player_app_import_library(app, path.as_ptr()) })
    }

    pub fn zero_out_library(&mut self) -> ClientResult {
        self.call(|app| unsafe { player_app_zero_out_library(app) })
    }

    pub fn import_folder(&mut self, path: impl AsRef<Path>) -> ClientResult {
        let path = c_path(path.as_ref())?;
        self.call(|app| unsafe { player_app_import_folder(app, path.as_ptr()) })
    }

    pub fn import_files(&mut self, paths: &[PathBuf]) -> ClientResult {
        let paths = paths
            .iter()
            .map(|path| path_text(path))
            .collect::<Result<Vec<_>, _>>()?;
        let paths = CString::new(
            serde_json::to_string(&paths)
                .map_err(|error| SilentAppClientError::new(error.to_string()))?,
        )?;
        self.call(|app| unsafe { player_app_import_files(app, paths.as_ptr()) })
    }

    pub fn library(&mut self) -> ClientResult {
        self.call(|app| unsafe { player_app_library(app) })
    }

    pub fn search(&mut self, query: &str, limit: usize) -> ClientResult {
        let query = CString::new(query)?;
        self.call(|app| unsafe { player_app_search(app, query.as_ptr(), limit) })
    }

    pub fn analyze(&mut self) -> ClientResult {
        self.call(|app| unsafe { player_app_analyze(app) })
    }

    pub fn audit_database(&mut self) -> ClientResult {
        self.call(|app| unsafe { player_app_audit_database(app) })
    }

    pub fn user_data(&mut self) -> ClientResult {
        self.call(|app| unsafe { player_app_user_data(app) })
    }

    pub fn play_path(&mut self, path: impl AsRef<Path>) -> ClientResult {
        let path = c_path(path.as_ref())?;
        self.call(|app| unsafe { player_app_play_path(app, path.as_ptr()) })
    }

    pub fn play_queue(&mut self, paths: &[PathBuf], start_path: impl AsRef<Path>) -> ClientResult {
        let paths = paths
            .iter()
            .map(|path| path_text(path))
            .collect::<Result<Vec<_>, _>>()?;
        let paths = CString::new(
            serde_json::to_string(&paths)
                .map_err(|error| SilentAppClientError::new(error.to_string()))?,
        )?;
        let start_path = c_path(start_path.as_ref())?;
        self.call(|app| unsafe { player_app_play_queue(app, paths.as_ptr(), start_path.as_ptr()) })
    }

    pub fn pause(&mut self) -> ClientResult {
        self.call(|app| unsafe { player_app_pause(app) })
    }

    pub fn resume(&mut self) -> ClientResult {
        self.call(|app| unsafe { player_app_resume(app) })
    }

    pub fn audio_interruption_began(&mut self) -> ClientResult {
        self.call(|app| unsafe { player_app_audio_interruption_began(app) })
    }

    pub fn audio_interruption_ended(&mut self, system_should_resume: bool) -> ClientResult {
        self.call(|app| unsafe { player_app_audio_interruption_ended(app, system_should_resume) })
    }

    pub fn audio_output_disconnected(&mut self) -> ClientResult {
        self.call(|app| unsafe { player_app_audio_output_disconnected(app) })
    }

    pub fn stop(&mut self) -> ClientResult {
        self.call(|app| unsafe { player_app_stop(app) })
    }

    pub fn next_track(&mut self) -> ClientResult {
        self.call(|app| unsafe { player_app_next(app) })
    }

    pub fn previous_track(&mut self) -> ClientResult {
        self.call(|app| unsafe { player_app_previous(app) })
    }

    pub fn seek(&mut self, position_ms: u64) -> ClientResult {
        self.call(|app| unsafe { player_app_seek(app, position_ms) })
    }

    pub fn poll(&mut self) -> ClientResult {
        self.call(|app| unsafe { player_app_poll(app) })
    }

    pub fn set_repeat_mode(&mut self, mode: &str) -> ClientResult {
        let mode = CString::new(mode)?;
        self.call(|app| unsafe { player_app_set_repeat_mode(app, mode.as_ptr()) })
    }

    pub fn set_shuffle(&mut self, enabled: bool) -> ClientResult {
        self.call(|app| unsafe { player_app_set_shuffle(app, enabled) })
    }

    pub fn queue(&mut self) -> ClientResult {
        self.call(|app| unsafe { player_app_queue(app) })
    }

    pub fn track_details(&mut self, path: impl AsRef<Path>) -> ClientResult {
        let path = c_path(path.as_ref())?;
        self.call(|app| unsafe { player_app_track_details(app, path.as_ptr()) })
    }

    pub fn edit_track_view(&mut self, path: impl AsRef<Path>, edit: &Value) -> ClientResult {
        let path = c_path(path.as_ref())?;
        let edit = CString::new(
            serde_json::to_string(edit)
                .map_err(|error| SilentAppClientError::new(error.to_string()))?,
        )?;
        self.call(|app| unsafe { player_app_edit_track_view(app, path.as_ptr(), edit.as_ptr()) })
    }

    pub fn set_track_notes(&mut self, path: impl AsRef<Path>, notes: &str) -> ClientResult {
        let path = c_path(path.as_ref())?;
        let notes = CString::new(notes)?;
        self.call(|app| unsafe { player_app_set_track_notes(app, path.as_ptr(), notes.as_ptr()) })
    }

    pub fn set_track_rating(&mut self, path: impl AsRef<Path>, rating: i32) -> ClientResult {
        let path = c_path(path.as_ref())?;
        self.call(|app| unsafe { player_app_set_track_rating(app, path.as_ptr(), rating) })
    }

    pub fn set_track_metadata(
        &mut self,
        path: impl AsRef<Path>,
        title: &str,
        artist: &str,
        album: &str,
    ) -> ClientResult {
        let path = c_path(path.as_ref())?;
        let title = CString::new(title)?;
        let artist = CString::new(artist)?;
        let album = CString::new(album)?;
        self.call(|app| unsafe {
            player_app_set_track_metadata(
                app,
                path.as_ptr(),
                title.as_ptr(),
                artist.as_ptr(),
                album.as_ptr(),
            )
        })
    }

    pub fn set_track_artwork(
        &mut self,
        path: impl AsRef<Path>,
        image_path: impl AsRef<Path>,
    ) -> ClientResult {
        let path = c_path(path.as_ref())?;
        let image_path = c_path(image_path.as_ref())?;
        self.call(|app| unsafe {
            player_app_set_track_artwork(app, path.as_ptr(), image_path.as_ptr())
        })
    }

    pub fn set_album_artwork(
        &mut self,
        path: impl AsRef<Path>,
        image_path: impl AsRef<Path>,
    ) -> ClientResult {
        let path = c_path(path.as_ref())?;
        let image_path = c_path(image_path.as_ref())?;
        self.call(|app| unsafe {
            player_app_set_album_artwork(app, path.as_ptr(), image_path.as_ptr())
        })
    }

    pub fn set_track_lyrics(
        &mut self,
        path: impl AsRef<Path>,
        lyrics_path: impl AsRef<Path>,
    ) -> ClientResult {
        let path = c_path(path.as_ref())?;
        let lyrics_path = c_path(lyrics_path.as_ref())?;
        self.call(|app| unsafe {
            player_app_set_track_lyrics(app, path.as_ptr(), lyrics_path.as_ptr())
        })
    }

    pub fn export_track_view(
        &mut self,
        path: impl AsRef<Path>,
        destination: impl AsRef<Path>,
    ) -> ClientResult {
        let path = c_path(path.as_ref())?;
        let destination = c_path(destination.as_ref())?;
        self.call(|app| unsafe {
            player_app_export_track_view(app, path.as_ptr(), destination.as_ptr())
        })
    }

    pub fn set_favorite(&mut self, path: impl AsRef<Path>, enabled: bool) -> ClientResult {
        let path = c_path(path.as_ref())?;
        self.call(|app| unsafe { player_app_set_favorite(app, path.as_ptr(), enabled) })
    }

    pub fn favorites(&mut self) -> ClientResult {
        self.call(|app| unsafe { player_app_favorites(app) })
    }

    pub fn history(&mut self, limit: usize) -> ClientResult {
        self.call(|app| unsafe { player_app_history(app, limit) })
    }

    pub fn playlists(&mut self) -> ClientResult {
        self.call(|app| unsafe { player_app_playlists(app) })
    }

    pub fn create_playlist(&mut self, name: &str) -> ClientResult {
        let name = CString::new(name)?;
        self.call(|app| unsafe { player_app_create_playlist(app, name.as_ptr()) })
    }

    pub fn rename_playlist(&mut self, old_name: &str, new_name: &str) -> ClientResult {
        let old_name = CString::new(old_name)?;
        let new_name = CString::new(new_name)?;
        self.call(|app| unsafe {
            player_app_rename_playlist(app, old_name.as_ptr(), new_name.as_ptr())
        })
    }

    pub fn set_playlist_artwork(
        &mut self,
        name: &str,
        image_path: impl AsRef<Path>,
    ) -> ClientResult {
        let name = CString::new(name)?;
        let image_path = c_path(image_path.as_ref())?;
        self.call(|app| unsafe {
            player_app_set_playlist_artwork(app, name.as_ptr(), image_path.as_ptr())
        })
    }

    pub fn delete_playlist(&mut self, name: &str) -> ClientResult {
        let name = CString::new(name)?;
        self.call(|app| unsafe { player_app_delete_playlist(app, name.as_ptr()) })
    }

    pub fn clear_playlist(&mut self, name: &str) -> ClientResult {
        let name = CString::new(name)?;
        self.call(|app| unsafe { player_app_clear_playlist(app, name.as_ptr()) })
    }

    pub fn add_to_playlist(&mut self, name: &str, path: impl AsRef<Path>) -> ClientResult {
        let name = CString::new(name)?;
        let path = c_path(path.as_ref())?;
        self.call(|app| unsafe { player_app_add_to_playlist(app, name.as_ptr(), path.as_ptr()) })
    }

    pub fn remove_from_playlist(&mut self, name: &str, path: impl AsRef<Path>) -> ClientResult {
        let name = CString::new(name)?;
        let path = c_path(path.as_ref())?;
        self.call(|app| unsafe {
            player_app_remove_from_playlist(app, name.as_ptr(), path.as_ptr())
        })
    }

    pub fn move_playlist_track(
        &mut self,
        name: &str,
        path: impl AsRef<Path>,
        delta: i32,
    ) -> ClientResult {
        let name = CString::new(name)?;
        let path = c_path(path.as_ref())?;
        self.call(|app| unsafe {
            player_app_move_playlist_track(app, name.as_ptr(), path.as_ptr(), delta)
        })
    }

    pub fn sort_playlist(&mut self, name: &str, sort: &str) -> ClientResult {
        let name = CString::new(name)?;
        let sort = CString::new(sort)?;
        self.call(|app| unsafe { player_app_sort_playlist(app, name.as_ptr(), sort.as_ptr()) })
    }

    pub fn playlist_tracks(&mut self, name: &str) -> ClientResult {
        let name = CString::new(name)?;
        self.call(|app| unsafe { player_app_playlist_tracks(app, name.as_ptr()) })
    }

    fn call(
        &mut self,
        operation: impl FnOnce(*mut PlayerApp) -> *mut std::ffi::c_char,
    ) -> ClientResult {
        let response = operation(self.app.as_ptr());
        decode_response(response)
    }
}

impl Drop for SilentAppClient {
    fn drop(&mut self) {
        unsafe {
            player_app_destroy(self.app.as_ptr());
        }
    }
}

type ClientResult = Result<Value, SilentAppClientError>;

fn c_path(path: &Path) -> Result<CString, SilentAppClientError> {
    CString::new(path_text(path)?).map_err(Into::into)
}

fn path_text(path: &Path) -> Result<&str, SilentAppClientError> {
    path.to_str()
        .ok_or_else(|| SilentAppClientError::new(format!("path is not valid UTF-8: {path:?}")))
}

fn decode_response(response: *mut std::ffi::c_char) -> Result<Value, SilentAppClientError> {
    if response.is_null() {
        return Err(SilentAppClientError::new(
            "Silent application returned a null response",
        ));
    }
    let json = unsafe {
        let json = CStr::from_ptr(response).to_str().map(ToOwned::to_owned);
        player_string_free(response);
        json
    }
    .map_err(|error| {
        SilentAppClientError::new(format!("app response is not valid UTF-8: {error}"))
    })?;
    let mut response: Value = serde_json::from_str(&json)
        .map_err(|error| SilentAppClientError::new(format!("invalid app response: {error}")))?;
    let ok = response.get("ok").and_then(Value::as_bool).ok_or_else(|| {
        SilentAppClientError::new("Silent application response has no boolean `ok` field")
    })?;
    if !ok {
        let error = response
            .get("error")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                SilentAppClientError::new(
                    "Silent application failure response has no string `error` field",
                )
            })?;
        return Err(SilentAppClientError::new(error));
    }
    response
        .get_mut("data")
        .map(Value::take)
        .ok_or_else(|| SilentAppClientError::new("Silent application returned no data"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(json: &str) -> Result<Value, SilentAppClientError> {
        let response = CString::new(json).unwrap().into_raw();
        decode_response(response)
    }

    #[test]
    fn response_envelope_is_strict() {
        assert_eq!(
            decode(r#"{"ok":true,"data":{"value":1},"error":null}"#).unwrap()["value"],
            1
        );
        assert!(decode(r#"{"data":null,"error":null}"#)
            .unwrap_err()
            .to_string()
            .contains("boolean `ok`"));
        assert!(decode(r#"{"ok":false,"data":null}"#)
            .unwrap_err()
            .to_string()
            .contains("string `error`"));
        assert!(decode(r#"{"ok":true,"error":null}"#)
            .unwrap_err()
            .to_string()
            .contains("no data"));

        let invalid_utf8 =
            unsafe { CString::from_vec_with_nul_unchecked(vec![0xff, 0]).into_raw() };
        assert!(decode_response(invalid_utf8)
            .unwrap_err()
            .to_string()
            .contains("not valid UTF-8"));
    }
}
