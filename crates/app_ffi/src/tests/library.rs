use super::*;

#[test]
fn zero_out_removes_an_unopenable_legacy_database_and_all_managed_files() {
    let library_root = temp_dir("zero_out_legacy_library");
    let db_path = library_root.join("player_library.sqlite3");
    let db_wal_path = PathBuf::from(format!("{}-wal", db_path.display()));
    let db_shm_path = PathBuf::from(format!("{}-shm", db_path.display()));
    let db_journal_path = PathBuf::from(format!("{}-journal", db_path.display()));
    let media_root = library_root.join("Music");
    let artwork_root = library_root.join("Artwork");
    let managed_audio = media_root.join("Album").join("track.mp3");
    let cached_artwork = artwork_root.join("Assets").join("cover.png");

    fs::create_dir_all(managed_audio.parent().unwrap()).unwrap();
    fs::create_dir_all(cached_artwork.parent().unwrap()).unwrap();
    fs::write(
        &db_path,
        b"legacy database that the current schema cannot open",
    )
    .unwrap();
    fs::write(&db_wal_path, b"legacy wal").unwrap();
    fs::write(&db_shm_path, b"legacy shm").unwrap();
    fs::write(&db_journal_path, b"legacy journal").unwrap();
    fs::write(&managed_audio, b"managed audio").unwrap();
    fs::write(&cached_artwork, b"cached artwork").unwrap();

    let app = create_app(&db_path, &media_root);
    let engine =
        PlayerEngine::spawn(NormalizationSettings::default(), || Ok(UnloadedBackend)).unwrap();
    unsafe {
        (*app).engine = Some(engine);
    }

    let zeroed = unsafe { call_json(player_app_zero_out_library(app)) };
    assert_ok(&zeroed);
    assert!(unsafe { (*app).engine.is_none() });
    assert!(!db_path.exists());
    assert!(!db_wal_path.exists());
    assert!(!db_shm_path.exists());
    assert!(!db_journal_path.exists());
    assert!(!media_root.exists());
    assert!(!artwork_root.exists());

    assert!(LibraryStore::open(&db_path)
        .unwrap()
        .tracks()
        .unwrap()
        .is_empty());

    unsafe { player_app_destroy(app) };
    fs::remove_dir_all(library_root).ok();
}

#[test]
fn app_import_search_collections_and_history_roundtrip() {
    let db_path = temp_db_path("roundtrip");
    let media_root = temp_dir("media");
    let audio_root = workspace_root().join("test-assets").join("audio");
    let source_fixture = audio_root.join("into_the_oceans_chorus.ogg");
    let app = create_app(&db_path, &media_root);

    let import = unsafe {
        call_json(player_app_import_folder(
            app,
            c_string_arg(&audio_root).as_ptr(),
        ))
    };
    assert_ok(&import);
    assert_eq!(import["data"]["imported"], 3);
    assert_eq!(import["data"]["copied"], 3);
    assert!(source_fixture.exists());

    let library = unsafe { call_json(player_app_library(app)) };
    assert_ok(&library);
    assert_eq!(library["data"].as_array().unwrap().len(), 3);
    for track in library["data"].as_array().unwrap() {
        let path = track["path"].as_str().unwrap();
        assert!(Path::new(path).starts_with(&media_root), "{path}");
        assert!(Path::new(path).exists(), "{path}");
    }

    let search = unsafe { call_json(player_app_search(app, c_string_arg("oceans").as_ptr(), 25)) };
    assert_ok(&search);
    assert!(search["data"]
        .as_array()
        .unwrap()
        .iter()
        .any(|track| track["path"].as_str().unwrap().contains("into_the_oceans")));
    let managed_fixture = search["data"][0]["path"].as_str().unwrap().to_owned();

    let favorite = unsafe {
        call_json(player_app_set_favorite(
            app,
            c_string_arg(&managed_fixture).as_ptr(),
            true,
        ))
    };
    assert_ok(&favorite);
    let favorites = unsafe { call_json(player_app_favorites(app)) };
    assert_ok(&favorites);
    assert_eq!(favorites["data"].as_array().unwrap().len(), 1);

    let playlist_name = c_string_arg("Mix");
    let playlist = unsafe { call_json(player_app_create_playlist(app, playlist_name.as_ptr())) };
    assert_ok(&playlist);
    let add = unsafe {
        call_json(player_app_add_to_playlist(
            app,
            playlist_name.as_ptr(),
            c_string_arg(&managed_fixture).as_ptr(),
        ))
    };
    assert_ok(&add);
    assert_eq!(add["data"]["added"], true);
    let duplicate_add = unsafe {
        call_json(player_app_add_to_playlist(
            app,
            playlist_name.as_ptr(),
            c_string_arg(&managed_fixture).as_ptr(),
        ))
    };
    assert_ok(&duplicate_add);
    assert_eq!(duplicate_add["data"]["added"], false);
    let playlists = unsafe { call_json(player_app_playlists(app)) };
    assert_ok(&playlists);
    assert_eq!(playlists["data"][0]["name"], "Mix");
    assert_eq!(playlists["data"][0]["track_count"], 1);
    let playlist_tracks =
        unsafe { call_json(player_app_playlist_tracks(app, playlist_name.as_ptr())) };
    assert_ok(&playlist_tracks);
    assert_eq!(playlist_tracks["data"].as_array().unwrap().len(), 1);
    let playlist_search = unsafe {
        call_json(player_app_search_playlist(
            app,
            playlist_name.as_ptr(),
            c_string_arg("oceans").as_ptr(),
            25,
        ))
    };
    assert_ok(&playlist_search);
    assert_eq!(playlist_search["data"].as_array().unwrap().len(), 1);
    let playlist_miss = unsafe {
        call_json(player_app_search_playlist(
            app,
            playlist_name.as_ptr(),
            c_string_arg("mountain").as_ptr(),
            25,
        ))
    };
    assert_ok(&playlist_miss);
    assert!(playlist_miss["data"].as_array().unwrap().is_empty());

    LibraryStore::open(&db_path)
        .unwrap()
        .record_playback(&managed_fixture, 123, true)
        .unwrap();
    let history = unsafe { call_json(player_app_history(app, 10)) };
    assert_ok(&history);
    assert_eq!(history["data"].as_array().unwrap().len(), 1);

    unsafe { player_app_destroy(app) };
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn app_exports_zeroes_and_imports_a_complete_library_package() {
    let source_root = temp_dir("library_package_source");
    let source_db = source_root.join("player_library.sqlite3");
    let source_media = source_root.join("Music");
    let source_audio = source_media.join("Portable Album").join("song.wav");
    fs::create_dir_all(source_audio.parent().unwrap()).unwrap();
    write_test_wav(&source_audio, b"Portable Song").unwrap();
    fs::write(
        source_audio.with_extension("lrc"),
        b"[00:00]Portable lyrics",
    )
    .unwrap();
    fs::write(source_audio.parent().unwrap().join("cover.jpg"), b"cover").unwrap();

    let image = ArtworkImage {
        picture_index: 0,
        mime_type: Some("image/png".to_owned()),
        picture_type: "CoverFront".to_owned(),
        description: Some("portable artwork".to_owned()),
        data: vec![1, 3, 5, 7],
    };
    {
        let mut store = LibraryStore::open(&source_db).unwrap();
        let mut track = Track::from_path(source_audio.clone());
        track.title = "Portable Song".to_owned();
        track.artist = Some("Portable Artist".to_owned());
        track.album = Some("Portable Album".to_owned());
        track.view_name = Some("Portable View".to_owned());
        track.user_rating = Some(9);
        track.set_primary_audio_hash("portable-audio-hash");
        store.upsert_track(&track).unwrap();
        store
            .set_track_notes(&source_audio, "portable note")
            .unwrap();
        store.set_favorite(&source_audio, true).unwrap();
        store.record_playback(&source_audio, 4321, true).unwrap();
        store.create_playlist("Portable Playlist").unwrap();
        store
            .add_playlist_track("Portable Playlist", &source_audio)
            .unwrap();
        store
            .save_artwork(&source_audio, std::slice::from_ref(&image))
            .unwrap();
        store
            .set_track_artwork_reference(&source_audio, &image)
            .unwrap();
        store
            .set_album_artwork_reference_for_track(&source_audio, &image)
            .unwrap();
        store
            .save_playlist_artwork("Portable Playlist", &image)
            .unwrap();
    }

    let package_root = temp_dir("library_package");
    let source_app = create_app(&source_db, &source_media);
    let exported = unsafe {
        call_json(player_app_export_library(
            source_app,
            c_string_arg(&package_root).as_ptr(),
        ))
    };
    assert_ok(&exported);
    assert_eq!(exported["data"]["tracks"], 1);
    assert_eq!(exported["data"]["playlists"], 1);
    assert_eq!(exported["data"]["audio_files"], 1);
    assert_eq!(exported["data"]["sidecar_files"], 2);
    assert!(package_root.join(LIBRARY_PACKAGE_DATABASE_FILE).is_file());
    assert!(package_root.join(LIBRARY_PACKAGE_MANIFEST_FILE).is_file());
    unsafe { player_app_destroy(source_app) };

    let target_root = temp_dir("library_package_target");
    let target_db = target_root.join("player_library.sqlite3");
    let target_media = target_root.join("Music");
    let old_audio = target_media.join("old.wav");
    fs::create_dir_all(&target_media).unwrap();
    write_test_wav(&old_audio, b"Old Song").unwrap();
    {
        let mut store = LibraryStore::open(&target_db).unwrap();
        store
            .upsert_track(&Track::from_path(old_audio.clone()))
            .unwrap();
    }
    let target_app = create_app(&target_db, &target_media);
    let zeroed = unsafe { call_json(player_app_zero_out_library(target_app)) };
    assert_ok(&zeroed);
    assert!(LibraryStore::open(&target_db)
        .unwrap()
        .tracks()
        .unwrap()
        .is_empty());
    assert!(!old_audio.exists());

    let imported = unsafe {
        call_json(player_app_import_library(
            target_app,
            c_string_arg(&package_root).as_ptr(),
        ))
    };
    assert_ok(&imported);
    assert_eq!(imported["data"]["tracks"], 1);
    assert_eq!(imported["data"]["playlists"], 1);
    assert_eq!(imported["data"]["audio_files"], 1);
    assert_eq!(imported["data"]["sidecar_files"], 2);

    let store = LibraryStore::open(&target_db).unwrap();
    let tracks = store.tracks().unwrap();
    assert_eq!(tracks.len(), 1);
    let imported_track = &tracks[0];
    assert_eq!(imported_track.title, "Portable Song");
    assert_eq!(imported_track.artist.as_deref(), Some("Portable Artist"));
    assert_eq!(imported_track.view_name.as_deref(), Some("Portable View"));
    assert_eq!(imported_track.user_rating, Some(9));
    assert!(imported_track.path.starts_with(&target_media));
    assert_eq!(
        fs::read(&imported_track.path).unwrap(),
        fs::read(&source_audio).unwrap()
    );
    assert_eq!(
        fs::read(imported_track.path.with_extension("lrc")).unwrap(),
        b"[00:00]Portable lyrics"
    );
    assert_eq!(
        fs::read(imported_track.path.parent().unwrap().join("cover.jpg")).unwrap(),
        b"cover"
    );
    assert_eq!(
        store.playlist_tracks("Portable Playlist").unwrap()[0]
            .track
            .path,
        imported_track.path
    );
    assert_eq!(
        store.favorite_tracks().unwrap()[0].path,
        imported_track.path
    );
    let history = store.play_history(10).unwrap();
    assert_eq!(history[0].track.path, imported_track.path);
    assert_eq!(history[0].position_ms, 4321);
    assert!(history[0].completed);
    assert_eq!(
        store.track_notes(&imported_track.path).unwrap().as_deref(),
        Some("portable note")
    );
    assert_eq!(
        store.artwork_for_path(&imported_track.path).unwrap()[0].data,
        image.data
    );
    assert!(store
        .track_artwork_reference(&imported_track.path)
        .unwrap()
        .is_some());
    assert!(store
        .album_artwork_reference(&imported_track.path)
        .unwrap()
        .is_some());
    assert_eq!(
        store
            .playlist_artwork("Portable Playlist")
            .unwrap()
            .unwrap()
            .data,
        image.data
    );

    unsafe { player_app_destroy(target_app) };
    fs::remove_dir_all(source_root).ok();
    fs::remove_dir_all(package_root).ok();
    fs::remove_dir_all(target_root).ok();
}

#[test]
fn library_package_import_rejects_escaping_paths_before_replacing_state() {
    let target_root = temp_dir("unsafe_library_package_target");
    let target_db = target_root.join("player_library.sqlite3");
    let target_media = target_root.join("Music");
    let existing_audio = target_media.join("existing.wav");
    fs::create_dir_all(&target_media).unwrap();
    write_test_wav(&existing_audio, b"Existing").unwrap();
    {
        let mut store = LibraryStore::open(&target_db).unwrap();
        store
            .upsert_track(&Track::from_path(existing_audio.clone()))
            .unwrap();
    }

    let package_root = temp_dir("unsafe_library_package");
    fs::create_dir_all(&package_root).unwrap();
    fs::copy(&target_db, package_root.join(LIBRARY_PACKAGE_DATABASE_FILE)).unwrap();
    fs::write(
        package_root.join(LIBRARY_PACKAGE_MANIFEST_FILE),
        serde_json::to_vec(&LibraryPackageManifest {
            format_version: LIBRARY_PACKAGE_FORMAT_VERSION,
            database_file: LIBRARY_PACKAGE_DATABASE_FILE.to_owned(),
            tracks: vec![LibraryPackageTrack {
                database_path: existing_audio.to_string_lossy().into_owned(),
                audio_file: "../outside.wav".to_owned(),
            }],
        })
        .unwrap(),
    )
    .unwrap();

    let app = create_app(&target_db, &target_media);
    let imported = unsafe {
        call_json(player_app_import_library(
            app,
            c_string_arg(&package_root).as_ptr(),
        ))
    };
    assert_eq!(imported["ok"], false);
    let library = unsafe { call_json(player_app_library(app)) };
    assert_ok(&library);
    assert_eq!(library["data"].as_array().unwrap().len(), 1);
    assert_eq!(
        library["data"][0]["path"],
        existing_audio.to_string_lossy().as_ref()
    );

    unsafe { player_app_destroy(app) };
    fs::remove_dir_all(package_root).ok();
    fs::remove_dir_all(target_root).ok();
}

#[test]
fn library_package_import_rejects_non_primary_track_identity() {
    let target_root = temp_dir("non_primary_package_target");
    let target_db = target_root.join("player_library.sqlite3");
    let target_media = target_root.join("Music");
    let existing_audio = target_media.join("existing.wav");
    fs::create_dir_all(&target_media).unwrap();
    write_test_wav(&existing_audio, b"Existing").unwrap();
    LibraryStore::open(&target_db)
        .unwrap()
        .upsert_track(&Track::from_path(existing_audio.clone()))
        .unwrap();
    let app = create_app(&target_db, &target_media);

    for (case, derived_kind) in [("derived", true), ("mismatched_primary", false)] {
        let package_root = temp_dir(case);
        fs::create_dir_all(&package_root).unwrap();
        let package_db = package_root.join(LIBRARY_PACKAGE_DATABASE_FILE);
        let database_path = PathBuf::from(format!("/package/{case}.ogg"));
        let mut track = Track::from_path(database_path.clone());
        track.set_primary_audio_hash(format!("{case}-audio"));
        track.view_id = TrackViewId::from_value(format!("audio:{case}-audio:view:invalid"));
        if derived_kind {
            track.view_kind = TrackViewKind::Derived;
            track.transform_spec = Some(r#"{"kind":"invalid"}"#.to_owned());
        }
        LibraryStore::open(&package_db)
            .unwrap()
            .upsert_track(&track)
            .unwrap();
        fs::write(
            package_root.join(LIBRARY_PACKAGE_MANIFEST_FILE),
            serde_json::to_vec(&LibraryPackageManifest {
                format_version: LIBRARY_PACKAGE_FORMAT_VERSION,
                database_file: LIBRARY_PACKAGE_DATABASE_FILE.to_owned(),
                tracks: vec![LibraryPackageTrack {
                    database_path: path_to_string_lossy(&database_path),
                    audio_file: format!("Music/{case}.ogg"),
                }],
            })
            .unwrap(),
        )
        .unwrap();

        let imported = unsafe {
            call_json(player_app_import_library(
                app,
                c_string_arg(&package_root).as_ptr(),
            ))
        };
        assert_eq!(imported["ok"], false, "{case}: {imported}");
        assert!(
            imported["error"]
                .as_str()
                .unwrap()
                .contains("non-primary track"),
            "{case}: {imported}"
        );
        fs::remove_dir_all(package_root).ok();
    }

    let library = unsafe { call_json(player_app_library(app)) };
    assert_ok(&library);
    assert_eq!(library["data"].as_array().unwrap().len(), 1);
    assert_eq!(
        library["data"][0]["path"],
        existing_audio.to_string_lossy().as_ref()
    );

    unsafe { player_app_destroy(app) };
    fs::remove_dir_all(target_root).ok();
}

#[test]
fn app_deletes_track_from_library_and_managed_storage_via_ffi() {
    let db_path = temp_db_path("delete_track");
    let media_root = temp_dir("delete_track_media");
    let album_root = media_root.join("Album");
    fs::create_dir_all(&album_root).unwrap();
    let deleted_path = album_root.join("song.ogg");
    let kept_path = album_root.join("kept.ogg");
    let lyrics_path = album_root.join("song.lrc");
    let track_cover_path = album_root.join("song.jpg");
    let album_cover_path = album_root.join("cover.jpg");
    for path in [
        &deleted_path,
        &kept_path,
        &lyrics_path,
        &track_cover_path,
        &album_cover_path,
    ] {
        fs::write(path, b"test").unwrap();
    }

    {
        let deleted = Track::from_path(deleted_path.clone());
        let kept = Track::from_path(kept_path.clone());
        let mut store = LibraryStore::open(&db_path).unwrap();
        store
            .upsert_tracks(&[deleted.clone(), kept.clone()])
            .unwrap();
        store.add_playlist_track("Road", &deleted.path).unwrap();
        store.add_playlist_track("Road", &kept.path).unwrap();
        store.set_favorite(&deleted.path, true).unwrap();
        store.record_playback(&deleted.path, 42, true).unwrap();
        store
            .save_playback_queue(
                &[deleted.path.clone(), kept.path.clone()],
                Some(0),
                500,
                RepeatMode::Off,
                false,
            )
            .unwrap();
    }

    let app = create_app(&db_path, &media_root);
    let deleted = unsafe {
        call_json(player_app_delete_from_library(
            app,
            c_string_arg(&deleted_path).as_ptr(),
        ))
    };
    assert_ok(&deleted);
    assert_eq!(deleted["data"]["managed_files_deleted"], 3);
    assert!(deleted["data"]["cleanup_error"].is_null());

    let store = LibraryStore::open(&db_path).unwrap();
    assert!(store.track_by_path(&deleted_path).unwrap().is_none());
    assert!(store.track_by_path(&kept_path).unwrap().is_some());
    assert_eq!(
        store
            .playlist_tracks("Road")
            .unwrap()
            .into_iter()
            .map(|entry| entry.track.path)
            .collect::<Vec<_>>(),
        vec![kept_path.clone()]
    );
    assert!(store.favorite_tracks().unwrap().is_empty());
    assert!(store.play_history(10).unwrap().is_empty());
    assert_eq!(
        store
            .load_playback_queue()
            .unwrap()
            .tracks
            .into_iter()
            .map(|track| track.path)
            .collect::<Vec<_>>(),
        vec![kept_path.clone()]
    );
    assert!(!deleted_path.exists());
    assert!(!lyrics_path.exists());
    assert!(!track_cover_path.exists());
    assert!(album_cover_path.exists());
    assert!(kept_path.exists());

    unsafe { player_app_destroy(app) };
    fs::remove_dir_all(media_root).ok();
    for path in sqlite_database_files(&db_path) {
        fs::remove_file(path).ok();
    }
}

#[test]
fn library_playback_plan_uses_the_unique_primary_tracks() {
    let db_path = temp_db_path("library_playback_plan");
    let media_root = temp_dir("library_playback_plan_media");
    fs::create_dir_all(&media_root).unwrap();
    let first_path = media_root.join("zulu.ogg");
    let second_path = media_root.join("alpha.ogg");

    let mut first = Track::from_path(first_path.clone());
    first.title = "Zulu".to_owned();
    first.set_primary_audio_hash("zulu-audio");
    let mut second = Track::from_path(second_path.clone());
    second.title = "Alpha".to_owned();
    second.set_primary_audio_hash("alpha-audio");

    let mut store = LibraryStore::open(&db_path).unwrap();
    store.upsert_tracks(&[first, second]).unwrap();

    let (tracks, start_index) = library_playback_plan(&store, None).unwrap();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].path, second_path);
    assert_eq!(tracks[1].path, first_path);
    assert_eq!(start_index, 0);

    let (tracks, start_index) = library_playback_plan(&store, Some(&first_path)).unwrap();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[start_index].path, first_path);

    let missing = media_root.join("missing.ogg");
    let error = library_playback_plan(&store, Some(&missing)).unwrap_err();
    assert!(error.to_string().contains("track is not in library"));

    drop(store);
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn library_page_reports_total_and_stable_order() {
    let db_path = temp_db_path("library_page");
    let media_root = temp_dir("library_page_media");
    fs::create_dir_all(&media_root).unwrap();
    let app = create_app(&db_path, &media_root);

    {
        let mut store = LibraryStore::open(&db_path).unwrap();
        let mut zulu = Track::from_path(media_root.join("zulu.ogg"));
        zulu.title = "Zulu".to_owned();
        let mut alpha = Track::from_path(media_root.join("alpha.ogg"));
        alpha.title = "Alpha".to_owned();
        let mut middle = Track::from_path(media_root.join("middle.ogg"));
        middle.title = "Middle".to_owned();
        store.upsert_tracks(&[zulu, alpha, middle]).unwrap();
    }

    let first = unsafe { call_json(player_app_library_page(app, 0, 2)) };
    assert_ok(&first);
    assert_eq!(first["data"]["total"], 3);
    assert_eq!(first["data"]["offset"], 0);
    assert_eq!(first["data"]["tracks"].as_array().unwrap().len(), 2);
    assert_eq!(first["data"]["tracks"][0]["title"], "Alpha");
    assert_eq!(first["data"]["tracks"][1]["title"], "Middle");

    let second = unsafe { call_json(player_app_library_page(app, 2, 2)) };
    assert_ok(&second);
    assert_eq!(second["data"]["total"], 3);
    assert_eq!(second["data"]["offset"], 2);
    assert_eq!(second["data"]["tracks"].as_array().unwrap().len(), 1);
    assert_eq!(second["data"]["tracks"][0]["title"], "Zulu");

    let invalid = unsafe { call_json(player_app_library_page(app, 0, 0)) };
    assert_eq!(invalid["ok"], false);
    assert!(invalid["error"]
        .as_str()
        .unwrap()
        .contains("limit must be greater than zero"));

    unsafe { player_app_destroy(app) };
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn play_library_rejects_an_empty_library_before_opening_audio() {
    let db_path = temp_db_path("empty_library_playback");
    let media_root = temp_dir("empty_library_playback_media");
    fs::create_dir_all(&media_root).unwrap();
    let app = create_app(&db_path, &media_root);

    let response = unsafe { call_json(player_app_play_library(app)) };
    assert_eq!(response["ok"], false);
    assert!(response["error"]
        .as_str()
        .unwrap()
        .contains("library is empty"));
    unsafe {
        assert!((*app).engine.is_none());
        player_app_destroy(app);
    }

    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn import_skips_duplicate_audio_even_when_file_hash_differs() {
    let source_dir = temp_dir("duplicate_source");
    fs::create_dir_all(&source_dir).unwrap();
    let first = source_dir.join("first title.wav");
    let second = source_dir.join("second title.wav");
    write_test_wav(&first, b"first title").unwrap();
    write_test_wav(&second, b"second title").unwrap();
    assert_ne!(file_hash(&first).unwrap(), file_hash(&second).unwrap());
    assert_eq!(
        audio_hash(&first).unwrap().hash,
        audio_hash(&second).unwrap().hash
    );

    let db_path = temp_db_path("duplicate_audio");
    let media_root = temp_dir("duplicate_media");
    let app = create_app(&db_path, &media_root);

    let import = unsafe {
        call_json(player_app_import_folder(
            app,
            c_string_arg(&source_dir).as_ptr(),
        ))
    };
    assert_ok(&import);
    assert_eq!(import["data"]["imported"], 1);
    assert_eq!(import["data"]["copied"], 1);
    assert_eq!(import["data"]["duplicates_skipped"], 1);

    let library = unsafe { call_json(player_app_library(app)) };
    assert_ok(&library);
    assert_eq!(library["data"].as_array().unwrap().len(), 1);
    let track = &library["data"][0];
    assert_eq!(track["id"], track["view_id"]);
    assert!(track["id"].as_str().unwrap().starts_with("audio:"));
    assert_eq!(track["primary_view_id"], track["view_id"]);
    assert_eq!(track["is_primary_view"], true);
    assert_eq!(track["view_kind"], "primary");
    assert_eq!(track["format_name"], "wav");

    unsafe { player_app_destroy(app) };
    fs::remove_dir_all(source_dir).ok();
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn import_files_imports_selected_audio_without_requiring_folder_selection() {
    let db_path = temp_db_path("import_files");
    let media_root = temp_dir("import_files_media");
    let audio_root = workspace_root().join("test-assets").join("audio");
    let selected = [
        audio_root.join("into_the_oceans_chorus.ogg"),
        audio_root.join("funk_room_reverb.ogg"),
        audio_root.join("SOURCES.md"),
    ];
    let paths_json = serde_json::to_string(
        &selected
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let paths_arg = CString::new(paths_json).unwrap();
    let app = create_app(&db_path, &media_root);

    let import = unsafe { call_json(player_app_import_files(app, paths_arg.as_ptr())) };
    assert_ok(&import);
    assert_eq!(import["data"]["imported"], 2);
    assert_eq!(import["data"]["copied"], 2);
    assert_eq!(import["data"]["duplicates_skipped"], 0);
    assert_eq!(import["data"]["metadata_warnings"], 1);

    let library = unsafe { call_json(player_app_library(app)) };
    assert_ok(&library);
    let tracks = library["data"].as_array().unwrap();
    assert_eq!(tracks.len(), 2);
    let imported_file_names: HashSet<_> = tracks
        .iter()
        .map(|track| {
            Path::new(track["path"].as_str().unwrap())
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert!(imported_file_names.contains("into_the_oceans_chorus.ogg"));
    assert!(imported_file_names.contains("funk_room_reverb.ogg"));
    for track in tracks {
        let path = Path::new(track["path"].as_str().unwrap());
        assert!(path.starts_with(&media_root), "{}", path.display());
        assert!(path.exists(), "{}", path.display());
        assert_eq!(
            path.parent().and_then(Path::file_name).unwrap(),
            std::ffi::OsStr::new("audio")
        );
    }

    unsafe { player_app_destroy(app) };
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn audit_database_merges_existing_duplicate_tracks() {
    let db_path = temp_db_path("audit");
    let media_root = temp_dir("audit_media");
    fs::create_dir_all(&media_root).unwrap();
    let first = media_root.join("a.wav");
    let second = media_root.join("b.wav");
    write_test_wav(&first, b"same audio first").unwrap();
    write_test_wav(&second, b"same audio second").unwrap();
    let app = create_app(&db_path, &media_root);

    {
        let mut store = LibraryStore::open(&db_path).unwrap();
        store
            .upsert_tracks(&[
                Track::from_path(first.clone()),
                Track::from_path(second.clone()),
            ])
            .unwrap();
        store.create_playlist("Audit").unwrap();
        store.add_playlist_track("Audit", &second).unwrap();
        store.set_track_notes(&second, "duplicate note").unwrap();
    }

    let audit = unsafe { call_json(player_app_audit_database(app)) };
    assert_ok(&audit);
    assert_eq!(audit["data"]["tracks_scanned"], 2);
    assert_eq!(audit["data"]["duplicate_groups"], 1);
    assert_eq!(audit["data"]["tracks_merged"], 1);

    let library = unsafe { call_json(player_app_library(app)) };
    assert_ok(&library);
    assert_eq!(library["data"].as_array().unwrap().len(), 1);
    assert_eq!(library["data"][0]["path"], first.to_string_lossy().as_ref());

    let playlist = unsafe {
        call_json(player_app_playlist_tracks(
            app,
            c_string_arg("Audit").as_ptr(),
        ))
    };
    assert_eq!(
        playlist["data"][0]["path"],
        first.to_string_lossy().as_ref()
    );
    let details =
        unsafe { call_json(player_app_track_details(app, c_string_arg(&first).as_ptr())) };
    assert_eq!(details["data"]["notes"], "duplicate note");

    unsafe { player_app_destroy(app) };
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn app_reports_json_errors_for_bad_inputs() {
    let null_response = unsafe { call_json(player_app_library(ptr::null_mut())) };
    assert_eq!(null_response["ok"], false);
    assert!(null_response["error"]
        .as_str()
        .unwrap()
        .contains("PlayerApp handle is null"));

    let db_path = temp_db_path("errors");
    let media_root = temp_dir("errors_media");
    let app = create_app(&db_path, &media_root);
    let bad_playlist =
        unsafe { call_json(player_app_create_playlist(app, c_string_arg("").as_ptr())) };
    assert_eq!(bad_playlist["ok"], false);

    let repeat_alias = unsafe {
        call_json(player_app_set_repeat_mode(
            app,
            c_string_arg("loop").as_ptr(),
        ))
    };
    assert_eq!(repeat_alias["ok"], false);

    let zero_limit_search =
        unsafe { call_json(player_app_search(app, c_string_arg("anything").as_ptr(), 0)) };
    assert_eq!(zero_limit_search["ok"], false);
    assert!(zero_limit_search["error"]
        .as_str()
        .unwrap()
        .contains("greater than zero"));

    let poll = unsafe { call_json(player_app_poll(app)) };
    assert_ok(&poll);
    assert_eq!(poll["data"]["is_playing"], false);
    assert!(poll["data"]["current_track"].is_null());

    unsafe { player_app_destroy(app) };
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}
