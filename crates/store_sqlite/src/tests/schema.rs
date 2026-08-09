use super::*;

#[test]
fn current_schema_initialization_does_not_upgrade_legacy_tables() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
            CREATE TABLE tracks (
                id TEXT NOT NULL,
                path TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL
            );
            "#,
    )
    .unwrap();
    let store = LibraryStore { conn };

    assert!(store.initialize_schema().is_err());
    let columns = store
        .conn
        .prepare("PRAGMA table_info(tracks)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(columns, ["id", "path", "title"]);
}
