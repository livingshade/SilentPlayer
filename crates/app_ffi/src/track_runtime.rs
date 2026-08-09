use std::fs;
use std::path::Path;

use domain::{Track, TrackId, TrackViewId, TrackViewKind};
use errors::{PlayerError, PlayerResult};
use fingerprint::{audio_hash, file_hash};
use library_fs::fingerprint_from_metadata;
use library_service::{
    copy_into_media_library, copy_related_sidecars, load_track_lyrics, LyricsDocument,
};
use store_sqlite::{LibraryStore, PlaylistSummary};

use crate::dto::{required_audio_hash, PlaylistDto, TrackDetailsDto};
use crate::file_support::{
    materialized_primary_view_id, playlist_artwork_path, resolved_artwork_path,
};
use crate::support::{now_unix_nanos, path_to_string_lossy};
use crate::PlayerApp;

impl PlayerApp {
    pub(crate) fn primary_track_for_edit(&self, path: &Path) -> PlayerResult<Track> {
        let track = self
            .store()?
            .track_by_path(path)?
            .ok_or_else(|| PlayerError::store(format!("track not found: {}", path.display())))?;
        if track.view_kind != TrackViewKind::Primary || track.view_id != track.primary_view_id {
            return Err(PlayerError::store(format!(
                "track is not a primary view: {}",
                path.display()
            )));
        }
        Ok(track)
    }

    pub(crate) fn materialize_track_view(
        &self,
        path: &Path,
        destination: &Path,
    ) -> PlayerResult<Track> {
        let mut store = self.store()?;
        let source = store
            .track_by_path(path)?
            .ok_or_else(|| PlayerError::store(format!("track not found: {}", path.display())))?;
        if store.track_by_path(destination)?.is_some() {
            return Err(PlayerError::store(format!(
                "destination is already in the library: {}",
                destination.display()
            )));
        }

        copy_into_media_library(&source.path, destination)?;
        copy_related_sidecars(&source.path, destination)?;

        let audio_fingerprint = audio_hash(destination)?;
        let materialized_file_hash = file_hash(destination)?;
        let metadata =
            fs::metadata(destination).map_err(|source| PlayerError::io(destination, source))?;
        let fingerprint = Some(fingerprint_from_metadata(&metadata));
        let created_at = now_unix_nanos();
        let view_id = materialized_primary_view_id(&audio_fingerprint.hash, created_at);
        let mut materialized = source.clone();
        materialized.id = TrackId::from_path(destination);
        materialized.path = destination.to_path_buf();
        materialized.view_id = TrackViewId::from_value(view_id.clone());
        materialized.primary_view_id = TrackViewId::from_value(view_id);
        materialized.view_kind = TrackViewKind::Primary;
        materialized.transform_spec = None;
        materialized.format_name = destination
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase());
        materialized.file_hash = Some(materialized_file_hash);
        materialized.audio_hash = Some(audio_fingerprint.hash);
        materialized.fingerprint = fingerprint;

        store.upsert_track(&materialized)?;
        if let Some(notes) = store.track_notes(&source.path)? {
            store.set_track_notes(destination, &notes)?;
        }
        store.copy_artwork_references(&source.path, destination)?;
        let artwork = store.artwork_for_path(&source.path)?;
        if !artwork.is_empty() {
            store.save_artwork(destination, &artwork)?;
        }
        store.track_by_path(destination)?.ok_or_else(|| {
            PlayerError::store(format!(
                "materialized view not found after export: {}",
                destination.display()
            ))
        })
    }

    pub(crate) fn track_details(&self, path: &Path) -> PlayerResult<TrackDetailsDto> {
        let store = self.store()?;
        let artwork = resolved_artwork_path(&store, &self.db_path, path)?;
        let lyrics = load_track_lyrics(path)?;
        let lyrics_document = lyrics
            .as_ref()
            .map(|asset| asset.document.clone())
            .unwrap_or_else(LyricsDocument::instrumental);
        let notes = store.track_notes(path)?;
        let playback_stats = store.playback_stats(path)?;
        let metadata = store
            .track_metadata(path)?
            .ok_or_else(|| PlayerError::store(format!("track not found: {}", path.display())))?;
        let audio_hash = required_audio_hash(metadata.audio_hash, path)?;

        Ok(TrackDetailsDto {
            view_id: metadata.view_id.clone(),
            primary_view_id: metadata.primary_view_id.clone(),
            is_primary_view: metadata.view_id == metadata.primary_view_id,
            view_kind: metadata.view_kind,
            view_name: metadata.view_name,
            rating: metadata.user_rating,
            transform_spec: metadata.transform_spec,
            quality_profile: metadata.quality_profile,
            format_name: metadata.format_name,
            artwork_path: artwork.as_ref().map(|(path, _)| path_to_string_lossy(path)),
            artwork_source: artwork.map(|(_, source)| source.to_owned()),
            lyrics_path: lyrics
                .as_ref()
                .map(|asset| path_to_string_lossy(&asset.path)),
            lyrics_text: lyrics.as_ref().map(|asset| asset.raw_text.clone()),
            lyrics_document,
            notes,
            audio_hash,
            original_title: metadata.original_title,
            original_artist: metadata.original_artist,
            original_album: metadata.original_album,
            display_title: metadata.display_title,
            display_artist: metadata.display_artist,
            display_album: metadata.display_album,
            play_count: playback_stats.play_count,
            playback_session_count: playback_stats.session_count,
            last_played_at_unix_seconds: playback_stats.last_played_at_unix_seconds,
            last_completed_at_unix_seconds: playback_stats.last_completed_at_unix_seconds,
        })
    }

    pub(crate) fn playlist_to_dto(
        &self,
        store: &LibraryStore,
        playlist: PlaylistSummary,
    ) -> PlayerResult<PlaylistDto> {
        let (artwork_path, artwork_source) =
            playlist_artwork_path(store, &self.db_path, playlist.id, &playlist.name)?
                .map(|(path, source)| (Some(path_to_string_lossy(path)), Some(source.to_owned())))
                .unwrap_or((None, None));

        Ok(PlaylistDto {
            id: playlist.id,
            name: playlist.name,
            track_count: playlist.track_count,
            artwork_path,
            artwork_source,
            created_at_unix_seconds: playlist.created_at_unix_seconds,
            updated_at_unix_seconds: playlist.updated_at_unix_seconds,
            last_used_at_unix_seconds: playlist.last_used_at_unix_seconds,
        })
    }
}
