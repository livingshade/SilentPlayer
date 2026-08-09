use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use domain::{
    ArtworkImage, FileFingerprint, LoudnessInfo, RepeatMode, Track, TrackId, TrackViewId,
    TrackViewKind,
};
use errors::{PlayerError, PlayerResult};
use rusqlite::{params, Connection, OptionalExtension};

mod analysis;
mod metadata_artwork;
mod playback;
mod playlists;
mod tracks;

#[derive(Clone, Debug, PartialEq)]
pub struct AlbumGroup {
    pub album_key: String,
    pub album_artist: Option<String>,
    pub album: String,
    pub tracks: Vec<Track>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaylistSummary {
    pub id: i64,
    pub name: String,
    pub track_count: usize,
    pub has_artwork: bool,
    pub created_at_unix_seconds: i64,
    pub updated_at_unix_seconds: i64,
    pub last_used_at_unix_seconds: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlaylistEntry {
    pub item_id: i64,
    pub position: u32,
    pub track: Track,
}

#[derive(Clone, Debug)]
pub struct StoredPlaybackQueue {
    pub tracks: Vec<Track>,
    pub current_index: Option<usize>,
    pub position_ms: u64,
    pub repeat_mode: RepeatMode,
    pub shuffle_enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackMetadataView {
    pub view_id: String,
    pub primary_view_id: String,
    pub view_kind: String,
    pub transform_spec: Option<String>,
    pub quality_profile: Option<String>,
    pub format_name: Option<String>,
    pub view_name: Option<String>,
    pub user_rating: Option<u8>,
    pub audio_hash: Option<String>,
    pub original_title: String,
    pub original_artist: Option<String>,
    pub original_album: Option<String>,
    pub display_title: String,
    pub display_artist: Option<String>,
    pub display_album: Option<String>,
    pub metadata_edited_at_unix_seconds: Option<i64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaylistSort {
    Manual,
    Title,
    Artist,
    Album,
    Rating,
}

impl PlaylistSort {
    pub fn parse(value: &str) -> PlayerResult<Self> {
        match value {
            "manual" => Ok(Self::Manual),
            "title" => Ok(Self::Title),
            "artist" => Ok(Self::Artist),
            "album" => Ok(Self::Album),
            "rating" => Ok(Self::Rating),
            other => Err(PlayerError::store(format!(
                "unknown playlist sort mode: {other}"
            ))),
        }
    }
}

#[cfg(test)]
#[test]
fn playlist_sort_parser_accepts_only_canonical_values() {
    assert_eq!(PlaylistSort::parse("manual").unwrap(), PlaylistSort::Manual);
    assert_eq!(PlaylistSort::parse("title").unwrap(), PlaylistSort::Title);
    assert_eq!(PlaylistSort::parse("artist").unwrap(), PlaylistSort::Artist);
    assert_eq!(PlaylistSort::parse("album").unwrap(), PlaylistSort::Album);
    assert_eq!(PlaylistSort::parse("rating").unwrap(), PlaylistSort::Rating);
    for alias in ["default", "position", "name", "author", "score", "Title"] {
        assert!(PlaylistSort::parse(alias).is_err(), "{alias}");
    }
}

#[derive(Clone, Debug)]
struct PlaylistSortItem {
    item_id: i64,
    track_path: String,
    title: String,
    artist: Option<String>,
    album: Option<String>,
    disc_number: Option<u32>,
    track_number: Option<u32>,
    user_rating: Option<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayHistoryEntry {
    pub id: i64,
    pub played_at_unix_seconds: i64,
    pub position_ms: u64,
    pub completed: bool,
    pub track: Track,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackStats {
    pub play_count: u64,
    pub session_count: u64,
    pub last_played_at_unix_seconds: Option<i64>,
    pub last_completed_at_unix_seconds: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtworkSummary {
    pub path: PathBuf,
    pub image_count: usize,
    pub byte_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtworkReferenceScope {
    Track,
    Album,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtworkReference {
    pub asset_id: String,
    pub image: ArtworkImage,
    pub scope: ArtworkReferenceScope,
}

pub struct LibraryStore {
    conn: Connection,
}

impl LibraryStore {
    pub fn open(path: impl AsRef<Path>) -> PlayerResult<Self> {
        let conn = Connection::open(path).map_err(to_store_error)?;
        let store = Self { conn };
        store.initialize_schema()?;
        Ok(store)
    }

    pub fn in_memory() -> PlayerResult<Self> {
        let conn = Connection::open_in_memory().map_err(to_store_error)?;
        let store = Self { conn };
        store.initialize_schema()?;
        Ok(store)
    }

    fn playlist_id_by_name(&self, name: &str) -> PlayerResult<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM playlists WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .optional()
            .map_err(to_store_error)
    }

    fn next_playlist_use_timestamp(&self) -> PlayerResult<i64> {
        let latest = self
            .conn
            .query_row(
                "SELECT MAX(last_used_at_unix_seconds) FROM playlists",
                [],
                |row| row.get::<_, Option<i64>>(0),
            )
            .map_err(to_store_error)?
            .unwrap_or(0);
        Ok(now_unix_seconds().max(latest.saturating_add(1)))
    }

    fn next_playlist_position(&self, playlist_id: i64) -> PlayerResult<u32> {
        let position: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(position), -1) + 1 FROM playlist_items WHERE playlist_id = ?1",
                params![playlist_id],
                |row| row.get(0),
            )
            .map_err(to_store_error)?;
        Ok(optional_u32(Some(position)).unwrap_or(u32::MAX))
    }

    fn playlist_item_rows(&self, playlist_id: i64) -> PlayerResult<Vec<(i64, u32, String)>> {
        let mut stmt = self
            .conn
            .prepare(
                r#"
                SELECT id, position, track_path
                FROM playlist_items
                WHERE playlist_id = ?1
                ORDER BY position, id
                "#,
            )
            .map_err(to_store_error)?;
        let rows = stmt
            .query_map(params![playlist_id], |row| {
                Ok((
                    row.get(0)?,
                    optional_u32(Some(row.get::<_, i64>(1)?)).unwrap_or(0),
                    row.get(2)?,
                ))
            })
            .map_err(to_store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_store_error)?;
        Ok(rows)
    }

    fn playlist_sort_items(&self, playlist_id: i64) -> PlayerResult<Vec<PlaylistSortItem>> {
        let mut stmt = self
            .conn
            .prepare(
                r#"
                SELECT playlist_items.id, playlist_items.track_path,
                       tracks.title, tracks.artist, tracks.album,
                       tracks.disc_number, tracks.track_number, tracks.user_rating
                FROM playlist_items
                JOIN tracks ON tracks.path = playlist_items.track_path
                WHERE playlist_items.playlist_id = ?1
                ORDER BY playlist_items.position, playlist_items.id
                "#,
            )
            .map_err(to_store_error)?;
        let rows = stmt
            .query_map(params![playlist_id], |row| {
                Ok(PlaylistSortItem {
                    item_id: row.get(0)?,
                    track_path: row.get(1)?,
                    title: row.get(2)?,
                    artist: row.get(3)?,
                    album: row.get(4)?,
                    disc_number: optional_u32(row.get::<_, Option<i64>>(5)?),
                    track_number: optional_u32(row.get::<_, Option<i64>>(6)?),
                    user_rating: optional_rating(row.get::<_, Option<i64>>(7)?),
                })
            })
            .map_err(to_store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_store_error)?;
        Ok(rows)
    }

    fn artwork_reference_from_table(
        &self,
        table: &str,
        path: &Path,
        scope: ArtworkReferenceScope,
    ) -> PlayerResult<Option<ArtworkReference>> {
        let sql = format!(
            r#"
            SELECT refs.asset_id, assets.mime_type, assets.description, assets.data
            FROM {table} AS refs
            JOIN artwork_assets AS assets ON assets.asset_id = refs.asset_id
            WHERE refs.track_path = ?1
            "#
        );
        self.conn
            .query_row(&sql, params![path_to_string(path)], |row| {
                Ok(ArtworkReference {
                    asset_id: row.get(0)?,
                    image: ArtworkImage {
                        picture_index: 0,
                        mime_type: row.get(1)?,
                        picture_type: "CoverFront".to_owned(),
                        description: row.get(2)?,
                        data: row.get(3)?,
                    },
                    scope,
                })
            })
            .optional()
            .map_err(to_store_error)
    }

    fn normalize_playlist_positions(&mut self, playlist_id: i64) -> PlayerResult<()> {
        let items = self.playlist_item_rows(playlist_id)?;
        let item_ids = items
            .iter()
            .map(|(item_id, _, _)| *item_id)
            .collect::<Vec<_>>();
        self.rewrite_playlist_positions(playlist_id, &item_ids)
    }

    fn rewrite_playlist_positions(
        &mut self,
        playlist_id: i64,
        item_ids: &[i64],
    ) -> PlayerResult<()> {
        let tx = self.conn.transaction().map_err(to_store_error)?;
        for (index, item_id) in item_ids.iter().enumerate() {
            tx.execute(
                "UPDATE playlist_items SET position = ?2 WHERE id = ?1",
                params![*item_id, saturating_i64_from_u64(index as u64)],
            )
            .map_err(to_store_error)?;
        }
        tx.execute(
            "UPDATE playlists SET updated_at_unix_seconds = ?2 WHERE id = ?1",
            params![playlist_id, now_unix_seconds()],
        )
        .map_err(to_store_error)?;
        tx.commit().map_err(to_store_error)?;
        Ok(())
    }

    fn initialize_schema(&self) -> PlayerResult<()> {
        self.conn
            .execute_batch(
                r#"
                PRAGMA foreign_keys = ON;

                CREATE TABLE IF NOT EXISTS tracks (
                    id TEXT NOT NULL,
                    path TEXT NOT NULL UNIQUE,
                    title TEXT NOT NULL,
                    artist TEXT,
                    album TEXT,
                    original_title TEXT NOT NULL,
                    original_artist TEXT,
                    original_album TEXT,
                    metadata_edited_at_unix_seconds INTEGER,
                    album_artist TEXT,
                    genre TEXT,
                    track_number INTEGER,
                    disc_number INTEGER,
                    year INTEGER,
                    duration_ms INTEGER,
                    artwork_count INTEGER NOT NULL DEFAULT 0,
                    size_bytes INTEGER,
                    modified_unix_seconds INTEGER,
                    file_hash TEXT,
                    audio_hash TEXT,
                    view_id TEXT NOT NULL,
                    primary_view_id TEXT NOT NULL,
                    view_kind TEXT NOT NULL,
                    transform_spec TEXT,
                    quality_profile TEXT,
                    format_name TEXT,
                    view_name TEXT,
                    user_rating INTEGER,
                    integrated_lufs REAL,
                    true_peak_dbtp REAL,
                    album_integrated_lufs REAL,
                    album_true_peak_dbtp REAL,
                    analysis_version INTEGER,
                    analysis_size_bytes INTEGER,
                    analysis_modified_unix_seconds INTEGER,
                    analyzed_at_unix_seconds INTEGER,
                    added_at_unix_seconds INTEGER NOT NULL,
                    updated_at_unix_seconds INTEGER NOT NULL
                );

                CREATE INDEX IF NOT EXISTS tracks_title_idx ON tracks(title);
                CREATE INDEX IF NOT EXISTS tracks_album_idx ON tracks(album);
                CREATE INDEX IF NOT EXISTS tracks_artist_idx ON tracks(artist);
                CREATE INDEX IF NOT EXISTS tracks_analysis_idx
                    ON tracks(analysis_version, integrated_lufs, true_peak_dbtp);
                CREATE TABLE IF NOT EXISTS playlists (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    created_at_unix_seconds INTEGER NOT NULL,
                    updated_at_unix_seconds INTEGER NOT NULL,
                    last_used_at_unix_seconds INTEGER NOT NULL DEFAULT 0
                );

                CREATE TABLE IF NOT EXISTS artwork_assets (
                    asset_id TEXT PRIMARY KEY,
                    mime_type TEXT,
                    description TEXT,
                    data BLOB NOT NULL,
                    byte_count INTEGER NOT NULL,
                    created_at_unix_seconds INTEGER NOT NULL,
                    updated_at_unix_seconds INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS playlist_artwork_refs (
                    playlist_id INTEGER PRIMARY KEY REFERENCES playlists(id) ON DELETE CASCADE,
                    asset_id TEXT NOT NULL REFERENCES artwork_assets(asset_id) ON DELETE RESTRICT,
                    updated_at_unix_seconds INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS playlist_items (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
                    position INTEGER NOT NULL,
                    track_path TEXT NOT NULL REFERENCES tracks(path) ON DELETE CASCADE,
                    added_at_unix_seconds INTEGER NOT NULL
                );

                CREATE INDEX IF NOT EXISTS playlist_items_playlist_idx
                    ON playlist_items(playlist_id, position);

                CREATE TABLE IF NOT EXISTS playback_queue_state (
                    singleton_id INTEGER PRIMARY KEY CHECK(singleton_id = 1),
                    current_index INTEGER,
                    position_ms INTEGER NOT NULL DEFAULT 0,
                    repeat_mode TEXT NOT NULL DEFAULT 'off',
                    shuffle_enabled INTEGER NOT NULL DEFAULT 0,
                    updated_at_unix_seconds INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS playback_queue_items (
                    position INTEGER PRIMARY KEY,
                    track_path TEXT NOT NULL REFERENCES tracks(path) ON DELETE CASCADE
                );

                CREATE TABLE IF NOT EXISTS favorite_tracks (
                    track_path TEXT PRIMARY KEY REFERENCES tracks(path) ON DELETE CASCADE,
                    created_at_unix_seconds INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS play_history (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    track_path TEXT NOT NULL REFERENCES tracks(path) ON DELETE CASCADE,
                    played_at_unix_seconds INTEGER NOT NULL,
                    position_ms INTEGER NOT NULL,
                    completed INTEGER NOT NULL
                );

                CREATE INDEX IF NOT EXISTS play_history_played_at_idx
                    ON play_history(played_at_unix_seconds DESC, id DESC);

                CREATE TABLE IF NOT EXISTS track_artwork (
                    track_path TEXT NOT NULL REFERENCES tracks(path) ON DELETE CASCADE,
                    picture_index INTEGER NOT NULL,
                    mime_type TEXT,
                    picture_type TEXT NOT NULL,
                    description TEXT,
                    data BLOB NOT NULL,
                    updated_at_unix_seconds INTEGER NOT NULL,
                    PRIMARY KEY(track_path, picture_index)
                );

                CREATE TABLE IF NOT EXISTS track_artwork_refs (
                    track_path TEXT PRIMARY KEY REFERENCES tracks(path) ON DELETE CASCADE,
                    asset_id TEXT NOT NULL REFERENCES artwork_assets(asset_id) ON DELETE RESTRICT,
                    updated_at_unix_seconds INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS album_artwork_refs (
                    track_path TEXT PRIMARY KEY REFERENCES tracks(path) ON DELETE CASCADE,
                    album_key TEXT NOT NULL,
                    asset_id TEXT NOT NULL REFERENCES artwork_assets(asset_id) ON DELETE RESTRICT,
                    updated_at_unix_seconds INTEGER NOT NULL
                );

                CREATE INDEX IF NOT EXISTS album_artwork_refs_album_idx
                    ON album_artwork_refs(album_key);

                CREATE TABLE IF NOT EXISTS track_notes (
                    track_path TEXT PRIMARY KEY REFERENCES tracks(path) ON DELETE CASCADE,
                    notes TEXT NOT NULL,
                    updated_at_unix_seconds INTEGER NOT NULL
                );

                CREATE INDEX IF NOT EXISTS tracks_file_hash_idx ON tracks(file_hash);
                CREATE INDEX IF NOT EXISTS tracks_audio_hash_idx ON tracks(audio_hash);
                CREATE INDEX IF NOT EXISTS tracks_view_id_idx ON tracks(view_id);
                CREATE INDEX IF NOT EXISTS tracks_primary_view_id_idx ON tracks(primary_view_id);
                CREATE INDEX IF NOT EXISTS tracks_user_rating_idx ON tracks(user_rating);
                "#,
            )
            .map_err(to_store_error)?;
        Ok(())
    }
}

fn sort_playlist_items(items: &mut [PlaylistSortItem], sort: PlaylistSort) {
    match sort {
        PlaylistSort::Manual => {
            items.sort_by_key(|item| item.item_id);
        }
        PlaylistSort::Title => {
            items.sort_by_key(|item| {
                (
                    normalized_text(&item.title),
                    normalized_optional_text(item.artist.as_deref()),
                    normalized_optional_text(item.album.as_deref()),
                    item.track_path.to_lowercase(),
                    item.item_id,
                )
            });
        }
        PlaylistSort::Artist => {
            items.sort_by_key(|item| {
                (
                    normalized_optional_text(item.artist.as_deref()),
                    normalized_text(&item.title),
                    normalized_optional_text(item.album.as_deref()),
                    item.track_path.to_lowercase(),
                    item.item_id,
                )
            });
        }
        PlaylistSort::Album => {
            items.sort_by_key(|item| {
                (
                    normalized_optional_text(item.album.as_deref()),
                    optional_track_number(item.disc_number),
                    optional_track_number(item.track_number),
                    normalized_text(&item.title),
                    normalized_optional_text(item.artist.as_deref()),
                    item.track_path.to_lowercase(),
                    item.item_id,
                )
            });
        }
        PlaylistSort::Rating => {
            items.sort_by_key(|item| {
                (
                    rating_sort_key(item.user_rating),
                    normalized_text(&item.title),
                    normalized_optional_text(item.artist.as_deref()),
                    item.track_path.to_lowercase(),
                    item.item_id,
                )
            });
        }
    }
}

fn normalized_text(value: &str) -> (bool, String) {
    let trimmed = value.trim();
    (trimmed.is_empty(), trimmed.to_lowercase())
}

fn normalized_optional_text(value: Option<&str>) -> (bool, String) {
    normalized_text(value.unwrap_or(""))
}

fn optional_track_number(value: Option<u32>) -> (bool, u32) {
    (value.is_none(), value.unwrap_or(u32::MAX))
}

fn rating_sort_key(value: Option<u8>) -> (bool, u8) {
    (value.is_none(), 10_u8.saturating_sub(value.unwrap_or(0)))
}

fn row_to_track(row: &rusqlite::Row<'_>) -> rusqlite::Result<Track> {
    row_to_track_at(row, 0)
}

fn row_to_track_at(row: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<Track> {
    let id_text: String = row.get(offset)?;
    let path_text: String = row.get(offset + 1)?;
    let mut track = Track::from_path(PathBuf::from(path_text));
    if let Ok(id) = id_text.parse::<u64>() {
        track.id = TrackId::from_value(id);
    }
    track.title = row.get(offset + 2)?;
    track.artist = row.get(offset + 3)?;
    track.album = row.get(offset + 4)?;
    track.album_artist = row.get(offset + 5)?;
    track.genre = row.get(offset + 6)?;
    track.track_number = optional_u32(row.get::<_, Option<i64>>(offset + 7)?);
    track.disc_number = optional_u32(row.get::<_, Option<i64>>(offset + 8)?);
    track.year = optional_i32(row.get::<_, Option<i64>>(offset + 9)?);
    track.duration_ms = optional_u64(row.get::<_, Option<i64>>(offset + 10)?);
    track.artwork_count = optional_u32(row.get::<_, Option<i64>>(offset + 11)?).unwrap_or(0);

    let size_bytes = optional_u64(row.get::<_, Option<i64>>(offset + 12)?);
    let modified_unix_seconds = row.get::<_, Option<i64>>(offset + 13)?;
    track.fingerprint = match (size_bytes, modified_unix_seconds) {
        (Some(size_bytes), Some(modified_unix_seconds)) => Some(FileFingerprint {
            size_bytes,
            modified_unix_seconds,
        }),
        _ => None,
    };

    let integrated_lufs = row.get::<_, Option<f64>>(offset + 14)?;
    let true_peak_dbtp = row.get::<_, Option<f64>>(offset + 15)?;
    track.loudness = match (integrated_lufs, true_peak_dbtp) {
        (Some(integrated_lufs), Some(true_peak_dbtp)) => Some(LoudnessInfo {
            integrated_lufs: integrated_lufs as f32,
            true_peak_dbtp: true_peak_dbtp as f32,
            album_integrated_lufs: row
                .get::<_, Option<f64>>(offset + 16)?
                .map(|value| value as f32),
            album_true_peak_dbtp: row
                .get::<_, Option<f64>>(offset + 17)?
                .map(|value| value as f32),
            analysis_version: u32::try_from(row.get::<_, i64>(offset + 18)?).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    offset + 18,
                    rusqlite::types::Type::Integer,
                    Box::new(error),
                )
            })?,
        }),
        _ => None,
    };
    track.file_hash = row.get(offset + 19)?;
    track.audio_hash = row.get(offset + 20)?;
    track.view_id = TrackViewId::from_value(row.get::<_, String>(offset + 21)?);
    track.primary_view_id = TrackViewId::from_value(row.get::<_, String>(offset + 22)?);
    let view_kind = row.get::<_, String>(offset + 23)?;
    track.view_kind = TrackViewKind::parse(&view_kind).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            offset + 23,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })?;
    track.transform_spec = row.get(offset + 24)?;
    track.quality_profile = row.get(offset + 25)?;
    track.format_name = row.get(offset + 26)?;
    track.view_name = row.get(offset + 27)?;
    track.user_rating = optional_rating(row.get::<_, Option<i64>>(offset + 28)?);

    Ok(track)
}

fn row_to_playlist_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlaylistSummary> {
    Ok(PlaylistSummary {
        id: row.get(0)?,
        name: row.get(1)?,
        track_count: optional_usize(Some(row.get::<_, i64>(2)?)).unwrap_or(0),
        has_artwork: row.get::<_, i64>(3)? != 0,
        created_at_unix_seconds: row.get(4)?,
        updated_at_unix_seconds: row.get(5)?,
        last_used_at_unix_seconds: row.get(6)?,
    })
}

fn repeat_mode_to_store_value(mode: RepeatMode) -> &'static str {
    match mode {
        RepeatMode::Off => "off",
        RepeatMode::One => "one",
        RepeatMode::All => "all",
    }
}

fn repeat_mode_from_store_value(value: &str) -> PlayerResult<RepeatMode> {
    match value {
        "off" => Ok(RepeatMode::Off),
        "one" => Ok(RepeatMode::One),
        "all" => Ok(RepeatMode::All),
        _ => Err(PlayerError::store(format!(
            "invalid persisted repeat mode: {value}"
        ))),
    }
}

fn row_to_artwork(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArtworkImage> {
    Ok(ArtworkImage {
        picture_index: optional_u32(Some(row.get::<_, i64>(0)?)).unwrap_or(0),
        mime_type: row.get(1)?,
        picture_type: row.get(2)?,
        description: row.get(3)?,
        data: row.get(4)?,
    })
}

fn to_store_error(error: rusqlite::Error) -> PlayerError {
    PlayerError::store(error.to_string())
}

fn merge_notes(
    tx: &rusqlite::Transaction<'_>,
    canonical_path: &str,
    duplicate_path: &str,
) -> PlayerResult<()> {
    let canonical_notes: Option<String> = tx
        .query_row(
            "SELECT notes FROM track_notes WHERE track_path = ?1",
            params![canonical_path],
            |row| row.get(0),
        )
        .optional()
        .map_err(to_store_error)?;
    let duplicate_notes: Option<String> = tx
        .query_row(
            "SELECT notes FROM track_notes WHERE track_path = ?1",
            params![duplicate_path],
            |row| row.get(0),
        )
        .optional()
        .map_err(to_store_error)?;

    match (canonical_notes, duplicate_notes) {
        (None, None) => {}
        (Some(_), None) => {}
        (None, Some(notes)) => {
            tx.execute(
                "UPDATE track_notes SET track_path = ?1, updated_at_unix_seconds = ?3 WHERE track_path = ?2",
                params![canonical_path, duplicate_path, now_unix_seconds()],
            )
            .map_err(to_store_error)?;
            if notes.trim().is_empty() {
                tx.execute(
                    "DELETE FROM track_notes WHERE track_path = ?1",
                    params![canonical_path],
                )
                .map_err(to_store_error)?;
            }
        }
        (Some(canonical), Some(duplicate)) => {
            let duplicate = duplicate.trim();
            if !duplicate.is_empty() && !canonical.contains(duplicate) {
                let merged = format!("{canonical}\n\n--- merged duplicate note ---\n{duplicate}");
                tx.execute(
                    "UPDATE track_notes SET notes = ?2, updated_at_unix_seconds = ?3 WHERE track_path = ?1",
                    params![canonical_path, merged, now_unix_seconds()],
                )
                .map_err(to_store_error)?;
            }
            tx.execute(
                "DELETE FROM track_notes WHERE track_path = ?1",
                params![duplicate_path],
            )
            .map_err(to_store_error)?;
        }
    }
    Ok(())
}

fn merge_artwork(
    tx: &rusqlite::Transaction<'_>,
    canonical_path: &str,
    duplicate_path: &str,
) -> PlayerResult<()> {
    let canonical_count: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM track_artwork WHERE track_path = ?1",
            params![canonical_path],
            |row| row.get(0),
        )
        .map_err(to_store_error)?;
    if canonical_count == 0 {
        tx.execute(
            "UPDATE track_artwork SET track_path = ?1 WHERE track_path = ?2",
            params![canonical_path, duplicate_path],
        )
        .map_err(to_store_error)?;
        tx.execute(
            r#"
            UPDATE tracks
            SET artwork_count = (
                    SELECT COUNT(*) FROM track_artwork WHERE track_path = ?1
                ),
                updated_at_unix_seconds = ?2
            WHERE path = ?1
            "#,
            params![canonical_path, now_unix_seconds()],
        )
        .map_err(to_store_error)?;
    } else {
        tx.execute(
            "DELETE FROM track_artwork WHERE track_path = ?1",
            params![duplicate_path],
        )
        .map_err(to_store_error)?;
    }
    Ok(())
}

fn merge_artwork_references(
    tx: &rusqlite::Transaction<'_>,
    canonical_path: &str,
    duplicate_path: &str,
) -> PlayerResult<()> {
    merge_artwork_reference_table(tx, "track_artwork_refs", canonical_path, duplicate_path)?;
    merge_artwork_reference_table(tx, "album_artwork_refs", canonical_path, duplicate_path)
}

fn merge_artwork_reference_table(
    tx: &rusqlite::Transaction<'_>,
    table: &str,
    canonical_path: &str,
    duplicate_path: &str,
) -> PlayerResult<()> {
    let canonical_count: i64 = tx
        .query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE track_path = ?1"),
            params![canonical_path],
            |row| row.get(0),
        )
        .map_err(to_store_error)?;
    if canonical_count == 0 {
        tx.execute(
            &format!(
                "UPDATE {table} SET track_path = ?1, updated_at_unix_seconds = ?3 WHERE track_path = ?2"
            ),
            params![canonical_path, duplicate_path, now_unix_seconds()],
        )
        .map_err(to_store_error)?;
    } else {
        tx.execute(
            &format!("DELETE FROM {table} WHERE track_path = ?1"),
            params![duplicate_path],
        )
        .map_err(to_store_error)?;
    }
    Ok(())
}

fn upsert_artwork_asset_tx(
    tx: &rusqlite::Transaction<'_>,
    image: &ArtworkImage,
) -> PlayerResult<String> {
    let asset_id = artwork_asset_id(image);
    let now = now_unix_seconds();
    tx.execute(
        r#"
        INSERT INTO artwork_assets
            (asset_id, mime_type, description, data, byte_count,
             created_at_unix_seconds, updated_at_unix_seconds)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
        ON CONFLICT(asset_id) DO UPDATE SET
            mime_type = COALESCE(artwork_assets.mime_type, excluded.mime_type),
            description = COALESCE(artwork_assets.description, excluded.description),
            updated_at_unix_seconds = excluded.updated_at_unix_seconds
        "#,
        params![
            asset_id.as_str(),
            image.mime_type.as_deref(),
            image.description.as_deref(),
            image.data.as_slice(),
            saturating_i64_from_u64(image.data.len() as u64),
            now,
        ],
    )
    .map_err(to_store_error)?;
    Ok(asset_id)
}

fn artwork_asset_id(image: &ArtworkImage) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"normalplayer-artwork-asset-v1");
    hasher.update(&image.data);
    format!("image:{}", hasher.finalize().to_hex())
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn clean_required_name(name: &str) -> PlayerResult<&str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(PlayerError::store("name cannot be empty"));
    }
    Ok(name)
}

fn clean_metadata_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn album_group_key(album_artist: Option<&str>, album: &str) -> String {
    format!(
        "{}\u{1f}{}",
        album_artist.unwrap_or("").to_lowercase(),
        album.to_lowercase()
    )
}

fn optional_track_album_key(track: &Track) -> Option<String> {
    let album = clean_metadata_value(track.album.as_deref())?;
    let album_artist = clean_metadata_value(track.album_artist.as_deref())
        .or_else(|| clean_metadata_value(track.artist.as_deref()));
    Some(album_group_key(album_artist.as_deref(), &album))
}

fn required_track_album_key(track: &Track) -> PlayerResult<String> {
    optional_track_album_key(track).ok_or_else(|| {
        PlayerError::store(format!(
            "track has no album identity: {}",
            track.path.display()
        ))
    })
}

fn optional_u32(value: Option<i64>) -> Option<u32> {
    value.and_then(|value| u32::try_from(value).ok())
}

fn optional_i32(value: Option<i64>) -> Option<i32> {
    value.and_then(|value| i32::try_from(value).ok())
}

fn optional_u64(value: Option<i64>) -> Option<u64> {
    value.and_then(|value| u64::try_from(value).ok())
}

fn optional_usize(value: Option<i64>) -> Option<usize> {
    value.and_then(|value| usize::try_from(value).ok())
}

fn optional_rating(value: Option<i64>) -> Option<u8> {
    value
        .and_then(|value| u8::try_from(value).ok())
        .filter(|value| (1..=10).contains(value))
}

fn rating_to_sql(value: Option<u8>) -> PlayerResult<Option<i64>> {
    match value {
        None => Ok(None),
        Some(value @ 1..=10) => Ok(Some(i64::from(value))),
        Some(_) => Err(PlayerError::store("rating must be between 1 and 10")),
    }
}

fn saturating_i64_from_u64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

fn like_pattern(query: &str) -> String {
    let mut escaped = String::from("%");
    for ch in query.trim().to_lowercase().chars() {
        match ch {
            '%' | '_' | '\\' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped.push('%');
    escaped
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
