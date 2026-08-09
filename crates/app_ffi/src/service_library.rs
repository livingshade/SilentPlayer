use std::path::{Path, PathBuf};

use analysis_ebur128::{
    analyze_album_loudness, analyze_pending, AlbumAnalysisOptions, BatchAnalysisOptions,
};
use errors::PlayerResult;
use library_service::{
    import_files as import_files_service, import_folder as import_folder_service,
};
use serde::Serialize;

use crate::dto::{
    import_summary_dto, track_dtos_with_artwork, AnalysisSummary, LibraryPageDto, UserDataDto,
};
use crate::support::path_to_string_lossy;
use crate::PlayerApp;

impl PlayerApp {
    pub(crate) fn service_export_library(
        &mut self,
        package_path: &Path,
    ) -> PlayerResult<impl Serialize> {
        self.poll_events();
        self.persist_queue_state()?;
        self.export_library(package_path)
    }

    pub(crate) fn service_import_library(
        &mut self,
        package_path: &Path,
    ) -> PlayerResult<impl Serialize> {
        self.import_library(package_path)
    }

    pub(crate) fn service_zero_out_library(&mut self) -> PlayerResult<impl Serialize> {
        self.zero_out_library()
    }

    pub(crate) fn service_delete_from_library(
        &mut self,
        path: &Path,
    ) -> PlayerResult<impl Serialize> {
        self.delete_from_library(path)
    }

    pub(crate) fn service_import_folder(&mut self, folder: &Path) -> PlayerResult<impl Serialize> {
        let summary = import_folder_service(&self.db_path, &self.media_root, folder, |_| Ok(()))?;
        Ok(import_summary_dto(summary))
    }

    pub(crate) fn service_import_files(
        &mut self,
        paths: &[PathBuf],
    ) -> PlayerResult<impl Serialize> {
        let summary = import_files_service(&self.db_path, &self.media_root, paths, |_| Ok(()))?;
        Ok(import_summary_dto(summary))
    }

    pub(crate) fn service_library(&mut self) -> PlayerResult<impl Serialize> {
        let store = self.store()?;
        let tracks = store.tracks()?;
        track_dtos_with_artwork(&tracks, &store, &self.db_path)
    }

    pub(crate) fn service_library_page(
        &mut self,
        offset: usize,
        limit: usize,
    ) -> PlayerResult<impl Serialize> {
        let store = self.store()?;
        let (total, tracks) = store.tracks_page(offset, limit)?;
        Ok(LibraryPageDto {
            total,
            offset,
            tracks: track_dtos_with_artwork(&tracks, &store, &self.db_path)?,
        })
    }

    pub(crate) fn service_search(
        &mut self,
        query: &str,
        limit: usize,
    ) -> PlayerResult<impl Serialize> {
        let store = self.store()?;
        let tracks = store.search_tracks(query, limit)?;
        track_dtos_with_artwork(&tracks, &store, &self.db_path)
    }

    pub(crate) fn service_search_playlist(
        &mut self,
        name: &str,
        query: &str,
        limit: usize,
    ) -> PlayerResult<impl Serialize> {
        let store = self.store()?;
        let tracks = store
            .search_playlist_tracks(name, query, limit)?
            .into_iter()
            .map(|entry| entry.track)
            .collect::<Vec<_>>();
        track_dtos_with_artwork(&tracks, &store, &self.db_path)
    }

    pub(crate) fn service_analyze(&mut self) -> PlayerResult<impl Serialize> {
        let mut store = self.store()?;
        let track_summary = analyze_pending(&mut store, BatchAnalysisOptions::default())?;
        let album_summary = analyze_album_loudness(&mut store, AlbumAnalysisOptions::default())?;
        Ok(AnalysisSummary {
            tracks_analyzed: track_summary.analyzed,
            track_failures: track_summary.failed,
            albums_analyzed: album_summary.albums_analyzed,
            album_tracks_updated: album_summary.tracks_updated,
            album_skipped: album_summary.skipped,
        })
    }

    pub(crate) fn service_audit_database(&mut self) -> PlayerResult<impl Serialize> {
        self.audit_database()
    }

    pub(crate) fn service_user_data(&mut self) -> PlayerResult<impl Serialize> {
        let profile = self.local_user()?.clone();
        Ok(UserDataDto {
            user_id: profile.user_id.clone(),
            display_name: profile.display_name.clone(),
            sync_enabled: profile.sync_enabled,
            profile_path: path_to_string_lossy(&self.activity_store.profile_path),
            history_path: path_to_string_lossy(&self.activity_store.history_path),
            created_at_unix_seconds: profile.created_at_unix_seconds,
        })
    }
}
