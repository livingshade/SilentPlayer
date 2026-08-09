use std::path::{Path, PathBuf};

use domain::{FileFingerprint, LoudnessInfo, Track};
use errors::PlayerResult;
use rusqlite::params;

use crate::{
    album_group_key, clean_metadata_value, now_unix_seconds, path_to_string, row_to_track,
    saturating_i64_from_u64, to_store_error, AlbumGroup, LibraryStore,
};

impl LibraryStore {
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
}
