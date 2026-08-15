use std::path::{Path, PathBuf};

use domain::Track;
use errors::{PlayerError, PlayerResult};
use rusqlite::{params, OptionalExtension};

use crate::{
    like_pattern, now_unix_seconds, path_to_string, row_to_track, saturating_i64_from_u64,
    to_store_error, LibraryStore,
};

impl LibraryStore {
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
        if limit == 0 {
            return Err(PlayerError::invalid_input(
                "track search limit must be greater than zero",
            ));
        }
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
                params![pattern, saturating_i64_from_u64(limit as u64)],
                row_to_track,
            )
            .map_err(to_store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_store_error)?;
        Ok(rows)
    }
}
