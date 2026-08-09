use std::path::{Path, PathBuf};

use errors::{PlayerError, PlayerResult};
use library_service::{
    load_lyrics_file, load_track_lyrics, remove_track_lyrics as remove_track_lyrics_service,
    set_track_lyrics as set_track_lyrics_service, LyricsContent, LyricsDocument,
};
use serde::Serialize;

use crate::dto::{
    track_dtos_with_artwork, track_view_id, AlbumArtworkSummary, Empty, TrackLyricsAtDto,
    TrackLyricsDto, TrackLyricsRemovalDto, TrackViewEditRequest,
};
use crate::file_support::read_artwork_image;
use crate::support::path_to_string_lossy;
use crate::PlayerApp;

impl PlayerApp {
    pub(crate) fn service_track_details(&mut self, path: &Path) -> PlayerResult<impl Serialize> {
        self.track_details(path)
    }

    pub(crate) fn service_track_lyrics(&mut self, path: &Path) -> PlayerResult<impl Serialize> {
        let track = self
            .store()?
            .track_by_path(path)?
            .ok_or_else(|| PlayerError::store(format!("track not found: {}", path.display())))?;
        let view_id = track_view_id(&track)?.to_owned();
        let lyrics = load_track_lyrics(&track.path)?;
        let lyrics_document = lyrics
            .as_ref()
            .map(|asset| asset.document.clone())
            .unwrap_or_else(LyricsDocument::instrumental);
        Ok(TrackLyricsDto {
            view_id,
            lyrics_path: lyrics
                .as_ref()
                .map(|asset| path_to_string_lossy(&asset.path)),
            lyrics_text: lyrics.as_ref().map(|asset| asset.raw_text.clone()),
            lyrics_document,
        })
    }

    pub(crate) fn service_track_lyrics_at(
        &mut self,
        path: &Path,
        position_ms: u64,
    ) -> PlayerResult<impl Serialize> {
        let track = self
            .store()?
            .track_by_path(path)?
            .ok_or_else(|| PlayerError::store(format!("track not found: {}", path.display())))?;
        let view_id = track_view_id(&track)?.to_owned();
        let lyrics = load_track_lyrics(&track.path)?;
        let Some(lyrics) = lyrics else {
            let document = LyricsDocument::instrumental();
            let display = document.display_at(position_ms);
            return Ok(TrackLyricsAtDto {
                view_id,
                position_ms,
                kind: "instrumental".to_owned(),
                line_index: None,
                line: None,
                previous_index: None,
                next_index: None,
                display_text: display.display_text().to_owned(),
                is_instrumental: display.is_instrumental(),
            });
        };
        let LyricsContent::Timed { lines } = &lyrics.document.content else {
            let display = lyrics.document.display_at(position_ms);
            let kind = match &lyrics.document.content {
                LyricsContent::Plain { .. } => "plain",
                LyricsContent::Instrumental => "instrumental",
                LyricsContent::Timed { .. } => unreachable!(),
            };
            return Ok(TrackLyricsAtDto {
                view_id,
                position_ms,
                kind: kind.to_owned(),
                line_index: None,
                line: None,
                previous_index: None,
                next_index: None,
                display_text: display.display_text().to_owned(),
                is_instrumental: display.is_instrumental(),
            });
        };
        let line_index = lyrics.document.active_line_index(position_ms);
        let previous_index = line_index.and_then(|index| index.checked_sub(1));
        let next_index = line_index
            .map(|index| index + 1)
            .filter(|index| *index < lines.len())
            .or_else(|| (!lines.is_empty() && line_index.is_none()).then_some(0));
        let line = line_index.and_then(|index| lines.get(index)).cloned();
        let display = lyrics.document.display_at(position_ms);
        Ok(TrackLyricsAtDto {
            view_id,
            position_ms,
            kind: "timed".to_owned(),
            line_index,
            line,
            previous_index,
            next_index,
            display_text: display.display_text().to_owned(),
            is_instrumental: display.is_instrumental(),
        })
    }

    pub(crate) fn service_edit_track_view(
        &mut self,
        path: &Path,
        request: TrackViewEditRequest,
    ) -> PlayerResult<impl Serialize> {
        if request.title.trim().is_empty() {
            return Err(PlayerError::metadata("track title cannot be empty"));
        }

        let artwork_image = request
            .artwork_path
            .as_deref()
            .map(|path| read_artwork_image(Path::new(path)))
            .transpose()?;
        let lyrics_path = request.lyrics_path.as_ref().map(PathBuf::from);
        if let Some(lyrics_path) = &lyrics_path {
            if !lyrics_path.is_file() {
                return Err(PlayerError::metadata(format!(
                    "lyrics file not found: {}",
                    lyrics_path.display()
                )));
            }
            load_lyrics_file(lyrics_path)?;
        }

        let primary = self.primary_track_for_edit(path)?;
        {
            let mut store = self.store()?;
            store.set_track_display_metadata(
                &primary.path,
                &request.title,
                request.artist.as_deref(),
                request.album.as_deref(),
            )?;
            if let Some(notes) = request.notes.as_deref() {
                store.set_track_notes(&primary.path, notes)?;
            }
            if let Some(artwork_image) = artwork_image.as_ref() {
                store.set_track_artwork_reference(&primary.path, artwork_image)?;
            }
        }
        if let Some(lyrics_path) = lyrics_path {
            set_track_lyrics_service(&primary.path, &lyrics_path)?;
        }

        let primary = self
            .store()?
            .track_by_path(&primary.path)?
            .ok_or_else(|| PlayerError::store("primary track edit disappeared"))?;
        let dto = self.track_to_dto_with_artwork(&primary)?;
        self.replace_cached_track(dto.clone());
        Ok(dto)
    }

    pub(crate) fn service_set_track_notes(
        &mut self,
        path: &Path,
        notes: &str,
    ) -> PlayerResult<impl Serialize> {
        let primary = self.primary_track_for_edit(path)?;
        self.store()?.set_track_notes(&primary.path, notes)?;
        let primary = self
            .store()?
            .track_by_path(&primary.path)?
            .ok_or_else(|| PlayerError::store("primary track notes disappeared"))?;
        let dto = self.track_to_dto_with_artwork(&primary)?;
        self.replace_cached_track(dto.clone());
        Ok(dto)
    }

    pub(crate) fn service_set_track_rating(
        &mut self,
        path: &Path,
        rating: i32,
    ) -> PlayerResult<impl Serialize> {
        let rating = match rating {
            0 => None,
            1..=10 => Some(rating as u8),
            _ => {
                return Err(PlayerError::store(
                    "rating must be 0 to clear or between 1 and 10",
                ));
            }
        };
        let updated = {
            let mut store = self.store()?;
            store.set_track_rating(path, rating)?;
            store
                .track_by_path(path)?
                .ok_or_else(|| PlayerError::store(format!("track not found: {}", path.display())))?
        };
        let dto = self.track_to_dto_with_artwork(&updated)?;
        self.replace_cached_track(dto.clone());
        Ok(dto)
    }

    pub(crate) fn service_set_track_metadata(
        &mut self,
        path: &Path,
        title: &str,
        artist: &str,
        album: &str,
    ) -> PlayerResult<impl Serialize> {
        let primary = self.primary_track_for_edit(path)?;
        self.store()?.set_track_display_metadata(
            &primary.path,
            title,
            Some(artist),
            Some(album),
        )?;
        let primary = self
            .store()?
            .track_by_path(&primary.path)?
            .ok_or_else(|| PlayerError::store("primary track metadata disappeared"))?;
        let dto = self.track_to_dto_with_artwork(&primary)?;
        self.replace_cached_track(dto.clone());
        Ok(dto)
    }

    pub(crate) fn service_set_track_artwork(
        &mut self,
        path: &Path,
        image_path: &Path,
    ) -> PlayerResult<impl Serialize> {
        let image = read_artwork_image(image_path)?;
        let primary = self.primary_track_for_edit(path)?;
        self.store()?
            .set_track_artwork_reference(&primary.path, &image)?;
        let primary = self
            .store()?
            .track_by_path(&primary.path)?
            .ok_or_else(|| PlayerError::store("primary track artwork disappeared"))?;
        let dto = self.track_to_dto_with_artwork(&primary)?;
        self.replace_cached_track(dto.clone());
        Ok(dto)
    }

    pub(crate) fn service_set_album_artwork(
        &mut self,
        path: &Path,
        image_path: &Path,
    ) -> PlayerResult<impl Serialize> {
        let image = read_artwork_image(image_path)?;
        let tracks_updated = self
            .store()?
            .set_album_artwork_reference_for_track(path, &image)?;
        Ok(AlbumArtworkSummary { tracks_updated })
    }

    pub(crate) fn service_set_track_lyrics(
        &mut self,
        path: &Path,
        lyrics_path: &Path,
    ) -> PlayerResult<impl Serialize> {
        let primary = self.primary_track_for_edit(path)?;
        set_track_lyrics_service(&primary.path, lyrics_path)?;
        let primary = self
            .store()?
            .track_by_path(&primary.path)?
            .ok_or_else(|| PlayerError::store("primary track lyrics disappeared"))?;
        let dto = self.track_to_dto_with_artwork(&primary)?;
        self.replace_cached_track(dto.clone());
        Ok(dto)
    }

    pub(crate) fn service_remove_track_lyrics(
        &mut self,
        path: &Path,
    ) -> PlayerResult<impl Serialize> {
        let primary = self.primary_track_for_edit(path)?;
        let removal = remove_track_lyrics_service(&primary.path)?;
        Ok(TrackLyricsRemovalDto {
            view_id: track_view_id(&primary)?.to_owned(),
            files_removed: removal.files_removed,
        })
    }

    pub(crate) fn service_export_track_view(
        &mut self,
        path: &Path,
        destination: &Path,
    ) -> PlayerResult<impl Serialize> {
        let track = self.materialize_track_view(path, destination)?;
        self.track_to_dto_with_artwork(&track)
    }

    pub(crate) fn service_set_favorite(
        &mut self,
        path: &Path,
        enabled: bool,
    ) -> PlayerResult<impl Serialize> {
        self.store()?
            .set_favorite(path.to_string_lossy().into_owned(), enabled)?;
        Ok(Empty {})
    }

    pub(crate) fn service_favorites(&mut self) -> PlayerResult<impl Serialize> {
        let store = self.store()?;
        let tracks = store.favorite_tracks()?;
        track_dtos_with_artwork(&tracks, &store, &self.db_path)
    }

    pub(crate) fn service_history(&mut self, limit: usize) -> PlayerResult<impl Serialize> {
        let store = self.store()?;
        let tracks = store
            .play_history(limit)?
            .into_iter()
            .map(|entry| entry.track)
            .collect::<Vec<_>>();
        track_dtos_with_artwork(&tracks, &store, &self.db_path)
    }
}
