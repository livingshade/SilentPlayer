use std::path::Path;

use audio_rodio::RodioBackend;
use domain::{NormalizationSettings, PlaybackLifecycleAction, Track};
use engine::{PlaybackEvent, PlayerEngine};
use errors::{PlayerError, PlayerResult};
use store_sqlite::LibraryStore;

use crate::dto::{track_dtos_with_artwork, track_to_dto_with_artwork, PlaybackSnapshot, TrackDto};
use crate::playback_helpers::{is_valid_queue_order, moved_queue_index, repeat_mode_name};
use crate::PlayerApp;

impl PlayerApp {
    pub(crate) fn play_queue_tracks(
        &mut self,
        tracks: Vec<Track>,
        start_index: usize,
        randomize_start: bool,
    ) -> PlayerResult<PlaybackSnapshot> {
        if tracks.is_empty() {
            return Err(PlayerError::invalid_input("queue is empty"));
        }
        if start_index >= tracks.len() {
            return Err(PlayerError::invalid_input(format!(
                "invalid queue index {start_index} for queue length {}",
                tracks.len()
            )));
        }

        self.poll_events();
        self.ensure_playback_can_start()?;
        self.finish_active_session_best_effort("played_other_track");

        let store = self.store()?;
        let queue_tracks = track_dtos_with_artwork(&tracks, &store, &self.db_path)?;
        let repeat_mode = self.repeat_mode;
        let shuffle_enabled = self.shuffle_enabled;
        {
            let engine = self.engine()?;
            engine.play_queue(
                tracks,
                start_index,
                repeat_mode,
                shuffle_enabled,
                randomize_start,
            )?;
        }
        self.queue_tracks = queue_tracks;
        self.reset_queue_playback_order();
        self.last_error = None;
        self.poll_events();
        self.persist_queue_state()?;
        Ok(self.snapshot())
    }

    pub(crate) fn valid_queue_playback_order(&self) -> Vec<usize> {
        if is_valid_queue_order(&self.queue_playback_order, self.queue_tracks.len()) {
            self.queue_playback_order.clone()
        } else {
            (0..self.queue_tracks.len()).collect()
        }
    }

    pub(crate) fn reset_queue_playback_order(&mut self) {
        self.queue_playback_order = (0..self.queue_tracks.len()).collect();
        self.queue_playback_position = self
            .queue_current_index
            .filter(|index| *index < self.queue_tracks.len());
    }

    pub(crate) fn source_queue_index(&self, displayed_index: usize) -> Option<usize> {
        self.valid_queue_playback_order()
            .get(displayed_index)
            .copied()
    }

    pub(crate) fn add_path_to_queue(
        &mut self,
        path: &Path,
        play_next: bool,
    ) -> PlayerResult<PlaybackSnapshot> {
        self.poll_events();
        let store = self.store()?;
        let track = store.track_by_path(path)?.ok_or_else(|| {
            PlayerError::store(format!("track is not in library: {}", path.display()))
        })?;
        let existing_index = self
            .queue_tracks
            .iter()
            .position(|queued| queued.primary_view_id == track.primary_view_id.value());

        if let Some(existing_index) = existing_index {
            if play_next {
                let current_index = self.queue_current_index.unwrap_or(0);
                if existing_index != current_index {
                    let target_index = if existing_index < current_index {
                        current_index
                    } else {
                        (current_index + 1).min(self.queue_tracks.len() - 1)
                    };
                    if let Some(engine) = self.engine.as_ref() {
                        engine.move_queue_item(existing_index, target_index)?;
                    } else {
                        self.queue_current_index = moved_queue_index(
                            self.queue_current_index,
                            existing_index,
                            target_index,
                        );
                    }
                    let queued = self.queue_tracks.remove(existing_index);
                    self.queue_tracks.insert(target_index, queued);
                    self.poll_events();
                }
            }
            self.persist_queue_state()?;
            return Ok(self.snapshot());
        }

        let dto = track_to_dto_with_artwork(&track, &store, &self.db_path)?;
        let insert_index = if play_next {
            self.queue_current_index
                .map(|index| (index + 1).min(self.queue_tracks.len()))
                .unwrap_or(self.queue_tracks.len())
        } else {
            self.queue_tracks.len()
        };
        if play_next {
            if let Some(engine) = self.engine.as_ref() {
                engine.insert_next(vec![track])?;
            }
            self.queue_tracks.insert(insert_index, dto);
        } else {
            if let Some(engine) = self.engine.as_ref() {
                engine.append_to_queue(vec![track])?;
            }
            self.queue_tracks.push(dto);
        }
        if self.queue_current_index.is_none() {
            self.queue_current_index = Some(0);
            self.current_track = self.queue_tracks.first().cloned();
            self.position_ms = 0;
        }
        self.reset_queue_playback_order();
        self.poll_events();
        self.persist_queue_state()?;
        Ok(self.snapshot())
    }

    pub(crate) fn play_queue_item(
        &mut self,
        displayed_index: usize,
    ) -> PlayerResult<PlaybackSnapshot> {
        self.poll_events();
        let source_index = self.source_queue_index(displayed_index).ok_or_else(|| {
            PlayerError::invalid_input(format!(
                "queue index {displayed_index} is outside queue length {}",
                self.queue_tracks.len()
            ))
        })?;
        self.ensure_playback_can_start()?;
        self.finish_active_session_best_effort("played_other_track");
        {
            let engine = self.ensure_engine_queue_loaded()?;
            engine.play_queue_item(source_index)?;
        }
        self.poll_events();
        self.persist_queue_state()?;
        Ok(self.snapshot())
    }

    pub(crate) fn move_queue_item(
        &mut self,
        from: usize,
        to: usize,
    ) -> PlayerResult<PlaybackSnapshot> {
        if self.shuffle_enabled {
            return Err(PlayerError::invalid_input(
                "turn shuffle off before reordering the queue",
            ));
        }
        let len = self.queue_tracks.len();
        if from >= len || to >= len {
            return Err(PlayerError::invalid_input(format!(
                "queue move indexes must be below {len}: {from} -> {to}"
            )));
        }
        if let Some(engine) = self.engine.as_ref() {
            engine.move_queue_item(from, to)?;
        } else {
            self.queue_current_index = moved_queue_index(self.queue_current_index, from, to);
        }
        let track = self.queue_tracks.remove(from);
        self.queue_tracks.insert(to, track);
        self.reset_queue_playback_order();
        self.poll_events();
        self.persist_queue_state()?;
        Ok(self.snapshot())
    }

    pub(crate) fn remove_queue_item(
        &mut self,
        displayed_index: usize,
    ) -> PlayerResult<PlaybackSnapshot> {
        let Some(index) = self.source_queue_index(displayed_index) else {
            return Err(PlayerError::invalid_input(format!(
                "queue index {displayed_index} is outside queue length {}",
                self.queue_tracks.len()
            )));
        };
        let removed_current = self.queue_current_index == Some(index);
        if removed_current {
            self.pending_session_end_reason = Some("removed_from_queue".to_owned());
        }
        if let Some(engine) = self.engine.as_ref() {
            engine.remove_queue_item(index)?;
        }
        self.queue_tracks.remove(index);
        if self.engine.is_none() {
            self.queue_current_index = match self.queue_current_index {
                _ if self.queue_tracks.is_empty() => None,
                Some(current) if current == index => Some(index.min(self.queue_tracks.len() - 1)),
                Some(current) if index < current => Some(current - 1),
                current => current,
            };
            self.current_track = self
                .queue_current_index
                .and_then(|current| self.queue_tracks.get(current).cloned());
            if removed_current {
                self.position_ms = 0;
            }
        }
        self.reset_queue_playback_order();
        self.poll_events();
        self.persist_queue_state()?;
        Ok(self.snapshot())
    }

    pub(crate) fn clear_queue(&mut self) -> PlayerResult<PlaybackSnapshot> {
        let had_engine = self.engine.is_some();
        if let Some(engine) = self.engine.as_ref() {
            self.pending_session_end_reason = Some("queue_cleared".to_owned());
            engine.clear_queue()?;
        }
        self.queue_tracks.clear();
        self.queue_current_index = None;
        self.queue_playback_order.clear();
        self.queue_playback_position = None;
        self.is_playing = false;
        self.position_ms = 0;
        self.poll_events();
        if !had_engine {
            self.current_track = None;
        }
        self.persist_queue_state()?;
        Ok(self.snapshot())
    }

    pub(crate) fn replace_cached_track(&mut self, updated: TrackDto) {
        if self
            .current_track
            .as_ref()
            .is_some_and(|track| track.path == updated.path)
        {
            self.current_track = Some(updated.clone());
        }

        for track in &mut self.queue_tracks {
            if track.path == updated.path {
                *track = updated.clone();
            }
        }

        if let Some(session) = &mut self.active_session {
            if session.track.path == updated.path {
                session.track = updated;
            }
        }
    }

    pub(crate) fn track_to_dto_with_artwork(&self, track: &Track) -> PlayerResult<TrackDto> {
        let store = self.store()?;
        track_to_dto_with_artwork(track, &store, &self.db_path)
    }

    pub(crate) fn store(&self) -> PlayerResult<LibraryStore> {
        LibraryStore::open(&self.db_path)
    }

    pub(crate) fn engine(&mut self) -> PlayerResult<&PlayerEngine> {
        if self.engine.is_none() {
            self.engine = Some(PlayerEngine::spawn(
                NormalizationSettings::default(),
                RodioBackend::open_default,
            )?);
        }
        Ok(self.engine.as_ref().expect("engine just initialized"))
    }

    pub(crate) fn ensure_engine_queue_loaded(&mut self) -> PlayerResult<&PlayerEngine> {
        if self.engine.is_none() {
            let restored = if self.queue_tracks.is_empty() {
                None
            } else {
                let store = self.store()?;
                let mut tracks = Vec::with_capacity(self.queue_tracks.len());
                for queued in &self.queue_tracks {
                    tracks.push(store.track_by_path(&queued.path)?.ok_or_else(|| {
                        PlayerError::store(format!(
                            "queued track is not in library: {}",
                            queued.path
                        ))
                    })?);
                }
                Some((
                    tracks,
                    self.queue_current_index.unwrap_or(0),
                    self.position_ms,
                    self.repeat_mode,
                    self.shuffle_enabled,
                ))
            };
            let engine =
                PlayerEngine::spawn(NormalizationSettings::default(), RodioBackend::open_default)?;
            if let Some((tracks, current_index, position_ms, repeat_mode, shuffle_enabled)) =
                restored
            {
                engine.restore_queue(
                    tracks,
                    current_index,
                    position_ms,
                    repeat_mode,
                    shuffle_enabled,
                )?;
            }
            self.engine = Some(engine);
            self.poll_events();
        }
        Ok(self.engine.as_ref().expect("engine just initialized"))
    }

    pub(crate) fn ensure_playback_can_start(&mut self) -> PlayerResult<()> {
        if self.playback_lifecycle.request_playback_start() {
            Ok(())
        } else {
            Err(PlayerError::audio(
                "playback cannot start while an audio interruption is active",
            ))
        }
    }

    pub(crate) fn apply_playback_lifecycle_action(
        &mut self,
        action: PlaybackLifecycleAction,
    ) -> PlayerResult<()> {
        match action {
            PlaybackLifecycleAction::None => {}
            PlaybackLifecycleAction::Pause => {
                if let Some(engine) = &self.engine {
                    engine.pause()?;
                    self.poll_events();
                }
                self.is_playing = false;
            }
            PlaybackLifecycleAction::Resume => {
                if self.current_track.is_some() {
                    self.ensure_engine_queue_loaded()?.play()?;
                    self.poll_events();
                }
            }
        }
        Ok(())
    }

    pub(crate) fn poll_events(&mut self) {
        let Some(engine) = &self.engine else {
            return;
        };

        let mut events = Vec::new();
        while let Some(event) = engine.try_recv_event() {
            events.push(event);
        }
        for event in events {
            self.apply_event(event);
        }
    }

    pub(crate) fn apply_event(&mut self, event: PlaybackEvent) {
        match event {
            PlaybackEvent::StateChanged(state) => {
                self.observe_active_position(state.position_ms);
                self.is_playing = state.is_playing;
                self.position_ms = state.position_ms;
                self.repeat_mode = state.repeat_mode;
                self.shuffle_enabled = state.shuffle;
            }
            PlaybackEvent::QueueOrderChanged {
                order,
                current_position,
            } => {
                if is_valid_queue_order(&order, self.queue_tracks.len()) {
                    self.queue_playback_order = order;
                    self.queue_playback_position =
                        current_position.filter(|position| *position < self.queue_tracks.len());
                } else {
                    self.reset_queue_playback_order();
                }
            }
            PlaybackEvent::TrackChanged(track) => {
                let next_track = match track
                    .as_deref()
                    .map(|track| self.track_to_dto_with_artwork(track))
                    .transpose()
                {
                    Ok(track) => track,
                    Err(error) => {
                        self.last_error = Some(error.to_string());
                        None
                    }
                };
                let old_path = self.current_track.as_ref().map(|track| track.path.as_str());
                let next_path = next_track.as_ref().map(|track| track.path.as_str());
                if old_path != next_path {
                    let reason = self
                        .pending_session_end_reason
                        .take()
                        .unwrap_or_else(|| "track_changed".to_owned());
                    self.finish_active_session_best_effort(&reason);
                    if self.is_playing {
                        if let Some(track) = next_track.clone() {
                            self.start_active_session(track, self.position_ms);
                        }
                    }
                } else {
                    self.pending_session_end_reason = None;
                    if self.is_playing && self.active_session.is_none() {
                        if let Some(track) = next_track.clone() {
                            self.start_active_session(track, self.position_ms);
                        }
                    }
                }
                self.queue_current_index = next_track.as_ref().and_then(|next_track| {
                    self.queue_tracks
                        .iter()
                        .position(|track| track.path == next_track.path)
                });
                self.queue_playback_position = self.queue_current_index.and_then(|current_index| {
                    self.valid_queue_playback_order()
                        .iter()
                        .position(|index| *index == current_index)
                });
                self.current_track = next_track;
            }
            PlaybackEvent::GainChanged(gain) => {
                self.gain_db = gain.as_ref().map(|gain| gain.gain_db);
                self.loudness_status = gain.map(|gain| format!("{:?}", gain.status));
            }
            PlaybackEvent::PositionChanged(position_ms) => {
                self.observe_active_position(position_ms);
                self.position_ms = position_ms;
            }
            PlaybackEvent::Error(error) => {
                self.finish_active_session_best_effort("error");
                self.last_error = Some(error);
                self.is_playing = false;
            }
            PlaybackEvent::Stopped => {
                let reason = self
                    .pending_session_end_reason
                    .take()
                    .unwrap_or_else(|| "stopped".to_owned());
                self.finish_active_session_best_effort(&reason);
                self.is_playing = false;
            }
        }
    }

    pub(crate) fn snapshot(&self) -> PlaybackSnapshot {
        PlaybackSnapshot {
            is_playing: self.is_playing,
            position_ms: self.position_ms,
            current_track: self.current_track.clone(),
            queue_len: self.queue_tracks.len(),
            queue_position: self.queue_playback_position,
            repeat_mode: repeat_mode_name(self.repeat_mode).to_owned(),
            shuffle_enabled: self.shuffle_enabled,
            gain_db: self.gain_db,
            loudness_status: self.loudness_status.clone(),
            error: self.last_error.clone(),
            interruption_active: self.playback_lifecycle.interruption_active(),
            resume_after_interruption: self.playback_lifecycle.resume_after_interruption(),
        }
    }
}
