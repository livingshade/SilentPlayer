use super::*;

#[test]
fn persists_playback_queue_and_updates_exported_track_paths() {
    let mut store = LibraryStore::in_memory().unwrap();
    let first = Track::from_path("/old-library/a.ogg".into());
    let second = Track::from_path("/old-library/b.ogg".into());
    store
        .upsert_tracks(&[first.clone(), second.clone()])
        .unwrap();
    store
        .save_playback_queue(
            &[first.path.clone(), second.path.clone()],
            Some(1),
            2_345,
            RepeatMode::All,
            true,
        )
        .unwrap();

    let restored = store.load_playback_queue().unwrap();
    assert_eq!(
        restored
            .tracks
            .iter()
            .map(|track| track.path.clone())
            .collect::<Vec<_>>(),
        vec![first.path.clone(), second.path.clone()]
    );
    assert_eq!(restored.current_index, Some(1));
    assert_eq!(restored.position_ms, 2_345);
    assert_eq!(restored.repeat_mode, RepeatMode::All);
    assert!(restored.shuffle_enabled);

    let relocated = PathBuf::from("/new-library/b.ogg");
    store
        .replace_track_paths(&[(second.path.clone(), relocated.clone())])
        .unwrap();
    assert_eq!(
        store.load_playback_queue().unwrap().tracks[1].path,
        relocated
    );

    store.zero_out().unwrap();
    let empty = store.load_playback_queue().unwrap();
    assert!(empty.tracks.is_empty());
    assert_eq!(empty.current_index, None);
    assert_eq!(empty.position_ms, 0);
}

#[test]
fn toggles_favorites_and_records_history() {
    let mut store = LibraryStore::in_memory().unwrap();
    let track = Track::from_path("/music/song.ogg".into());
    store.upsert_track(&track).unwrap();

    store.set_favorite("/music/song.ogg", true).unwrap();
    assert_eq!(store.favorite_tracks().unwrap().len(), 1);

    store
        .record_playback("/music/song.ogg", 42_000, true)
        .unwrap();
    let history = store.play_history(5).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].position_ms, 42_000);
    assert!(history[0].completed);

    store
        .record_playback("/music/song.ogg", 5_000, false)
        .unwrap();
    let stats = store.playback_stats("/music/song.ogg").unwrap();
    assert_eq!(stats.play_count, 1);
    assert_eq!(stats.session_count, 2);
    assert!(stats.last_played_at_unix_seconds.is_some());
    assert!(stats.last_completed_at_unix_seconds.is_some());

    let empty_stats = store.playback_stats("/music/other.ogg").unwrap();
    assert_eq!(empty_stats.play_count, 0);
    assert_eq!(empty_stats.session_count, 0);
    assert_eq!(empty_stats.last_played_at_unix_seconds, None);
    assert_eq!(empty_stats.last_completed_at_unix_seconds, None);

    store.set_favorite("/music/song.ogg", false).unwrap();
    assert_eq!(store.favorite_tracks().unwrap().len(), 0);
}

#[test]
fn rejects_zero_history_limit() {
    let store = LibraryStore::in_memory().unwrap();

    let error = store.play_history(0).unwrap_err();

    assert!(error.to_string().contains("greater than zero"));
}
