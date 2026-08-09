use super::*;

#[test]
fn track_details_find_imported_sidecar_artwork_and_lyrics() {
    let source_dir = temp_dir("detail_source");
    fs::create_dir_all(&source_dir).unwrap();
    let source_audio = source_dir.join("song.ogg");
    fs::copy(
        workspace_root()
            .join("test-assets")
            .join("audio")
            .join("into_the_oceans_chorus.ogg"),
        &source_audio,
    )
    .unwrap();
    fs::write(
        source_dir.join("song.lrc"),
        "[00:01.00]hello normal player\n",
    )
    .unwrap();
    fs::write(source_dir.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xD9]).unwrap();

    let db_dir = temp_dir("details_db");
    fs::create_dir_all(&db_dir).unwrap();
    let db_path = db_dir.join("library.sqlite3");
    let media_root = temp_dir("details_media");
    let app = create_app(&db_path, &media_root);

    let import = unsafe {
        call_json(player_app_import_folder(
            app,
            c_string_arg(&source_dir).as_ptr(),
        ))
    };
    assert_ok(&import);
    assert_eq!(import["data"]["imported"], 1);

    let library = unsafe { call_json(player_app_library(app)) };
    assert_ok(&library);
    let managed_path = library["data"][0]["path"].as_str().unwrap().to_owned();
    let details = unsafe {
        call_json(player_app_track_details(
            app,
            c_string_arg(&managed_path).as_ptr(),
        ))
    };
    assert_ok(&details);
    let data = &details["data"];

    let lyrics_path = PathBuf::from(data["lyrics_path"].as_str().unwrap());
    assert!(
        lyrics_path.starts_with(&media_root),
        "{}",
        lyrics_path.display()
    );
    assert!(lyrics_path.exists(), "{}", lyrics_path.display());
    assert!(data["lyrics_text"]
        .as_str()
        .unwrap()
        .contains("hello normal player"));
    assert_eq!(data["lyrics_document"]["format"], "lrc");
    assert_eq!(data["lyrics_document"]["content"]["kind"], "timed");
    assert_eq!(
        data["lyrics_document"]["content"]["lines"][0]["start_ms"],
        1_000
    );
    assert_eq!(
        data["lyrics_document"]["content"]["lines"][0]["text"],
        "hello normal player"
    );

    let artwork_path = PathBuf::from(data["artwork_path"].as_str().unwrap());
    assert!(
        artwork_path.starts_with(&media_root),
        "{}",
        artwork_path.display()
    );
    assert_eq!(artwork_path.file_name().unwrap(), "cover.jpg");

    let notes = unsafe {
        call_json(player_app_set_track_notes(
            app,
            c_string_arg(&managed_path).as_ptr(),
            c_string_arg("listen again").as_ptr(),
        ))
    };
    assert_ok(&notes);
    let notes_path = notes["data"]["path"].as_str().unwrap();
    assert_eq!(notes_path, managed_path);
    assert_eq!(notes["data"]["primary_view_id"], data["primary_view_id"]);
    assert_eq!(notes["data"]["view_kind"], "primary");
    let details = unsafe {
        call_json(player_app_track_details(
            app,
            c_string_arg(notes_path).as_ptr(),
        ))
    };
    assert_eq!(details["data"]["notes"], "listen again");
    assert!(details["data"]["lyrics_text"]
        .as_str()
        .unwrap()
        .contains("hello normal player"));

    unsafe { player_app_destroy(app) };
    fs::remove_dir_all(source_dir).ok();
    fs::remove_dir_all(db_dir).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn app_edits_primary_metadata_in_place() {
    let db_path = temp_db_path("metadata_edit");
    let media_root = temp_dir("metadata_edit_media");
    fs::create_dir_all(&media_root).unwrap();
    let source_path = media_root.join("first.ogg");
    fs::write(&source_path, b"not decoded by this test").unwrap();
    let app = create_app(&db_path, &media_root);

    {
        let mut first = Track::from_path(source_path.clone());
        first.title = "Original Title".to_owned();
        first.artist = Some("Original Artist".to_owned());
        first.album = Some("Original Album".to_owned());
        first.set_primary_audio_hash("same-audio");

        LibraryStore::open(&db_path)
            .unwrap()
            .upsert_track(&first)
            .unwrap();
    }
    unsafe {
        let app = &mut *app;
        let track = app
            .store()
            .unwrap()
            .track_by_path(&source_path)
            .unwrap()
            .unwrap();
        let dto = app.track_to_dto_with_artwork(&track).unwrap();
        app.current_track = Some(dto.clone());
        app.queue_tracks = vec![dto];
        app.queue_current_index = Some(0);
    }

    let edit = unsafe {
        call_json(player_app_set_track_metadata(
            app,
            c_string_arg(&source_path).as_ptr(),
            c_string_arg("Display Title").as_ptr(),
            c_string_arg("Display Artist").as_ptr(),
            c_string_arg("Display Album").as_ptr(),
        ))
    };
    assert_ok(&edit);
    let edited_path = edit["data"]["path"].as_str().unwrap();
    assert_eq!(edited_path, source_path.to_string_lossy());
    assert_eq!(edit["data"]["primary_view_id"], "audio:same-audio");
    assert_eq!(edit["data"]["view_kind"], "primary");
    assert_eq!(edit["data"]["title"], "Display Title");
    unsafe {
        let app = &*app;
        assert_eq!(app.current_track.as_ref().unwrap().title, "Display Title");
        assert_eq!(app.queue_tracks[0].title, "Display Title");
    }

    let library = unsafe { call_json(player_app_library(app)) };
    assert_ok(&library);
    let tracks = library["data"].as_array().unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0]["path"], source_path.to_string_lossy().as_ref());
    assert_eq!(tracks[0]["view_kind"], "primary");
    assert_eq!(tracks[0]["title"], "Display Title");

    let details = unsafe {
        call_json(player_app_track_details(
            app,
            c_string_arg(&source_path).as_ptr(),
        ))
    };
    assert_ok(&details);
    assert_eq!(details["data"]["audio_hash"], "same-audio");
    assert_eq!(details["data"]["original_title"], "Original Title");
    assert_eq!(details["data"]["original_artist"], "Original Artist");
    assert_eq!(details["data"]["original_album"], "Original Album");
    assert_eq!(details["data"]["display_title"], "Display Title");
    assert_eq!(details["data"]["display_artist"], "Display Artist");
    assert_eq!(details["data"]["display_album"], "Display Album");
    assert_eq!(details["data"]["play_count"], 0);
    assert_eq!(details["data"]["playback_session_count"], 0);
    assert!(details["data"]["last_played_at_unix_seconds"].is_null());
    assert!(details["data"]["last_completed_at_unix_seconds"].is_null());

    unsafe { player_app_destroy(app) };
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn app_rejects_a_non_primary_selector_for_editing() {
    let db_path = temp_db_path("derived_selector_edit");
    let media_root = temp_dir("derived_selector_edit_media");
    fs::create_dir_all(&media_root).unwrap();
    let primary_path = media_root.join("primary.ogg");
    let derived_path = media_root.join("derived.ogg");
    fs::write(&primary_path, b"same audio").unwrap();
    fs::write(&derived_path, b"same audio").unwrap();
    let app = create_app(&db_path, &media_root);

    {
        let mut primary = Track::from_path(primary_path.clone());
        primary.title = "Primary".to_owned();
        primary.set_primary_audio_hash("derived-selector-audio");
        let mut derived = primary.clone();
        derived.id = TrackId::from_path(&derived_path);
        derived.path = derived_path.clone();
        derived.view_id = TrackViewId::from_value("audio:derived-selector-audio:view:old");
        derived.view_kind = TrackViewKind::Derived;
        derived.transform_spec = Some(r#"{"kind":"old"}"#.to_owned());
        let mut store = LibraryStore::open(&db_path).unwrap();
        store.upsert_tracks(&[primary, derived]).unwrap();
    }

    let edited = unsafe {
        call_json(player_app_set_track_notes(
            app,
            c_string_arg(&derived_path).as_ptr(),
            c_string_arg("primary note").as_ptr(),
        ))
    };
    assert_eq!(edited["ok"], false);
    assert!(edited["error"]
        .as_str()
        .unwrap()
        .contains("not a primary view"));

    let store = LibraryStore::open(&db_path).unwrap();
    assert!(store.track_notes(&primary_path).unwrap().is_none());
    assert!(store.track_notes(&derived_path).unwrap().is_none());
    assert_eq!(store.tracks().unwrap().len(), 2);

    unsafe { player_app_destroy(app) };
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn app_sets_track_rating_via_ffi_and_reports_invalid_values() {
    let db_path = temp_db_path("rating");
    let media_root = temp_dir("rating_media");
    fs::create_dir_all(&media_root).unwrap();
    let track_path = media_root.join("rated.ogg");
    let app = create_app(&db_path, &media_root);

    {
        let mut track = Track::from_path(track_path.clone());
        track.title = "Rated Song".to_owned();
        track.set_primary_audio_hash("rating-audio");
        LibraryStore::open(&db_path)
            .unwrap()
            .upsert_track(&track)
            .unwrap();
    }

    let rated = unsafe {
        call_json(player_app_set_track_rating(
            app,
            c_string_arg(&track_path).as_ptr(),
            8,
        ))
    };
    assert_ok(&rated);
    assert_eq!(rated["data"]["rating"], 8);

    let details = unsafe {
        call_json(player_app_track_details(
            app,
            c_string_arg(&track_path).as_ptr(),
        ))
    };
    assert_ok(&details);
    assert_eq!(details["data"]["rating"], 8);

    let cleared = unsafe {
        call_json(player_app_set_track_rating(
            app,
            c_string_arg(&track_path).as_ptr(),
            0,
        ))
    };
    assert_ok(&cleared);
    assert!(cleared["data"]["rating"].is_null());

    let invalid_high = unsafe {
        call_json(player_app_set_track_rating(
            app,
            c_string_arg(&track_path).as_ptr(),
            11,
        ))
    };
    assert_eq!(invalid_high["ok"], false);
    assert!(invalid_high["error"]
        .as_str()
        .unwrap()
        .contains("between 1 and 10"));

    let invalid_negative = unsafe {
        call_json(player_app_set_track_rating(
            app,
            c_string_arg(&track_path).as_ptr(),
            -1,
        ))
    };
    assert_eq!(invalid_negative["ok"], false);

    unsafe { player_app_destroy(app) };
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn app_edits_full_primary_payload_in_place() {
    let db_path = temp_db_path("view_edit");
    let media_root = temp_dir("view_edit_media");
    fs::create_dir_all(&media_root).unwrap();
    let source_path = media_root.join("source.ogg");
    fs::write(&source_path, b"not decoded by this test").unwrap();
    let artwork_path = media_root.join("cover.png");
    fs::write(&artwork_path, b"\x89PNG\r\n\x1A\npayload").unwrap();
    let lyrics_path = media_root.join("words.lrc");
    fs::write(&lyrics_path, "[00:00.00]new words\n").unwrap();
    let app = create_app(&db_path, &media_root);

    {
        let mut source = Track::from_path(source_path.clone());
        source.title = "Source Title".to_owned();
        source.artist = Some("Source Artist".to_owned());
        source.album = Some("Source Album".to_owned());
        source.set_primary_audio_hash("view-edit-audio");
        let mut store = LibraryStore::open(&db_path).unwrap();
        store.upsert_track(&source).unwrap();
        store.set_track_notes(&source.path, "source note").unwrap();
    }

    let edit_payload = serde_json::json!({
        "title": "Edited Title",
        "artist": "Edited Artist",
        "album": "Edited Album",
        "notes": "edited note",
        "artwork_path": artwork_path.to_string_lossy(),
        "lyrics_path": lyrics_path.to_string_lossy()
    })
    .to_string();
    let edit = unsafe {
        call_json(player_app_edit_track_view(
            app,
            c_string_arg(&source_path).as_ptr(),
            c_string_arg(edit_payload.as_str()).as_ptr(),
        ))
    };
    assert_ok(&edit);
    assert_eq!(edit["data"]["title"], "Edited Title");
    assert_eq!(edit["data"]["view_kind"], "primary");
    let edited_path = edit["data"]["path"].as_str().unwrap();
    assert_eq!(edited_path, source_path.to_string_lossy());

    let library = unsafe { call_json(player_app_library(app)) };
    assert_ok(&library);
    let tracks = library["data"].as_array().unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0]["view_kind"], "primary");

    let details = unsafe {
        call_json(player_app_track_details(
            app,
            c_string_arg(edited_path).as_ptr(),
        ))
    };
    assert_ok(&details);
    assert_eq!(details["data"]["display_title"], "Edited Title");
    assert_eq!(details["data"]["display_artist"], "Edited Artist");
    assert_eq!(details["data"]["display_album"], "Edited Album");
    assert_eq!(details["data"]["notes"], "edited note");
    assert!(details["data"]["lyrics_text"]
        .as_str()
        .unwrap()
        .contains("new words"));
    assert!(Path::new(details["data"]["artwork_path"].as_str().unwrap()).exists());

    unsafe { player_app_destroy(app) };
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn app_sets_album_artwork_asset_for_album_tracks_persistently() {
    let db_dir = temp_dir("album_artwork_db");
    fs::create_dir_all(&db_dir).unwrap();
    let db_path = db_dir.join("library.sqlite");
    let media_root = temp_dir("album_artwork_media");
    fs::create_dir_all(&media_root).unwrap();
    let cover_path = media_root.join("album-cover.png");
    fs::write(&cover_path, b"\x89PNG\r\n\x1A\nalbum").unwrap();
    let first_path = media_root.join("01.ogg");
    let second_path = media_root.join("02.ogg");
    let other_path = media_root.join("other.ogg");
    let app = create_app(&db_path, &media_root);

    {
        let mut first = Track::from_path(first_path.clone());
        first.title = "First".to_owned();
        first.album = Some("Shared".to_owned());
        first.album_artist = Some("Band".to_owned());
        first.set_primary_audio_hash("album-artwork-a");

        let mut second = Track::from_path(second_path.clone());
        second.title = "Second".to_owned();
        second.album = Some("Shared".to_owned());
        second.artist = Some("Band".to_owned());
        second.set_primary_audio_hash("album-artwork-b");

        let mut other = Track::from_path(other_path.clone());
        other.title = "Other".to_owned();
        other.album = Some("Shared".to_owned());
        other.artist = Some("Other Band".to_owned());
        other.set_primary_audio_hash("album-artwork-c");

        LibraryStore::open(&db_path)
            .unwrap()
            .upsert_tracks(&[first, second, other])
            .unwrap();
    }

    let updated = unsafe {
        call_json(player_app_set_album_artwork(
            app,
            c_string_arg(&first_path).as_ptr(),
            c_string_arg(&cover_path).as_ptr(),
        ))
    };
    assert_ok(&updated);
    assert_eq!(updated["data"]["tracks_updated"], 2);

    let library = unsafe { call_json(player_app_library(app)) };
    assert_ok(&library);
    let album_artwork_tracks = library["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|track| track["artwork_source"] == "album")
        .collect::<Vec<_>>();
    assert_eq!(album_artwork_tracks.len(), 2);
    assert!(album_artwork_tracks.iter().all(|track| {
        Path::new(track["artwork_path"].as_str().unwrap())
            .starts_with(db_dir.join("Artwork").join("Assets"))
    }));
    assert!(album_artwork_tracks
        .iter()
        .all(|track| track["has_album_identity"] == true));
    fs::remove_file(&cover_path).unwrap();
    for path in [&first_path, &second_path] {
        let details =
            unsafe { call_json(player_app_track_details(app, c_string_arg(path).as_ptr())) };
        assert_ok(&details);
        let artwork_path = PathBuf::from(details["data"]["artwork_path"].as_str().unwrap());
        assert!(artwork_path.starts_with(db_dir.join("Artwork").join("Assets")));
        assert_eq!(fs::read(&artwork_path).unwrap(), b"\x89PNG\r\n\x1A\nalbum");
        assert_eq!(details["data"]["artwork_source"], "album");
    }

    let other_details = unsafe {
        call_json(player_app_track_details(
            app,
            c_string_arg(&other_path).as_ptr(),
        ))
    };
    assert_ok(&other_details);
    assert!(other_details["data"]["artwork_path"].is_null());

    unsafe { player_app_destroy(app) };
    fs::remove_dir_all(db_dir).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn app_track_artwork_reference_overrides_album_artwork_reference() {
    let db_path = temp_db_path("track_over_album_artwork");
    let media_root = temp_dir("track_over_album_artwork_media");
    fs::create_dir_all(&media_root).unwrap();
    let album_cover = media_root.join("album-reference.png");
    let track_cover = media_root.join("track-reference.png");
    fs::write(&album_cover, b"\x89PNG\r\n\x1A\nalbum").unwrap();
    fs::write(&track_cover, b"\x89PNG\r\n\x1A\ntrack").unwrap();
    let source_path = media_root.join("song.ogg");
    fs::write(&source_path, b"not decoded by this test").unwrap();
    let app = create_app(&db_path, &media_root);

    {
        let mut track = Track::from_path(source_path.clone());
        track.title = "Song".to_owned();
        track.album = Some("Album".to_owned());
        track.artist = Some("Artist".to_owned());
        track.set_primary_audio_hash("track-over-album-artwork");
        LibraryStore::open(&db_path)
            .unwrap()
            .upsert_track(&track)
            .unwrap();
    }

    let album = unsafe {
        call_json(player_app_set_album_artwork(
            app,
            c_string_arg(&source_path).as_ptr(),
            c_string_arg(&album_cover).as_ptr(),
        ))
    };
    assert_ok(&album);

    let track_edit = unsafe {
        call_json(player_app_set_track_artwork(
            app,
            c_string_arg(&source_path).as_ptr(),
            c_string_arg(&track_cover).as_ptr(),
        ))
    };
    assert_ok(&track_edit);
    assert_eq!(track_edit["data"]["view_kind"], "primary");
    let track_edit_artwork_path =
        PathBuf::from(track_edit["data"]["artwork_path"].as_str().unwrap());
    assert!(track_edit_artwork_path
        .starts_with(db_path.parent().unwrap().join("Artwork").join("Assets")));
    assert_eq!(
        fs::read(&track_edit_artwork_path).unwrap(),
        b"\x89PNG\r\n\x1A\ntrack"
    );
    assert_eq!(track_edit["data"]["artwork_source"], "track");
    let edited_path = PathBuf::from(track_edit["data"]["path"].as_str().unwrap());
    assert_eq!(edited_path, source_path);
    fs::remove_file(&track_cover).unwrap();
    fs::remove_file(&album_cover).unwrap();

    let details = unsafe {
        call_json(player_app_track_details(
            app,
            c_string_arg(&edited_path).as_ptr(),
        ))
    };
    assert_ok(&details);
    let details_artwork_path = PathBuf::from(details["data"]["artwork_path"].as_str().unwrap());
    assert!(
        details_artwork_path.starts_with(db_path.parent().unwrap().join("Artwork").join("Assets"))
    );
    assert_eq!(
        fs::read(&details_artwork_path).unwrap(),
        b"\x89PNG\r\n\x1A\ntrack"
    );
    assert_eq!(details["data"]["artwork_source"], "track");

    let source_details = unsafe {
        call_json(player_app_track_details(
            app,
            c_string_arg(&source_path).as_ptr(),
        ))
    };
    let source_artwork_path =
        PathBuf::from(source_details["data"]["artwork_path"].as_str().unwrap());
    assert!(
        source_artwork_path.starts_with(db_path.parent().unwrap().join("Artwork").join("Assets"))
    );
    assert_eq!(
        fs::read(&source_artwork_path).unwrap(),
        b"\x89PNG\r\n\x1A\ntrack"
    );
    assert_eq!(source_details["data"]["artwork_source"], "track");

    let library = unsafe { call_json(player_app_library(app)) };
    assert_ok(&library);
    let tracks = library["data"].as_array().unwrap();
    assert_eq!(tracks.len(), 1);
    let primary = tracks
        .iter()
        .find(|track| track["is_primary_view"] == true)
        .unwrap();
    assert_eq!(primary["artwork_source"], "track");

    unsafe { player_app_destroy(app) };
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
}

#[test]
fn app_materializes_current_music_view_as_independent_primary() {
    let db_path = temp_db_path("export_view");
    let media_root = temp_dir("export_view_media");
    fs::create_dir_all(&media_root).unwrap();
    let track_path = media_root.join("exportable.wav");
    let export_path = temp_dir("export_view_out").join("portable.wav");
    write_test_wav(&track_path, b"portable source").unwrap();
    fs::write(
        media_root.join("exportable.lrc"),
        "[00:00.00]portable lyric\n",
    )
    .unwrap();
    let audio = audio_hash(&track_path).unwrap().hash;
    let app = create_app(&db_path, &media_root);

    {
        let mut store = LibraryStore::open(&db_path).unwrap();
        let mut track = Track::from_path(track_path.clone());
        track.title = "Portable Title".to_owned();
        track.artist = Some("Portable Artist".to_owned());
        track.album = Some("Portable Album".to_owned());
        track.set_primary_audio_hash(audio.clone());
        store.upsert_track(&track).unwrap();
        store
            .set_track_notes(&track.path, "portable notes")
            .unwrap();
        store
            .save_artwork(
                &track.path,
                &[ArtworkImage {
                    picture_index: 0,
                    mime_type: Some("image/png".to_owned()),
                    picture_type: "CoverFront".to_owned(),
                    description: None,
                    data: vec![1, 2, 3, 4],
                }],
            )
            .unwrap();
    }

    let export = unsafe {
        call_json(player_app_export_track_view(
            app,
            c_string_arg(&track_path).as_ptr(),
            c_string_arg(&export_path).as_ptr(),
        ))
    };
    assert_ok(&export);
    assert_eq!(
        fs::read(&export_path).unwrap(),
        fs::read(&track_path).unwrap()
    );
    assert!(export_path.with_extension("lrc").exists());
    assert_eq!(
        export["data"]["path"],
        export_path.to_string_lossy().as_ref()
    );
    assert_eq!(export["data"]["title"], "Portable Title");
    assert_eq!(export["data"]["view_kind"], "primary");
    assert_eq!(export["data"]["is_primary_view"], true);
    assert_eq!(export["data"]["view_id"], export["data"]["primary_view_id"]);
    assert_ne!(export["data"]["primary_view_id"], format!("audio:{audio}"));
    assert!(export["data"]["primary_view_id"]
        .as_str()
        .unwrap()
        .starts_with(&format!("audio:{audio}:materialized:")));

    let details = unsafe {
        call_json(player_app_track_details(
            app,
            c_string_arg(&export_path).as_ptr(),
        ))
    };
    assert_ok(&details);
    assert_eq!(details["data"]["audio_hash"], audio);
    assert_eq!(details["data"]["is_primary_view"], true);
    assert_eq!(details["data"]["original_title"], "Portable Title");
    assert_eq!(details["data"]["display_title"], "Portable Title");
    assert_eq!(details["data"]["notes"], "portable notes");
    assert!(details["data"]["lyrics_text"]
        .as_str()
        .unwrap()
        .contains("portable lyric"));
    assert!(Path::new(details["data"]["artwork_path"].as_str().unwrap()).exists());

    let library = unsafe { call_json(player_app_library(app)) };
    assert_ok(&library);
    let tracks = library["data"].as_array().unwrap();
    assert_eq!(tracks.len(), 2);
    assert_eq!(
        tracks
            .iter()
            .filter(|track| track["view_kind"] == "primary")
            .count(),
        2
    );

    unsafe { player_app_destroy(app) };
    fs::remove_file(db_path).ok();
    fs::remove_dir_all(media_root).ok();
    fs::remove_dir_all(export_path.parent().unwrap()).ok();
}

#[test]
fn track_details_exports_cached_artwork_to_persistent_file() {
    let db_dir = temp_dir("artwork_db");
    fs::create_dir_all(&db_dir).unwrap();
    let db_path = db_dir.join("library.sqlite3");
    let media_root = temp_dir("artwork_media");
    let app = create_app(&db_path, &media_root);
    let track_path = media_root.join("song.ogg");
    let image_data = vec![0xFF, 0xD8, 0xFF, 0xD9];

    {
        let mut store = LibraryStore::open(&db_path).unwrap();
        let mut track = Track::from_path(track_path.clone());
        track.set_primary_audio_hash("artwork-audio-hash");
        store.upsert_track(&track).unwrap();
        store
            .save_artwork(
                &track_path,
                &[ArtworkImage {
                    picture_index: 0,
                    mime_type: Some("image/jpeg".to_owned()),
                    picture_type: "CoverFront".to_owned(),
                    description: None,
                    data: image_data.clone(),
                }],
            )
            .unwrap();
    }

    let details = unsafe {
        call_json(player_app_track_details(
            app,
            c_string_arg(&track_path).as_ptr(),
        ))
    };
    assert_ok(&details);
    let artwork_path = PathBuf::from(details["data"]["artwork_path"].as_str().unwrap());
    assert!(artwork_path.starts_with(db_dir.join("Artwork")));
    assert!(artwork_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("audio-artwork-audio-hash-"));
    assert_eq!(artwork_path.extension().unwrap(), "jpg");
    assert_eq!(fs::read(artwork_path).unwrap(), image_data);
    assert!(details["data"]["lyrics_path"].is_null());
    assert!(details["data"]["lyrics_text"].is_null());
    assert_eq!(details["data"]["lyrics_document"]["format"], "instrumental");
    assert_eq!(
        details["data"]["lyrics_document"]["instrumental_token"],
        "♪"
    );
    assert_eq!(
        details["data"]["lyrics_document"]["content"]["kind"],
        "instrumental"
    );
    assert_eq!(details["data"]["audio_hash"], "artwork-audio-hash");
    assert_eq!(details["data"]["view_id"], "audio:artwork-audio-hash");
    assert_eq!(
        details["data"]["primary_view_id"],
        "audio:artwork-audio-hash"
    );
    assert_eq!(details["data"]["is_primary_view"], true);
    assert_eq!(details["data"]["view_kind"], "primary");
    assert_eq!(details["data"]["format_name"], "ogg");
    assert!(details["data"]["quality_profile"].is_null());

    unsafe { player_app_destroy(app) };
    fs::remove_dir_all(db_dir).ok();
    fs::remove_dir_all(media_root).ok();
}
