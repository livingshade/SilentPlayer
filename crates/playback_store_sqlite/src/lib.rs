use std::path::{Path, PathBuf};

use domain::{
    GlobalQueueSnapshot, PlaybackMode, QueueItemId, ShuffleQueueSnapshot, Track, TrackViewId,
};
use errors::{PlayerError, PlayerResult};
use rusqlite::{params, Connection, OptionalExtension, Transaction};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredQueueItem {
    pub internal_id: QueueItemId,
    pub primary_view_id: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredPlaybackState {
    pub items: Vec<StoredQueueItem>,
    pub queue: GlobalQueueSnapshot,
    pub position_ms: u64,
}

pub struct PlaybackStateStore {
    connection: Connection,
}

impl PlaybackStateStore {
    pub fn open(path: impl AsRef<Path>) -> PlayerResult<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| PlayerError::io(parent, source))?;
        }
        let connection = Connection::open(path).map_err(to_store_error)?;
        connection
            .execute_batch(
                r#"
                PRAGMA foreign_keys = ON;
                PRAGMA journal_mode = WAL;

                CREATE TABLE IF NOT EXISTS queue_state (
                    singleton_id INTEGER PRIMARY KEY CHECK(singleton_id = 1),
                    next_internal_id INTEGER NOT NULL,
                    current_internal_id INTEGER,
                    playback_mode TEXT NOT NULL,
                    shuffle_activation_pending INTEGER NOT NULL,
                    position_ms INTEGER NOT NULL,
                    shuffle_active_cycle INTEGER,
                    shuffle_position INTEGER
                );

                CREATE TABLE IF NOT EXISTS queue_items (
                    base_position INTEGER PRIMARY KEY,
                    internal_id INTEGER NOT NULL UNIQUE,
                    primary_view_id TEXT NOT NULL,
                    path TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS shuffle_entries (
                    cycle_offset INTEGER NOT NULL,
                    cycle_position INTEGER NOT NULL,
                    internal_id INTEGER NOT NULL,
                    PRIMARY KEY(cycle_offset, cycle_position),
                    FOREIGN KEY(internal_id) REFERENCES queue_items(internal_id)
                        ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS shuffle_entries_internal_id_idx
                    ON shuffle_entries(internal_id);
                "#,
            )
            .map_err(to_store_error)?;
        Ok(Self { connection })
    }

    pub fn save(
        &mut self,
        queue_items: &[(QueueItemId, &Track)],
        snapshot: &GlobalQueueSnapshot,
        position_ms: u64,
    ) -> PlayerResult<()> {
        validate_save_input(queue_items, snapshot)?;
        let transaction = self.connection.transaction().map_err(to_store_error)?;
        transaction
            .execute("DELETE FROM shuffle_entries", [])
            .map_err(to_store_error)?;
        transaction
            .execute("DELETE FROM queue_items", [])
            .map_err(to_store_error)?;

        insert_queue_items(&transaction, queue_items)?;
        insert_shuffle_entries(&transaction, snapshot.shuffle.as_ref())?;
        let (active_cycle, shuffle_position) = snapshot
            .shuffle
            .as_ref()
            .map(|shuffle| {
                (
                    Some(to_i64(shuffle.active_cycle as u64)),
                    Some(to_i64(shuffle.position as u64)),
                )
            })
            .unwrap_or((None, None));
        transaction
            .execute(
                r#"
                INSERT INTO queue_state (
                    singleton_id,
                    next_internal_id,
                    current_internal_id,
                    playback_mode,
                    shuffle_activation_pending,
                    position_ms,
                    shuffle_active_cycle,
                    shuffle_position
                ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ON CONFLICT(singleton_id) DO UPDATE SET
                    next_internal_id = excluded.next_internal_id,
                    current_internal_id = excluded.current_internal_id,
                    playback_mode = excluded.playback_mode,
                    shuffle_activation_pending = excluded.shuffle_activation_pending,
                    position_ms = excluded.position_ms,
                    shuffle_active_cycle = excluded.shuffle_active_cycle,
                    shuffle_position = excluded.shuffle_position
                "#,
                params![
                    to_i64(snapshot.next_internal_id),
                    snapshot.current_id.map(|id| to_i64(id.value())),
                    snapshot.mode.as_str(),
                    snapshot.shuffle_activation_pending,
                    to_i64(position_ms),
                    active_cycle,
                    shuffle_position,
                ],
            )
            .map_err(to_store_error)?;
        transaction.commit().map_err(to_store_error)
    }

    pub fn load(&self) -> PlayerResult<Option<StoredPlaybackState>> {
        let state = self
            .connection
            .query_row(
                r#"
                SELECT next_internal_id,
                       current_internal_id,
                       playback_mode,
                       shuffle_activation_pending,
                       position_ms,
                       shuffle_active_cycle,
                       shuffle_position
                FROM queue_state
                WHERE singleton_id = 1
                "#,
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<i64>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, bool>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                        row.get::<_, Option<i64>>(6)?,
                    ))
                },
            )
            .optional()
            .map_err(to_store_error)?;
        let Some((next_id, current_id, mode, pending, position_ms, active_cycle, shuffle_position)) =
            state
        else {
            return Ok(None);
        };

        let items = self.load_items()?;
        let ordered_ids = items.iter().map(|item| item.internal_id).collect();
        let shuffle = self.load_shuffle(active_cycle, shuffle_position)?;
        Ok(Some(StoredPlaybackState {
            items,
            queue: GlobalQueueSnapshot {
                ordered_ids,
                next_internal_id: to_u64(next_id, "next internal ID")?,
                current_id: current_id
                    .map(|value| to_u64(value, "current internal ID").map(QueueItemId::from_value))
                    .transpose()?,
                mode: PlaybackMode::parse(&mode)
                    .map_err(|error| PlayerError::store(error.to_string()))?,
                shuffle_activation_pending: pending,
                shuffle,
            },
            position_ms: to_u64(position_ms, "position")?,
        }))
    }

    pub fn clear(&mut self) -> PlayerResult<()> {
        let transaction = self.connection.transaction().map_err(to_store_error)?;
        transaction
            .execute("DELETE FROM shuffle_entries", [])
            .map_err(to_store_error)?;
        transaction
            .execute("DELETE FROM queue_items", [])
            .map_err(to_store_error)?;
        transaction
            .execute("DELETE FROM queue_state", [])
            .map_err(to_store_error)?;
        transaction.commit().map_err(to_store_error)
    }

    /// Moves the pre-v2 queue out of a Library database exactly once.
    /// Returns true when legacy queue tables were found and removed.
    pub fn migrate_legacy_library(&mut self, library_path: impl AsRef<Path>) -> PlayerResult<bool> {
        let should_import = self.load()?.is_none();
        let mut library = Connection::open(library_path).map_err(to_store_error)?;
        let has_legacy_tables = library
            .query_row(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM sqlite_master
                    WHERE type = 'table' AND name = 'playback_queue_state'
                ) AND EXISTS(
                    SELECT 1 FROM sqlite_master
                    WHERE type = 'table' AND name = 'playback_queue_items'
                )
                "#,
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(to_store_error)?;
        if !has_legacy_tables {
            return Ok(false);
        }

        if should_import {
            let legacy_state = library
                .query_row(
                    r#"
                SELECT current_index, position_ms, repeat_mode, shuffle_enabled
                FROM playback_queue_state WHERE singleton_id = 1
                "#,
                    [],
                    |row| {
                        Ok((
                            row.get::<_, Option<i64>>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, bool>(3)?,
                        ))
                    },
                )
                .optional()
                .map_err(to_store_error)?;
            let mut statement = library
                .prepare(
                    r#"
                SELECT tracks.path, tracks.primary_view_id
                FROM playback_queue_items
                JOIN tracks ON tracks.path = playback_queue_items.track_path
                ORDER BY playback_queue_items.position
                "#,
                )
                .map_err(to_store_error)?;
            let legacy_items = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(to_store_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(to_store_error)?;
            drop(statement);

            if let Some((current_index, position_ms, repeat_mode, shuffle_enabled)) = legacy_state {
                let tracks = legacy_items
                    .iter()
                    .map(|(path, primary_view_id)| {
                        let mut track = Track::from_path(PathBuf::from(path));
                        track.primary_view_id = TrackViewId::from_value(primary_view_id.clone());
                        track
                    })
                    .collect::<Vec<_>>();
                let ordered_ids = (1..=tracks.len() as u64)
                    .map(QueueItemId::from_value)
                    .collect::<Vec<_>>();
                let mode = if repeat_mode == "one" {
                    PlaybackMode::RepeatOne
                } else if shuffle_enabled {
                    PlaybackMode::Shuffle
                } else {
                    PlaybackMode::Sequential
                };
                let snapshot = GlobalQueueSnapshot {
                    current_id: current_index
                        .and_then(|index| usize::try_from(index).ok())
                        .and_then(|index| ordered_ids.get(index).copied())
                        .or_else(|| ordered_ids.first().copied()),
                    next_internal_id: tracks.len() as u64 + 1,
                    ordered_ids,
                    mode,
                    shuffle_activation_pending: mode == PlaybackMode::Shuffle,
                    shuffle: None,
                };
                let queue_items = snapshot
                    .ordered_ids
                    .iter()
                    .copied()
                    .zip(tracks.iter())
                    .collect::<Vec<_>>();
                self.save(
                    &queue_items,
                    &snapshot,
                    u64::try_from(position_ms).unwrap_or(0),
                )?;
            }
        }

        let transaction = library.transaction().map_err(to_store_error)?;
        transaction
            .execute_batch("DROP TABLE playback_queue_items; DROP TABLE playback_queue_state;")
            .map_err(to_store_error)?;
        transaction.commit().map_err(to_store_error)?;
        Ok(true)
    }

    fn load_items(&self) -> PlayerResult<Vec<StoredQueueItem>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT internal_id, primary_view_id, path FROM queue_items ORDER BY base_position",
            )
            .map_err(to_store_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(to_store_error)?
            .map(|row| {
                let (internal_id, primary_view_id, path) = row.map_err(to_store_error)?;
                Ok(StoredQueueItem {
                    internal_id: QueueItemId::from_value(to_u64(internal_id, "internal ID")?),
                    primary_view_id,
                    path: PathBuf::from(path),
                })
            })
            .collect::<PlayerResult<Vec<_>>>()?;
        Ok(rows)
    }

    fn load_shuffle(
        &self,
        active_cycle: Option<i64>,
        shuffle_position: Option<i64>,
    ) -> PlayerResult<Option<ShuffleQueueSnapshot>> {
        let mut statement = self
            .connection
            .prepare(
                r#"
                SELECT cycle_offset, cycle_position, internal_id
                FROM shuffle_entries
                ORDER BY cycle_offset, cycle_position
                "#,
            )
            .map_err(to_store_error)?;
        let entries = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(to_store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_store_error)?;
        if entries.is_empty() {
            if active_cycle.is_some() || shuffle_position.is_some() {
                return Err(PlayerError::store(
                    "shuffle cursor exists without shuffle entries",
                ));
            }
            return Ok(None);
        }
        let active_cycle = to_usize(
            active_cycle.ok_or_else(|| PlayerError::store("missing shuffle active cycle"))?,
            "shuffle active cycle",
        )?;
        let position = to_usize(
            shuffle_position.ok_or_else(|| PlayerError::store("missing shuffle position"))?,
            "shuffle position",
        )?;
        let mut cycles = Vec::<Vec<QueueItemId>>::new();
        for (cycle, cycle_position, internal_id) in entries {
            let cycle = to_usize(cycle, "shuffle cycle")?;
            let cycle_position = to_usize(cycle_position, "shuffle cycle position")?;
            if cycle > cycles.len() {
                return Err(PlayerError::store("shuffle cycles are not contiguous"));
            }
            if cycle == cycles.len() {
                cycles.push(Vec::new());
            }
            if cycle_position != cycles[cycle].len() {
                return Err(PlayerError::store(
                    "shuffle cycle positions are not contiguous",
                ));
            }
            cycles[cycle].push(QueueItemId::from_value(to_u64(
                internal_id,
                "shuffle internal ID",
            )?));
        }
        Ok(Some(ShuffleQueueSnapshot {
            cycles,
            active_cycle,
            position,
        }))
    }
}

fn insert_queue_items(
    transaction: &Transaction<'_>,
    queue_items: &[(QueueItemId, &Track)],
) -> PlayerResult<()> {
    let mut statement = transaction
        .prepare(
            r#"
            INSERT INTO queue_items (base_position, internal_id, primary_view_id, path)
            VALUES (?1, ?2, ?3, ?4)
            "#,
        )
        .map_err(to_store_error)?;
    for (position, (internal_id, track)) in queue_items.iter().enumerate() {
        statement
            .execute(params![
                to_i64(position as u64),
                to_i64(internal_id.value()),
                track.primary_view_id.value(),
                track.path.to_string_lossy(),
            ])
            .map_err(to_store_error)?;
    }
    Ok(())
}

fn insert_shuffle_entries(
    transaction: &Transaction<'_>,
    shuffle: Option<&ShuffleQueueSnapshot>,
) -> PlayerResult<()> {
    let Some(shuffle) = shuffle else {
        return Ok(());
    };
    let mut statement = transaction
        .prepare(
            r#"
            INSERT INTO shuffle_entries (
                cycle_offset, cycle_position, internal_id
            ) VALUES (?1, ?2, ?3)
            "#,
        )
        .map_err(to_store_error)?;
    for (cycle, order) in shuffle.cycles.iter().enumerate() {
        for (position, internal_id) in order.iter().enumerate() {
            statement
                .execute(params![
                    to_i64(cycle as u64),
                    to_i64(position as u64),
                    to_i64(internal_id.value()),
                ])
                .map_err(to_store_error)?;
        }
    }
    Ok(())
}

fn validate_save_input(
    queue_items: &[(QueueItemId, &Track)],
    snapshot: &GlobalQueueSnapshot,
) -> PlayerResult<()> {
    let item_ids = queue_items.iter().map(|(id, _)| *id).collect::<Vec<_>>();
    if item_ids != snapshot.ordered_ids {
        return Err(PlayerError::invalid_input(
            "queue items do not match the snapshot internal ID order",
        ));
    }
    Ok(())
}

fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn to_u64(value: i64, label: &str) -> PlayerResult<u64> {
    u64::try_from(value).map_err(|_| PlayerError::store(format!("invalid {label}: {value}")))
}

fn to_usize(value: i64, label: &str) -> PlayerResult<usize> {
    usize::try_from(value).map_err(|_| PlayerError::store(format!("invalid {label}: {value}")))
}

fn to_store_error(error: rusqlite::Error) -> PlayerError {
    PlayerError::store(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{NormalizationSettings, PlayerSession};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn track(name: &str) -> Track {
        Track::from_path(format!("/{name}.mp3").into())
    }

    fn temporary_database(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("silent-{name}-{nonce}.sqlite3"))
    }

    #[test]
    fn exact_internal_order_and_shuffle_future_round_trip() {
        let path = temporary_database("playback-roundtrip");
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session
            .set_queue(vec![track("a"), track("b"), track("c")], 0)
            .unwrap();
        session.start_shuffled().unwrap();
        let snapshot = session.queue_snapshot();
        let mut store = PlaybackStateStore::open(&path).unwrap();
        store
            .save(&session.queue_items(), &snapshot, 4_321)
            .unwrap();

        let restored = store.load().unwrap().unwrap();

        assert_eq!(restored.queue, snapshot);
        assert_eq!(restored.position_ms, 4_321);
        assert_eq!(
            restored
                .items
                .iter()
                .map(|item| item.path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["/a.mp3", "/b.mp3", "/c.mp3"]
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn playback_database_contains_no_library_tables() {
        let path = temporary_database("playback-schema");
        let store = PlaybackStateStore::open(&path).unwrap();
        let table_count: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'tracks'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table_count, 0);
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn clear_removes_only_playback_state() {
        let path = temporary_database("playback-clear");
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session.set_queue(vec![track("a")], 0).unwrap();
        let mut store = PlaybackStateStore::open(&path).unwrap();
        store
            .save(&session.queue_items(), &session.queue_snapshot(), 0)
            .unwrap();
        store.clear().unwrap();
        assert!(store.load().unwrap().is_none());
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn migrates_and_removes_legacy_library_queue_tables() {
        let library_path = temporary_database("legacy-library");
        let playback_path = temporary_database("legacy-playback");
        let library = Connection::open(&library_path).unwrap();
        library
            .execute_batch(
                r#"
                CREATE TABLE tracks (
                    path TEXT PRIMARY KEY,
                    primary_view_id TEXT NOT NULL
                );
                CREATE TABLE playback_queue_state (
                    singleton_id INTEGER PRIMARY KEY,
                    current_index INTEGER,
                    position_ms INTEGER NOT NULL,
                    repeat_mode TEXT NOT NULL,
                    shuffle_enabled INTEGER NOT NULL
                );
                CREATE TABLE playback_queue_items (
                    position INTEGER PRIMARY KEY,
                    track_path TEXT NOT NULL
                );
                INSERT INTO tracks VALUES ('/a.mp3', 'audio:a'), ('/b.mp3', 'audio:b');
                INSERT INTO playback_queue_items VALUES (0, '/a.mp3'), (1, '/b.mp3');
                INSERT INTO playback_queue_state VALUES (1, 1, 987, 'all', 1);
                "#,
            )
            .unwrap();
        drop(library);

        let mut store = PlaybackStateStore::open(&playback_path).unwrap();
        assert!(store.migrate_legacy_library(&library_path).unwrap());
        let restored = store.load().unwrap().unwrap();

        assert_eq!(restored.position_ms, 987);
        assert_eq!(restored.queue.mode, PlaybackMode::Shuffle);
        assert!(restored.queue.shuffle_activation_pending);
        assert_eq!(restored.queue.current_id, Some(QueueItemId::from_value(2)));
        assert_eq!(restored.items.len(), 2);
        let library = Connection::open(&library_path).unwrap();
        let remaining: i64 = library
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name LIKE 'playback_queue%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 0);

        drop(store);
        drop(library);
        let _ = std::fs::remove_file(library_path);
        let _ = std::fs::remove_file(playback_path);
    }
}
