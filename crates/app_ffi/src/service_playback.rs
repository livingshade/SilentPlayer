use std::path::{Path, PathBuf};

use errors::{PlayerError, PlayerResult};
use serde::Serialize;

use crate::dto::PlaybackQueueDto;
use crate::playback_helpers::{library_playback_plan, parse_repeat_mode, repeat_mode_name};
use crate::PlayerApp;

impl PlayerApp {
    pub(crate) fn service_play_library(&mut self) -> PlayerResult<impl Serialize> {
        let (tracks, start_index) = library_playback_plan(&self.store()?, None)?;
        self.play_queue_tracks(tracks, start_index, false)
    }

    pub(crate) fn service_play_path(&mut self, path: &Path) -> PlayerResult<impl Serialize> {
        let (tracks, start_index) = library_playback_plan(&self.store()?, Some(path))?;
        self.play_queue_tracks(tracks, start_index, false)
    }

    pub(crate) fn service_play_queue(
        &mut self,
        paths: &[PathBuf],
        start_path: &Path,
    ) -> PlayerResult<impl Serialize> {
        if paths.is_empty() {
            return Err(PlayerError::invalid_input("queue is empty"));
        }
        let store = self.store()?;
        let mut tracks = Vec::with_capacity(paths.len());
        for path in paths {
            let track = store.track_by_path(path)?.ok_or_else(|| {
                PlayerError::store(format!("track is not in library: {}", path.display()))
            })?;
            tracks.push(track);
        }
        let start_index = tracks
            .iter()
            .position(|track| track.path == start_path)
            .ok_or_else(|| {
                PlayerError::store(format!(
                    "queue start track is not in queue: {}",
                    start_path.display()
                ))
            })?;
        self.play_queue_tracks(tracks, start_index, false)
    }

    pub(crate) fn service_play_playlist(
        &mut self,
        name: &str,
        start_path: Option<&Path>,
        shuffle: bool,
    ) -> PlayerResult<impl Serialize> {
        let mut store = self.store()?;
        let tracks = store
            .playlist_tracks(name)?
            .into_iter()
            .map(|entry| entry.track)
            .collect::<Vec<_>>();
        if tracks.is_empty() {
            return Err(PlayerError::invalid_input(format!(
                "playlist is empty: {name}"
            )));
        }
        let start_index = match start_path {
            None => 0,
            Some(start_path) => tracks
                .iter()
                .position(|track| track.path == start_path)
                .ok_or_else(|| {
                    PlayerError::invalid_input(format!(
                        "playlist start track is not in {name}: {}",
                        start_path.display()
                    ))
                })?,
        };
        store.touch_playlist(name)?;
        self.shuffle_enabled = shuffle;
        self.play_queue_tracks(tracks, start_index, shuffle && start_path.is_none())
    }

    pub(crate) fn service_pause(&mut self) -> PlayerResult<impl Serialize> {
        self.playback_lifecycle.user_stopped_playback();
        if let Some(engine) = &self.engine {
            engine.pause()?;
            self.poll_events();
        } else {
            self.is_playing = false;
        }
        self.persist_queue_state()?;
        Ok(self.snapshot())
    }

    pub(crate) fn service_resume(&mut self) -> PlayerResult<impl Serialize> {
        self.ensure_playback_can_start()?;
        self.ensure_engine_queue_loaded()?.play()?;
        self.poll_events();
        self.persist_queue_state()?;
        Ok(self.snapshot())
    }

    pub(crate) fn service_audio_interruption_began(&mut self) -> PlayerResult<impl Serialize> {
        self.poll_events();
        let action = self.playback_lifecycle.begin_interruption(self.is_playing);
        self.apply_playback_lifecycle_action(action)?;
        Ok(self.snapshot())
    }

    pub(crate) fn service_audio_interruption_ended(
        &mut self,
        system_should_resume: bool,
    ) -> PlayerResult<impl Serialize> {
        self.poll_events();
        let action = self
            .playback_lifecycle
            .end_interruption(system_should_resume, self.current_track.is_some());
        self.apply_playback_lifecycle_action(action)?;
        Ok(self.snapshot())
    }

    pub(crate) fn service_audio_output_disconnected(&mut self) -> PlayerResult<impl Serialize> {
        self.poll_events();
        let action = self.playback_lifecycle.output_disconnected(self.is_playing);
        self.apply_playback_lifecycle_action(action)?;
        Ok(self.snapshot())
    }

    pub(crate) fn service_stop(&mut self) -> PlayerResult<impl Serialize> {
        self.playback_lifecycle.user_stopped_playback();
        if self.current_track.is_some() {
            if let Some(engine) = self.engine.as_ref() {
                engine.pause()?;
            }
            if self.engine.is_some() {
                self.poll_events();
                self.finish_active_session_best_effort("stopped");
            }
        }
        if let Some(engine) = self.engine.take() {
            engine.shutdown()?;
        }
        self.is_playing = false;
        self.position_ms = 0;
        self.current_track = None;
        self.queue_tracks.clear();
        self.queue_current_index = None;
        self.last_error = None;
        self.persist_queue_state()?;
        Ok(self.snapshot())
    }

    pub(crate) fn service_next(&mut self) -> PlayerResult<impl Serialize> {
        self.pending_session_end_reason = Some("next".to_owned());
        self.ensure_engine_queue_loaded()?.next()?;
        self.poll_events();
        self.persist_queue_state()?;
        Ok(self.snapshot())
    }

    pub(crate) fn service_previous(&mut self) -> PlayerResult<impl Serialize> {
        self.pending_session_end_reason = Some("previous".to_owned());
        self.ensure_engine_queue_loaded()?.previous()?;
        self.poll_events();
        self.persist_queue_state()?;
        Ok(self.snapshot())
    }

    pub(crate) fn service_seek(&mut self, position_ms: u64) -> PlayerResult<impl Serialize> {
        self.observe_active_position(self.position_ms);
        self.ensure_engine_queue_loaded()?.seek_to(position_ms)?;
        self.position_ms = position_ms;
        if let Some(session) = &mut self.active_session {
            session.seek_count = session.seek_count.saturating_add(1);
            session.last_position_ms = position_ms;
        }
        self.poll_events();
        self.persist_queue_state()?;
        Ok(self.snapshot())
    }

    pub(crate) fn service_poll(&mut self) -> PlayerResult<impl Serialize> {
        if self.is_playing {
            if let Some(engine) = self.engine.as_ref() {
                engine.refresh()?;
            }
        }
        self.poll_events();
        self.persist_queue_if_progressed()?;
        Ok(self.snapshot())
    }

    pub(crate) fn service_set_repeat_mode(
        &mut self,
        repeat_mode: &str,
    ) -> PlayerResult<impl Serialize> {
        let repeat_mode = parse_repeat_mode(repeat_mode)?;
        self.repeat_mode = repeat_mode;
        if let Some(engine) = self.engine.as_ref() {
            engine.set_repeat_mode(repeat_mode)?;
            self.poll_events();
        }
        self.persist_queue_state()?;
        Ok(self.snapshot())
    }

    pub(crate) fn service_set_shuffle(&mut self, enabled: bool) -> PlayerResult<impl Serialize> {
        self.shuffle_enabled = enabled;
        if let Some(engine) = self.engine.as_ref() {
            engine.set_shuffle(enabled)?;
            self.poll_events();
        }
        self.persist_queue_state()?;
        Ok(self.snapshot())
    }

    pub(crate) fn service_queue(&mut self) -> PlayerResult<impl Serialize> {
        self.poll_events();
        let order = self.valid_queue_playback_order();
        Ok(PlaybackQueueDto {
            tracks: order
                .iter()
                .filter_map(|index| self.queue_tracks.get(*index).cloned())
                .collect(),
            current_index: self
                .queue_playback_position
                .filter(|position| *position < order.len()),
            repeat_mode: repeat_mode_name(self.repeat_mode).to_owned(),
            shuffle_enabled: self.shuffle_enabled,
        })
    }

    pub(crate) fn service_queue_play_next(&mut self, path: &Path) -> PlayerResult<impl Serialize> {
        self.add_path_to_queue(path, true)
    }

    pub(crate) fn service_queue_play(&mut self, index: usize) -> PlayerResult<impl Serialize> {
        self.play_queue_item(index)
    }

    pub(crate) fn service_queue_add(&mut self, path: &Path) -> PlayerResult<impl Serialize> {
        self.add_path_to_queue(path, false)
    }

    pub(crate) fn service_queue_move(
        &mut self,
        from: usize,
        to: usize,
    ) -> PlayerResult<impl Serialize> {
        self.move_queue_item(from, to)
    }

    pub(crate) fn service_queue_remove(&mut self, index: usize) -> PlayerResult<impl Serialize> {
        self.remove_queue_item(index)
    }

    pub(crate) fn service_queue_clear(&mut self) -> PlayerResult<impl Serialize> {
        self.clear_queue()
    }
}
