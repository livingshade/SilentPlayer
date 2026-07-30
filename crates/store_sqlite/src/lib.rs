use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use domain::{
    ArtworkImage, FileFingerprint, LoudnessInfo, RepeatMode, Track, TrackId, TrackViewId,
    TrackViewKind,
};
use errors::{PlayerError, PlayerResult};
use rusqlite::{params, Connection, OptionalExtension};

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

    pub fn upsert_track(&mut self, track: &Track) -> PlayerResult<()> {
        self.conn
            .execute(
                r#"
                INSERT INTO tracks (
                    id, path, title, artist, album, album_artist, genre,
                    track_number, disc_number, year, duration_ms, artwork_count,
                    size_bytes, modified_unix_seconds, integrated_lufs, true_peak_dbtp,
                    album_integrated_lufs, album_true_peak_dbtp, analysis_version,
                    analysis_size_bytes, analysis_modified_unix_seconds,
                    file_hash, audio_hash, view_id, primary_view_id, view_kind,
                    transform_spec, quality_profile, format_name, view_name, user_rating,
                    analyzed_at_unix_seconds, added_at_unix_seconds, updated_at_unix_seconds,
                    original_title, original_artist, original_album
                )
                VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                    ?8, ?9, ?10, ?11, ?12,
                    ?13, ?14, ?15, ?16,
                    ?17, ?18, ?19,
                    ?20, ?21,
                    ?22, ?23, ?24, ?25, ?26,
                    ?27, ?28, ?29, ?30,
                    ?31, ?32, ?33, ?34,
                    ?35, ?36, ?37
                )
                ON CONFLICT(path) DO UPDATE SET
                    id = excluded.id,
                    view_id = excluded.view_id,
                    primary_view_id = excluded.primary_view_id,
                    view_kind = excluded.view_kind,
                    transform_spec = excluded.transform_spec,
                    quality_profile = excluded.quality_profile,
                    format_name = excluded.format_name,
                    view_name = COALESCE(excluded.view_name, tracks.view_name),
                    user_rating = COALESCE(excluded.user_rating, tracks.user_rating),
                    title = CASE
                        WHEN tracks.metadata_edited_at_unix_seconds IS NULL THEN excluded.title
                        ELSE tracks.title
                    END,
                    artist = CASE
                        WHEN tracks.metadata_edited_at_unix_seconds IS NULL THEN excluded.artist
                        ELSE tracks.artist
                    END,
                    album = CASE
                        WHEN tracks.metadata_edited_at_unix_seconds IS NULL THEN excluded.album
                        ELSE tracks.album
                    END,
                    original_title = tracks.original_title,
                    original_artist = tracks.original_artist,
                    original_album = tracks.original_album,
                    album_artist = excluded.album_artist,
                    genre = excluded.genre,
                    track_number = excluded.track_number,
                    disc_number = excluded.disc_number,
                    year = excluded.year,
                    duration_ms = excluded.duration_ms,
                    artwork_count = excluded.artwork_count,
                    size_bytes = excluded.size_bytes,
                    modified_unix_seconds = excluded.modified_unix_seconds,
                    integrated_lufs = COALESCE(excluded.integrated_lufs, tracks.integrated_lufs),
                    true_peak_dbtp = COALESCE(excluded.true_peak_dbtp, tracks.true_peak_dbtp),
                    album_integrated_lufs = COALESCE(excluded.album_integrated_lufs, tracks.album_integrated_lufs),
                    album_true_peak_dbtp = COALESCE(excluded.album_true_peak_dbtp, tracks.album_true_peak_dbtp),
                    analysis_version = COALESCE(excluded.analysis_version, tracks.analysis_version),
                    analysis_size_bytes = COALESCE(excluded.analysis_size_bytes, tracks.analysis_size_bytes),
                    analysis_modified_unix_seconds = COALESCE(excluded.analysis_modified_unix_seconds, tracks.analysis_modified_unix_seconds),
                    file_hash = COALESCE(excluded.file_hash, tracks.file_hash),
                    audio_hash = COALESCE(excluded.audio_hash, tracks.audio_hash),
                    analyzed_at_unix_seconds = COALESCE(excluded.analyzed_at_unix_seconds, tracks.analyzed_at_unix_seconds),
                    updated_at_unix_seconds = excluded.updated_at_unix_seconds
                "#,
                params![
                    track.id.value().to_string(),
                    path_to_string(&track.path),
                    track.title,
                    track.artist,
                    track.album,
                    track.album_artist,
                    track.genre,
                    track.track_number.map(i64::from),
                    track.disc_number.map(i64::from),
                    track.year.map(i64::from),
                    track.duration_ms.map(saturating_i64_from_u64),
                    i64::from(track.artwork_count),
                    track.fingerprint.map(|fingerprint| saturating_i64_from_u64(fingerprint.size_bytes)),
                    track.fingerprint.map(|fingerprint| fingerprint.modified_unix_seconds),
                    track.loudness.as_ref().map(|loudness| f64::from(loudness.integrated_lufs)),
                    track.loudness.as_ref().map(|loudness| f64::from(loudness.true_peak_dbtp)),
                    track
                        .loudness
                        .as_ref()
                        .and_then(|loudness| loudness.album_integrated_lufs)
                        .map(f64::from),
                    track
                        .loudness
                        .as_ref()
                        .and_then(|loudness| loudness.album_true_peak_dbtp)
                        .map(f64::from),
                    track.loudness.as_ref().map(|loudness| i64::from(loudness.analysis_version)),
                    track.loudness.as_ref().and(track.fingerprint).map(|fingerprint| {
                        saturating_i64_from_u64(fingerprint.size_bytes)
                    }),
                    track
                        .loudness
                        .as_ref()
                        .and(track.fingerprint)
                        .map(|fingerprint| fingerprint.modified_unix_seconds),
                    track.file_hash.as_deref(),
                    track.audio_hash.as_deref(),
                    track.view_id.value(),
                    track.primary_view_id.value(),
                    track.view_kind.as_str(),
                    track.transform_spec.as_deref(),
                    track.quality_profile.as_deref(),
                    track.format_name.as_deref(),
                    track.view_name.as_deref(),
                    track.user_rating.map(i64::from),
                    track.loudness.as_ref().map(|_| now_unix_seconds()),
                    now_unix_seconds(),
                    now_unix_seconds(),
                    track.title,
                    track.artist,
                    track.album,
                ],
            )
            .map_err(to_store_error)?;
        Ok(())
    }

    pub fn upsert_tracks(&mut self, tracks: &[Track]) -> PlayerResult<()> {
        let tx = self.conn.transaction().map_err(to_store_error)?;
        {
            let mut stmt = tx
                .prepare(
                    r#"
                    INSERT INTO tracks (
                        id, path, title, artist, album, album_artist, genre,
                        track_number, disc_number, year, duration_ms, artwork_count,
                        size_bytes, modified_unix_seconds, integrated_lufs, true_peak_dbtp,
                        album_integrated_lufs, album_true_peak_dbtp, analysis_version,
                        analysis_size_bytes, analysis_modified_unix_seconds,
                        file_hash, audio_hash, view_id, primary_view_id, view_kind,
                        transform_spec, quality_profile, format_name, view_name, user_rating,
                        analyzed_at_unix_seconds, added_at_unix_seconds, updated_at_unix_seconds,
                        original_title, original_artist, original_album
                    )
                    VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                        ?8, ?9, ?10, ?11, ?12,
                        ?13, ?14, ?15, ?16,
                        ?17, ?18, ?19,
                        ?20, ?21,
                        ?22, ?23, ?24, ?25, ?26,
                        ?27, ?28, ?29, ?30,
                        ?31, ?32, ?33, ?34,
                        ?35, ?36, ?37
                    )
                    ON CONFLICT(path) DO UPDATE SET
                        id = excluded.id,
                        view_id = excluded.view_id,
                        primary_view_id = excluded.primary_view_id,
                        view_kind = excluded.view_kind,
                        transform_spec = excluded.transform_spec,
                        quality_profile = excluded.quality_profile,
                        format_name = excluded.format_name,
                        view_name = COALESCE(excluded.view_name, tracks.view_name),
                        user_rating = COALESCE(excluded.user_rating, tracks.user_rating),
                        title = CASE
                            WHEN tracks.metadata_edited_at_unix_seconds IS NULL THEN excluded.title
                            ELSE tracks.title
                        END,
                        artist = CASE
                            WHEN tracks.metadata_edited_at_unix_seconds IS NULL THEN excluded.artist
                            ELSE tracks.artist
                        END,
                        album = CASE
                            WHEN tracks.metadata_edited_at_unix_seconds IS NULL THEN excluded.album
                            ELSE tracks.album
                        END,
                        original_title = tracks.original_title,
                        original_artist = tracks.original_artist,
                        original_album = tracks.original_album,
                        album_artist = excluded.album_artist,
                        genre = excluded.genre,
                        track_number = excluded.track_number,
                        disc_number = excluded.disc_number,
                        year = excluded.year,
                        duration_ms = excluded.duration_ms,
                        artwork_count = excluded.artwork_count,
                        size_bytes = excluded.size_bytes,
                        modified_unix_seconds = excluded.modified_unix_seconds,
                        integrated_lufs = COALESCE(excluded.integrated_lufs, tracks.integrated_lufs),
                        true_peak_dbtp = COALESCE(excluded.true_peak_dbtp, tracks.true_peak_dbtp),
                        album_integrated_lufs = COALESCE(excluded.album_integrated_lufs, tracks.album_integrated_lufs),
                        album_true_peak_dbtp = COALESCE(excluded.album_true_peak_dbtp, tracks.album_true_peak_dbtp),
                        analysis_version = COALESCE(excluded.analysis_version, tracks.analysis_version),
                        analysis_size_bytes = COALESCE(excluded.analysis_size_bytes, tracks.analysis_size_bytes),
                        analysis_modified_unix_seconds = COALESCE(excluded.analysis_modified_unix_seconds, tracks.analysis_modified_unix_seconds),
                        file_hash = COALESCE(excluded.file_hash, tracks.file_hash),
                        audio_hash = COALESCE(excluded.audio_hash, tracks.audio_hash),
                        analyzed_at_unix_seconds = COALESCE(excluded.analyzed_at_unix_seconds, tracks.analyzed_at_unix_seconds),
                        updated_at_unix_seconds = excluded.updated_at_unix_seconds
                    "#,
                )
                .map_err(to_store_error)?;

            let now = now_unix_seconds();
            for track in tracks {
                stmt.execute(params![
                    track.id.value().to_string(),
                    path_to_string(&track.path),
                    track.title,
                    track.artist,
                    track.album,
                    track.album_artist,
                    track.genre,
                    track.track_number.map(i64::from),
                    track.disc_number.map(i64::from),
                    track.year.map(i64::from),
                    track.duration_ms.map(saturating_i64_from_u64),
                    i64::from(track.artwork_count),
                    track
                        .fingerprint
                        .map(|fingerprint| saturating_i64_from_u64(fingerprint.size_bytes)),
                    track
                        .fingerprint
                        .map(|fingerprint| fingerprint.modified_unix_seconds),
                    track
                        .loudness
                        .as_ref()
                        .map(|loudness| f64::from(loudness.integrated_lufs)),
                    track
                        .loudness
                        .as_ref()
                        .map(|loudness| f64::from(loudness.true_peak_dbtp)),
                    track
                        .loudness
                        .as_ref()
                        .and_then(|loudness| loudness.album_integrated_lufs)
                        .map(f64::from),
                    track
                        .loudness
                        .as_ref()
                        .and_then(|loudness| loudness.album_true_peak_dbtp)
                        .map(f64::from),
                    track
                        .loudness
                        .as_ref()
                        .map(|loudness| i64::from(loudness.analysis_version)),
                    track
                        .loudness
                        .as_ref()
                        .and(track.fingerprint)
                        .map(|fingerprint| { saturating_i64_from_u64(fingerprint.size_bytes) }),
                    track
                        .loudness
                        .as_ref()
                        .and(track.fingerprint)
                        .map(|fingerprint| fingerprint.modified_unix_seconds),
                    track.file_hash.as_deref(),
                    track.audio_hash.as_deref(),
                    track.view_id.value(),
                    track.primary_view_id.value(),
                    track.view_kind.as_str(),
                    track.transform_spec.as_deref(),
                    track.quality_profile.as_deref(),
                    track.format_name.as_deref(),
                    track.view_name.as_deref(),
                    track.user_rating.map(i64::from),
                    track.loudness.as_ref().map(|_| now),
                    now,
                    now,
                    track.title,
                    track.artist,
                    track.album,
                ])
                .map_err(to_store_error)?;
            }
        }
        tx.commit().map_err(to_store_error)?;
        Ok(())
    }

    pub fn tracks(&self) -> PlayerResult<Vec<Track>> {
        let mut stmt = self
            .conn
            .prepare(
                r#"
                SELECT id, path, title, artist, album, album_artist, genre,
                       track_number, disc_number, year, duration_ms, artwork_count,
                       size_bytes, modified_unix_seconds, integrated_lufs, true_peak_dbtp,
                       album_integrated_lufs, album_true_peak_dbtp, analysis_version,
                       file_hash, audio_hash,
                       view_id, primary_view_id, view_kind, transform_spec,
                       quality_profile, format_name, view_name, user_rating
                FROM tracks
                ORDER BY lower(title), path
                "#,
            )
            .map_err(to_store_error)?;

        let rows = stmt
            .query_map([], row_to_track)
            .map_err(to_store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_store_error)?;
        Ok(rows)
    }

    pub fn tracks_page(&self, offset: usize, limit: usize) -> PlayerResult<(usize, Vec<Track>)> {
        if limit == 0 {
            return Err(PlayerError::invalid_input(
                "library page limit must be greater than zero",
            ));
        }

        let total = self
            .conn
            .query_row("SELECT COUNT(*) FROM tracks", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(to_store_error)?
            .max(0) as usize;
        let mut stmt = self
            .conn
            .prepare(
                r#"
                SELECT id, path, title, artist, album, album_artist, genre,
                       track_number, disc_number, year, duration_ms, artwork_count,
                       size_bytes, modified_unix_seconds, integrated_lufs, true_peak_dbtp,
                       album_integrated_lufs, album_true_peak_dbtp, analysis_version,
                       file_hash, audio_hash,
                       view_id, primary_view_id, view_kind, transform_spec,
                       quality_profile, format_name, view_name, user_rating
                FROM tracks
                ORDER BY lower(title), path
                LIMIT ?1 OFFSET ?2
                "#,
            )
            .map_err(to_store_error)?;
        let tracks = stmt
            .query_map(
                params![
                    saturating_i64_from_u64(limit as u64),
                    saturating_i64_from_u64(offset as u64)
                ],
                row_to_track,
            )
            .map_err(to_store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_store_error)?;
        Ok((total, tracks))
    }

    pub fn replace_track_paths(&mut self, replacements: &[(PathBuf, PathBuf)]) -> PlayerResult<()> {
        self.conn
            .execute_batch("PRAGMA foreign_keys = OFF;")
            .map_err(to_store_error)?;

        let result = (|| {
            let tx = self.conn.transaction().map_err(to_store_error)?;
            for (old_path, new_path) in replacements {
                let old_path = path_to_string(old_path);
                let new_path = path_to_string(new_path);
                for table in [
                    "playlist_items",
                    "favorite_tracks",
                    "play_history",
                    "track_artwork",
                    "track_artwork_refs",
                    "album_artwork_refs",
                    "track_notes",
                    "playback_queue_items",
                ] {
                    tx.execute(
                        &format!("UPDATE {table} SET track_path = ?2 WHERE track_path = ?1"),
                        params![old_path.as_str(), new_path.as_str()],
                    )
                    .map_err(to_store_error)?;
                }
                tx.execute(
                    "UPDATE tracks SET path = ?2 WHERE path = ?1",
                    params![old_path.as_str(), new_path.as_str()],
                )
                .map_err(to_store_error)?;
            }
            tx.commit().map_err(to_store_error)
        })();

        self.conn
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(to_store_error)?;
        result
    }

    pub fn zero_out(&mut self) -> PlayerResult<()> {
        let tx = self.conn.transaction().map_err(to_store_error)?;
        tx.execute("DELETE FROM playback_queue_items", [])
            .map_err(to_store_error)?;
        tx.execute("DELETE FROM playback_queue_state", [])
            .map_err(to_store_error)?;
        tx.execute("DELETE FROM playlists", [])
            .map_err(to_store_error)?;
        tx.execute("DELETE FROM tracks", [])
            .map_err(to_store_error)?;
        tx.execute("DELETE FROM artwork_assets", [])
            .map_err(to_store_error)?;
        tx.execute(
            "DELETE FROM sqlite_sequence WHERE name IN ('playlists', 'playlist_items', 'play_history')",
            [],
        )
        .map_err(to_store_error)?;
        tx.commit().map_err(to_store_error)?;
        Ok(())
    }

    pub fn track_by_path(&self, path: impl AsRef<Path>) -> PlayerResult<Option<Track>> {
        self.conn
            .query_row(
                r#"
                SELECT id, path, title, artist, album, album_artist, genre,
                       track_number, disc_number, year, duration_ms, artwork_count,
                       size_bytes, modified_unix_seconds, integrated_lufs, true_peak_dbtp,
                       album_integrated_lufs, album_true_peak_dbtp, analysis_version,
                       file_hash, audio_hash,
                       view_id, primary_view_id, view_kind, transform_spec,
                       quality_profile, format_name, view_name, user_rating
                FROM tracks
                WHERE path = ?1
                "#,
                params![path_to_string(path.as_ref())],
                row_to_track,
            )
            .optional()
            .map_err(to_store_error)
    }

    pub fn delete_track(&mut self, path: impl AsRef<Path>) -> PlayerResult<bool> {
        let path = path_to_string(path.as_ref());
        let tx = self.conn.transaction().map_err(to_store_error)?;
        let playlist_ids = {
            let mut stmt = tx
                .prepare("SELECT DISTINCT playlist_id FROM playlist_items WHERE track_path = ?1")
                .map_err(to_store_error)?;
            let rows = stmt
                .query_map(params![path.as_str()], |row| row.get::<_, i64>(0))
                .map_err(to_store_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(to_store_error)?;
            rows
        };
        let removed_queue_positions = {
            let mut stmt = tx
                .prepare(
                    "SELECT position FROM playback_queue_items WHERE track_path = ?1 ORDER BY position",
                )
                .map_err(to_store_error)?;
            let rows = stmt
                .query_map(params![path.as_str()], |row| row.get::<_, i64>(0))
                .map_err(to_store_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(to_store_error)?;
            rows
        };
        let deleted = tx
            .execute("DELETE FROM tracks WHERE path = ?1", params![path.as_str()])
            .map_err(to_store_error)?;
        if deleted == 0 {
            tx.commit().map_err(to_store_error)?;
            return Ok(false);
        }

        for playlist_id in playlist_ids {
            let item_ids = {
                let mut stmt = tx
                    .prepare(
                        "SELECT id FROM playlist_items WHERE playlist_id = ?1 ORDER BY position, id",
                    )
                    .map_err(to_store_error)?;
                let rows = stmt
                    .query_map(params![playlist_id], |row| row.get::<_, i64>(0))
                    .map_err(to_store_error)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(to_store_error)?;
                rows
            };
            for (position, item_id) in item_ids.into_iter().enumerate() {
                tx.execute(
                    "UPDATE playlist_items SET position = ?2 WHERE id = ?1",
                    params![item_id, saturating_i64_from_u64(position as u64)],
                )
                .map_err(to_store_error)?;
            }
            tx.execute(
                "UPDATE playlists SET updated_at_unix_seconds = ?2 WHERE id = ?1",
                params![playlist_id, now_unix_seconds()],
            )
            .map_err(to_store_error)?;
        }

        let queue_paths = {
            let mut stmt = tx
                .prepare("SELECT track_path FROM playback_queue_items ORDER BY position")
                .map_err(to_store_error)?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(to_store_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(to_store_error)?;
            rows
        };
        tx.execute("DELETE FROM playback_queue_items", [])
            .map_err(to_store_error)?;
        for (position, queue_path) in queue_paths.iter().enumerate() {
            tx.execute(
                "INSERT INTO playback_queue_items (position, track_path) VALUES (?1, ?2)",
                params![saturating_i64_from_u64(position as u64), queue_path],
            )
            .map_err(to_store_error)?;
        }
        if !removed_queue_positions.is_empty() {
            let current_index = tx
                .query_row(
                    "SELECT current_index FROM playback_queue_state WHERE singleton_id = 1",
                    [],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .optional()
                .map_err(to_store_error)?
                .flatten();
            let adjusted_index = current_index.and_then(|current| {
                if queue_paths.is_empty() {
                    return None;
                }
                let removed_before = removed_queue_positions
                    .iter()
                    .filter(|position| **position < current)
                    .count() as i64;
                let shifted = current.saturating_sub(removed_before).max(0);
                Some(shifted.min(queue_paths.len().saturating_sub(1) as i64))
            });
            tx.execute(
                r#"
                UPDATE playback_queue_state
                SET current_index = ?1,
                    position_ms = 0,
                    updated_at_unix_seconds = ?2
                WHERE singleton_id = 1
                "#,
                params![adjusted_index, now_unix_seconds()],
            )
            .map_err(to_store_error)?;
        }

        tx.execute(
            r#"
            DELETE FROM artwork_assets
            WHERE NOT EXISTS (
                SELECT 1 FROM track_artwork_refs WHERE track_artwork_refs.asset_id = artwork_assets.asset_id
                UNION ALL
                SELECT 1 FROM album_artwork_refs WHERE album_artwork_refs.asset_id = artwork_assets.asset_id
                UNION ALL
                SELECT 1 FROM playlist_artwork_refs WHERE playlist_artwork_refs.asset_id = artwork_assets.asset_id
            )
            "#,
            [],
        )
        .map_err(to_store_error)?;
        tx.commit().map_err(to_store_error)?;
        Ok(true)
    }

    pub fn track_by_file_hash(&self, file_hash: &str) -> PlayerResult<Option<Track>> {
        self.track_by_hash_column("file_hash", file_hash)
    }

    pub fn track_by_audio_hash(&self, audio_hash: &str) -> PlayerResult<Option<Track>> {
        self.track_by_hash_column("audio_hash", audio_hash)
    }

    fn track_by_hash_column(&self, column: &str, hash: &str) -> PlayerResult<Option<Track>> {
        if hash.trim().is_empty() {
            return Ok(None);
        }

        let sql = format!(
            r#"
            SELECT id, path, title, artist, album, album_artist, genre,
                   track_number, disc_number, year, duration_ms, artwork_count,
                   size_bytes, modified_unix_seconds, integrated_lufs, true_peak_dbtp,
                   album_integrated_lufs, album_true_peak_dbtp, analysis_version,
                   file_hash, audio_hash,
                       view_id, primary_view_id, view_kind, transform_spec,
                       quality_profile, format_name, view_name, user_rating
            FROM tracks
            WHERE {column} = ?1
            ORDER BY added_at_unix_seconds ASC, path
            LIMIT 1
            "#
        );

        self.conn
            .query_row(&sql, params![hash], row_to_track)
            .optional()
            .map_err(to_store_error)
    }

    pub fn search_tracks(&self, query: &str, limit: usize) -> PlayerResult<Vec<Track>> {
        let pattern = like_pattern(query);
        let mut stmt = self
            .conn
            .prepare(
                r#"
                SELECT id, path, title, artist, album, album_artist, genre,
                       track_number, disc_number, year, duration_ms, artwork_count,
                       size_bytes, modified_unix_seconds, integrated_lufs, true_peak_dbtp,
                       album_integrated_lufs, album_true_peak_dbtp, analysis_version,
                       file_hash, audio_hash,
                       view_id, primary_view_id, view_kind, transform_spec,
                       quality_profile, format_name, view_name, user_rating
                FROM tracks
                WHERE lower(title) LIKE ?1 ESCAPE '\'
                   OR lower(COALESCE(artist, '')) LIKE ?1 ESCAPE '\'
                   OR lower(COALESCE(album, '')) LIKE ?1 ESCAPE '\'
                   OR lower(COALESCE(album_artist, '')) LIKE ?1 ESCAPE '\'
                   OR lower(COALESCE(genre, '')) LIKE ?1 ESCAPE '\'
                   OR lower(path) LIKE ?1 ESCAPE '\'
                ORDER BY lower(title), path
                LIMIT ?2
                "#,
            )
            .map_err(to_store_error)?;

        let rows = stmt
            .query_map(
                params![pattern, saturating_i64_from_u64(limit.max(1) as u64)],
                row_to_track,
            )
            .map_err(to_store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_store_error)?;
        Ok(rows)
    }

    pub fn create_playlist(&mut self, name: &str) -> PlayerResult<i64> {
        let name = clean_required_name(name)?;
        let now = now_unix_seconds();
        let last_used = self.next_playlist_use_timestamp()?;
        self.conn
            .execute(
                r#"
                INSERT INTO playlists (
                    name,
                    created_at_unix_seconds,
                    updated_at_unix_seconds,
                    last_used_at_unix_seconds
                )
                VALUES (?1, ?2, ?2, ?3)
                ON CONFLICT(name) DO UPDATE SET updated_at_unix_seconds = playlists.updated_at_unix_seconds
                "#,
                params![name, now, last_used],
            )
            .map_err(to_store_error)?;

        self.playlist_id_by_name(name)?
            .ok_or_else(|| PlayerError::store("playlist was not created"))
    }

    pub fn playlists(&self) -> PlayerResult<Vec<PlaylistSummary>> {
        let mut stmt = self
            .conn
            .prepare(
                r#"
                SELECT playlists.id, playlists.name,
                       COUNT(playlist_items.id) AS track_count,
                       EXISTS(
                           SELECT 1 FROM playlist_artwork_refs
                           WHERE playlist_artwork_refs.playlist_id = playlists.id
                       ) AS has_artwork,
                       playlists.created_at_unix_seconds,
                       playlists.updated_at_unix_seconds,
                       playlists.last_used_at_unix_seconds
                FROM playlists
                LEFT JOIN playlist_items ON playlist_items.playlist_id = playlists.id
                GROUP BY playlists.id, playlists.name
                ORDER BY lower(playlists.name)
                "#,
            )
            .map_err(to_store_error)?;

        let rows = stmt
            .query_map([], row_to_playlist_summary)
            .map_err(to_store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_store_error)?;
        Ok(rows)
    }

    pub fn recent_playlists(&self, limit: usize) -> PlayerResult<Vec<PlaylistSummary>> {
        let mut stmt = self
            .conn
            .prepare(
                r#"
                SELECT playlists.id, playlists.name,
                       COUNT(playlist_items.id) AS track_count,
                       EXISTS(
                           SELECT 1 FROM playlist_artwork_refs
                           WHERE playlist_artwork_refs.playlist_id = playlists.id
                       ) AS has_artwork,
                       playlists.created_at_unix_seconds,
                       playlists.updated_at_unix_seconds,
                       playlists.last_used_at_unix_seconds
                FROM playlists
                LEFT JOIN playlist_items ON playlist_items.playlist_id = playlists.id
                GROUP BY playlists.id, playlists.name
                ORDER BY playlists.last_used_at_unix_seconds DESC,
                         playlists.updated_at_unix_seconds DESC,
                         lower(playlists.name)
                LIMIT ?1
                "#,
            )
            .map_err(to_store_error)?;

        let rows = stmt
            .query_map(
                params![saturating_i64_from_u64(limit.max(1) as u64)],
                row_to_playlist_summary,
            )
            .map_err(to_store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_store_error)?;
        Ok(rows)
    }

    pub fn touch_playlist(&mut self, name: &str) -> PlayerResult<bool> {
        let name = clean_required_name(name)?;
        let last_used = self.next_playlist_use_timestamp()?;
        let updated = self
            .conn
            .execute(
                "UPDATE playlists SET last_used_at_unix_seconds = ?2 WHERE name = ?1",
                params![name, last_used],
            )
            .map_err(to_store_error)?;
        Ok(updated > 0)
    }

    pub fn rename_playlist(&mut self, old_name: &str, new_name: &str) -> PlayerResult<()> {
        let old_name = clean_required_name(old_name)?;
        let new_name = clean_required_name(new_name)?;
        let updated = self
            .conn
            .execute(
                "UPDATE playlists SET name = ?2, updated_at_unix_seconds = ?3 WHERE name = ?1",
                params![old_name, new_name, now_unix_seconds()],
            )
            .map_err(to_store_error)?;
        if updated == 0 {
            return Err(PlayerError::store(format!(
                "playlist not found: {old_name}"
            )));
        }
        Ok(())
    }

    pub fn delete_playlist(&mut self, name: &str) -> PlayerResult<bool> {
        let name = clean_required_name(name)?;
        let deleted = self
            .conn
            .execute("DELETE FROM playlists WHERE name = ?1", params![name])
            .map_err(to_store_error)?;
        Ok(deleted > 0)
    }

    pub fn clear_playlist(&mut self, name: &str) -> PlayerResult<usize> {
        let Some(playlist_id) = self.playlist_id_by_name(clean_required_name(name)?)? else {
            return Ok(0);
        };
        let deleted = self
            .conn
            .execute(
                "DELETE FROM playlist_items WHERE playlist_id = ?1",
                params![playlist_id],
            )
            .map_err(to_store_error)?;
        self.conn
            .execute(
                "UPDATE playlists SET updated_at_unix_seconds = ?2 WHERE id = ?1",
                params![playlist_id, now_unix_seconds()],
            )
            .map_err(to_store_error)?;
        Ok(deleted)
    }

    pub fn playlist_tracks(&self, name: &str) -> PlayerResult<Vec<PlaylistEntry>> {
        let Some(playlist_id) = self.playlist_id_by_name(name)? else {
            return Ok(Vec::new());
        };
        let mut stmt = self
            .conn
            .prepare(
                r#"
                SELECT playlist_items.id, playlist_items.position,
                       tracks.id, tracks.path, tracks.title, tracks.artist, tracks.album,
                       tracks.album_artist, tracks.genre, tracks.track_number, tracks.disc_number,
                       tracks.year, tracks.duration_ms, tracks.artwork_count,
                       tracks.size_bytes, tracks.modified_unix_seconds, tracks.integrated_lufs,
                       tracks.true_peak_dbtp, tracks.album_integrated_lufs,
                       tracks.album_true_peak_dbtp, tracks.analysis_version,
                       tracks.file_hash, tracks.audio_hash,
                       tracks.view_id, tracks.primary_view_id, tracks.view_kind, tracks.transform_spec,
                       tracks.quality_profile, tracks.format_name, tracks.view_name, tracks.user_rating
                FROM playlist_items
                JOIN tracks ON tracks.path = playlist_items.track_path
                WHERE playlist_items.playlist_id = ?1
                ORDER BY playlist_items.position, playlist_items.id
                "#,
            )
            .map_err(to_store_error)?;

        let rows = stmt
            .query_map(params![playlist_id], |row| {
                Ok(PlaylistEntry {
                    item_id: row.get(0)?,
                    position: optional_u32(Some(row.get::<_, i64>(1)?)).unwrap_or(0),
                    track: row_to_track_at(row, 2)?,
                })
            })
            .map_err(to_store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_store_error)?;
        Ok(rows)
    }

    pub fn search_playlist_tracks(
        &self,
        name: &str,
        query: &str,
        limit: usize,
    ) -> PlayerResult<Vec<PlaylistEntry>> {
        let Some(playlist_id) = self.playlist_id_by_name(name)? else {
            return Ok(Vec::new());
        };
        let pattern = like_pattern(query);
        let mut stmt = self
            .conn
            .prepare(
                r#"
                SELECT playlist_items.id, playlist_items.position,
                       tracks.id, tracks.path, tracks.title, tracks.artist, tracks.album,
                       tracks.album_artist, tracks.genre, tracks.track_number, tracks.disc_number,
                       tracks.year, tracks.duration_ms, tracks.artwork_count,
                       tracks.size_bytes, tracks.modified_unix_seconds, tracks.integrated_lufs,
                       tracks.true_peak_dbtp, tracks.album_integrated_lufs,
                       tracks.album_true_peak_dbtp, tracks.analysis_version,
                       tracks.file_hash, tracks.audio_hash,
                       tracks.view_id, tracks.primary_view_id, tracks.view_kind, tracks.transform_spec,
                       tracks.quality_profile, tracks.format_name, tracks.view_name, tracks.user_rating
                FROM playlist_items
                JOIN tracks ON tracks.path = playlist_items.track_path
                WHERE playlist_items.playlist_id = ?1
                  AND (
                       lower(tracks.title) LIKE ?2 ESCAPE '\'
                    OR lower(COALESCE(tracks.artist, '')) LIKE ?2 ESCAPE '\'
                    OR lower(COALESCE(tracks.album, '')) LIKE ?2 ESCAPE '\'
                    OR lower(COALESCE(tracks.album_artist, '')) LIKE ?2 ESCAPE '\'
                    OR lower(COALESCE(tracks.genre, '')) LIKE ?2 ESCAPE '\'
                    OR lower(tracks.path) LIKE ?2 ESCAPE '\'
                  )
                ORDER BY playlist_items.position, playlist_items.id
                LIMIT ?3
                "#,
            )
            .map_err(to_store_error)?;

        let rows = stmt
            .query_map(
                params![
                    playlist_id,
                    pattern,
                    saturating_i64_from_u64(limit.max(1) as u64)
                ],
                |row| {
                    Ok(PlaylistEntry {
                        item_id: row.get(0)?,
                        position: optional_u32(Some(row.get::<_, i64>(1)?)).unwrap_or(0),
                        track: row_to_track_at(row, 2)?,
                    })
                },
            )
            .map_err(to_store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_store_error)?;
        Ok(rows)
    }

    pub fn add_playlist_track(
        &mut self,
        playlist_name: &str,
        path: impl AsRef<Path>,
    ) -> PlayerResult<i64> {
        let playlist_id = self.create_playlist(playlist_name)?;
        let path = path_to_string(path.as_ref());
        let position = self.next_playlist_position(playlist_id)?;
        let now = now_unix_seconds();
        self.conn
            .execute(
                r#"
                INSERT INTO playlist_items
                    (playlist_id, position, track_path, added_at_unix_seconds)
                VALUES (?1, ?2, ?3, ?4)
                "#,
                params![playlist_id, i64::from(position), path, now],
            )
            .map_err(to_store_error)?;
        self.conn
            .execute(
                "UPDATE playlists SET updated_at_unix_seconds = ?2 WHERE id = ?1",
                params![playlist_id, now],
            )
            .map_err(to_store_error)?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn remove_playlist_track(
        &mut self,
        playlist_name: &str,
        path: impl AsRef<Path>,
    ) -> PlayerResult<usize> {
        let Some(playlist_id) = self.playlist_id_by_name(clean_required_name(playlist_name)?)?
        else {
            return Ok(0);
        };
        let path = path_to_string(path.as_ref());
        let deleted = self
            .conn
            .execute(
                r#"
                DELETE FROM playlist_items
                WHERE id IN (
                    SELECT id FROM playlist_items
                    WHERE playlist_id = ?1 AND track_path = ?2
                    ORDER BY position, id
                    LIMIT 1
                )
                "#,
                params![playlist_id, path],
            )
            .map_err(to_store_error)?;
        if deleted > 0 {
            self.normalize_playlist_positions(playlist_id)?;
        }
        Ok(deleted)
    }

    pub fn move_playlist_track(
        &mut self,
        playlist_name: &str,
        path: impl AsRef<Path>,
        delta: i32,
    ) -> PlayerResult<bool> {
        if delta == 0 {
            return Ok(false);
        }
        let Some(playlist_id) = self.playlist_id_by_name(clean_required_name(playlist_name)?)?
        else {
            return Ok(false);
        };
        let path = path_to_string(path.as_ref());
        let items = self.playlist_item_rows(playlist_id)?;
        let Some(index) = items
            .iter()
            .position(|(_, _, item_path)| item_path == &path)
        else {
            return Ok(false);
        };
        let target_index = if delta < 0 {
            index.checked_sub(1)
        } else if index + 1 < items.len() {
            Some(index + 1)
        } else {
            None
        };
        let Some(target_index) = target_index else {
            return Ok(false);
        };
        let (item_id, item_position, _) = &items[index];
        let (target_id, target_position, _) = &items[target_index];
        let tx = self.conn.transaction().map_err(to_store_error)?;
        tx.execute(
            "UPDATE playlist_items SET position = ?2 WHERE id = ?1",
            params![item_id, target_position],
        )
        .map_err(to_store_error)?;
        tx.execute(
            "UPDATE playlist_items SET position = ?2 WHERE id = ?1",
            params![target_id, item_position],
        )
        .map_err(to_store_error)?;
        tx.execute(
            "UPDATE playlists SET updated_at_unix_seconds = ?2 WHERE id = ?1",
            params![playlist_id, now_unix_seconds()],
        )
        .map_err(to_store_error)?;
        tx.commit().map_err(to_store_error)?;
        Ok(true)
    }

    pub fn sort_playlist(
        &mut self,
        playlist_name: &str,
        sort: PlaylistSort,
    ) -> PlayerResult<usize> {
        let Some(playlist_id) = self.playlist_id_by_name(clean_required_name(playlist_name)?)?
        else {
            return Ok(0);
        };
        let mut items = self.playlist_sort_items(playlist_id)?;
        if items.len() <= 1 {
            return Ok(items.len());
        }

        sort_playlist_items(&mut items, sort);
        let item_ids = items.iter().map(|item| item.item_id).collect::<Vec<_>>();
        self.rewrite_playlist_positions(playlist_id, &item_ids)?;
        Ok(item_ids.len())
    }

    pub fn save_playback_queue(
        &mut self,
        track_paths: &[PathBuf],
        current_index: Option<usize>,
        position_ms: u64,
        repeat_mode: RepeatMode,
        shuffle_enabled: bool,
    ) -> PlayerResult<()> {
        if let Some(index) = current_index {
            if index >= track_paths.len() {
                return Err(PlayerError::invalid_input(format!(
                    "invalid persisted queue index {index} for queue length {}",
                    track_paths.len()
                )));
            }
        }

        let tx = self.conn.transaction().map_err(to_store_error)?;
        tx.execute("DELETE FROM playback_queue_items", [])
            .map_err(to_store_error)?;
        {
            let mut stmt = tx
                .prepare(
                    r#"
                    INSERT INTO playback_queue_items (position, track_path)
                    VALUES (?1, ?2)
                    "#,
                )
                .map_err(to_store_error)?;
            for (index, path) in track_paths.iter().enumerate() {
                stmt.execute(params![
                    saturating_i64_from_u64(index as u64),
                    path_to_string(path)
                ])
                .map_err(to_store_error)?;
            }
        }
        tx.execute(
            r#"
            INSERT INTO playback_queue_state (
                singleton_id,
                current_index,
                position_ms,
                repeat_mode,
                shuffle_enabled,
                updated_at_unix_seconds
            )
            VALUES (1, ?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(singleton_id) DO UPDATE SET
                current_index = excluded.current_index,
                position_ms = excluded.position_ms,
                repeat_mode = excluded.repeat_mode,
                shuffle_enabled = excluded.shuffle_enabled,
                updated_at_unix_seconds = excluded.updated_at_unix_seconds
            "#,
            params![
                current_index.map(|index| saturating_i64_from_u64(index as u64)),
                saturating_i64_from_u64(position_ms),
                repeat_mode_to_store_value(repeat_mode),
                shuffle_enabled,
                now_unix_seconds(),
            ],
        )
        .map_err(to_store_error)?;
        tx.commit().map_err(to_store_error)
    }

    pub fn load_playback_queue(&self) -> PlayerResult<StoredPlaybackQueue> {
        let state = self
            .conn
            .query_row(
                r#"
                SELECT current_index, position_ms, repeat_mode, shuffle_enabled
                FROM playback_queue_state
                WHERE singleton_id = 1
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

        let mut stmt = self
            .conn
            .prepare(
                r#"
                SELECT tracks.id, tracks.path, tracks.title, tracks.artist, tracks.album,
                       tracks.album_artist, tracks.genre, tracks.track_number, tracks.disc_number,
                       tracks.year, tracks.duration_ms, tracks.artwork_count,
                       tracks.size_bytes, tracks.modified_unix_seconds, tracks.integrated_lufs,
                       tracks.true_peak_dbtp, tracks.album_integrated_lufs,
                       tracks.album_true_peak_dbtp, tracks.analysis_version,
                       tracks.file_hash, tracks.audio_hash,
                       tracks.view_id, tracks.primary_view_id, tracks.view_kind,
                       tracks.transform_spec, tracks.quality_profile, tracks.format_name,
                       tracks.view_name, tracks.user_rating
                FROM playback_queue_items
                JOIN tracks ON tracks.path = playback_queue_items.track_path
                ORDER BY playback_queue_items.position
                "#,
            )
            .map_err(to_store_error)?;
        let tracks = stmt
            .query_map([], row_to_track)
            .map_err(to_store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_store_error)?;

        let Some((stored_index, position_ms, repeat_mode, shuffle_enabled)) = state else {
            return Ok(StoredPlaybackQueue {
                tracks,
                current_index: None,
                position_ms: 0,
                repeat_mode: RepeatMode::Off,
                shuffle_enabled: false,
            });
        };
        let current_index = stored_index
            .and_then(|value| optional_usize(Some(value)))
            .filter(|index| *index < tracks.len());
        Ok(StoredPlaybackQueue {
            position_ms: if current_index.is_some() {
                position_ms.max(0) as u64
            } else {
                0
            },
            tracks,
            current_index,
            repeat_mode: repeat_mode_from_store_value(&repeat_mode)?,
            shuffle_enabled,
        })
    }

    pub fn set_favorite(&mut self, path: impl AsRef<Path>, favorite: bool) -> PlayerResult<()> {
        let path = path_to_string(path.as_ref());
        if favorite {
            self.conn
                .execute(
                    r#"
                    INSERT OR IGNORE INTO favorite_tracks (track_path, created_at_unix_seconds)
                    VALUES (?1, ?2)
                    "#,
                    params![path, now_unix_seconds()],
                )
                .map_err(to_store_error)?;
        } else {
            self.conn
                .execute(
                    "DELETE FROM favorite_tracks WHERE track_path = ?1",
                    params![path],
                )
                .map_err(to_store_error)?;
        }
        Ok(())
    }

    pub fn favorite_tracks(&self) -> PlayerResult<Vec<Track>> {
        let mut stmt = self
            .conn
            .prepare(
                r#"
                SELECT tracks.id, tracks.path, tracks.title, tracks.artist, tracks.album,
                       tracks.album_artist, tracks.genre, tracks.track_number, tracks.disc_number,
                       tracks.year, tracks.duration_ms, tracks.artwork_count,
                       tracks.size_bytes, tracks.modified_unix_seconds, tracks.integrated_lufs,
                       tracks.true_peak_dbtp, tracks.album_integrated_lufs,
                       tracks.album_true_peak_dbtp, tracks.analysis_version,
                       tracks.file_hash, tracks.audio_hash,
                       tracks.view_id, tracks.primary_view_id, tracks.view_kind, tracks.transform_spec,
                       tracks.quality_profile, tracks.format_name, tracks.view_name, tracks.user_rating
                FROM favorite_tracks
                JOIN tracks ON tracks.path = favorite_tracks.track_path
                ORDER BY favorite_tracks.created_at_unix_seconds DESC, tracks.path
                "#,
            )
            .map_err(to_store_error)?;

        let rows = stmt
            .query_map([], row_to_track)
            .map_err(to_store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_store_error)?;
        Ok(rows)
    }

    pub fn record_playback(
        &mut self,
        path: impl AsRef<Path>,
        position_ms: u64,
        completed: bool,
    ) -> PlayerResult<i64> {
        self.conn
            .execute(
                r#"
                INSERT INTO play_history
                    (track_path, played_at_unix_seconds, position_ms, completed)
                VALUES (?1, ?2, ?3, ?4)
                "#,
                params![
                    path_to_string(path.as_ref()),
                    now_unix_seconds(),
                    saturating_i64_from_u64(position_ms),
                    if completed { 1_i64 } else { 0_i64 },
                ],
            )
            .map_err(to_store_error)?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn play_history(&self, limit: usize) -> PlayerResult<Vec<PlayHistoryEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                r#"
                SELECT play_history.id, play_history.played_at_unix_seconds,
                       play_history.position_ms, play_history.completed,
                       tracks.id, tracks.path, tracks.title, tracks.artist, tracks.album,
                       tracks.album_artist, tracks.genre, tracks.track_number, tracks.disc_number,
                       tracks.year, tracks.duration_ms, tracks.artwork_count,
                       tracks.size_bytes, tracks.modified_unix_seconds, tracks.integrated_lufs,
                       tracks.true_peak_dbtp, tracks.album_integrated_lufs,
                       tracks.album_true_peak_dbtp, tracks.analysis_version,
                       tracks.file_hash, tracks.audio_hash,
                       tracks.view_id, tracks.primary_view_id, tracks.view_kind, tracks.transform_spec,
                       tracks.quality_profile, tracks.format_name, tracks.view_name, tracks.user_rating
                FROM play_history
                JOIN tracks ON tracks.path = play_history.track_path
                ORDER BY play_history.played_at_unix_seconds DESC, play_history.id DESC
                LIMIT ?1
                "#,
            )
            .map_err(to_store_error)?;

        let rows = stmt
            .query_map(
                params![saturating_i64_from_u64(limit.max(1) as u64)],
                |row| {
                    Ok(PlayHistoryEntry {
                        id: row.get(0)?,
                        played_at_unix_seconds: row.get(1)?,
                        position_ms: optional_u64(Some(row.get::<_, i64>(2)?)).unwrap_or(0),
                        completed: row.get::<_, i64>(3)? != 0,
                        track: row_to_track_at(row, 4)?,
                    })
                },
            )
            .map_err(to_store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_store_error)?;
        Ok(rows)
    }

    pub fn save_artwork(
        &mut self,
        path: impl AsRef<Path>,
        images: &[ArtworkImage],
    ) -> PlayerResult<usize> {
        let path = path_to_string(path.as_ref());
        let tx = self.conn.transaction().map_err(to_store_error)?;
        tx.execute(
            "DELETE FROM track_artwork WHERE track_path = ?1",
            params![path],
        )
        .map_err(to_store_error)?;
        let now = now_unix_seconds();
        {
            let mut stmt = tx
                .prepare(
                    r#"
                    INSERT INTO track_artwork
                        (track_path, picture_index, mime_type, picture_type, description, data,
                         updated_at_unix_seconds)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                    "#,
                )
                .map_err(to_store_error)?;

            for image in images {
                stmt.execute(params![
                    path.as_str(),
                    i64::from(image.picture_index),
                    image.mime_type.as_deref(),
                    image.picture_type.as_str(),
                    image.description.as_deref(),
                    image.data.as_slice(),
                    now,
                ])
                .map_err(to_store_error)?;
            }
        }
        tx.execute(
            "UPDATE tracks SET artwork_count = ?2, updated_at_unix_seconds = ?3 WHERE path = ?1",
            params![path, saturating_i64_from_u64(images.len() as u64), now],
        )
        .map_err(to_store_error)?;
        tx.commit().map_err(to_store_error)?;
        Ok(images.len())
    }

    pub fn artwork_for_path(&self, path: impl AsRef<Path>) -> PlayerResult<Vec<ArtworkImage>> {
        let mut stmt = self
            .conn
            .prepare(
                r#"
                SELECT picture_index, mime_type, picture_type, description, data
                FROM track_artwork
                WHERE track_path = ?1
                ORDER BY picture_index
                "#,
            )
            .map_err(to_store_error)?;

        let rows = stmt
            .query_map(params![path_to_string(path.as_ref())], row_to_artwork)
            .map_err(to_store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_store_error)?;
        Ok(rows)
    }

    pub fn set_track_artwork_reference(
        &mut self,
        path: impl AsRef<Path>,
        image: &ArtworkImage,
    ) -> PlayerResult<usize> {
        let path = path_to_string(path.as_ref());
        let now = now_unix_seconds();
        let tx = self.conn.transaction().map_err(to_store_error)?;
        let asset_id = upsert_artwork_asset_tx(&tx, image)?;
        tx.execute(
            r#"
            INSERT INTO track_artwork_refs
                (track_path, asset_id, updated_at_unix_seconds)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(track_path) DO UPDATE SET
                asset_id = excluded.asset_id,
                updated_at_unix_seconds = excluded.updated_at_unix_seconds
            "#,
            params![path.as_str(), asset_id.as_str(), now],
        )
        .map_err(to_store_error)?;
        let updated = tx
            .execute(
                r#"
                UPDATE tracks
                SET artwork_count = CASE
                        WHEN artwork_count = 0 THEN 1
                        ELSE artwork_count
                    END,
                    updated_at_unix_seconds = ?2
                WHERE path = ?1
                "#,
                params![path.as_str(), now],
            )
            .map_err(to_store_error)?;
        tx.commit().map_err(to_store_error)?;
        Ok(updated)
    }

    pub fn set_album_artwork_reference_for_track(
        &mut self,
        path: impl AsRef<Path>,
        image: &ArtworkImage,
    ) -> PlayerResult<usize> {
        let track = self.track_by_path(path.as_ref())?.ok_or_else(|| {
            PlayerError::store(format!("track not found: {}", path.as_ref().display()))
        })?;
        let album_key = required_track_album_key(&track)?;
        let member_paths = self
            .tracks()?
            .into_iter()
            .filter(|candidate| {
                optional_track_album_key(candidate).as_deref() == Some(album_key.as_str())
            })
            .map(|track| path_to_string(&track.path))
            .collect::<Vec<_>>();
        if member_paths.is_empty() {
            return Ok(0);
        }

        let now = now_unix_seconds();
        let tx = self.conn.transaction().map_err(to_store_error)?;
        let asset_id = upsert_artwork_asset_tx(&tx, image)?;
        {
            let mut stmt = tx
                .prepare(
                    r#"
                    INSERT INTO album_artwork_refs
                        (track_path, album_key, asset_id, updated_at_unix_seconds)
                    VALUES (?1, ?2, ?3, ?4)
                    ON CONFLICT(track_path) DO UPDATE SET
                        album_key = excluded.album_key,
                        asset_id = excluded.asset_id,
                        updated_at_unix_seconds = excluded.updated_at_unix_seconds
                    "#,
                )
                .map_err(to_store_error)?;
            for member_path in &member_paths {
                stmt.execute(params![
                    member_path.as_str(),
                    album_key.as_str(),
                    asset_id.as_str(),
                    now,
                ])
                .map_err(to_store_error)?;
            }
        }

        {
            let mut stmt = tx
                .prepare(
                    r#"
                    UPDATE tracks
                    SET artwork_count = CASE
                            WHEN artwork_count = 0 THEN 1
                            ELSE artwork_count
                        END,
                        updated_at_unix_seconds = ?2
                    WHERE path = ?1
                    "#,
                )
                .map_err(to_store_error)?;
            for member_path in &member_paths {
                stmt.execute(params![member_path.as_str(), now])
                    .map_err(to_store_error)?;
            }
        }

        tx.commit().map_err(to_store_error)?;
        Ok(member_paths.len())
    }

    pub fn track_artwork_reference(
        &self,
        path: impl AsRef<Path>,
    ) -> PlayerResult<Option<ArtworkReference>> {
        self.artwork_reference_from_table(
            "track_artwork_refs",
            path.as_ref(),
            ArtworkReferenceScope::Track,
        )
    }

    pub fn album_artwork_reference(
        &self,
        path: impl AsRef<Path>,
    ) -> PlayerResult<Option<ArtworkReference>> {
        self.artwork_reference_from_table(
            "album_artwork_refs",
            path.as_ref(),
            ArtworkReferenceScope::Album,
        )
    }

    pub fn effective_artwork_reference(
        &self,
        path: impl AsRef<Path>,
    ) -> PlayerResult<Option<ArtworkReference>> {
        if let Some(reference) = self.track_artwork_reference(path.as_ref())? {
            return Ok(Some(reference));
        }
        self.album_artwork_reference(path)
    }

    pub fn copy_artwork_references(
        &mut self,
        source_path: impl AsRef<Path>,
        destination_path: impl AsRef<Path>,
    ) -> PlayerResult<()> {
        let source_path = path_to_string(source_path.as_ref());
        let destination_path = path_to_string(destination_path.as_ref());
        let now = now_unix_seconds();
        let tx = self.conn.transaction().map_err(to_store_error)?;
        tx.execute(
            r#"
            INSERT INTO track_artwork_refs
                (track_path, asset_id, updated_at_unix_seconds)
            SELECT ?2, asset_id, ?3
            FROM track_artwork_refs
            WHERE track_path = ?1
            ON CONFLICT(track_path) DO UPDATE SET
                asset_id = excluded.asset_id,
                updated_at_unix_seconds = excluded.updated_at_unix_seconds
            "#,
            params![source_path.as_str(), destination_path.as_str(), now],
        )
        .map_err(to_store_error)?;
        tx.execute(
            r#"
            INSERT INTO album_artwork_refs
                (track_path, album_key, asset_id, updated_at_unix_seconds)
            SELECT ?2, album_key, asset_id, ?3
            FROM album_artwork_refs
            WHERE track_path = ?1
            ON CONFLICT(track_path) DO UPDATE SET
                album_key = excluded.album_key,
                asset_id = excluded.asset_id,
                updated_at_unix_seconds = excluded.updated_at_unix_seconds
            "#,
            params![source_path.as_str(), destination_path.as_str(), now],
        )
        .map_err(to_store_error)?;
        tx.execute(
            r#"
            UPDATE tracks
            SET artwork_count = CASE
                    WHEN EXISTS (
                        SELECT 1 FROM track_artwork_refs
                        WHERE track_path = ?1 AND asset_id IS NOT NULL
                        UNION
                        SELECT 1 FROM album_artwork_refs
                        WHERE track_path = ?1 AND asset_id IS NOT NULL
                    ) AND artwork_count = 0 THEN 1
                    ELSE artwork_count
                END,
                updated_at_unix_seconds = ?2
            WHERE path = ?1
            "#,
            params![destination_path.as_str(), now],
        )
        .map_err(to_store_error)?;
        tx.commit().map_err(to_store_error)?;
        Ok(())
    }

    pub fn track_notes(&self, path: impl AsRef<Path>) -> PlayerResult<Option<String>> {
        self.conn
            .query_row(
                "SELECT notes FROM track_notes WHERE track_path = ?1",
                params![path_to_string(path.as_ref())],
                |row| row.get(0),
            )
            .optional()
            .map_err(to_store_error)
    }

    pub fn set_track_notes(&mut self, path: impl AsRef<Path>, notes: &str) -> PlayerResult<()> {
        let path = path_to_string(path.as_ref());
        let notes = notes.trim();
        if notes.is_empty() {
            self.conn
                .execute(
                    "DELETE FROM track_notes WHERE track_path = ?1",
                    params![path],
                )
                .map_err(to_store_error)?;
            return Ok(());
        }

        self.conn
            .execute(
                r#"
                INSERT INTO track_notes (track_path, notes, updated_at_unix_seconds)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(track_path) DO UPDATE SET
                    notes = excluded.notes,
                    updated_at_unix_seconds = excluded.updated_at_unix_seconds
                "#,
                params![path, notes, now_unix_seconds()],
            )
            .map_err(to_store_error)?;
        Ok(())
    }

    pub fn track_metadata(
        &self,
        path: impl AsRef<Path>,
    ) -> PlayerResult<Option<TrackMetadataView>> {
        self.conn
            .query_row(
                r#"
                SELECT view_id, primary_view_id, view_kind, transform_spec,
                       quality_profile, format_name, view_name, user_rating, audio_hash,
                       original_title, original_artist, original_album,
                       title, artist, album, metadata_edited_at_unix_seconds
                FROM tracks
                WHERE path = ?1
                "#,
                params![path_to_string(path.as_ref())],
                |row| {
                    Ok(TrackMetadataView {
                        view_id: row.get(0)?,
                        primary_view_id: row.get(1)?,
                        view_kind: row.get(2)?,
                        transform_spec: row.get(3)?,
                        quality_profile: row.get(4)?,
                        format_name: row.get(5)?,
                        view_name: row.get(6)?,
                        user_rating: optional_rating(row.get::<_, Option<i64>>(7)?),
                        audio_hash: row.get(8)?,
                        original_title: row.get(9)?,
                        original_artist: row.get(10)?,
                        original_album: row.get(11)?,
                        display_title: row.get(12)?,
                        display_artist: row.get(13)?,
                        display_album: row.get(14)?,
                        metadata_edited_at_unix_seconds: row.get(15)?,
                    })
                },
            )
            .optional()
            .map_err(to_store_error)
    }

    pub fn set_track_display_metadata(
        &mut self,
        path: impl AsRef<Path>,
        title: &str,
        artist: Option<&str>,
        album: Option<&str>,
    ) -> PlayerResult<usize> {
        let path = path_to_string(path.as_ref());
        let title = clean_required_name(title)?;
        let artist = clean_metadata_value(artist);
        let album = clean_metadata_value(album);
        let updated = self
            .conn
            .execute(
                r#"
                UPDATE tracks
                SET title = ?1,
                    artist = ?2,
                    album = ?3,
                    metadata_edited_at_unix_seconds = ?4,
                    updated_at_unix_seconds = ?4
                WHERE path = ?5
                "#,
                params![title, artist, album, now_unix_seconds(), path],
            )
            .map_err(to_store_error)?;
        Ok(updated)
    }

    pub fn set_track_rating(
        &mut self,
        path: impl AsRef<Path>,
        rating: Option<u8>,
    ) -> PlayerResult<usize> {
        let path = path_to_string(path.as_ref());
        let rating = rating_to_sql(rating)?;
        let updated = self
            .conn
            .execute(
                r#"
                UPDATE tracks
                SET user_rating = ?1,
                    updated_at_unix_seconds = ?2
                WHERE path = ?3
                "#,
                params![rating, now_unix_seconds(), path],
            )
            .map_err(to_store_error)?;
        Ok(updated)
    }

    pub fn save_playlist_artwork(
        &mut self,
        playlist_name: &str,
        image: &ArtworkImage,
    ) -> PlayerResult<()> {
        let playlist_id = self
            .playlist_id_by_name(clean_required_name(playlist_name)?)?
            .ok_or_else(|| PlayerError::store(format!("playlist not found: {playlist_name}")))?;
        let now = now_unix_seconds();
        let tx = self.conn.transaction().map_err(to_store_error)?;
        let asset_id = upsert_artwork_asset_tx(&tx, image)?;
        tx.execute(
            r#"
            INSERT INTO playlist_artwork_refs
                (playlist_id, asset_id, updated_at_unix_seconds)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(playlist_id) DO UPDATE SET
                asset_id = excluded.asset_id,
                updated_at_unix_seconds = excluded.updated_at_unix_seconds
            "#,
            params![playlist_id, asset_id.as_str(), now],
        )
        .map_err(to_store_error)?;
        tx.execute(
            "UPDATE playlists SET updated_at_unix_seconds = ?2 WHERE id = ?1",
            params![playlist_id, now],
        )
        .map_err(to_store_error)?;
        tx.commit().map_err(to_store_error)?;
        Ok(())
    }

    pub fn playlist_artwork(&self, playlist_name: &str) -> PlayerResult<Option<ArtworkImage>> {
        let Some(playlist_id) = self.playlist_id_by_name(clean_required_name(playlist_name)?)?
        else {
            return Ok(None);
        };
        self.conn
            .query_row(
                r#"
                SELECT assets.mime_type, assets.description, assets.data
                FROM playlist_artwork_refs AS refs
                JOIN artwork_assets AS assets ON assets.asset_id = refs.asset_id
                WHERE refs.playlist_id = ?1
                "#,
                params![playlist_id],
                |row| {
                    Ok(ArtworkImage {
                        picture_index: 0,
                        mime_type: row.get(0)?,
                        picture_type: "CoverFront".to_owned(),
                        description: row.get(1)?,
                        data: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(to_store_error)
    }

    pub fn playlist_artwork_asset_id(&self, playlist_name: &str) -> PlayerResult<Option<String>> {
        let Some(playlist_id) = self.playlist_id_by_name(clean_required_name(playlist_name)?)?
        else {
            return Ok(None);
        };
        self.conn
            .query_row(
                "SELECT asset_id FROM playlist_artwork_refs WHERE playlist_id = ?1",
                params![playlist_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(to_store_error)
    }

    pub fn update_track_hashes(
        &mut self,
        path: impl AsRef<Path>,
        file_hash: Option<&str>,
        audio_hash: Option<&str>,
        fingerprint: Option<FileFingerprint>,
    ) -> PlayerResult<()> {
        let primary_view_id = audio_hash
            .filter(|hash| !hash.trim().is_empty())
            .map(|hash| {
                TrackViewId::primary_from_audio_hash(hash)
                    .value()
                    .to_owned()
            });
        self.conn
            .execute(
                r#"
                UPDATE tracks
                SET file_hash = COALESCE(?2, file_hash),
                    audio_hash = COALESCE(?3, audio_hash),
                    size_bytes = COALESCE(?4, size_bytes),
                    modified_unix_seconds = COALESCE(?5, modified_unix_seconds),
                    view_id = CASE
                        WHEN ?7 IS NOT NULL AND view_kind = 'primary' THEN ?7
                        ELSE view_id
                    END,
                    primary_view_id = COALESCE(?7, primary_view_id),
                    updated_at_unix_seconds = ?6
                WHERE path = ?1
                "#,
                params![
                    path_to_string(path.as_ref()),
                    file_hash,
                    audio_hash,
                    fingerprint.map(|fingerprint| saturating_i64_from_u64(fingerprint.size_bytes)),
                    fingerprint.map(|fingerprint| fingerprint.modified_unix_seconds),
                    now_unix_seconds(),
                    primary_view_id.as_deref(),
                ],
            )
            .map_err(to_store_error)?;
        Ok(())
    }

    pub fn merge_duplicate_track(
        &mut self,
        canonical_path: impl AsRef<Path>,
        duplicate_path: impl AsRef<Path>,
    ) -> PlayerResult<bool> {
        let canonical_path = path_to_string(canonical_path.as_ref());
        let duplicate_path = path_to_string(duplicate_path.as_ref());
        if canonical_path == duplicate_path {
            return Ok(false);
        }

        let tx = self.conn.transaction().map_err(to_store_error)?;
        let canonical_exists = tx
            .query_row(
                "SELECT 1 FROM tracks WHERE path = ?1",
                params![canonical_path.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(to_store_error)?
            .is_some();
        let duplicate_exists = tx
            .query_row(
                "SELECT 1 FROM tracks WHERE path = ?1",
                params![duplicate_path.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(to_store_error)?
            .is_some();
        if !canonical_exists || !duplicate_exists {
            return Ok(false);
        }

        merge_notes(&tx, &canonical_path, &duplicate_path)?;
        merge_artwork(&tx, &canonical_path, &duplicate_path)?;
        merge_artwork_references(&tx, &canonical_path, &duplicate_path)?;
        tx.execute(
            r#"
            UPDATE tracks
            SET user_rating = COALESCE(
                    user_rating,
                    (SELECT user_rating FROM tracks WHERE path = ?2)
                ),
                updated_at_unix_seconds = ?3
            WHERE path = ?1
            "#,
            params![
                canonical_path.as_str(),
                duplicate_path.as_str(),
                now_unix_seconds()
            ],
        )
        .map_err(to_store_error)?;

        tx.execute(
            r#"
            DELETE FROM favorite_tracks
            WHERE track_path = ?2
              AND EXISTS (SELECT 1 FROM favorite_tracks WHERE track_path = ?1)
            "#,
            params![canonical_path.as_str(), duplicate_path.as_str()],
        )
        .map_err(to_store_error)?;
        tx.execute(
            "UPDATE favorite_tracks SET track_path = ?1 WHERE track_path = ?2",
            params![canonical_path.as_str(), duplicate_path.as_str()],
        )
        .map_err(to_store_error)?;
        tx.execute(
            "UPDATE playlist_items SET track_path = ?1 WHERE track_path = ?2",
            params![canonical_path.as_str(), duplicate_path.as_str()],
        )
        .map_err(to_store_error)?;
        tx.execute(
            "UPDATE play_history SET track_path = ?1 WHERE track_path = ?2",
            params![canonical_path.as_str(), duplicate_path.as_str()],
        )
        .map_err(to_store_error)?;
        tx.execute(
            "DELETE FROM tracks WHERE path = ?1",
            params![duplicate_path.as_str()],
        )
        .map_err(to_store_error)?;

        tx.commit().map_err(to_store_error)?;
        Ok(true)
    }

    pub fn artwork_summaries(&self) -> PlayerResult<Vec<ArtworkSummary>> {
        let mut stmt = self
            .conn
            .prepare(
                r#"
                SELECT track_path, COUNT(*), COALESCE(SUM(length(data)), 0)
                FROM track_artwork
                GROUP BY track_path
                ORDER BY track_path
                "#,
            )
            .map_err(to_store_error)?;

        let rows = stmt
            .query_map([], |row| {
                Ok(ArtworkSummary {
                    path: PathBuf::from(row.get::<_, String>(0)?),
                    image_count: optional_usize(Some(row.get::<_, i64>(1)?)).unwrap_or(0),
                    byte_count: optional_usize(Some(row.get::<_, i64>(2)?)).unwrap_or(0),
                })
            })
            .map_err(to_store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_store_error)?;
        Ok(rows)
    }

    pub fn pending_analysis(
        &self,
        analysis_version: u32,
        limit: Option<usize>,
    ) -> PlayerResult<Vec<Track>> {
        let sql = match limit {
            Some(_) => {
                r#"
                SELECT id, path, title, artist, album, album_artist, genre,
                       track_number, disc_number, year, duration_ms, artwork_count,
                       size_bytes, modified_unix_seconds, integrated_lufs, true_peak_dbtp,
                       album_integrated_lufs, album_true_peak_dbtp, analysis_version,
                       file_hash, audio_hash,
                       view_id, primary_view_id, view_kind, transform_spec,
                       quality_profile, format_name, view_name, user_rating
                FROM tracks
                WHERE integrated_lufs IS NULL
                   OR true_peak_dbtp IS NULL
                   OR analysis_version IS NULL
                   OR analysis_version != ?1
                   OR (
                        size_bytes IS NOT NULL
                        AND COALESCE(analysis_size_bytes, -1) != size_bytes
                   )
                   OR (
                        modified_unix_seconds IS NOT NULL
                        AND COALESCE(analysis_modified_unix_seconds, -1) != modified_unix_seconds
                   )
                ORDER BY updated_at_unix_seconds ASC, path
                LIMIT ?2
                "#
            }
            None => {
                r#"
                SELECT id, path, title, artist, album, album_artist, genre,
                       track_number, disc_number, year, duration_ms, artwork_count,
                       size_bytes, modified_unix_seconds, integrated_lufs, true_peak_dbtp,
                       album_integrated_lufs, album_true_peak_dbtp, analysis_version,
                       file_hash, audio_hash,
                       view_id, primary_view_id, view_kind, transform_spec,
                       quality_profile, format_name, view_name, user_rating
                FROM tracks
                WHERE integrated_lufs IS NULL
                   OR true_peak_dbtp IS NULL
                   OR analysis_version IS NULL
                   OR analysis_version != ?1
                   OR (
                        size_bytes IS NOT NULL
                        AND COALESCE(analysis_size_bytes, -1) != size_bytes
                   )
                   OR (
                        modified_unix_seconds IS NOT NULL
                        AND COALESCE(analysis_modified_unix_seconds, -1) != modified_unix_seconds
                   )
                ORDER BY updated_at_unix_seconds ASC, path
                "#
            }
        };

        let mut stmt = self.conn.prepare(sql).map_err(to_store_error)?;
        let rows = if let Some(limit) = limit {
            stmt.query_map(
                params![
                    i64::from(analysis_version),
                    saturating_i64_from_u64(limit as u64)
                ],
                row_to_track,
            )
            .map_err(to_store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_store_error)?
        } else {
            stmt.query_map(params![i64::from(analysis_version)], row_to_track)
                .map_err(to_store_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(to_store_error)?
        };
        Ok(rows)
    }

    pub fn save_loudness(
        &mut self,
        path: impl AsRef<Path>,
        fingerprint: Option<FileFingerprint>,
        loudness: LoudnessInfo,
    ) -> PlayerResult<()> {
        self.save_loudness_with_duration(path, fingerprint, None, loudness)
    }

    pub fn save_loudness_with_duration(
        &mut self,
        path: impl AsRef<Path>,
        fingerprint: Option<FileFingerprint>,
        duration_ms: Option<u64>,
        loudness: LoudnessInfo,
    ) -> PlayerResult<()> {
        self.conn
            .execute(
                r#"
                UPDATE tracks
                SET integrated_lufs = ?2,
                    true_peak_dbtp = ?3,
                    album_integrated_lufs = ?4,
                    album_true_peak_dbtp = ?5,
                    analysis_version = ?6,
                    analyzed_at_unix_seconds = ?7,
                    size_bytes = COALESCE(?8, size_bytes),
                    modified_unix_seconds = COALESCE(?9, modified_unix_seconds),
                    analysis_size_bytes = COALESCE(?8, analysis_size_bytes),
                    analysis_modified_unix_seconds = COALESCE(?9, analysis_modified_unix_seconds),
                    duration_ms = COALESCE(?10, duration_ms),
                    updated_at_unix_seconds = ?7
                WHERE path = ?1
                "#,
                params![
                    path_to_string(path.as_ref()),
                    f64::from(loudness.integrated_lufs),
                    f64::from(loudness.true_peak_dbtp),
                    loudness.album_integrated_lufs.map(f64::from),
                    loudness.album_true_peak_dbtp.map(f64::from),
                    i64::from(loudness.analysis_version),
                    now_unix_seconds(),
                    fingerprint.map(|fingerprint| saturating_i64_from_u64(fingerprint.size_bytes)),
                    fingerprint.map(|fingerprint| fingerprint.modified_unix_seconds),
                    duration_ms.map(saturating_i64_from_u64),
                ],
            )
            .map_err(to_store_error)?;
        Ok(())
    }

    pub fn album_groups(&self) -> PlayerResult<Vec<AlbumGroup>> {
        use std::collections::BTreeMap;

        let mut groups: BTreeMap<String, AlbumGroup> = BTreeMap::new();
        for track in self.tracks()? {
            let Some(album) = clean_metadata_value(track.album.as_deref()) else {
                continue;
            };
            let album_artist = clean_metadata_value(track.album_artist.as_deref())
                .or_else(|| clean_metadata_value(track.artist.as_deref()));
            let key = album_group_key(album_artist.as_deref(), &album);

            groups
                .entry(key.clone())
                .or_insert_with(|| AlbumGroup {
                    album_key: key,
                    album_artist,
                    album,
                    tracks: Vec::new(),
                })
                .tracks
                .push(track);
        }

        let mut groups = groups.into_values().collect::<Vec<_>>();
        for group in &mut groups {
            group.tracks.sort_by(|left, right| {
                (
                    left.disc_number.unwrap_or(0),
                    left.track_number.unwrap_or(0),
                    left.title.to_lowercase(),
                    path_to_string(&left.path),
                )
                    .cmp(&(
                        right.disc_number.unwrap_or(0),
                        right.track_number.unwrap_or(0),
                        right.title.to_lowercase(),
                        path_to_string(&right.path),
                    ))
            });
        }

        Ok(groups)
    }

    pub fn save_album_loudness_for_paths(
        &mut self,
        paths: &[PathBuf],
        album_integrated_lufs: f32,
        album_true_peak_dbtp: f32,
        analysis_version: u32,
    ) -> PlayerResult<usize> {
        let tx = self.conn.transaction().map_err(to_store_error)?;
        let updated_at = now_unix_seconds();
        let mut updated = 0_usize;

        {
            let mut stmt = tx
                .prepare(
                    r#"
                    UPDATE tracks
                    SET album_integrated_lufs = ?2,
                        album_true_peak_dbtp = ?3,
                        analysis_version = ?4,
                        updated_at_unix_seconds = ?5
                    WHERE path = ?1
                    "#,
                )
                .map_err(to_store_error)?;

            for path in paths {
                updated += stmt
                    .execute(params![
                        path_to_string(path),
                        f64::from(album_integrated_lufs),
                        f64::from(album_true_peak_dbtp),
                        i64::from(analysis_version),
                        updated_at,
                    ])
                    .map_err(to_store_error)?;
            }
        }

        tx.commit().map_err(to_store_error)?;
        Ok(updated)
    }

    pub fn count_tracks(&self) -> PlayerResult<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM tracks", [], |row| row.get(0))
            .map_err(to_store_error)?;
        Ok(count.max(0) as usize)
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
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

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

    #[test]
    fn analyzed_rows_require_an_explicit_analysis_version() {
        let mut store = LibraryStore::in_memory().unwrap();
        let mut track = Track::from_path("/music/song.flac".into());
        track.loudness = Some(LoudnessInfo::track(-18.0, -2.0));
        store.upsert_track(&track).unwrap();
        store
            .conn
            .execute(
                "UPDATE tracks SET analysis_version = NULL WHERE path = ?1",
                params!["/music/song.flac"],
            )
            .unwrap();

        assert!(store.track_by_path("/music/song.flac").is_err());
    }

    #[test]
    fn stores_and_loads_track_metadata_and_loudness() {
        let mut store = LibraryStore::in_memory().unwrap();
        let mut track = Track::from_path("/music/song.flac".into());
        track.title = "Song".to_owned();
        track.artist = Some("Artist".to_owned());
        track.album = Some("Album".to_owned());
        track.album_artist = Some("Album Artist".to_owned());
        track.genre = Some("Rock".to_owned());
        track.track_number = Some(3);
        track.disc_number = Some(1);
        track.year = Some(2026);
        track.duration_ms = Some(12_345);
        track.artwork_count = 2;
        track.fingerprint = Some(FileFingerprint::new(99, 1));
        track.file_hash = Some("file-hash".to_owned());
        track.view_name = Some("Reference view".to_owned());
        track.user_rating = Some(9);
        track.set_primary_audio_hash("audio-hash");
        track.loudness = Some(LoudnessInfo {
            integrated_lufs: -18.0,
            true_peak_dbtp: -2.0,
            album_integrated_lufs: Some(-17.0),
            album_true_peak_dbtp: Some(-1.5),
            analysis_version: 7,
        });

        store.upsert_track(&track).unwrap();
        let loaded = store.track_by_path("/music/song.flac").unwrap().unwrap();

        assert_eq!(loaded.title, "Song");
        assert_eq!(loaded.artist.as_deref(), Some("Artist"));
        assert_eq!(loaded.album_artist.as_deref(), Some("Album Artist"));
        assert_eq!(loaded.track_number, Some(3));
        assert_eq!(loaded.duration_ms, Some(12_345));
        assert_eq!(loaded.artwork_count, 2);
        assert_eq!(loaded.file_hash.as_deref(), Some("file-hash"));
        assert_eq!(loaded.audio_hash.as_deref(), Some("audio-hash"));
        assert_eq!(loaded.view_id.value(), "audio:audio-hash");
        assert_eq!(loaded.primary_view_id.value(), "audio:audio-hash");
        assert_eq!(loaded.view_kind, TrackViewKind::Primary);
        assert_eq!(loaded.format_name.as_deref(), Some("flac"));
        assert_eq!(loaded.view_name.as_deref(), Some("Reference view"));
        assert_eq!(loaded.user_rating, Some(9));
        assert!(loaded.transform_spec.is_none());
        assert!(loaded.quality_profile.is_none());
        assert_eq!(loaded.loudness.unwrap().analysis_version, 7);
        let metadata = store.track_metadata(&track.path).unwrap().unwrap();
        assert_eq!(metadata.view_id, "audio:audio-hash");
        assert_eq!(metadata.primary_view_id, "audio:audio-hash");
        assert_eq!(metadata.view_kind, "primary");
        assert_eq!(metadata.format_name.as_deref(), Some("flac"));
        assert_eq!(metadata.view_name.as_deref(), Some("Reference view"));
        assert_eq!(metadata.user_rating, Some(9));
        assert_eq!(
            store.track_by_file_hash("file-hash").unwrap().unwrap().path,
            track.path
        );
        assert_eq!(
            store
                .track_by_audio_hash("audio-hash")
                .unwrap()
                .unwrap()
                .path,
            track.path
        );
    }

    #[test]
    fn replaces_track_paths_and_zeroes_out_everything() {
        let mut store = LibraryStore::in_memory().unwrap();
        let old_path = PathBuf::from("/old-library/song.ogg");
        let new_path = PathBuf::from("/new-library/song.ogg");
        let mut track = Track::from_path(old_path.clone());
        track.title = "Portable Song".to_owned();
        track.artist = Some("Portable Artist".to_owned());
        track.album = Some("Portable Album".to_owned());
        track.set_primary_audio_hash("portable-audio");
        store.upsert_track(&track).unwrap();

        let image = ArtworkImage {
            picture_index: 0,
            mime_type: Some("image/png".to_owned()),
            picture_type: "CoverFront".to_owned(),
            description: Some("portable".to_owned()),
            data: vec![4, 5, 6],
        };
        store.create_playlist("Portable List").unwrap();
        store
            .add_playlist_track("Portable List", &old_path)
            .unwrap();
        store.set_favorite(&old_path, true).unwrap();
        store.record_playback(&old_path, 321, true).unwrap();
        store.set_track_notes(&old_path, "portable note").unwrap();
        store
            .save_artwork(&old_path, std::slice::from_ref(&image))
            .unwrap();
        store
            .set_track_artwork_reference(&old_path, &image)
            .unwrap();
        store
            .set_album_artwork_reference_for_track(&old_path, &image)
            .unwrap();
        store
            .save_playlist_artwork("Portable List", &image)
            .unwrap();

        store
            .replace_track_paths(&[(old_path.clone(), new_path.clone())])
            .unwrap();

        assert!(store.track_by_path(&old_path).unwrap().is_none());
        assert_eq!(
            store.track_by_path(&new_path).unwrap().unwrap().title,
            "Portable Song"
        );
        assert_eq!(
            store.playlist_tracks("Portable List").unwrap()[0]
                .track
                .path,
            new_path
        );
        assert_eq!(store.favorite_tracks().unwrap()[0].path, new_path);
        assert_eq!(store.play_history(10).unwrap()[0].track.path, new_path);
        assert_eq!(
            store.track_notes(&new_path).unwrap().as_deref(),
            Some("portable note")
        );
        assert_eq!(
            store.artwork_for_path(&new_path).unwrap()[0].data,
            image.data
        );
        assert!(store.track_artwork_reference(&new_path).unwrap().is_some());
        assert!(store.album_artwork_reference(&new_path).unwrap().is_some());
        assert!(store.playlist_artwork("Portable List").unwrap().is_some());

        store.zero_out().unwrap();

        assert!(store.tracks().unwrap().is_empty());
        assert!(store.playlists().unwrap().is_empty());
        assert!(store.favorite_tracks().unwrap().is_empty());
        assert!(store.play_history(10).unwrap().is_empty());
        assert!(store.artwork_summaries().unwrap().is_empty());
    }

    #[test]
    fn finds_pending_analysis_and_updates_cache() {
        let mut store = LibraryStore::in_memory().unwrap();
        let mut track = Track::from_path("/music/pending.ogg".into());
        track.fingerprint = Some(FileFingerprint::new(10, 1));
        store.upsert_track(&track).unwrap();

        assert_eq!(store.pending_analysis(1, None).unwrap().len(), 1);
        store
            .save_loudness(
                &track.path,
                track.fingerprint,
                LoudnessInfo::track(-12.0, -1.0),
            )
            .unwrap();

        assert_eq!(store.pending_analysis(1, None).unwrap().len(), 0);
        assert_eq!(store.pending_analysis(2, None).unwrap().len(), 1);
    }

    #[test]
    fn upsert_preserves_existing_loudness_when_metadata_refresh_has_none() {
        let mut store = LibraryStore::in_memory().unwrap();
        let mut analyzed = Track::from_path("/music/song.ogg".into());
        analyzed.fingerprint = Some(FileFingerprint {
            size_bytes: 1,
            modified_unix_seconds: 10,
        });
        analyzed.loudness = Some(LoudnessInfo::track(-12.0, -1.0));
        store.upsert_track(&analyzed).unwrap();

        let mut metadata_refresh = Track::from_path("/music/song.ogg".into());
        metadata_refresh.title = "Fresh".to_owned();
        metadata_refresh.fingerprint = analyzed.fingerprint;
        store.set_track_rating(&analyzed.path, Some(8)).unwrap();
        store.upsert_track(&metadata_refresh).unwrap();

        let loaded = store.track_by_path("/music/song.ogg").unwrap().unwrap();
        assert_eq!(loaded.title, "Fresh");
        assert!(loaded.loudness.is_some());
        assert_eq!(loaded.user_rating, Some(8));
    }

    #[test]
    fn sets_clears_and_validates_track_rating() {
        let mut store = LibraryStore::in_memory().unwrap();
        let track = Track::from_path("/music/rated.ogg".into());
        store.upsert_track(&track).unwrap();

        assert_eq!(store.set_track_rating(&track.path, Some(10)).unwrap(), 1);
        assert_eq!(
            store
                .track_by_path(&track.path)
                .unwrap()
                .unwrap()
                .user_rating,
            Some(10)
        );
        assert_eq!(
            store
                .track_metadata(&track.path)
                .unwrap()
                .unwrap()
                .user_rating,
            Some(10)
        );
        assert!(store.set_track_rating(&track.path, Some(0)).is_err());
        assert!(store.set_track_rating(&track.path, Some(11)).is_err());

        assert_eq!(store.set_track_rating(&track.path, None).unwrap(), 1);
        assert_eq!(
            store
                .track_by_path(&track.path)
                .unwrap()
                .unwrap()
                .user_rating,
            None
        );
    }

    #[test]
    fn fingerprint_change_marks_track_pending_again() {
        let mut store = LibraryStore::in_memory().unwrap();
        let mut analyzed = Track::from_path("/music/song.ogg".into());
        analyzed.fingerprint = Some(FileFingerprint {
            size_bytes: 1,
            modified_unix_seconds: 10,
        });
        analyzed.loudness = Some(LoudnessInfo::track(-12.0, -1.0));
        store.upsert_track(&analyzed).unwrap();
        assert_eq!(store.pending_analysis(1, None).unwrap().len(), 0);

        let mut changed = Track::from_path("/music/song.ogg".into());
        changed.fingerprint = Some(FileFingerprint {
            size_bytes: 2,
            modified_unix_seconds: 10,
        });
        store.upsert_track(&changed).unwrap();

        assert_eq!(store.pending_analysis(1, None).unwrap().len(), 1);
    }

    #[test]
    fn groups_album_tracks_and_saves_album_loudness() {
        let mut store = LibraryStore::in_memory().unwrap();
        let first = analyzed_album_track("/music/02.ogg", "Album", "Band", 2, -20.0);
        let second = analyzed_album_track("/music/01.ogg", "Album", "Band", 1, -10.0);
        store.upsert_tracks(&[first, second]).unwrap();

        let groups = store.album_groups().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].album, "Album");
        assert_eq!(groups[0].album_artist.as_deref(), Some("Band"));
        assert_eq!(groups[0].tracks[0].track_number, Some(1));

        let paths = groups[0]
            .tracks
            .iter()
            .map(|track| track.path.clone())
            .collect::<Vec<_>>();
        let updated = store
            .save_album_loudness_for_paths(&paths, -12.0, -0.5, 9)
            .unwrap();
        assert_eq!(updated, 2);

        let loaded = store.track_by_path("/music/01.ogg").unwrap().unwrap();
        let loudness = loaded.loudness.unwrap();
        assert_eq!(loudness.integrated_lufs, -10.0);
        assert_eq!(loudness.album_integrated_lufs, Some(-12.0));
        assert_eq!(loudness.album_true_peak_dbtp, Some(-0.5));
        assert_eq!(loudness.analysis_version, 9);
    }

    #[test]
    fn searches_across_core_track_fields() {
        let mut store = LibraryStore::in_memory().unwrap();
        let mut ocean = Track::from_path("/music/ocean.ogg".into());
        ocean.title = "Ocean Chorus".to_owned();
        ocean.artist = Some("Sea Band".to_owned());
        ocean.album = Some("Blue Album".to_owned());
        let mut mountain = Track::from_path("/music/mountain.ogg".into());
        mountain.title = "Mountain Theme".to_owned();
        store.upsert_tracks(&[ocean, mountain]).unwrap();

        let results = store.search_tracks("sea", 10).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Ocean Chorus");
    }

    #[test]
    fn search_treats_like_metacharacters_as_literals() {
        let mut store = LibraryStore::in_memory().unwrap();
        let mut literal = Track::from_path("/music/literal.ogg".into());
        literal.title = "100%_Literal".to_owned();
        let mut wildcard_decoy = Track::from_path("/music/decoy.ogg".into());
        wildcard_decoy.title = "100xxLiteral".to_owned();
        store.upsert_tracks(&[literal, wildcard_decoy]).unwrap();

        let percent_results = store.search_tracks("100%", 10).unwrap();
        let underscore_results = store.search_tracks("%_", 10).unwrap();

        assert_eq!(percent_results.len(), 1);
        assert_eq!(percent_results[0].title, "100%_Literal");
        assert_eq!(underscore_results.len(), 1);
        assert_eq!(underscore_results[0].title, "100%_Literal");
    }

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
        let item_a = store.add_playlist_track("Road", "/music/a.ogg").unwrap();
        let item_b = store.add_playlist_track("Road", "/music/b.ogg").unwrap();
        store.add_playlist_track("Road", "/music/c.ogg").unwrap();
        let summaries = store.playlists().unwrap();
        let entries = store.playlist_tracks("Road").unwrap();

        assert!(playlist_id > 0);
        assert!(item_b > item_a);
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
    fn stores_track_notes_and_merges_duplicate_track_references() {
        let mut store = LibraryStore::in_memory().unwrap();
        let canonical = Track::from_path("/music/a.ogg".into());
        let duplicate = Track::from_path("/music/b.ogg".into());
        store
            .upsert_tracks(&[canonical.clone(), duplicate.clone()])
            .unwrap();
        store.create_playlist("Mix").unwrap();
        store.add_playlist_track("Mix", &duplicate.path).unwrap();
        store.set_favorite(&duplicate.path, true).unwrap();
        store.record_playback(&duplicate.path, 99, true).unwrap();
        store
            .set_track_notes(&canonical.path, "canonical note")
            .unwrap();
        store
            .set_track_notes(&duplicate.path, "duplicate note")
            .unwrap();
        store.set_track_rating(&duplicate.path, Some(7)).unwrap();
        store
            .set_track_artwork_reference(&duplicate.path, &artwork_image(0, vec![4, 5, 6]))
            .unwrap();
        store
            .save_artwork(&duplicate.path, &[artwork_image(0, vec![7, 8, 9])])
            .unwrap();

        assert!(store
            .merge_duplicate_track(&canonical.path, &duplicate.path)
            .unwrap());

        assert!(store.track_by_path(&duplicate.path).unwrap().is_none());
        assert_eq!(
            store.playlist_tracks("Mix").unwrap()[0].track.path,
            canonical.path
        );
        assert_eq!(store.favorite_tracks().unwrap()[0].path, canonical.path);
        assert_eq!(
            store.play_history(10).unwrap()[0].track.path,
            canonical.path
        );
        assert!(store
            .track_notes(&canonical.path)
            .unwrap()
            .unwrap()
            .contains("duplicate note"));
        assert_eq!(
            store.artwork_for_path(&canonical.path).unwrap()[0].data,
            vec![7, 8, 9]
        );
        assert_eq!(
            store
                .track_by_path(&canonical.path)
                .unwrap()
                .unwrap()
                .user_rating,
            Some(7)
        );
        assert_eq!(
            store
                .track_artwork_reference(&canonical.path)
                .unwrap()
                .unwrap()
                .image
                .data,
            vec![4, 5, 6]
        );
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

        store.set_favorite("/music/song.ogg", false).unwrap();
        assert_eq!(store.favorite_tracks().unwrap().len(), 0);
    }

    #[test]
    fn saves_and_reads_artwork_cache() {
        let mut store = LibraryStore::in_memory().unwrap();
        let track = Track::from_path("/music/song.ogg".into());
        store.upsert_track(&track).unwrap();
        let images = vec![ArtworkImage {
            picture_index: 0,
            mime_type: Some("image/png".to_owned()),
            picture_type: "CoverFront".to_owned(),
            description: Some("front".to_owned()),
            data: vec![1, 2, 3, 4],
        }];

        let saved = store.save_artwork("/music/song.ogg", &images).unwrap();
        let loaded = store.artwork_for_path("/music/song.ogg").unwrap();
        let summaries = store.artwork_summaries().unwrap();
        let track = store.track_by_path("/music/song.ogg").unwrap().unwrap();

        assert_eq!(saved, 1);
        assert_eq!(loaded, images);
        assert_eq!(summaries[0].image_count, 1);
        assert_eq!(summaries[0].byte_count, 4);
        assert_eq!(track.artwork_count, 1);
    }

    #[test]
    fn resolves_track_artwork_before_album_artwork_with_deduped_assets() {
        let mut store = LibraryStore::in_memory().unwrap();
        let mut first = Track::from_path("/music/album/01.ogg".into());
        first.title = "First".to_owned();
        first.album = Some("Shared Album".to_owned());
        first.album_artist = Some("Band".to_owned());
        let mut second = Track::from_path("/music/album/02.ogg".into());
        second.title = "Second".to_owned();
        second.album = Some("Shared Album".to_owned());
        second.artist = Some("Band".to_owned());
        let mut other_artist = Track::from_path("/music/other/01.ogg".into());
        other_artist.title = "Other".to_owned();
        other_artist.album = Some("Shared Album".to_owned());
        other_artist.artist = Some("Other Band".to_owned());
        store
            .upsert_tracks(&[first.clone(), second.clone(), other_artist.clone()])
            .unwrap();
        let album_artwork = artwork_image(0, vec![1, 2, 3]);
        let track_artwork = artwork_image(0, vec![4, 5, 6]);

        assert_eq!(
            store
                .set_album_artwork_reference_for_track(&first.path, &album_artwork)
                .unwrap(),
            2
        );
        let first_reference = store
            .effective_artwork_reference(&first.path)
            .unwrap()
            .unwrap();
        assert_eq!(first_reference.scope, ArtworkReferenceScope::Album);
        assert_eq!(first_reference.image.data, album_artwork.data);
        assert_eq!(
            store
                .effective_artwork_reference(&second.path)
                .unwrap()
                .unwrap()
                .scope,
            ArtworkReferenceScope::Album
        );
        assert_eq!(artwork_asset_count(&store), 1);
        assert!(store
            .effective_artwork_reference(&other_artist.path)
            .unwrap()
            .is_none());
        assert!(store.artwork_for_path(&first.path).unwrap().is_empty());

        assert_eq!(
            store
                .set_track_artwork_reference(&second.path, &track_artwork)
                .unwrap(),
            1
        );
        let second_reference = store
            .effective_artwork_reference(&second.path)
            .unwrap()
            .unwrap();
        assert_eq!(second_reference.scope, ArtworkReferenceScope::Track);
        assert_eq!(second_reference.image.data, track_artwork.data);
        assert_eq!(artwork_asset_count(&store), 2);

        let materialized = Track::from_path("/music/materialized/second.ogg".into());
        store.upsert_track(&materialized).unwrap();
        store
            .copy_artwork_references(&second.path, &materialized.path)
            .unwrap();
        assert_eq!(
            store
                .track_artwork_reference(&materialized.path)
                .unwrap()
                .unwrap()
                .image
                .data,
            vec![4, 5, 6]
        );
        assert_eq!(
            store
                .album_artwork_reference(&materialized.path)
                .unwrap()
                .unwrap()
                .image
                .data,
            vec![1, 2, 3]
        );
        assert_eq!(artwork_asset_count(&store), 2);
    }

    #[test]
    fn artwork_save_replaces_previous_images_and_can_clear_cache() {
        let mut store = LibraryStore::in_memory().unwrap();
        let track = Track::from_path("/music/song.ogg".into());
        store.upsert_track(&track).unwrap();
        store
            .save_artwork(
                "/music/song.ogg",
                &[
                    artwork_image(0, vec![1, 2, 3]),
                    artwork_image(1, vec![4, 5]),
                ],
            )
            .unwrap();

        store
            .save_artwork("/music/song.ogg", &[artwork_image(0, vec![9])])
            .unwrap();
        let loaded = store.artwork_for_path("/music/song.ogg").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].data, vec![9]);
        assert_eq!(
            store
                .track_by_path("/music/song.ogg")
                .unwrap()
                .unwrap()
                .artwork_count,
            1
        );

        store.save_artwork("/music/song.ogg", &[]).unwrap();
        assert!(store
            .artwork_for_path("/music/song.ogg")
            .unwrap()
            .is_empty());
        assert!(store.artwork_summaries().unwrap().is_empty());
        assert_eq!(
            store
                .track_by_path("/music/song.ogg")
                .unwrap()
                .unwrap()
                .artwork_count,
            0
        );
    }

    #[test]
    fn deterministic_random_user_workflow_preserves_store_invariants() {
        let mut store = LibraryStore::in_memory().unwrap();
        let tracks = (0..12)
            .map(|index| {
                let mut track = Track::from_path(format!("/music/random/{index:02}.ogg").into());
                track.title = format!("Song {index:02}");
                track.artist = Some(format!("Artist {}", index % 3));
                track.album = Some(format!("Album {}", index % 2));
                track.duration_ms = Some(30_000 + index as u64);
                track
            })
            .collect::<Vec<_>>();
        store.upsert_tracks(&tracks).unwrap();

        let paths = tracks
            .iter()
            .map(|track| track.path.clone())
            .collect::<Vec<_>>();
        let playlist_names = ["Mix", "Commute", "Late"];
        let mut rng = TestRng::new(0xC0FFEE);
        let mut expected_favorites = BTreeSet::new();
        let mut expected_playlist_counts = BTreeMap::<String, usize>::new();
        let mut expected_history_count = 0_usize;
        let mut expected_artwork = BTreeMap::<PathBuf, (usize, usize)>::new();
        let mut expected_ratings = BTreeMap::<PathBuf, Option<u8>>::new();

        for step in 0..250 {
            let path = paths[rng.usize(paths.len())].clone();
            match rng.usize(7) {
                0 => {
                    let query = format!("artist {}", rng.usize(3));
                    let results = store.search_tracks(&query, 50).unwrap();
                    assert!(!results.is_empty(), "query={query}");
                    assert!(results.iter().all(|track| {
                        track
                            .artist
                            .as_deref()
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(&query)
                    }));
                }
                1 => {
                    let name = playlist_names[rng.usize(playlist_names.len())].to_owned();
                    store.add_playlist_track(&name, &path).unwrap();
                    *expected_playlist_counts.entry(name).or_default() += 1;
                }
                2 => {
                    let enabled = rng.bool();
                    store.set_favorite(&path, enabled).unwrap();
                    if enabled {
                        expected_favorites.insert(path);
                    } else {
                        expected_favorites.remove(&path);
                    }
                }
                3 => {
                    let position_ms = rng.usize(240_000) as u64;
                    let completed = rng.bool();
                    store
                        .record_playback(&path, position_ms, completed)
                        .unwrap();
                    expected_history_count += 1;
                }
                4 => {
                    let image_count = rng.usize(3);
                    let images = (0..image_count)
                        .map(|index| {
                            let byte_count = 1 + rng.usize(8);
                            artwork_image(index as u32, vec![index as u8; byte_count])
                        })
                        .collect::<Vec<_>>();
                    let byte_count = images.iter().map(|image| image.data.len()).sum();
                    store.save_artwork(&path, &images).unwrap();
                    if image_count == 0 {
                        expected_artwork.remove(&path);
                    } else {
                        expected_artwork.insert(path, (image_count, byte_count));
                    }
                }
                5 => {
                    let rating = if rng.bool() {
                        Some((1 + rng.usize(10)) as u8)
                    } else {
                        None
                    };
                    store.set_track_rating(&path, rating).unwrap();
                    expected_ratings.insert(path, rating);
                }
                _ => {
                    let loaded = store.track_by_path(&path).unwrap().unwrap();
                    assert_eq!(loaded.path, path);
                    assert!(loaded.title.starts_with("Song "));
                }
            }

            assert_eq!(store.count_tracks().unwrap(), tracks.len(), "step={step}");
            assert_eq!(
                paths
                    .iter()
                    .map(|path| store.track_by_path(path).unwrap().is_some())
                    .filter(|exists| *exists)
                    .count(),
                tracks.len(),
                "step={step}"
            );
            assert_eq!(
                store.favorite_tracks().unwrap().len(),
                expected_favorites.len(),
                "step={step}"
            );

            let actual_playlist_counts = store
                .playlists()
                .unwrap()
                .into_iter()
                .map(|summary| (summary.name, summary.track_count))
                .collect::<BTreeMap<_, _>>();
            assert_eq!(
                actual_playlist_counts, expected_playlist_counts,
                "step={step}"
            );

            assert_eq!(
                store.play_history(1_000).unwrap().len(),
                expected_history_count,
                "step={step}"
            );

            let actual_artwork = store
                .artwork_summaries()
                .unwrap()
                .into_iter()
                .map(|summary| (summary.path, (summary.image_count, summary.byte_count)))
                .collect::<BTreeMap<_, _>>();
            assert_eq!(actual_artwork, expected_artwork, "step={step}");

            for (path, expected_rating) in &expected_ratings {
                assert_eq!(
                    store.track_by_path(path).unwrap().unwrap().user_rating,
                    *expected_rating,
                    "step={step}"
                );
            }
        }
    }

    fn analyzed_album_track(
        path: &str,
        album: &str,
        album_artist: &str,
        track_number: u32,
        integrated_lufs: f32,
    ) -> Track {
        let mut track = Track::from_path(path.into());
        track.album = Some(album.to_owned());
        track.album_artist = Some(album_artist.to_owned());
        track.track_number = Some(track_number);
        track.duration_ms = Some(60_000);
        track.loudness = Some(LoudnessInfo::track(integrated_lufs, -1.0));
        track
    }

    fn assert_playlist_paths(store: &LibraryStore, playlist: &str, expected: &[&PathBuf]) {
        let actual = store
            .playlist_tracks(playlist)
            .unwrap()
            .into_iter()
            .map(|entry| entry.track.path)
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            expected
                .iter()
                .map(|path| (*path).clone())
                .collect::<Vec<_>>()
        );
    }

    fn artwork_image(picture_index: u32, data: Vec<u8>) -> ArtworkImage {
        ArtworkImage {
            picture_index,
            mime_type: Some("image/png".to_owned()),
            picture_type: "CoverFront".to_owned(),
            description: None,
            data,
        }
    }

    fn artwork_asset_count(store: &LibraryStore) -> i64 {
        store
            .conn
            .query_row("SELECT COUNT(*) FROM artwork_assets", [], |row| row.get(0))
            .unwrap()
    }

    struct TestRng(u64);

    impl TestRng {
        fn new(seed: u64) -> Self {
            Self(seed)
        }

        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            self.0
        }

        fn usize(&mut self, upper: usize) -> usize {
            assert!(upper > 0);
            (self.next() as usize) % upper
        }

        fn bool(&mut self) -> bool {
            self.usize(2) == 0
        }
    }
}
