use super::*;

#[test]
fn manages_playlists_in_order() {
    let mut store = LibraryStore::in_memory().unwrap();
    let first = Track::from_path("/music/a.ogg".into());
    let second = Track::from_path("/music/b.ogg".into());
    let third = Track::from_path("/music/c.ogg".into());
    store
        .upsert_tracks(&[first.clone(), second.clone(), third.clone()])
        .unwrap();

    let playlist_id = store.create_playlist("Road").unwrap();
    assert!(store.add_playlist_track("Road", "/music/a.ogg").unwrap());
    assert!(store.add_playlist_track("Road", "/music/b.ogg").unwrap());
    assert!(store.add_playlist_track("Road", "/music/c.ogg").unwrap());
    assert!(!store.add_playlist_track("Road", "/music/a.ogg").unwrap());
    let summaries = store.playlists().unwrap();
    let entries = store.playlist_tracks("Road").unwrap();

    assert!(playlist_id > 0);
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].name, "Road");
    assert_eq!(summaries[0].track_count, 3);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].position, 0);
    assert_eq!(entries[0].track.path, PathBuf::from("/music/a.ogg"));
    assert_eq!(entries[1].position, 1);

    assert!(store.move_playlist_track("Road", &third.path, -1).unwrap());
    let entries = store.playlist_tracks("Road").unwrap();
    assert_eq!(entries[1].track.path, third.path);
    assert_eq!(entries[2].track.path, second.path);

    assert_eq!(store.remove_playlist_track("Road", &third.path).unwrap(), 1);
    let entries = store.playlist_tracks("Road").unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.position)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(entries[0].track.path, first.path);
    assert_eq!(entries[1].track.path, second.path);

    assert_eq!(store.clear_playlist("Road").unwrap(), 2);
    assert!(store.playlist_tracks("Road").unwrap().is_empty());
    assert!(store.delete_playlist("Road").unwrap());
    assert!(store.playlists().unwrap().is_empty());
}

#[test]
fn schema_initialization_migrates_duplicate_playlist_memberships() {
    let mut store = LibraryStore::in_memory().unwrap();
    let track = Track::from_path("/music/a.ogg".into());
    store.upsert_track(&track).unwrap();
    let playlist_id = store.create_playlist("Road").unwrap();
    assert!(store.add_playlist_track("Road", &track.path).unwrap());

    store
        .conn
        .execute("DROP INDEX playlist_items_membership_idx", [])
        .unwrap();
    store
        .conn
        .execute(
            r#"
            INSERT INTO playlist_items
                (playlist_id, position, track_path, added_at_unix_seconds)
            VALUES (?1, 4, ?2, 1)
            "#,
            params![playlist_id, track.path.to_string_lossy().as_ref()],
        )
        .unwrap();

    store.initialize_schema().unwrap();

    let entries = store.playlist_tracks("Road").unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].position, 0);
    assert!(!store.add_playlist_track("Road", &track.path).unwrap());
}

#[test]
fn deleting_track_cascades_references_and_normalizes_order() {
    let mut store = LibraryStore::in_memory().unwrap();
    let first = Track::from_path("/music/a.ogg".into());
    let second = Track::from_path("/music/b.ogg".into());
    let third = Track::from_path("/music/c.ogg".into());
    store
        .upsert_tracks(&[first.clone(), second.clone(), third.clone()])
        .unwrap();
    store.add_playlist_track("Road", &first.path).unwrap();
    store.add_playlist_track("Road", &second.path).unwrap();
    store.add_playlist_track("Road", &third.path).unwrap();
    store.set_favorite(&second.path, true).unwrap();
    store.record_playback(&second.path, 42, true).unwrap();
    store
        .save_playback_queue(
            &[first.path.clone(), second.path.clone(), third.path.clone()],
            Some(1),
            12_345,
            RepeatMode::All,
            true,
        )
        .unwrap();
    store
        .set_track_artwork_reference(&second.path, &artwork_image(0, vec![1, 2, 3]))
        .unwrap();

    assert!(store.delete_track(&second.path).unwrap());
    assert!(!store.delete_track(&second.path).unwrap());
    assert!(store.track_by_path(&second.path).unwrap().is_none());
    assert!(store.favorite_tracks().unwrap().is_empty());
    assert!(store.play_history(10).unwrap().is_empty());

    let playlist = store.playlist_tracks("Road").unwrap();
    assert_eq!(
        playlist
            .iter()
            .map(|entry| (&entry.track.path, entry.position))
            .collect::<Vec<_>>(),
        vec![(&first.path, 0), (&third.path, 1)]
    );
    let queue = store.load_playback_queue().unwrap();
    assert_eq!(
        queue
            .tracks
            .iter()
            .map(|track| &track.path)
            .collect::<Vec<_>>(),
        vec![&first.path, &third.path]
    );
    assert_eq!(queue.current_index, Some(1));
    assert_eq!(queue.position_ms, 0);
    assert_eq!(queue.repeat_mode, RepeatMode::All);
    assert!(queue.shuffle_enabled);
    assert_eq!(
        store
            .conn
            .query_row("SELECT COUNT(*) FROM artwork_assets", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        0
    );
}

#[test]
fn searches_only_tracks_in_the_requested_playlist_and_preserves_order() {
    let mut store = LibraryStore::in_memory().unwrap();
    let mut first = Track::from_path("/music/first.ogg".into());
    first.title = "Ocean Intro".to_owned();
    let mut second = Track::from_path("/music/second.ogg".into());
    second.title = "Mountain Break".to_owned();
    second.artist = Some("Ocean Band".to_owned());
    let mut outside = Track::from_path("/music/outside.ogg".into());
    outside.title = "Ocean Outside".to_owned();
    store
        .upsert_tracks(&[first.clone(), second.clone(), outside])
        .unwrap();
    store.add_playlist_track("Road", &second.path).unwrap();
    store.add_playlist_track("Road", &first.path).unwrap();

    let results = store.search_playlist_tracks("Road", "ocean", 10).unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].track.path, second.path);
    assert_eq!(results[1].track.path, first.path);
    assert!(store
        .search_playlist_tracks("Missing", "ocean", 10)
        .unwrap()
        .is_empty());
}

#[test]
fn orders_recent_playlists_by_last_use() {
    let mut store = LibraryStore::in_memory().unwrap();
    store.create_playlist("Morning").unwrap();
    store.create_playlist("Night").unwrap();
    store
        .conn
        .execute(
            "UPDATE playlists SET last_used_at_unix_seconds = 10 WHERE name = 'Morning'",
            [],
        )
        .unwrap();
    store
        .conn
        .execute(
            "UPDATE playlists SET last_used_at_unix_seconds = 20 WHERE name = 'Night'",
            [],
        )
        .unwrap();

    let recent = store.recent_playlists(1).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].name, "Night");
    assert_eq!(recent[0].last_used_at_unix_seconds, 20);

    assert!(store.touch_playlist("Morning").unwrap());
    assert_eq!(
        store.recent_playlists(2).unwrap()[0].name,
        "Morning",
        "opening a playlist should move it to the front of Recents"
    );
    assert!(!store.touch_playlist("Missing").unwrap());
}

#[test]
fn rejects_zero_playlist_query_limits() {
    let store = LibraryStore::in_memory().unwrap();

    assert!(store.recent_playlists(0).is_err());
    assert!(store.search_playlist_tracks("Road", "ocean", 0).is_err());
}

#[test]
fn sorts_playlists_by_manual_title_artist_and_album() {
    let mut store = LibraryStore::in_memory().unwrap();
    let mut first = Track::from_path("/music/a.ogg".into());
    first.title = "Delta".to_owned();
    first.artist = Some("Beta".to_owned());
    first.album = Some("Second".to_owned());
    first.disc_number = Some(1);
    first.track_number = Some(2);
    first.user_rating = Some(8);

    let mut second = Track::from_path("/music/b.ogg".into());
    second.title = "Alpha".to_owned();
    second.artist = Some("Gamma".to_owned());
    second.album = Some("First".to_owned());
    second.disc_number = Some(1);
    second.track_number = Some(2);

    let mut third = Track::from_path("/music/c.ogg".into());
    third.title = "Charlie".to_owned();
    third.artist = Some("Alpha".to_owned());
    third.album = Some("First".to_owned());
    third.disc_number = Some(1);
    third.track_number = Some(1);
    third.user_rating = Some(10);

    store
        .upsert_tracks(&[first.clone(), second.clone(), third.clone()])
        .unwrap();
    store.add_playlist_track("Road", &second.path).unwrap();
    store.add_playlist_track("Road", &first.path).unwrap();
    store.add_playlist_track("Road", &third.path).unwrap();

    assert_playlist_paths(&store, "Road", &[&second.path, &first.path, &third.path]);

    assert_eq!(store.sort_playlist("Road", PlaylistSort::Title).unwrap(), 3);
    assert_playlist_paths(&store, "Road", &[&second.path, &third.path, &first.path]);

    assert_eq!(
        store.sort_playlist("Road", PlaylistSort::Artist).unwrap(),
        3
    );
    assert_playlist_paths(&store, "Road", &[&third.path, &first.path, &second.path]);

    assert_eq!(store.sort_playlist("Road", PlaylistSort::Album).unwrap(), 3);
    assert_playlist_paths(&store, "Road", &[&third.path, &second.path, &first.path]);

    assert_eq!(
        store.sort_playlist("Road", PlaylistSort::Rating).unwrap(),
        3
    );
    assert_playlist_paths(&store, "Road", &[&third.path, &first.path, &second.path]);

    assert_eq!(
        store.sort_playlist("Road", PlaylistSort::Manual).unwrap(),
        3
    );
    assert_playlist_paths(&store, "Road", &[&second.path, &first.path, &third.path]);
}

#[test]
fn renames_playlists_and_stores_playlist_artwork() {
    let mut store = LibraryStore::in_memory().unwrap();
    store.create_playlist("Road").unwrap();
    assert!(!store.playlists().unwrap()[0].has_artwork);

    store.rename_playlist("Road", "Night Drive").unwrap();
    store
        .save_playlist_artwork(
            "Night Drive",
            &ArtworkImage {
                picture_index: 0,
                mime_type: Some("image/png".to_owned()),
                picture_type: "CoverFront".to_owned(),
                description: Some("cover".to_owned()),
                data: vec![1, 2, 3],
            },
        )
        .unwrap();

    let summaries = store.playlists().unwrap();
    assert_eq!(summaries[0].name, "Night Drive");
    assert!(summaries[0].has_artwork);
    let artwork = store.playlist_artwork("Night Drive").unwrap().unwrap();
    assert_eq!(artwork.mime_type.as_deref(), Some("image/png"));
    assert_eq!(artwork.data, vec![1, 2, 3]);
}

#[test]
fn rejects_invalid_collection_references_and_empty_playlist_names() {
    let mut store = LibraryStore::in_memory().unwrap();

    assert!(store.create_playlist("   ").is_err());
    assert!(store
        .add_playlist_track("Road", "/music/missing.ogg")
        .is_err());
    assert!(store.set_favorite("/music/missing.ogg", true).is_err());
    assert!(store
        .record_playback("/music/missing.ogg", 0, false)
        .is_err());
    assert!(store
        .save_artwork(
            "/music/missing.ogg",
            &[ArtworkImage {
                picture_index: 0,
                mime_type: Some("image/png".to_owned()),
                picture_type: "CoverFront".to_owned(),
                description: None,
                data: vec![1, 2, 3],
            }],
        )
        .is_err());
}
