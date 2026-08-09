use super::*;

#[test]
fn app_sorts_playlist_via_ffi() {
    let db_path = temp_db_path("sort_playlist");
    let media_root = temp_dir("sort_playlist_media");
    fs::create_dir_all(&media_root).unwrap();
    let first_path = media_root.join("a.ogg");
    let second_path = media_root.join("b.ogg");
    let third_path = media_root.join("c.ogg");
    let app = create_app(&db_path, &media_root);

    {
        let mut first = Track::from_path(first_path.clone());
        first.title = "Delta".to_owned();
        first.artist = Some("Beta".to_owned());
        first.album = Some("Second".to_owned());
        first.track_number = Some(2);
        first.user_rating = Some(8);
        first.set_primary_audio_hash("audio-a");

        let mut second = Track::from_path(second_path.clone());
        second.title = "Alpha".to_owned();
        second.artist = Some("Gamma".to_owned());
        second.album = Some("First".to_owned());
        second.track_number = Some(2);
        second.set_primary_audio_hash("audio-b");

        let mut third = Track::from_path(third_path.clone());
        third.title = "Charlie".to_owned();
        third.artist = Some("Alpha".to_owned());
        third.album = Some("First".to_owned());
        third.track_number = Some(1);
        third.user_rating = Some(10);
        third.set_primary_audio_hash("audio-c");

        let mut store = LibraryStore::open(&db_path).unwrap();
        store.upsert_tracks(&[first, second, third]).unwrap();
        store.add_playlist_track("Road", &second_path).unwrap();
        store.add_playlist_track("Road", &first_path).unwrap();
        store.add_playlist_track("Road", &third_path).unwrap();
    }

    let sort = unsafe {
        call_json(player_app_sort_playlist(
            app,
            c_string_arg("Road").as_ptr(),
            c_string_arg("title").as_ptr(),
        ))
    };
    assert_ok(&sort);
    let sorted = unsafe {
        call_json(player_app_playlist_tracks(
            app,
            c_string_arg("Road").as_ptr(),
        ))
    };
    assert_ok(&sorted);
    assert_eq!(
        playlist_paths(&sorted),
        vec![
            second_path.to_string_lossy().into_owned(),
            third_path.to_string_lossy().into_owned(),
            first_path.to_string_lossy().into_owned()
        ]
    );

    let sort_rating = unsafe {
        call_json(player_app_sort_playlist(
            app,
            c_string_arg("Road").as_ptr(),
            c_string_arg("rating").as_ptr(),
        ))
    };
    assert_ok(&sort_rating);
    let sorted = unsafe {
        call_json(player_app_playlist_tracks(
            app,
            c_string_arg("Road").as_ptr(),
        ))
    };
    assert_eq!(
        playlist_paths(&sorted),
        vec![
            third_path.to_string_lossy().into_owned(),
            first_path.to_string_lossy().into_owned(),
            second_path.to_string_lossy().into_owned()
        ]
    );

    let reset = unsafe {
        call_json(player_app_sort_playlist(
            app,
            c_string_arg("Road").as_ptr(),
            c_string_arg("manual").as_ptr(),
        ))
    };
    assert_ok(&reset);
    let sorted = unsafe {
        call_json(player_app_playlist_tracks(
            app,
            c_string_arg("Road").as_ptr(),
        ))
    };
    assert_eq!(
        playlist_paths(&sorted),
        vec![
            second_path.to_string_lossy().into_owned(),
            first_path.to_string_lossy().into_owned(),
            third_path.to_string_lossy().into_owned()
        ]
    );

    unsafe { player_app_destroy(app) };
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn app_playlist_artwork_defaults_to_first_track_and_custom_overrides() {
    let db_dir = temp_dir("playlist_artwork_db");
    fs::create_dir_all(&db_dir).unwrap();
    let db_path = db_dir.join("library.sqlite3");
    let media_root = temp_dir("playlist_artwork_media");
    fs::create_dir_all(&media_root).unwrap();
    let first_path = media_root.join("first.ogg");
    let second_path = media_root.join("second.ogg");
    let first_cover = media_root
        .join("first-cover.png")
        .canonicalize()
        .unwrap_or_else(|_| media_root.join("first-cover.png"));
    let custom_cover = media_root.join("custom-playlist.png");
    let custom_cover_two = media_root.join("custom-playlist-two.png");
    fs::write(&first_path, b"not decoded by this test").unwrap();
    fs::write(&second_path, b"not decoded by this test").unwrap();
    fs::write(&first_cover, b"\x89PNG\r\n\x1A\nfirst").unwrap();
    fs::write(&custom_cover, b"\x89PNG\r\n\x1A\ncustom").unwrap();
    fs::write(&custom_cover_two, b"\x89PNG\r\n\x1A\nsecond").unwrap();
    let app = create_app(&db_path, &media_root);

    {
        let mut first = Track::from_path(first_path.clone());
        first.title = "First".to_owned();
        first.set_primary_audio_hash("playlist-artwork-first");

        let mut second = Track::from_path(second_path.clone());
        second.title = "Second".to_owned();
        second.set_primary_audio_hash("playlist-artwork-second");

        let mut store = LibraryStore::open(&db_path).unwrap();
        store
            .upsert_tracks(&[first.clone(), second.clone()])
            .unwrap();
        let first_image = read_artwork_image(&first_cover).unwrap();
        store
            .set_track_artwork_reference(&first.path, &first_image)
            .unwrap();
        store.add_playlist_track("Mix", &first.path).unwrap();
        store.add_playlist_track("Mix", &second.path).unwrap();
    }

    let playlists = unsafe { call_json(player_app_playlists(app)) };
    assert_ok(&playlists);
    assert_eq!(playlists["data"][0]["name"], "Mix");
    assert_eq!(playlists["data"][0]["track_count"], 2);
    let first_cached_path = PathBuf::from(playlists["data"][0]["artwork_path"].as_str().unwrap());
    assert!(first_cached_path.starts_with(db_dir.join("Artwork").join("Assets")));
    assert_eq!(
        fs::read(&first_cached_path).unwrap(),
        b"\x89PNG\r\n\x1A\nfirst"
    );
    assert_eq!(playlists["data"][0]["artwork_source"], "track");

    let updated = unsafe {
        call_json(player_app_set_playlist_artwork(
            app,
            c_string_arg("Mix").as_ptr(),
            c_string_arg(&custom_cover).as_ptr(),
        ))
    };
    assert_ok(&updated);
    fs::remove_file(&custom_cover).unwrap();

    let playlists = unsafe { call_json(player_app_playlists(app)) };
    assert_ok(&playlists);
    let custom_path = PathBuf::from(playlists["data"][0]["artwork_path"].as_str().unwrap());
    assert_eq!(playlists["data"][0]["artwork_source"], "playlist");
    assert!(custom_path.starts_with(db_dir.join("Artwork").join("Playlists")));
    assert_eq!(fs::read(&custom_path).unwrap(), b"\x89PNG\r\n\x1A\ncustom");

    let updated = unsafe {
        call_json(player_app_set_playlist_artwork(
            app,
            c_string_arg("Mix").as_ptr(),
            c_string_arg(&custom_cover_two).as_ptr(),
        ))
    };
    assert_ok(&updated);
    fs::remove_file(&custom_cover_two).unwrap();
    let playlists = unsafe { call_json(player_app_playlists(app)) };
    assert_ok(&playlists);
    let rewritten_custom_path =
        PathBuf::from(playlists["data"][0]["artwork_path"].as_str().unwrap());
    assert_eq!(rewritten_custom_path, custom_path);
    assert_eq!(
        fs::read(&rewritten_custom_path).unwrap(),
        b"\x89PNG\r\n\x1A\nsecond"
    );

    unsafe { player_app_destroy(app) };
    fs::remove_dir_all(db_dir).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn app_exposes_recent_playlists_with_portable_timestamps() {
    let db_path = temp_db_path("recent_playlists");
    let media_root = temp_dir("recent_playlists_media");
    fs::create_dir_all(&media_root).unwrap();
    let app = create_app(&db_path, &media_root);
    for name in ["Morning", "Night"] {
        assert_ok(&unsafe {
            call_json(player_app_create_playlist(app, c_string_arg(name).as_ptr()))
        });
    }

    let recent = unsafe { call_json(player_app_recent_playlists(app, 1)) };
    assert_ok(&recent);
    let playlists = recent["data"].as_array().unwrap();
    assert_eq!(playlists.len(), 1);
    assert!(playlists[0]["created_at_unix_seconds"]
        .as_i64()
        .is_some_and(|value| value > 0));
    assert!(playlists[0]["last_used_at_unix_seconds"]
        .as_i64()
        .is_some_and(|value| value > 0));

    unsafe { player_app_destroy(app) };
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}
