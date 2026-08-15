use super::*;

#[test]
fn stopping_an_initialized_engine_without_a_track_is_idempotent() {
    let db_path = temp_db_path("stop_without_track");
    let media_root = temp_dir("stop_without_track_media");
    let app = create_app(&db_path, &media_root);
    let engine =
        PlayerEngine::spawn(NormalizationSettings::default(), || Ok(UnloadedBackend)).unwrap();
    unsafe {
        (*app).engine = Some(engine);
    }

    let stopped = unsafe { call_json(player_app_stop(app)) };
    assert_ok(&stopped);
    assert_eq!(stopped["data"]["is_playing"], false);
    assert!(stopped["data"]["current_track"].is_null());
    assert!(unsafe { (*app).engine.is_none() });

    unsafe { player_app_destroy(app) };
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn stopping_active_playback_releases_the_engine() {
    let db_path = temp_db_path("stop_active_engine");
    let media_root = temp_dir("stop_active_engine_media");
    fs::create_dir_all(&media_root).unwrap();
    let app = create_app(&db_path, &media_root);
    let mut track = Track::from_path(media_root.join("active.ogg"));
    track.title = "Active".to_owned();
    track.set_primary_audio_hash("active-stop-hash");
    LibraryStore::open(&db_path)
        .unwrap()
        .upsert_track(&track)
        .unwrap();

    let engine =
        PlayerEngine::spawn(NormalizationSettings::default(), || Ok(LoadedBackend)).unwrap();
    engine
        .play_queue(vec![track.clone()], 0, RepeatMode::Off, false, false)
        .unwrap();
    unsafe {
        (*app).queue_tracks = track_dtos(std::slice::from_ref(&track)).unwrap();
        (*app).engine = Some(engine);
        (*app).poll_events();
        assert!((*app).current_track.is_some());
    }

    let stopped = unsafe { call_json(player_app_stop(app)) };
    assert_ok(&stopped);
    assert_eq!(stopped["data"]["is_playing"], false);
    assert!(stopped["data"]["current_track"].is_null());
    assert!(unsafe { (*app).engine.is_none() });

    unsafe { player_app_destroy(app) };
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn clicking_a_track_appends_once_then_jumps_to_its_global_queue_item() {
    let db_path = temp_db_path("play_path_global_queue");
    let media_root = temp_dir("play_path_global_queue_media");
    fs::create_dir_all(&media_root).unwrap();
    let first_path = media_root.join("first.ogg");
    let second_path = media_root.join("second.ogg");
    let mut first = Track::from_path(first_path.clone());
    first.set_primary_audio_hash("play-path-first");
    let mut second = Track::from_path(second_path.clone());
    second.set_primary_audio_hash("play-path-second");
    LibraryStore::open(&db_path)
        .unwrap()
        .upsert_tracks(&[first, second])
        .unwrap();
    let app = create_app(&db_path, &media_root);
    unsafe {
        (*app).engine = Some(
            PlayerEngine::spawn(NormalizationSettings::default(), || Ok(LoadedBackend)).unwrap(),
        );
    }

    let second_played = unsafe {
        call_json(player_app_play_path(
            app,
            c_string_arg(&second_path).as_ptr(),
        ))
    };
    assert_ok(&second_played);
    assert_eq!(second_played["data"]["queue_len"], 1);
    assert_eq!(
        second_played["data"]["current_track"]["path"],
        second_path.to_string_lossy().as_ref()
    );
    let second_id = unsafe { (&(*app).queue_item_ids)[0] };

    let first_played = unsafe {
        call_json(player_app_play_path(
            app,
            c_string_arg(&first_path).as_ptr(),
        ))
    };
    assert_ok(&first_played);
    assert_eq!(first_played["data"]["queue_len"], 2);
    assert_eq!(
        first_played["data"]["current_track"]["path"],
        first_path.to_string_lossy().as_ref()
    );

    let second_again = unsafe {
        call_json(player_app_play_path(
            app,
            c_string_arg(&second_path).as_ptr(),
        ))
    };
    assert_ok(&second_again);
    assert_eq!(second_again["data"]["queue_len"], 2);
    assert_eq!(
        second_again["data"]["current_track"]["path"],
        second_path.to_string_lossy().as_ref()
    );
    assert_eq!(unsafe { (&(*app).queue_item_ids)[0] }, second_id);

    unsafe { player_app_destroy(app) };
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn app_plays_only_the_requested_playlist_with_explicit_modes() {
    let db_path = temp_db_path("play_playlist");
    let media_root = temp_dir("play_playlist_media");
    fs::create_dir_all(&media_root).unwrap();
    let first_path = media_root.join("first.ogg");
    let second_path = media_root.join("second.ogg");
    let outside_path = media_root.join("outside.ogg");
    let app = create_app(&db_path, &media_root);

    {
        let mut first = Track::from_path(first_path.clone());
        first.title = "First".to_owned();
        first.set_primary_audio_hash("playlist-first");
        let mut second = Track::from_path(second_path.clone());
        second.title = "Second".to_owned();
        second.set_primary_audio_hash("playlist-second");
        let mut outside = Track::from_path(outside_path);
        outside.title = "Outside".to_owned();
        outside.set_primary_audio_hash("playlist-outside");

        let mut store = LibraryStore::open(&db_path).unwrap();
        store.upsert_tracks(&[first, second, outside]).unwrap();
        store.add_playlist_track("Phone Mix", &first_path).unwrap();
        store.add_playlist_track("Phone Mix", &second_path).unwrap();
        store.create_playlist("Empty").unwrap();
    }

    let engine =
        PlayerEngine::spawn(NormalizationSettings::default(), || Ok(UnloadedBackend)).unwrap();
    unsafe {
        (*app).engine = Some(engine);
    }

    let shuffled = unsafe {
        call_json(player_app_play_playlist(
            app,
            c_string_arg("Phone Mix").as_ptr(),
            c_string_arg(&second_path).as_ptr(),
            true,
        ))
    };
    assert_ok(&shuffled);
    assert_eq!(shuffled["data"]["queue_len"], 2);
    assert_eq!(
        shuffled["data"]["current_track"]["path"],
        second_path.to_string_lossy().as_ref()
    );
    assert_eq!(shuffled["data"]["shuffle_enabled"], true);

    let randomized_start = unsafe {
        call_json(player_app_play_playlist(
            app,
            c_string_arg("Phone Mix").as_ptr(),
            ptr::null(),
            true,
        ))
    };
    assert_ok(&randomized_start);
    let randomized_queue = unsafe { call_json(player_app_queue(app)) };
    assert_ok(&randomized_queue);
    assert_eq!(randomized_queue["data"]["current_index"], 0);
    assert_eq!(
        randomized_queue["data"]["tracks"][0]["path"],
        randomized_start["data"]["current_track"]["path"]
    );
    let mut randomized_paths = queue_paths(&randomized_queue);
    randomized_paths.sort();
    let mut expected_paths = vec![
        first_path.to_string_lossy().into_owned(),
        second_path.to_string_lossy().into_owned(),
    ];
    expected_paths.sort();
    assert_eq!(randomized_paths, expected_paths);
    let jumped = unsafe { call_json(player_app_queue_play(app, 1)) };
    assert_ok(&jumped);
    assert_eq!(
        jumped["data"]["current_track"]["path"],
        randomized_queue["data"]["tracks"][1]["path"]
    );
    assert_eq!(jumped["data"]["queue_position"], 1);
    assert_eq!(jumped["data"]["is_playing"], true);

    let sequential = unsafe {
        call_json(player_app_play_playlist(
            app,
            c_string_arg("Phone Mix").as_ptr(),
            ptr::null(),
            false,
        ))
    };
    assert_ok(&sequential);
    assert_eq!(
        sequential["data"]["current_track"]["path"],
        first_path.to_string_lossy().as_ref()
    );
    assert_eq!(sequential["data"]["shuffle_enabled"], false);

    let empty = unsafe {
        call_json(player_app_play_playlist(
            app,
            c_string_arg("Empty").as_ptr(),
            ptr::null(),
            false,
        ))
    };
    assert_eq!(empty["ok"], false);
    assert!(empty["error"]
        .as_str()
        .unwrap()
        .contains("playlist is empty: Empty"));

    unsafe { player_app_destroy(app) };
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn app_output_disconnect_pauses_and_allows_manual_resume() {
    let db_path = temp_db_path("output_disconnect_resume");
    let media_root = temp_dir("output_disconnect_resume_media");
    fs::create_dir_all(&media_root).unwrap();
    let app = create_app(&db_path, &media_root);
    let mut track = Track::from_path(media_root.join("driving.ogg"));
    track.title = "Driving".to_owned();
    track.set_primary_audio_hash("output-disconnect-resume");
    LibraryStore::open(&db_path)
        .unwrap()
        .upsert_track(&track)
        .unwrap();

    let engine =
        PlayerEngine::spawn(NormalizationSettings::default(), || Ok(LoadedBackend)).unwrap();
    unsafe {
        (*app).engine = Some(engine);
        (*app).play_queue_tracks(vec![track], 0, false).unwrap();
    }

    let disconnected = unsafe { call_json(player_app_audio_output_disconnected(app)) };
    assert_ok(&disconnected);
    assert_eq!(disconnected["data"]["current_track"]["title"], "Driving");
    assert_eq!(disconnected["data"]["is_playing"], false);
    assert_eq!(disconnected["data"]["interruption_active"], false);
    assert_eq!(disconnected["data"]["resume_after_interruption"], false);

    let resumed = unsafe { call_json(player_app_resume(app)) };
    assert_ok(&resumed);
    assert_eq!(resumed["data"]["current_track"]["title"], "Driving");
    assert_eq!(resumed["data"]["is_playing"], true);
    assert_eq!(resumed["data"]["interruption_active"], false);

    unsafe { player_app_destroy(app) };
    fs::remove_file(db_path).ok();
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
    assert_eq!(repeat["data"]["playback_mode"], "repeat_one");
    assert_eq!(repeat["data"]["shuffle_enabled"], false);
    assert_eq!(repeat["data"]["queue_len"], 0);
    assert!(repeat["data"]["queue_position"].is_null());

    let shuffle = unsafe { call_json(player_app_set_shuffle(app, true)) };
    assert_ok(&shuffle);
    assert_eq!(shuffle["data"]["repeat_mode"], "all");
    assert_eq!(shuffle["data"]["playback_mode"], "shuffle");
    assert_eq!(shuffle["data"]["shuffle_enabled"], true);

    let queue = unsafe { call_json(player_app_queue(app)) };
    assert_ok(&queue);
    assert_eq!(queue["data"]["tracks"].as_array().unwrap().len(), 0);
    assert_eq!(queue["data"]["repeat_mode"], "all");
    assert_eq!(queue["data"]["playback_mode"], "shuffle");
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
        let error = match (*app).play_queue_tracks(vec![track], 0, false) {
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
    LibraryStore::open(&db_path)
        .unwrap()
        .upsert_tracks(&[first.clone(), second.clone()])
        .unwrap();

    unsafe {
        let app_ref = &mut *app;
        app_ref.queue_tracks = track_dtos(&[first.clone(), second.clone()]).unwrap();
        app_ref.apply_event(PlaybackEvent::StateChanged(domain::PlaybackState {
            is_playing: true,
            current_index: Some(1),
            position_ms: 1_234,
            playback_mode: domain::PlaybackMode::Shuffle,
            repeat_mode: RepeatMode::All,
            shuffle: true,
        }));
        app_ref.apply_event(PlaybackEvent::QueueOrderChanged {
            order: vec![1, 0],
            current_position: Some(0),
        });
        app_ref.apply_event(PlaybackEvent::TrackChanged(Some(Box::new(second))));
    }

    let snapshot = unsafe { call_json(player_app_poll(app)) };
    assert_ok(&snapshot);
    assert_eq!(snapshot["data"]["queue_len"], 2);
    assert_eq!(snapshot["data"]["queue_position"], 0);
    assert_eq!(snapshot["data"]["repeat_mode"], "all");
    assert_eq!(snapshot["data"]["shuffle_enabled"], true);
    assert_eq!(snapshot["data"]["current_track"]["title"], "Second");
    let queue = unsafe { call_json(player_app_queue(app)) };
    assert_ok(&queue);
    assert_eq!(queue["data"]["tracks"][0]["title"], "Second");
    assert_eq!(queue["data"]["tracks"][1]["title"], "First");

    unsafe { player_app_destroy(app) };
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn app_edits_and_restores_the_persisted_queue_without_opening_audio() {
    let db_path = temp_db_path("persisted_queue");
    let media_root = temp_dir("persisted_queue_media");
    fs::create_dir_all(&media_root).unwrap();
    let tracks = ["first", "second", "third"]
        .into_iter()
        .map(|name| {
            let path = media_root.join(format!("{name}.ogg"));
            fs::write(&path, b"queue fixture").unwrap();
            let mut track = Track::from_path(path);
            track.title = name.to_owned();
            track.set_primary_audio_hash(format!("queue-{name}"));
            track
        })
        .collect::<Vec<_>>();
    LibraryStore::open(&db_path)
        .unwrap()
        .upsert_tracks(&tracks)
        .unwrap();

    let app = create_app(&db_path, &media_root);
    for track in &tracks[..2] {
        let response = unsafe {
            call_json(player_app_queue_add(
                app,
                c_string_arg(&track.path).as_ptr(),
            ))
        };
        assert_ok(&response);
    }
    let play_next = unsafe {
        call_json(player_app_queue_play_next(
            app,
            c_string_arg(&tracks[2].path).as_ptr(),
        ))
    };
    assert_ok(&play_next);
    unsafe { player_app_destroy(app) };

    let restored_app = create_app(&db_path, &media_root);
    let restored = unsafe { call_json(player_app_queue(restored_app)) };
    assert_ok(&restored);
    assert_eq!(
        queue_paths(&restored),
        vec![
            path_to_string_lossy(&tracks[0].path),
            path_to_string_lossy(&tracks[2].path),
            path_to_string_lossy(&tracks[1].path),
        ]
    );
    assert_eq!(restored["data"]["current_index"], 0);

    assert_ok(&unsafe { call_json(player_app_queue_move(restored_app, 2, 1)) });
    assert_ok(&unsafe { call_json(player_app_queue_remove(restored_app, 0)) });
    let edited = unsafe { call_json(player_app_queue(restored_app)) };
    assert_eq!(
        queue_paths(&edited),
        vec![
            path_to_string_lossy(&tracks[1].path),
            path_to_string_lossy(&tracks[2].path),
        ]
    );
    assert_eq!(edited["data"]["current_index"], 0);

    assert_ok(&unsafe { call_json(player_app_queue_clear(restored_app)) });
    let cleared = unsafe { call_json(player_app_queue(restored_app)) };
    assert!(cleared["data"]["tracks"].as_array().unwrap().is_empty());

    unsafe { player_app_destroy(restored_app) };
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}
