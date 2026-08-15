use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use domain::{
    GlobalQueueSnapshot, PlaybackLifecycle, PlaybackMode, QueueItemId, RepeatMode, TrackViewKind,
};
use errors::{PlayerError, PlayerResult};
use library_service::{copy_into_media_library, copy_related_sidecars};
use playback_store_sqlite::PlaybackStateStore;
use store_sqlite::LibraryStore;

use crate::dto::{
    track_dtos_with_artwork, DeleteTrackSummary, Empty, LibraryPackageManifest,
    LibraryPackageSummary, LibraryPackageTrack,
};
use crate::file_support::{
    delete_managed_track_files, library_package_audio_path, remove_library_storage,
    validated_package_file,
};
use crate::support::path_to_string_lossy;
use crate::{
    PlayerApp, LIBRARY_PACKAGE_DATABASE_FILE, LIBRARY_PACKAGE_FORMAT_VERSION,
    LIBRARY_PACKAGE_MANIFEST_FILE, LIBRARY_PACKAGE_MUSIC_DIRECTORY,
};

impl PlayerApp {
    pub(crate) fn restore_persisted_queue(&mut self) -> PlayerResult<()> {
        let store = self.store()?;
        let mut playback_store = PlaybackStateStore::open(&self.playback_state_path)?;
        playback_store.migrate_legacy_library(&self.db_path)?;
        let (tracks, queue_state, position_ms) = if let Some(restored) = playback_store.load()? {
            let library_tracks = store.tracks()?;
            let mut resolved = Vec::with_capacity(restored.items.len());
            for item in &restored.items {
                let track = library_tracks
                    .iter()
                    .find(|track| track.primary_view_id.value() == item.primary_view_id)
                    .or_else(|| library_tracks.iter().find(|track| track.path == item.path))
                    .cloned();
                if let Some(track) = track {
                    resolved.push((item.internal_id, track));
                }
            }
            if resolved.len() == restored.items.len() {
                (
                    resolved.into_iter().map(|(_, track)| track).collect(),
                    restored.queue,
                    restored.position_ms,
                )
            } else {
                let ordered_ids = resolved.iter().map(|(id, _)| *id).collect::<Vec<_>>();
                let queue_state = reconciled_queue_snapshot(
                    &restored.queue,
                    ordered_ids,
                    restored.queue.current_id,
                );
                (
                    resolved.into_iter().map(|(_, track)| track).collect(),
                    queue_state,
                    restored.position_ms,
                )
            }
        } else {
            (
                Vec::new(),
                legacy_queue_snapshot(0, None, RepeatMode::All, false),
                0,
            )
        };
        let queue_tracks = track_dtos_with_artwork(&tracks, &store, &self.db_path)?;
        let current_index = queue_state.current_id.and_then(|id| {
            queue_state
                .ordered_ids
                .iter()
                .position(|candidate| *candidate == id)
        });

        self.queue_tracks = queue_tracks;
        self.queue_item_ids = queue_state.ordered_ids.clone();
        self.queue_current_index = current_index;
        self.queue_state = Some(queue_state.clone());
        self.reset_queue_playback_order();
        self.current_track = current_index.and_then(|index| self.queue_tracks.get(index).cloned());
        self.position_ms = if current_index.is_some() {
            position_ms
        } else {
            0
        };
        self.last_persisted_queue_position_ms = self.position_ms;
        self.last_persisted_queue_index = current_index;
        self.playback_mode = queue_state.mode;
        self.repeat_mode = if queue_state.mode == PlaybackMode::RepeatOne {
            RepeatMode::One
        } else {
            RepeatMode::All
        };
        self.shuffle_enabled = queue_state.mode == PlaybackMode::Shuffle;
        self.is_playing = false;
        Ok(())
    }

    pub(crate) fn persist_queue_state(&mut self) -> PlayerResult<()> {
        let library = self.store()?;
        let tracks = self
            .queue_tracks
            .iter()
            .map(|queued| {
                library.track_by_path(&queued.path)?.ok_or_else(|| {
                    PlayerError::store(format!("queued track is not in library: {}", queued.path))
                })
            })
            .collect::<PlayerResult<Vec<_>>>()?;
        self.reconcile_cached_queue_structure();
        let queue_state = self.queue_state.clone().unwrap_or_else(|| {
            legacy_queue_snapshot(
                tracks.len(),
                self.queue_current_index,
                self.repeat_mode,
                self.shuffle_enabled,
            )
        });
        let current_index = self
            .queue_current_index
            .filter(|index| *index < tracks.len());
        let position_ms = if current_index.is_some() {
            self.position_ms
        } else {
            0
        };
        let queue_items = queue_state
            .ordered_ids
            .iter()
            .copied()
            .zip(tracks.iter())
            .collect::<Vec<_>>();
        PlaybackStateStore::open(&self.playback_state_path)?.save(
            &queue_items,
            &queue_state,
            position_ms,
        )?;
        self.queue_state = Some(queue_state);
        self.last_persisted_queue_position_ms = position_ms;
        self.last_persisted_queue_index = current_index;
        Ok(())
    }

    pub(crate) fn persist_queue_if_progressed(&mut self) -> PlayerResult<()> {
        let position_delta = self
            .position_ms
            .abs_diff(self.last_persisted_queue_position_ms);
        if self.queue_current_index != self.last_persisted_queue_index || position_delta >= 5_000 {
            self.persist_queue_state()?;
        }
        Ok(())
    }

    pub(crate) fn export_library(
        &self,
        package_path: &Path,
    ) -> PlayerResult<LibraryPackageSummary> {
        let store = self.store()?;
        let tracks = store.tracks()?;
        let playlist_count = store.playlists()?.len();
        let package_music_root = package_path.join(LIBRARY_PACKAGE_MUSIC_DIRECTORY);
        fs::create_dir_all(&package_music_root)
            .map_err(|source| PlayerError::io(&package_music_root, source))?;

        let mut manifest_tracks = Vec::with_capacity(tracks.len());
        let mut sidecar_files = 0_usize;
        for (index, track) in tracks.iter().enumerate() {
            let audio_file = library_package_audio_path(index, &track.path);
            let destination = package_path.join(&audio_file);
            copy_into_media_library(&track.path, &destination)?;
            sidecar_files += copy_related_sidecars(&track.path, &destination)?;
            manifest_tracks.push(LibraryPackageTrack {
                database_path: path_to_string_lossy(&track.path),
                audio_file: path_to_string_lossy(&audio_file),
            });
        }

        let package_database = package_path.join(LIBRARY_PACKAGE_DATABASE_FILE);
        fs::copy(&self.db_path, &package_database)
            .map_err(|source| PlayerError::io(&package_database, source))?;
        let manifest = LibraryPackageManifest {
            format_version: LIBRARY_PACKAGE_FORMAT_VERSION,
            database_file: LIBRARY_PACKAGE_DATABASE_FILE.to_owned(),
            tracks: manifest_tracks,
        };
        let manifest_data = serde_json::to_vec_pretty(&manifest)
            .map_err(|error| PlayerError::store(error.to_string()))?;
        let manifest_path = package_path.join(LIBRARY_PACKAGE_MANIFEST_FILE);
        fs::write(&manifest_path, manifest_data)
            .map_err(|source| PlayerError::io(&manifest_path, source))?;

        Ok(LibraryPackageSummary {
            tracks: tracks.len(),
            playlists: playlist_count,
            audio_files: tracks.len(),
            sidecar_files,
        })
    }

    pub(crate) fn import_library(
        &mut self,
        package_path: &Path,
    ) -> PlayerResult<LibraryPackageSummary> {
        let manifest_path = package_path.join(LIBRARY_PACKAGE_MANIFEST_FILE);
        let manifest_data =
            fs::read(&manifest_path).map_err(|source| PlayerError::io(&manifest_path, source))?;
        let manifest: LibraryPackageManifest = serde_json::from_slice(&manifest_data)
            .map_err(|error| PlayerError::store(error.to_string()))?;
        if manifest.format_version != LIBRARY_PACKAGE_FORMAT_VERSION {
            return Err(PlayerError::store(format!(
                "unsupported library package version: {}",
                manifest.format_version
            )));
        }
        if manifest.database_file != LIBRARY_PACKAGE_DATABASE_FILE {
            return Err(PlayerError::store(format!(
                "library package database must be `{LIBRARY_PACKAGE_DATABASE_FILE}`"
            )));
        }

        let package_root = package_path
            .canonicalize()
            .map_err(|source| PlayerError::io(package_path, source))?;
        let package_database =
            validated_package_file(&package_root, &manifest.database_file, "database")?;
        let database_tracks = LibraryStore::open(&package_database)?.tracks()?;
        if let Some(track) = database_tracks.iter().find(|track| {
            track.view_kind != TrackViewKind::Primary || track.view_id != track.primary_view_id
        }) {
            return Err(PlayerError::store(format!(
                "library package contains a non-primary track: {}",
                track.path.display()
            )));
        }
        let database_track_paths = database_tracks
            .into_iter()
            .map(|track| track.path)
            .collect::<HashSet<_>>();
        let manifest_database_paths = manifest
            .tracks
            .iter()
            .map(|track| PathBuf::from(&track.database_path))
            .collect::<HashSet<_>>();
        if manifest_database_paths.len() != manifest.tracks.len()
            || manifest_database_paths != database_track_paths
        {
            return Err(PlayerError::store(
                "library package manifest tracks do not match its database",
            ));
        }
        let mut validated_audio_files = Vec::with_capacity(manifest.tracks.len());
        let mut unique_audio_files = HashSet::with_capacity(manifest.tracks.len());
        for track in &manifest.tracks {
            let audio_file = Path::new(&track.audio_file);
            let is_below_music_directory = audio_file
                .strip_prefix(LIBRARY_PACKAGE_MUSIC_DIRECTORY)
                .is_ok_and(|relative| !relative.as_os_str().is_empty());
            if !is_below_music_directory {
                return Err(PlayerError::store(format!(
                    "library package audio path must be below `{LIBRARY_PACKAGE_MUSIC_DIRECTORY}`: {}",
                    track.audio_file
                )));
            }
            if !unique_audio_files.insert(audio_file.to_path_buf()) {
                return Err(PlayerError::store(format!(
                    "library package contains duplicate audio path: {}",
                    track.audio_file
                )));
            }
            validated_audio_files.push(validated_package_file(
                &package_root,
                &track.audio_file,
                "audio",
            )?);
        }

        self.reset_library_runtime_state();
        if let Some(parent) = self.db_path.parent() {
            fs::create_dir_all(parent).map_err(|source| PlayerError::io(parent, source))?;
        }
        fs::copy(&package_database, &self.db_path)
            .map_err(|source| PlayerError::io(&self.db_path, source))?;
        fs::create_dir_all(&self.media_root)
            .map_err(|source| PlayerError::io(&self.media_root, source))?;

        let mut replacements = Vec::with_capacity(manifest.tracks.len());
        let mut sidecar_files = 0_usize;
        for (track, source) in manifest.tracks.iter().zip(validated_audio_files) {
            let audio_file = PathBuf::from(&track.audio_file);
            let relative_audio_path = audio_file
                .strip_prefix(LIBRARY_PACKAGE_MUSIC_DIRECTORY)
                .map_err(|_| PlayerError::store("validated package audio path lost its prefix"))?;
            let destination = self.media_root.join(relative_audio_path);
            copy_into_media_library(&source, &destination)?;
            sidecar_files += copy_related_sidecars(&source, &destination)?;
            replacements.push((PathBuf::from(&track.database_path), destination));
        }

        self.store()?.replace_track_paths(&replacements)?;
        self.restore_persisted_queue()?;
        let playlist_count = self.store()?.playlists()?.len();
        Ok(LibraryPackageSummary {
            tracks: manifest.tracks.len(),
            playlists: playlist_count,
            audio_files: manifest.tracks.len(),
            sidecar_files,
        })
    }

    pub(crate) fn zero_out_library(&mut self) -> PlayerResult<Empty> {
        self.finish_active_session_best_effort("library_zero_out");
        self.reset_library_runtime_state();
        PlaybackStateStore::open(&self.playback_state_path)?.clear()?;
        remove_library_storage(&self.db_path, &self.media_root)?;
        Ok(Empty {})
    }

    pub(crate) fn delete_from_library(&mut self, path: &Path) -> PlayerResult<DeleteTrackSummary> {
        let track = self.store()?.track_by_path(path)?.ok_or_else(|| {
            PlayerError::store(format!("track is not in library: {}", path.display()))
        })?;

        let queue_indexes = self
            .queue_tracks
            .iter()
            .enumerate()
            .filter_map(|(index, queued)| {
                (Path::new(&queued.path) == track.path.as_path()).then_some(index)
            })
            .collect::<Vec<_>>();
        for index in queue_indexes.into_iter().rev() {
            self.remove_queue_item(index)?;
        }

        if !self.store()?.delete_track(&track.path)? {
            return Err(PlayerError::store(format!(
                "track disappeared while deleting: {}",
                track.path.display()
            )));
        }
        let (managed_files_deleted, cleanup_error) =
            match delete_managed_track_files(&self.media_root, &track.path) {
                Ok(deleted) => (deleted, None),
                Err(error) => (0, Some(error.to_string())),
            };
        Ok(DeleteTrackSummary {
            managed_files_deleted,
            cleanup_error,
        })
    }

    pub(crate) fn reset_library_runtime_state(&mut self) {
        self.engine = None;
        self.active_session = None;
        self.pending_session_end_reason = None;
        self.current_track = None;
        self.queue_tracks.clear();
        self.queue_item_ids.clear();
        self.queue_current_index = None;
        self.queue_playback_order.clear();
        self.queue_playback_position = None;
        self.queue_state = None;
        self.playback_mode = PlaybackMode::Sequential;
        self.repeat_mode = RepeatMode::All;
        self.shuffle_enabled = false;
        self.is_playing = false;
        self.position_ms = 0;
        self.last_persisted_queue_position_ms = 0;
        self.last_persisted_queue_index = None;
        self.gain_db = None;
        self.loudness_status = None;
        self.last_error = None;
        self.playback_lifecycle = PlaybackLifecycle::default();
    }
}

fn legacy_queue_snapshot(
    queue_len: usize,
    current_index: Option<usize>,
    repeat_mode: RepeatMode,
    shuffle: bool,
) -> GlobalQueueSnapshot {
    let ordered_ids = (1..=queue_len as u64)
        .map(QueueItemId::from_value)
        .collect::<Vec<_>>();
    let mode = if repeat_mode == RepeatMode::One {
        PlaybackMode::RepeatOne
    } else if shuffle {
        PlaybackMode::Shuffle
    } else {
        PlaybackMode::Sequential
    };
    GlobalQueueSnapshot {
        current_id: current_index
            .and_then(|index| ordered_ids.get(index).copied())
            .or_else(|| ordered_ids.first().copied()),
        next_internal_id: queue_len as u64 + 1,
        ordered_ids,
        mode,
        shuffle_activation_pending: mode == PlaybackMode::Shuffle,
        shuffle: None,
    }
}

fn reconciled_queue_snapshot(
    previous: &GlobalQueueSnapshot,
    ordered_ids: Vec<QueueItemId>,
    preferred_current: Option<QueueItemId>,
) -> GlobalQueueSnapshot {
    let current_id = preferred_current
        .filter(|id| ordered_ids.contains(id))
        .or_else(|| ordered_ids.first().copied());
    GlobalQueueSnapshot {
        ordered_ids,
        next_internal_id: previous.next_internal_id,
        current_id,
        mode: previous.mode,
        shuffle_activation_pending: previous.mode == PlaybackMode::Shuffle,
        shuffle: None,
    }
}
