use std::path::Path;

use domain::Track;
use errors::{PlayerError, PlayerResult};
use rusqlite::params;

use crate::{
    now_unix_seconds, optional_u64, path_to_string, row_to_track, row_to_track_at,
    saturating_i64_from_u64, to_store_error, LibraryStore, PlayHistoryEntry, PlaybackStats,
};

impl LibraryStore {
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
        if limit == 0 {
            return Err(PlayerError::invalid_input(
                "play history limit must be greater than zero",
            ));
        }
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
            .query_map(params![saturating_i64_from_u64(limit as u64)], |row| {
                Ok(PlayHistoryEntry {
                    id: row.get(0)?,
                    played_at_unix_seconds: row.get(1)?,
                    position_ms: optional_u64(Some(row.get::<_, i64>(2)?)).unwrap_or(0),
                    completed: row.get::<_, i64>(3)? != 0,
                    track: row_to_track_at(row, 4)?,
                })
            })
            .map_err(to_store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_store_error)?;
        Ok(rows)
    }

    pub fn playback_stats(&self, path: impl AsRef<Path>) -> PlayerResult<PlaybackStats> {
        self.conn
            .query_row(
                r#"
                SELECT COUNT(*),
                       COALESCE(SUM(CASE WHEN completed = 1 THEN 1 ELSE 0 END), 0),
                       MAX(played_at_unix_seconds),
                       MAX(CASE WHEN completed = 1 THEN played_at_unix_seconds END)
                FROM play_history
                WHERE track_path = ?1
                "#,
                params![path_to_string(path.as_ref())],
                |row| {
                    let session_count = row.get::<_, i64>(0)?;
                    let play_count = row.get::<_, i64>(1)?;
                    Ok(PlaybackStats {
                        play_count: optional_u64(Some(play_count)).unwrap_or(0),
                        session_count: optional_u64(Some(session_count)).unwrap_or(0),
                        last_played_at_unix_seconds: row.get(2)?,
                        last_completed_at_unix_seconds: row.get(3)?,
                    })
                },
            )
            .map_err(to_store_error)
    }
}
