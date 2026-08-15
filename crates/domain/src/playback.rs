use crate::global_queue::{GlobalPlaybackQueue, GlobalQueueSnapshot, PlaybackMode, QueueItemId};
use crate::loudness::{gain_for_track, GainDecision, NormalizationSettings};
use crate::model::Track;
use crate::playback_error::{PlaybackError, PlaybackResult};

/// Compatibility type for callers that have not migrated to [`PlaybackMode`].
/// New Rust code should use `RepeatOne`, `Sequential`, or `Shuffle` directly.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RepeatMode {
    Off,
    One,
    #[default]
    All,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlaybackCommand {
    Play,
    Pause,
    Next,
    Previous,
    SeekTo { position_ms: u64 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackState {
    pub is_playing: bool,
    pub current_index: Option<usize>,
    pub position_ms: u64,
    pub playback_mode: PlaybackMode,
    /// Temporary compatibility projection. It is not the source of truth.
    pub repeat_mode: RepeatMode,
    /// Temporary compatibility projection. It is not the source of truth.
    pub shuffle: bool,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            is_playing: false,
            current_index: None,
            position_ms: 0,
            playback_mode: PlaybackMode::Sequential,
            repeat_mode: RepeatMode::All,
            shuffle: false,
        }
    }
}

#[derive(Debug)]
pub struct PlayerSession {
    queue: GlobalPlaybackQueue,
    state: PlaybackState,
    normalization: NormalizationSettings,
}

impl PlayerSession {
    pub fn new(normalization: NormalizationSettings) -> Self {
        Self {
            queue: GlobalPlaybackQueue::new(),
            state: PlaybackState::default(),
            normalization,
        }
    }

    pub fn queue(&self) -> Vec<&Track> {
        self.queue.tracks().collect()
    }

    pub fn state(&self) -> &PlaybackState {
        &self.state
    }

    pub fn queue_snapshot(&self) -> GlobalQueueSnapshot {
        self.queue.snapshot()
    }

    pub fn queue_items(&self) -> Vec<(QueueItemId, &Track)> {
        self.queue.queue_items()
    }

    pub fn playback_order(&self) -> Vec<usize> {
        self.queue.playback_order()
    }

    pub fn playback_position(&self) -> Option<usize> {
        self.queue.playback_position()
    }

    pub fn set_queue(&mut self, queue: Vec<Track>, start_index: usize) -> PlaybackResult<()> {
        self.queue.replace(queue, start_index)?;
        self.state.position_ms = 0;
        if self.queue.is_empty() {
            self.state.is_playing = false;
        }
        self.sync_state();
        Ok(())
    }

    pub fn restore_queue(
        &mut self,
        queue: Vec<Track>,
        snapshot: GlobalQueueSnapshot,
        position_ms: u64,
    ) -> PlaybackResult<()> {
        self.queue.restore(queue, snapshot)?;
        self.state.position_ms = if self.queue.current_id().is_some() {
            position_ms
        } else {
            0
        };
        self.state.is_playing = false;
        self.sync_state();
        Ok(())
    }

    pub fn append_to_queue(&mut self, tracks: Vec<Track>) {
        self.queue.append(tracks);
        self.sync_state();
    }

    pub fn insert_next(&mut self, tracks: Vec<Track>) {
        self.queue.insert_next(tracks);
        self.sync_state();
    }

    pub fn move_queue_item(&mut self, from: usize, to: usize) -> PlaybackResult<()> {
        self.queue.move_item(from, to)?;
        self.sync_state();
        Ok(())
    }

    pub fn remove_queue_item(&mut self, index: usize) -> PlaybackResult<Track> {
        let removed = self.queue.remove(index)?;
        if self.queue.is_empty() {
            self.state.is_playing = false;
            self.state.position_ms = 0;
        }
        self.sync_state();
        Ok(removed)
    }

    pub fn clear_queue(&mut self) {
        self.queue.clear();
        self.state.current_index = None;
        self.state.position_ms = 0;
        self.state.is_playing = false;
    }

    pub fn set_playback_mode(&mut self, mode: PlaybackMode) {
        self.queue.set_mode(mode);
        self.sync_state();
    }

    pub fn set_repeat_mode(&mut self, repeat_mode: RepeatMode) {
        match repeat_mode {
            RepeatMode::One => self.set_playback_mode(PlaybackMode::RepeatOne),
            RepeatMode::Off | RepeatMode::All => {
                if self.queue.mode() == PlaybackMode::RepeatOne {
                    self.set_playback_mode(PlaybackMode::Sequential);
                }
            }
        }
    }

    pub fn set_shuffle(&mut self, enabled: bool) {
        if enabled {
            self.set_playback_mode(PlaybackMode::Shuffle);
        } else if self.queue.mode() == PlaybackMode::Shuffle {
            self.set_playback_mode(PlaybackMode::Sequential);
        }
    }

    pub fn start_shuffled(&mut self) -> PlaybackResult<()> {
        self.queue.start_shuffled()?;
        self.state.position_ms = 0;
        self.sync_state();
        Ok(())
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.queue.current_track()
    }

    pub fn select_queue_index(&mut self, index: usize) -> PlaybackResult<()> {
        self.queue.select_index(index)?;
        self.state.position_ms = 0;
        self.sync_state();
        Ok(())
    }

    pub fn current_gain(&self) -> Option<GainDecision> {
        self.current_track()
            .map(|track| gain_for_track(track, self.normalization))
    }

    pub fn apply(&mut self, command: PlaybackCommand) -> PlaybackResult<()> {
        match command {
            PlaybackCommand::Play => self.play(),
            PlaybackCommand::Pause => {
                self.state.is_playing = false;
                Ok(())
            }
            PlaybackCommand::Next => self.next(),
            PlaybackCommand::Previous => self.previous(),
            PlaybackCommand::SeekTo { position_ms } => {
                self.state.position_ms = position_ms;
                Ok(())
            }
        }
    }

    fn play(&mut self) -> PlaybackResult<()> {
        if self.queue.is_empty() {
            return Err(PlaybackError::EmptyQueue);
        }
        self.state.is_playing = true;
        self.sync_state();
        Ok(())
    }

    fn next(&mut self) -> PlaybackResult<()> {
        self.queue.advance()?;
        self.state.position_ms = 0;
        self.sync_state();
        Ok(())
    }

    fn previous(&mut self) -> PlaybackResult<()> {
        if self.state.position_ms > 3_000 {
            self.state.position_ms = 0;
            return Ok(());
        }
        self.queue.rewind()?;
        self.state.position_ms = 0;
        self.sync_state();
        Ok(())
    }

    fn sync_state(&mut self) {
        let mode = self.queue.mode();
        self.state.current_index = self.queue.current_index();
        self.state.playback_mode = mode;
        self.state.repeat_mode = match mode {
            PlaybackMode::RepeatOne => RepeatMode::One,
            PlaybackMode::Sequential | PlaybackMode::Shuffle => RepeatMode::All,
        };
        self.state.shuffle = mode == PlaybackMode::Shuffle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(name: &str) -> Track {
        Track::from_path(format!("{name}.mp3").into())
    }

    #[test]
    fn sequential_playback_is_circular() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session.set_queue(vec![track("a"), track("b")], 0).unwrap();
        session.apply(PlaybackCommand::Play).unwrap();

        session.apply(PlaybackCommand::Next).unwrap();
        assert_eq!(session.current_track().unwrap().title, "b");
        session.apply(PlaybackCommand::Next).unwrap();
        assert_eq!(session.current_track().unwrap().title, "a");
        assert!(session.state().is_playing);
    }

    #[test]
    fn rejects_invalid_start_index() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        let error = session.set_queue(vec![track("a")], 3).unwrap_err();
        assert!(matches!(
            error,
            PlaybackError::InvalidQueueIndex { index: 3, len: 1 }
        ));
    }

    #[test]
    fn append_deduplicates_by_primary_track_identity() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session.append_to_queue(vec![track("a"), track("a"), track("b")]);

        assert_eq!(session.queue().len(), 2);
        assert_eq!(session.current_track().unwrap().title, "a");
        assert!(!session.state().is_playing);
    }

    #[test]
    fn inserting_next_moves_an_existing_item_in_sequential_order() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session
            .set_queue(vec![track("a"), track("b"), track("c")], 0)
            .unwrap();
        session.insert_next(vec![track("c")]);

        let titles = session
            .queue()
            .into_iter()
            .map(|track| track.title.as_str())
            .collect::<Vec<_>>();
        assert_eq!(titles, vec!["a", "c", "b"]);
    }

    #[test]
    fn moving_queue_items_preserves_current_track_identity() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session
            .set_queue(vec![track("a"), track("b"), track("c")], 1)
            .unwrap();

        session.move_queue_item(0, 2).unwrap();
        assert_eq!(session.current_track().unwrap().title, "b");
        assert_eq!(session.state().current_index, Some(0));
    }

    #[test]
    fn removing_current_selects_its_successor() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session
            .set_queue(vec![track("a"), track("b"), track("c")], 1)
            .unwrap();

        let removed = session.remove_queue_item(1).unwrap();
        assert_eq!(removed.title, "b");
        assert_eq!(session.current_track().unwrap().title, "c");
    }

    #[test]
    fn playback_modes_are_mutually_exclusive() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session.set_queue(vec![track("a"), track("b")], 0).unwrap();
        session.set_playback_mode(PlaybackMode::Shuffle);
        assert_eq!(session.state().playback_mode, PlaybackMode::Shuffle);
        assert!(session.state().shuffle);

        session.set_playback_mode(PlaybackMode::RepeatOne);
        assert_eq!(session.state().playback_mode, PlaybackMode::RepeatOne);
        assert!(!session.state().shuffle);
        assert_eq!(session.state().repeat_mode, RepeatMode::One);
    }

    #[test]
    fn shuffle_switch_materializes_on_next_not_on_toggle() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session
            .set_queue(vec![track("a"), track("b"), track("c")], 0)
            .unwrap();
        session.set_playback_mode(PlaybackMode::Shuffle);
        assert!(session.queue_snapshot().shuffle.is_none());

        session.apply(PlaybackCommand::Next).unwrap();
        assert_eq!(session.queue_snapshot().shuffle.unwrap().cycles.len(), 3);
    }

    #[test]
    fn restoring_a_session_keeps_the_realized_shuffle_future() {
        let tracks = vec![track("a"), track("b"), track("c")];
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session.set_queue(tracks.clone(), 0).unwrap();
        session.start_shuffled().unwrap();
        session.apply(PlaybackCommand::Next).unwrap();
        let snapshot = session.queue_snapshot();
        let current = session.current_track().unwrap().title.clone();

        let mut restored = PlayerSession::new(NormalizationSettings::default());
        restored
            .restore_queue(tracks, snapshot.clone(), 1_234)
            .unwrap();
        assert_eq!(restored.queue_snapshot(), snapshot);
        assert_eq!(restored.current_track().unwrap().title, current);
        assert_eq!(restored.state().position_ms, 1_234);
    }

    #[test]
    fn previous_restarts_when_past_three_seconds() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session.set_queue(vec![track("a"), track("b")], 1).unwrap();
        session
            .apply(PlaybackCommand::SeekTo { position_ms: 3_500 })
            .unwrap();

        session.apply(PlaybackCommand::Previous).unwrap();
        assert_eq!(session.current_track().unwrap().title, "b");
        assert_eq!(session.state().position_ms, 0);
    }

    #[test]
    fn exposes_current_gain() {
        let mut quiet = track("quiet");
        quiet.loudness = Some(crate::model::LoudnessInfo::track(-20.0, -8.0));
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session.set_queue(vec![quiet], 0).unwrap();

        assert_eq!(session.current_gain().unwrap().gain_db, 4.0);
    }
}
