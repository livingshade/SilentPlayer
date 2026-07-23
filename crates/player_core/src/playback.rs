use crate::loudness::{gain_for_track, GainDecision, NormalizationSettings};
use crate::model::Track;
use crate::playback_error::{PlaybackError, PlaybackResult};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RepeatMode {
    #[default]
    Off,
    One,
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
    pub repeat_mode: RepeatMode,
    pub shuffle: bool,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            is_playing: false,
            current_index: None,
            position_ms: 0,
            repeat_mode: RepeatMode::Off,
            shuffle: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PlayerSession {
    queue: Vec<Track>,
    state: PlaybackState,
    normalization: NormalizationSettings,
    shuffle_order: Vec<usize>,
    shuffle_cursor: Option<usize>,
    shuffle_rng: ShuffleRng,
}

impl PlayerSession {
    pub fn new(normalization: NormalizationSettings) -> Self {
        Self {
            queue: Vec::new(),
            state: PlaybackState::default(),
            normalization,
            shuffle_order: Vec::new(),
            shuffle_cursor: None,
            shuffle_rng: ShuffleRng::seeded_from_time(),
        }
    }

    pub fn queue(&self) -> &[Track] {
        &self.queue
    }

    pub fn state(&self) -> &PlaybackState {
        &self.state
    }

    pub fn set_queue(&mut self, queue: Vec<Track>, start_index: usize) -> PlaybackResult<()> {
        if queue.is_empty() {
            self.queue = queue;
            self.state.current_index = None;
            self.state.position_ms = 0;
            self.state.is_playing = false;
            self.shuffle_order.clear();
            self.shuffle_cursor = None;
            return Ok(());
        }

        if start_index >= queue.len() {
            return Err(PlaybackError::InvalidQueueIndex {
                index: start_index,
                len: queue.len(),
            });
        }

        self.queue = queue;
        self.state.current_index = Some(start_index);
        self.state.position_ms = 0;
        self.reset_queue_order(start_index);
        Ok(())
    }

    pub fn append_to_queue(&mut self, tracks: Vec<Track>) {
        if tracks.is_empty() {
            return;
        }

        let was_empty = self.queue.is_empty();
        self.queue.extend(tracks);
        if was_empty {
            self.state.current_index = Some(0);
            self.state.position_ms = 0;
        }
        self.rebuild_order_after_queue_edit();
    }

    pub fn insert_next(&mut self, tracks: Vec<Track>) {
        if tracks.is_empty() {
            return;
        }

        if self.queue.is_empty() {
            self.append_to_queue(tracks);
            return;
        }

        let insert_index = self
            .state
            .current_index
            .map(|index| index + 1)
            .unwrap_or(self.queue.len());
        self.queue.splice(insert_index..insert_index, tracks);
        self.rebuild_order_after_queue_edit();
    }

    pub fn move_queue_item(&mut self, from: usize, to: usize) -> PlaybackResult<()> {
        let len = self.queue.len();
        if from >= len {
            return Err(PlaybackError::InvalidQueueIndex { index: from, len });
        }
        if to >= len {
            return Err(PlaybackError::InvalidQueueIndex { index: to, len });
        }
        if from == to {
            return Ok(());
        }

        let current_index = self.state.current_index;
        let track = self.queue.remove(from);
        self.queue.insert(to, track);
        self.state.current_index = current_index.map(|current| {
            if current == from {
                to
            } else if from < current && to >= current {
                current - 1
            } else if from > current && to <= current {
                current + 1
            } else {
                current
            }
        });
        self.rebuild_order_after_queue_edit();
        Ok(())
    }

    pub fn remove_queue_item(&mut self, index: usize) -> PlaybackResult<Track> {
        let len = self.queue.len();
        if index >= len {
            return Err(PlaybackError::InvalidQueueIndex { index, len });
        }

        let removed = self.queue.remove(index);
        self.state.current_index = match self.state.current_index {
            _ if self.queue.is_empty() => {
                self.state.is_playing = false;
                self.state.position_ms = 0;
                None
            }
            Some(current) if current == index => {
                self.state.position_ms = 0;
                Some(index.min(self.queue.len() - 1))
            }
            Some(current) if index < current => Some(current - 1),
            current => current,
        };
        self.rebuild_order_after_queue_edit();
        Ok(removed)
    }

    pub fn clear_queue(&mut self) {
        self.queue.clear();
        self.state.current_index = None;
        self.state.position_ms = 0;
        self.state.is_playing = false;
        self.shuffle_order.clear();
        self.shuffle_cursor = None;
    }

    pub fn set_repeat_mode(&mut self, repeat_mode: RepeatMode) {
        self.state.repeat_mode = repeat_mode;
    }

    pub fn set_shuffle(&mut self, enabled: bool) {
        if self.state.shuffle == enabled {
            return;
        }

        self.state.shuffle = enabled;
        if enabled {
            self.rebuild_shuffle_order_from_current();
        } else {
            self.reset_linear_order();
        }
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.state
            .current_index
            .and_then(|index| self.queue.get(index))
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

        if self.state.current_index.is_none() {
            self.state.current_index = Some(0);
        }

        self.state.is_playing = true;
        Ok(())
    }

    fn next(&mut self) -> PlaybackResult<()> {
        let Some(index) = self.state.current_index else {
            return Err(PlaybackError::EmptyQueue);
        };

        let next_index = if self.state.repeat_mode == RepeatMode::One {
            Some(index)
        } else if self.state.shuffle {
            self.next_shuffle_index(index)
        } else {
            match (index + 1 < self.queue.len(), self.state.repeat_mode) {
                (true, _) => Some(index + 1),
                (false, RepeatMode::All) => Some(0),
                (false, RepeatMode::One) => Some(index),
                (false, RepeatMode::Off) => None,
            }
        };

        self.state.current_index = next_index;
        self.state.position_ms = 0;
        if next_index.is_none() {
            self.state.is_playing = false;
        }
        Ok(())
    }

    fn previous(&mut self) -> PlaybackResult<()> {
        let Some(index) = self.state.current_index else {
            return Err(PlaybackError::EmptyQueue);
        };

        if self.state.position_ms > 3000 {
            self.state.position_ms = 0;
        } else if self.state.shuffle {
            if let Some(previous_index) = self.previous_shuffle_index(index) {
                self.state.current_index = Some(previous_index);
            }
            self.state.position_ms = 0;
        } else if index == 0 {
            if self.state.repeat_mode == RepeatMode::All && self.queue.len() > 1 {
                self.state.current_index = Some(self.queue.len() - 1);
            }
            self.state.position_ms = 0;
        } else {
            self.state.current_index = Some(index - 1);
            self.state.position_ms = 0;
        }
        Ok(())
    }

    fn reset_queue_order(&mut self, start_index: usize) {
        if self.state.shuffle {
            self.rebuild_shuffle_order_from(start_index);
        } else {
            self.reset_linear_order();
        }
    }

    fn rebuild_order_after_queue_edit(&mut self) {
        match self.state.current_index {
            Some(index) if self.state.shuffle => self.rebuild_shuffle_order_from(index),
            Some(_) => self.reset_linear_order(),
            None => {
                self.shuffle_order.clear();
                self.shuffle_cursor = None;
            }
        }
    }

    fn reset_linear_order(&mut self) {
        self.shuffle_order = (0..self.queue.len()).collect();
        self.shuffle_cursor = self.state.current_index;
    }

    fn rebuild_shuffle_order_from_current(&mut self) {
        if let Some(index) = self.state.current_index {
            self.rebuild_shuffle_order_from(index);
        } else {
            self.shuffle_order.clear();
            self.shuffle_cursor = None;
        }
    }

    fn rebuild_shuffle_order_from(&mut self, current_index: usize) {
        if self.queue.is_empty() {
            self.shuffle_order.clear();
            self.shuffle_cursor = None;
            return;
        }

        // Shuffle is a bag, not an independent random draw per "next".
        // Every queue item appears once before the bag is rebuilt.
        let current_index = current_index.min(self.queue.len() - 1);
        let mut rest = (0..self.queue.len())
            .filter(|index| *index != current_index)
            .collect::<Vec<_>>();
        self.shuffle_rng.shuffle(&mut rest);
        self.shuffle_order.clear();
        self.shuffle_order.push(current_index);
        self.shuffle_order.extend(rest);
        self.shuffle_cursor = Some(0);
    }

    fn next_shuffle_index(&mut self, current_index: usize) -> Option<usize> {
        let cursor = self.sync_shuffle_cursor(current_index);
        if cursor + 1 < self.shuffle_order.len() {
            self.shuffle_cursor = Some(cursor + 1);
            return self.shuffle_order.get(cursor + 1).copied();
        }

        if self.state.repeat_mode != RepeatMode::All {
            return None;
        }

        self.rebuild_next_shuffle_cycle_after(current_index)
    }

    fn previous_shuffle_index(&mut self, current_index: usize) -> Option<usize> {
        let cursor = self.sync_shuffle_cursor(current_index);
        if cursor > 0 {
            self.shuffle_cursor = Some(cursor - 1);
            return self.shuffle_order.get(cursor - 1).copied();
        }

        if self.state.repeat_mode == RepeatMode::All && self.shuffle_order.len() > 1 {
            self.shuffle_cursor = Some(self.shuffle_order.len() - 1);
            return self.shuffle_order.last().copied();
        }

        None
    }

    fn sync_shuffle_cursor(&mut self, current_index: usize) -> usize {
        if self.shuffle_order.len() != self.queue.len()
            || !self.shuffle_order.contains(&current_index)
        {
            self.rebuild_shuffle_order_from(current_index);
            return 0;
        }

        if let Some(cursor) = self.shuffle_cursor {
            if self.shuffle_order.get(cursor).copied() == Some(current_index) {
                return cursor;
            }
        }

        let cursor = self
            .shuffle_order
            .iter()
            .position(|index| *index == current_index)
            .unwrap_or(0);
        self.shuffle_cursor = Some(cursor);
        cursor
    }

    fn rebuild_next_shuffle_cycle_after(&mut self, current_index: usize) -> Option<usize> {
        if self.queue.is_empty() {
            self.shuffle_order.clear();
            self.shuffle_cursor = None;
            return None;
        }

        let previous_index = current_index.min(self.queue.len() - 1);
        let mut next_order = (0..self.queue.len()).collect::<Vec<_>>();
        self.shuffle_rng.shuffle(&mut next_order);
        if next_order.len() > 1 && next_order.first().copied() == Some(previous_index) {
            let swap_with = next_order
                .iter()
                .position(|index| *index != previous_index)
                .unwrap_or(0);
            next_order.swap(0, swap_with);
        }

        self.shuffle_order = next_order;
        self.shuffle_cursor = Some(0);
        self.shuffle_order.first().copied()
    }
}

#[derive(Clone, Debug)]
struct ShuffleRng {
    state: u64,
}

impl ShuffleRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    fn seeded_from_time() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0xA5A5_5A5A_D3C3_B4B4);
        Self::new(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    }

    fn shuffle<T>(&mut self, values: &mut [T]) {
        for index in (1..values.len()).rev() {
            let swap_with = (self.next_u64() as usize) % (index + 1);
            values.swap(index, swap_with);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(name: &str) -> Track {
        Track::from_path(format!("{name}.mp3").into())
    }

    #[test]
    fn can_move_through_queue() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session.set_queue(vec![track("a"), track("b")], 0).unwrap();
        session.apply(PlaybackCommand::Play).unwrap();
        assert!(session.state().is_playing);

        session.apply(PlaybackCommand::Next).unwrap();
        assert_eq!(session.current_track().unwrap().title, "b");

        session.apply(PlaybackCommand::Next).unwrap();
        assert!(session.current_track().is_none());
        assert!(!session.state().is_playing);
    }

    #[test]
    fn rejects_invalid_start_index() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        let err = session.set_queue(vec![track("a")], 3).unwrap_err();
        assert!(matches!(
            err,
            PlaybackError::InvalidQueueIndex { index: 3, len: 1 }
        ));
    }

    #[test]
    fn appending_to_an_empty_queue_selects_the_first_track_without_playing() {
        let mut session = PlayerSession::new(NormalizationSettings::default());

        session.append_to_queue(vec![track("a"), track("b")]);

        assert_eq!(session.current_track().unwrap().title, "a");
        assert!(!session.state().is_playing);
    }

    #[test]
    fn inserting_next_keeps_the_current_track_and_places_items_after_it() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session.set_queue(vec![track("a"), track("d")], 0).unwrap();

        session.insert_next(vec![track("b"), track("c")]);

        let titles = session
            .queue()
            .iter()
            .map(|track| track.title.as_str())
            .collect::<Vec<_>>();
        assert_eq!(titles, vec!["a", "b", "c", "d"]);
        assert_eq!(session.current_track().unwrap().title, "a");
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

        session.move_queue_item(0, 2).unwrap();
        assert_eq!(session.current_track().unwrap().title, "b");
        assert_eq!(session.state().current_index, Some(2));
    }

    #[test]
    fn removing_the_current_item_selects_its_successor_or_previous_tail() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session
            .set_queue(vec![track("a"), track("b"), track("c")], 1)
            .unwrap();

        let removed = session.remove_queue_item(1).unwrap();
        assert_eq!(removed.title, "b");
        assert_eq!(session.current_track().unwrap().title, "c");

        session.remove_queue_item(1).unwrap();
        assert_eq!(session.current_track().unwrap().title, "a");
    }

    #[test]
    fn queue_edits_rebuild_shuffle_indices_and_clear_resets_playback() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session
            .set_queue(vec![track("a"), track("b"), track("c")], 1)
            .unwrap();
        session.set_shuffle(true);
        session.apply(PlaybackCommand::Play).unwrap();

        session.append_to_queue(vec![track("d")]);
        let mut order = session.shuffle_order.clone();
        order.sort_unstable();
        assert_eq!(order, vec![0, 1, 2, 3]);
        let cursor = session.shuffle_cursor.unwrap();
        assert_eq!(session.shuffle_order[cursor], 1);

        session.clear_queue();
        assert!(session.queue().is_empty());
        assert!(session.current_track().is_none());
        assert!(!session.state().is_playing);
        assert!(session.shuffle_order.is_empty());
        assert!(session.shuffle_cursor.is_none());
    }

    #[test]
    fn repeat_all_wraps_to_start() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session.set_queue(vec![track("a"), track("b")], 1).unwrap();
        session.state.repeat_mode = RepeatMode::All;

        session.apply(PlaybackCommand::Next).unwrap();

        assert_eq!(session.current_track().unwrap().title, "a");
    }

    #[test]
    fn repeat_one_stays_on_same_track() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session.set_queue(vec![track("a")], 0).unwrap();
        session.set_repeat_mode(RepeatMode::One);

        session.apply(PlaybackCommand::Next).unwrap();

        assert_eq!(session.current_track().unwrap().title, "a");
    }

    #[test]
    fn shuffle_visits_every_track_once_before_stopping() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session.shuffle_rng = ShuffleRng::new(42);
        session
            .set_queue(vec![track("a"), track("b"), track("c"), track("d")], 0)
            .unwrap();
        session.set_shuffle(true);
        session.apply(PlaybackCommand::Play).unwrap();

        let mut visited = vec![session.current_track().unwrap().title.clone()];
        while session.current_track().is_some() {
            session.apply(PlaybackCommand::Next).unwrap();
            if let Some(track) = session.current_track() {
                visited.push(track.title.clone());
            }
        }

        visited.sort();
        assert_eq!(visited, vec!["a", "b", "c", "d"]);
        assert!(!session.state().is_playing);
    }

    #[test]
    fn shuffle_repeat_all_starts_a_new_random_cycle_without_repeating_current_immediately() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session.shuffle_rng = ShuffleRng::new(7);
        session
            .set_queue(vec![track("a"), track("b"), track("c")], 0)
            .unwrap();
        session.set_shuffle(true);
        session.set_repeat_mode(RepeatMode::All);

        let mut last = session.current_track().unwrap().title.clone();
        for _ in 0..8 {
            session.apply(PlaybackCommand::Next).unwrap();
            let current = session.current_track().unwrap().title.clone();
            assert_ne!(current, last);
            last = current;
        }
    }

    #[test]
    fn shuffle_repeat_all_keeps_every_track_balanced_over_many_cycles() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session.shuffle_rng = ShuffleRng::new(0xFA17);
        session
            .set_queue(
                (0..10)
                    .map(|index| track(&format!("song-{index}")))
                    .collect(),
                0,
            )
            .unwrap();
        session.set_shuffle(true);
        session.set_repeat_mode(RepeatMode::All);

        let mut counts = [0_usize; 10];
        let mut last_title = None;
        for _ in 0..1_000 {
            let title = session.current_track().unwrap().title.clone();
            assert_ne!(Some(title.as_str()), last_title.as_deref());
            let index = title
                .strip_prefix("song-")
                .unwrap()
                .parse::<usize>()
                .unwrap();
            counts[index] += 1;
            last_title = Some(title);
            session.apply(PlaybackCommand::Next).unwrap();
        }

        assert_eq!(counts, [100_usize; 10]);
    }

    #[test]
    fn shuffle_previous_follows_recent_shuffle_order() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session.shuffle_rng = ShuffleRng::new(99);
        session
            .set_queue(vec![track("a"), track("b"), track("c"), track("d")], 0)
            .unwrap();
        session.set_shuffle(true);

        let first = session.current_track().unwrap().title.clone();
        session.apply(PlaybackCommand::Next).unwrap();
        let second = session.current_track().unwrap().title.clone();
        assert_ne!(first, second);

        session
            .apply(PlaybackCommand::SeekTo { position_ms: 500 })
            .unwrap();
        session.apply(PlaybackCommand::Previous).unwrap();

        assert_eq!(session.current_track().unwrap().title, first);
    }

    #[test]
    fn disabling_shuffle_resumes_linear_order_from_current_track() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session.shuffle_rng = ShuffleRng::new(101);
        session
            .set_queue(vec![track("a"), track("b"), track("c")], 1)
            .unwrap();
        session.set_shuffle(true);
        session.set_shuffle(false);

        session.apply(PlaybackCommand::Next).unwrap();

        assert_eq!(session.current_track().unwrap().title, "c");
    }

    #[test]
    fn previous_wraps_to_queue_end_when_repeat_all_is_enabled() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session.set_queue(vec![track("a"), track("b")], 0).unwrap();
        session.set_repeat_mode(RepeatMode::All);

        session.apply(PlaybackCommand::Previous).unwrap();

        assert_eq!(session.current_track().unwrap().title, "b");
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
    fn previous_moves_back_near_track_start() {
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session.set_queue(vec![track("a"), track("b")], 1).unwrap();
        session
            .apply(PlaybackCommand::SeekTo { position_ms: 500 })
            .unwrap();

        session.apply(PlaybackCommand::Previous).unwrap();

        assert_eq!(session.current_track().unwrap().title, "a");
    }

    #[test]
    fn exposes_current_gain() {
        let mut track = track("quiet");
        track.loudness = Some(crate::model::LoudnessInfo::track(-20.0, -8.0));
        let mut session = PlayerSession::new(NormalizationSettings::default());
        session.set_queue(vec![track], 0).unwrap();

        let gain = session.current_gain().unwrap();

        assert_eq!(gain.gain_db, 4.0);
    }
}
