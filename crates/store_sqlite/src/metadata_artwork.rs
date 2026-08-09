use std::path::{Path, PathBuf};

use domain::{ArtworkImage, FileFingerprint, TrackViewId};
use errors::{PlayerError, PlayerResult};
use rusqlite::{params, OptionalExtension};

use crate::{
    clean_metadata_value, clean_required_name, merge_artwork, merge_artwork_references,
    merge_notes, now_unix_seconds, optional_rating, optional_track_album_key, optional_usize,
    path_to_string, rating_to_sql, required_track_album_key, row_to_artwork,
    saturating_i64_from_u64, to_store_error, upsert_artwork_asset_tx, ArtworkReference,
    ArtworkReferenceScope, ArtworkSummary, LibraryStore, TrackMetadataView,
};

impl LibraryStore {
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
}
