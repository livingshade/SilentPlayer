use std::path::Path;

use errors::{PlayerError, PlayerResult};
use rusqlite::params;

use crate::{
    clean_required_name, like_pattern, now_unix_seconds, optional_u32, path_to_string,
    row_to_playlist_summary, row_to_track_at, saturating_i64_from_u64, sort_playlist_items,
    to_store_error, LibraryStore, PlaylistEntry, PlaylistSort, PlaylistSummary,
};

impl LibraryStore {
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
        if limit == 0 {
            return Err(PlayerError::invalid_input(
                "recent playlist limit must be greater than zero",
            ));
        }
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
                params![saturating_i64_from_u64(limit as u64)],
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
        if limit == 0 {
            return Err(PlayerError::invalid_input(
                "playlist search limit must be greater than zero",
            ));
        }
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
                params![playlist_id, pattern, saturating_i64_from_u64(limit as u64)],
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
}
