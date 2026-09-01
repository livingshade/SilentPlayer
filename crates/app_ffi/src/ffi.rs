#![allow(clippy::missing_safety_doc, clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::{c_char, CString};
use std::path::PathBuf;
use std::ptr;

use errors::PlayerError;

use crate::dto::TrackViewEditRequest;
use crate::ffi_support::{app_mut, c_string, ffi_result};
use crate::PlayerApp;

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
    Box::into_raw(Box::new(PlayerApp::new(db_path, media_root)))
}

#[no_mangle]
pub unsafe extern "C" fn player_app_destroy(app: *mut PlayerApp) {
    if app.is_null() {
        return;
    }
    let mut app = Box::from_raw(app);
    app.close();
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
        app.service_export_library(&package_path)
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
        app.service_import_library(&package_path)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_zero_out_library(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_zero_out_library()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_delete_from_library(
    app: *mut PlayerApp,
    path: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let path = PathBuf::from(c_string(path)?);
        app.service_delete_from_library(&path)
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
        app.service_import_folder(&folder)
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
        let paths = paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
        app.service_import_files(&paths)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_library(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_library()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_library_page(
    app: *mut PlayerApp,
    offset: usize,
    limit: usize,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_library_page(offset, limit)
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
        app.service_search(&query, limit)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_search_playlist(
    app: *mut PlayerApp,
    name: *const c_char,
    query: *const c_char,
    limit: usize,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let name = c_string(name)?;
        let query = c_string(query)?;
        app.service_search_playlist(&name, &query, limit)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_analyze(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_analyze()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_audit_database(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_audit_database()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_user_data(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_user_data()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_play_library(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_play_library()
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
        app.service_play_path(&path)
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
        let start_path = PathBuf::from(c_string(start_path)?);
        let paths = paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
        app.service_play_queue(&paths, &start_path)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_play_playlist(
    app: *mut PlayerApp,
    name: *const c_char,
    start_path: *const c_char,
    shuffle: bool,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let name = c_string(name)?;
        let start_path = if start_path.is_null() {
            None
        } else {
            Some(PathBuf::from(c_string(start_path)?))
        };
        app.service_play_playlist(&name, start_path.as_deref(), shuffle)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_pause(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_pause()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_resume(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_resume()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_audio_interruption_began(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_audio_interruption_began()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_audio_interruption_ended(
    app: *mut PlayerApp,
    system_should_resume: bool,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_audio_interruption_ended(system_should_resume)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_audio_output_disconnected(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_audio_output_disconnected()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_stop(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_stop()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_next(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_next()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_previous(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_previous()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_seek(app: *mut PlayerApp, position_ms: u64) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_seek(position_ms)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_poll(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_poll()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_discord_presence_configure(
    app: *mut PlayerApp,
    application_id: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let application_id = c_string(application_id)?;
        app.service_configure_discord_presence(Some(&application_id))
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_discord_presence_disable(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_configure_discord_presence(None)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_discord_presence_sync(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_sync_discord_presence()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_discord_presence_test(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_test_discord_presence()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_map_public_artwork_urls(
    app: *mut PlayerApp,
    public_url_prefix: *const c_char,
    export_directory: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let public_url_prefix = c_string(public_url_prefix)?;
        let export_directory = PathBuf::from(c_string(export_directory)?);
        app.service_map_public_artwork_urls(&public_url_prefix, &export_directory)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_set_repeat_mode(
    app: *mut PlayerApp,
    repeat_mode: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let repeat_mode = c_string(repeat_mode)?;
        app.service_set_repeat_mode(&repeat_mode)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_set_shuffle(app: *mut PlayerApp, enabled: bool) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_set_shuffle(enabled)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_set_playback_mode(
    app: *mut PlayerApp,
    playback_mode: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let playback_mode = c_string(playback_mode)?;
        app.service_set_playback_mode(&playback_mode)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_queue(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_queue()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_queue_play_next(
    app: *mut PlayerApp,
    path: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let path = PathBuf::from(c_string(path)?);
        app.service_queue_play_next(&path)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_queue_play(app: *mut PlayerApp, index: usize) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_queue_play(index)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_queue_add(
    app: *mut PlayerApp,
    path: *const c_char,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        let path = PathBuf::from(c_string(path)?);
        app.service_queue_add(&path)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_queue_move(
    app: *mut PlayerApp,
    from: usize,
    to: usize,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_queue_move(from, to)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_queue_remove(app: *mut PlayerApp, index: usize) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_queue_remove(index)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_queue_clear(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_queue_clear()
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
        app.service_track_details(&path)
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
        app.service_edit_track_view(&path, request)
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
        app.service_set_track_notes(&path, &notes)
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
        app.service_set_track_rating(&path, rating)
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
        app.service_set_track_metadata(&path, &title, &artist, &album)
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
        app.service_set_track_artwork(&path, &image_path)
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
        app.service_set_album_artwork(&path, &image_path)
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
        app.service_set_track_lyrics(&path, &lyrics_path)
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
        app.service_export_track_view(&path, &destination)
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
        let path = PathBuf::from(c_string(path)?);
        app.service_set_favorite(&path, enabled)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_favorites(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_favorites()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_history(app: *mut PlayerApp, limit: usize) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_history(limit)
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_playlists(app: *mut PlayerApp) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_playlists()
    })
}

#[no_mangle]
pub unsafe extern "C" fn player_app_recent_playlists(
    app: *mut PlayerApp,
    limit: usize,
) -> *mut c_char {
    ffi_result(|| {
        let app = app_mut(app)?;
        app.service_recent_playlists(limit)
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
        app.service_create_playlist(&name)
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
        app.service_rename_playlist(&old_name, &new_name)
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
        app.service_set_playlist_artwork(&name, &image_path)
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
        app.service_delete_playlist(&name)
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
        app.service_clear_playlist(&name)
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
        let path = PathBuf::from(c_string(path)?);
        app.service_add_to_playlist(&name, &path)
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
        let path = PathBuf::from(c_string(path)?);
        app.service_remove_from_playlist(&name, &path)
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
        let path = PathBuf::from(c_string(path)?);
        app.service_move_playlist_track(&name, &path, delta)
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
        let sort = c_string(sort)?;
        app.service_sort_playlist(&name, &sort)
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
        app.service_playlist_tracks(&name)
    })
}
