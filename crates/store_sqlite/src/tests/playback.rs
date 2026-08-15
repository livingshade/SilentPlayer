use super::*;

#[test]
fn library_database_contains_no_playback_queue_schema() {
    let store = LibraryStore::in_memory().unwrap();
    let queue_tables: i64 = store
        .conn
        .query_row(
            r#"
            SELECT COUNT(*) FROM sqlite_master
            WHERE type = 'table' AND name IN (
                'playback_queue_state',
                'playback_queue_items',
                'queue_state',
                'queue_items',
                'shuffle_entries'
            )
            "#,
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(queue_tables, 0);
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
