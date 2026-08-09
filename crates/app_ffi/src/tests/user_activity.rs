use super::*;

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
    }

    let edited = unsafe {
        call_json(player_app_set_track_metadata(
            app,
            c_string_arg(&track_path).as_ptr(),
            c_string_arg("Edited Summary Song").as_ptr(),
            c_string_arg("Edited Artist").as_ptr(),
            c_string_arg("Edited Album").as_ptr(),
        ))
    };
    assert_ok(&edited);

    unsafe {
        let app = &mut *app;
        app.position_ms = 90_000;
        app.observe_active_position(90_000);
        app.finish_active_session("stopped").unwrap();
    }

    let history_text = fs::read_to_string(&history_path).unwrap();
    let history_lines = history_text.lines().collect::<Vec<_>>();
    assert_eq!(history_lines.len(), 1);
    let event: Value = serde_json::from_str(history_lines[0]).unwrap();
    assert_eq!(event["record_type"], "playback_session");
    assert_eq!(event["track"]["title"], "Edited Summary Song");
    assert_eq!(event["track"]["artist"], "Edited Artist");
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
    assert_eq!(sqlite_history[0].track.title, "Edited Summary Song");
    assert_eq!(sqlite_history[0].position_ms, 90_000);
    assert!(!sqlite_history[0].completed);

    unsafe { player_app_destroy(app) };
    fs::remove_dir_all(db_dir).ok();
    fs::remove_dir_all(media_root).ok();
}
