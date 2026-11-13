use std::path::Path;

use errors::PlayerResult;
use serde::Serialize;
use store_sqlite::PlaylistSort;

use crate::dto::{track_dtos_with_artwork, Empty};
use crate::file_support::read_artwork_image;
use crate::support::path_to_string_lossy;
use crate::PlayerApp;

#[derive(Serialize)]
struct PlaylistAddSummary {
    added: bool,
}

#[derive(Default, Serialize)]
struct DeletePlaylistTracksSummary {
    tracks_deleted: usize,
    deleted_paths: Vec<String>,
    managed_files_deleted: usize,
    errors: Vec<String>,
}

impl PlayerApp {
    pub(crate) fn service_delete_playlist_tracks_from_library(
        &mut self,
        name: &str,
    ) -> PlayerResult<impl Serialize> {
        // Snapshot the complete playlist before deletion changes its membership.
        let entries = self.store()?.playlist_tracks(name)?;
        let mut summary = DeletePlaylistTracksSummary::default();
        for entry in entries {
            match self.delete_from_library(&entry.track.path) {
                Ok(deleted) => {
                    summary.tracks_deleted += 1;
                    summary
                        .deleted_paths
                        .push(path_to_string_lossy(&entry.track.path));
                    summary.managed_files_deleted += deleted.managed_files_deleted;
                    if let Some(error) = deleted.cleanup_error {
                        summary.errors.push(error);
                    }
                }
                Err(error) => summary
                    .errors
                    .push(format!("{}: {error}", entry.track.path.display())),
            }
        }
        Ok(summary)
    }

    pub(crate) fn service_playlists(&mut self) -> PlayerResult<impl Serialize> {
        self.service_search_playlists("")
    }

    pub(crate) fn service_search_playlists(&mut self, query: &str) -> PlayerResult<impl Serialize> {
        let query = query.trim().to_lowercase();
        let store = self.store()?;
        store
            .playlists()?
            .into_iter()
            .filter(|playlist| query.is_empty() || playlist.name.to_lowercase().contains(&query))
            .map(|playlist| self.playlist_to_dto(&store, playlist))
            .collect::<PlayerResult<Vec<_>>>()
    }

    pub(crate) fn service_recent_playlists(
        &mut self,
        limit: usize,
    ) -> PlayerResult<impl Serialize> {
        let store = self.store()?;
        store
            .recent_playlists(limit)?
            .into_iter()
            .map(|playlist| self.playlist_to_dto(&store, playlist))
            .collect::<PlayerResult<Vec<_>>>()
    }

    pub(crate) fn service_create_playlist(&mut self, name: &str) -> PlayerResult<impl Serialize> {
        self.store()?.create_playlist(name)?;
        Ok(Empty {})
    }

    pub(crate) fn service_rename_playlist(
        &mut self,
        old_name: &str,
        new_name: &str,
    ) -> PlayerResult<impl Serialize> {
        self.store()?.rename_playlist(old_name, new_name)?;
        Ok(Empty {})
    }

    pub(crate) fn service_set_playlist_artwork(
        &mut self,
        name: &str,
        image_path: &Path,
    ) -> PlayerResult<impl Serialize> {
        let image = read_artwork_image(image_path)?;
        self.store()?.save_playlist_artwork(name, &image)?;
        Ok(Empty {})
    }

    pub(crate) fn service_delete_playlist(&mut self, name: &str) -> PlayerResult<impl Serialize> {
        self.store()?.delete_playlist(name)?;
        Ok(Empty {})
    }

    pub(crate) fn service_clear_playlist(&mut self, name: &str) -> PlayerResult<impl Serialize> {
        self.store()?.clear_playlist(name)?;
        Ok(Empty {})
    }

    pub(crate) fn service_add_to_playlist(
        &mut self,
        name: &str,
        path: &Path,
    ) -> PlayerResult<impl Serialize> {
        let added = self
            .store()?
            .add_playlist_track(name, path.to_string_lossy().into_owned())?;
        Ok(PlaylistAddSummary { added })
    }

    pub(crate) fn service_remove_from_playlist(
        &mut self,
        name: &str,
        path: &Path,
    ) -> PlayerResult<impl Serialize> {
        self.store()?
            .remove_playlist_track(name, path.to_string_lossy().into_owned())?;
        Ok(Empty {})
    }

    pub(crate) fn service_move_playlist_track(
        &mut self,
        name: &str,
        path: &Path,
        delta: i32,
    ) -> PlayerResult<impl Serialize> {
        self.store()?
            .move_playlist_track(name, path.to_string_lossy().into_owned(), delta)?;
        Ok(Empty {})
    }

    pub(crate) fn service_sort_playlist(
        &mut self,
        name: &str,
        sort: &str,
    ) -> PlayerResult<impl Serialize> {
        self.store()?
            .sort_playlist(name, PlaylistSort::parse(sort)?)?;
        Ok(Empty {})
    }

    pub(crate) fn service_playlist_tracks(&mut self, name: &str) -> PlayerResult<impl Serialize> {
        let mut store = self.store()?;
        let tracks = store
            .playlist_tracks(name)?
            .into_iter()
            .map(|entry| entry.track)
            .collect::<Vec<_>>();
        store.touch_playlist(name)?;
        track_dtos_with_artwork(&tracks, &store, &self.db_path)
    }
}
