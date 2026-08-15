use std::collections::{HashMap, VecDeque};

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::RngExt;
#[cfg(test)]
use rand::SeedableRng;

use crate::{PlaybackError, PlaybackResult, Track};

const SHUFFLE_HISTORY_CYCLES: usize = 1;
const SHUFFLE_FUTURE_CYCLES: usize = 2;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct QueueItemId(u64);

impl QueueItemId {
    pub fn from_value(value: u64) -> Self {
        Self(value)
    }

    pub fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PlaybackMode {
    RepeatOne,
    #[default]
    Sequential,
    Shuffle,
}

impl PlaybackMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RepeatOne => "repeat_one",
            Self::Sequential => "sequential",
            Self::Shuffle => "shuffle",
        }
    }

    pub fn parse(value: &str) -> PlaybackResult<Self> {
        match value {
            "repeat_one" => Ok(Self::RepeatOne),
            "sequential" => Ok(Self::Sequential),
            "shuffle" => Ok(Self::Shuffle),
            _ => Err(PlaybackError::InvalidQueueState(format!(
                "unknown playback mode `{value}`"
            ))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShuffleQueueSnapshot {
    pub cycles: Vec<Vec<QueueItemId>>,
    pub active_cycle: usize,
    pub position: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalQueueSnapshot {
    pub ordered_ids: Vec<QueueItemId>,
    pub next_internal_id: u64,
    pub current_id: Option<QueueItemId>,
    pub mode: PlaybackMode,
    pub shuffle_activation_pending: bool,
    pub shuffle: Option<ShuffleQueueSnapshot>,
}

#[derive(Clone, Debug)]
struct QueueItem {
    id: QueueItemId,
    track: Track,
}

#[derive(Clone, Debug)]
struct ShuffleCycle {
    order: Vec<QueueItemId>,
    position_by_id: HashMap<QueueItemId, usize>,
}

impl ShuffleCycle {
    fn new(order: Vec<QueueItemId>) -> Self {
        let mut cycle = Self {
            order,
            position_by_id: HashMap::new(),
        };
        cycle.reindex();
        cycle
    }

    fn reindex(&mut self) {
        self.position_by_id.clear();
        self.position_by_id.extend(
            self.order
                .iter()
                .copied()
                .enumerate()
                .map(|(position, id)| (id, position)),
        );
    }
}

#[derive(Clone, Debug, Default)]
struct ShuffleQueue {
    cycles: VecDeque<ShuffleCycle>,
    active_cycle: usize,
    position: usize,
}

impl ShuffleQueue {
    fn clear(&mut self) {
        self.cycles.clear();
        self.active_cycle = 0;
        self.position = 0;
    }

    fn snapshot(&self) -> Option<ShuffleQueueSnapshot> {
        if self.cycles.is_empty() {
            return None;
        }
        Some(ShuffleQueueSnapshot {
            cycles: self
                .cycles
                .iter()
                .map(|cycle| cycle.order.clone())
                .collect(),
            active_cycle: self.active_cycle,
            position: self.position,
        })
    }

    fn active(&self) -> Option<&ShuffleCycle> {
        self.cycles.get(self.active_cycle)
    }

    fn current_id(&self) -> Option<QueueItemId> {
        self.active()?.order.get(self.position).copied()
    }
}

#[derive(Debug)]
pub struct GlobalPlaybackQueue {
    items: Vec<QueueItem>,
    index_by_id: HashMap<QueueItemId, usize>,
    id_by_track: HashMap<String, QueueItemId>,
    next_internal_id: u64,
    current_id: Option<QueueItemId>,
    mode: PlaybackMode,
    shuffle_activation_pending: bool,
    shuffle: ShuffleQueue,
    rng: StdRng,
}

impl Default for GlobalPlaybackQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalPlaybackQueue {
    pub fn new() -> Self {
        Self::with_rng(rand::make_rng())
    }

    #[cfg(test)]
    fn seeded(seed: u64) -> Self {
        Self::with_rng(StdRng::seed_from_u64(seed))
    }

    fn with_rng(rng: StdRng) -> Self {
        Self {
            items: Vec::new(),
            index_by_id: HashMap::new(),
            id_by_track: HashMap::new(),
            next_internal_id: 1,
            current_id: None,
            mode: PlaybackMode::Sequential,
            shuffle_activation_pending: false,
            shuffle: ShuffleQueue::default(),
            rng,
        }
    }

    pub fn tracks(&self) -> impl ExactSizeIterator<Item = &Track> {
        self.items.iter().map(|item| &item.track)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn mode(&self) -> PlaybackMode {
        self.mode
    }

    pub fn current_id(&self) -> Option<QueueItemId> {
        self.current_id
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current_id
            .and_then(|current_id| self.index_by_id.get(&current_id).copied())
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.current_index()
            .and_then(|index| self.items.get(index))
            .map(|item| &item.track)
    }

    pub fn ordered_ids(&self) -> Vec<QueueItemId> {
        self.items.iter().map(|item| item.id).collect()
    }

    pub fn queue_items(&self) -> Vec<(QueueItemId, &Track)> {
        self.items
            .iter()
            .map(|item| (item.id, &item.track))
            .collect()
    }

    pub fn playback_order(&self) -> Vec<usize> {
        if self.mode == PlaybackMode::Shuffle && !self.shuffle_activation_pending {
            if let Some(cycle) = self.shuffle.active() {
                return cycle
                    .order
                    .iter()
                    .filter_map(|id| self.index_by_id.get(id).copied())
                    .collect();
            }
        }
        (0..self.items.len()).collect()
    }

    pub fn playback_position(&self) -> Option<usize> {
        self.current_id?;
        if self.mode == PlaybackMode::Shuffle && !self.shuffle_activation_pending {
            return Some(self.shuffle.position);
        }
        self.current_index()
    }

    pub fn snapshot(&self) -> GlobalQueueSnapshot {
        GlobalQueueSnapshot {
            ordered_ids: self.ordered_ids(),
            next_internal_id: self.next_internal_id,
            current_id: self.current_id,
            mode: self.mode,
            shuffle_activation_pending: self.shuffle_activation_pending,
            shuffle: self.shuffle.snapshot(),
        }
    }

    pub fn replace(&mut self, tracks: Vec<Track>, start_index: usize) -> PlaybackResult<()> {
        if tracks.is_empty() {
            self.clear();
            return Ok(());
        }
        if start_index >= tracks.len() {
            return Err(PlaybackError::InvalidQueueIndex {
                index: start_index,
                len: tracks.len(),
            });
        }
        let selected_identity = track_identity(&tracks[start_index]);
        let tracks = deduplicate_tracks(tracks);
        let start_index = tracks
            .iter()
            .position(|track| track_identity(track) == selected_identity)
            .expect("the selected track survives de-duplication");

        self.items.clear();
        self.next_internal_id = 1;
        for track in tracks {
            let id = self.allocate_id();
            self.items.push(QueueItem { id, track });
        }
        self.reindex();
        self.current_id = self.items.get(start_index).map(|item| item.id);
        self.shuffle.clear();
        self.shuffle_activation_pending = self.mode == PlaybackMode::Shuffle;
        Ok(())
    }

    pub fn restore(
        &mut self,
        tracks: Vec<Track>,
        snapshot: GlobalQueueSnapshot,
    ) -> PlaybackResult<()> {
        if tracks.len() != snapshot.ordered_ids.len() {
            return Err(PlaybackError::InvalidQueueState(
                "persisted queue IDs do not match track count".to_owned(),
            ));
        }
        let mut unique_ids = snapshot.ordered_ids.clone();
        unique_ids.sort_unstable_by_key(|id| id.value());
        unique_ids.dedup();
        if unique_ids.len() != snapshot.ordered_ids.len() {
            return Err(PlaybackError::InvalidQueueState(
                "persisted queue contains duplicate internal IDs".to_owned(),
            ));
        }

        self.items = snapshot
            .ordered_ids
            .iter()
            .copied()
            .zip(tracks)
            .map(|(id, track)| QueueItem { id, track })
            .collect();
        self.reindex();
        if snapshot
            .current_id
            .is_some_and(|id| !self.index_by_id.contains_key(&id))
        {
            return Err(PlaybackError::InvalidQueueState(
                "persisted current item is not in the queue".to_owned(),
            ));
        }
        self.next_internal_id = snapshot.next_internal_id.max(
            unique_ids
                .last()
                .map_or(1, |id| id.value().saturating_add(1)),
        );
        self.current_id = snapshot.current_id;
        self.mode = snapshot.mode;
        self.shuffle_activation_pending = snapshot.shuffle_activation_pending;
        self.shuffle = self.restore_shuffle(snapshot.shuffle)?;
        if self.mode == PlaybackMode::Shuffle
            && !self.shuffle_activation_pending
            && self.shuffle.current_id() != self.current_id
        {
            return Err(PlaybackError::InvalidQueueState(
                "persisted shuffle cursor does not match current item".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn append(&mut self, tracks: Vec<Track>) -> Vec<QueueItemId> {
        let mut added = Vec::new();
        for track in tracks {
            let identity = track_identity(&track);
            if self.id_by_track.contains_key(&identity) {
                continue;
            }
            let id = self.allocate_id();
            self.id_by_track.insert(identity, id);
            self.index_by_id.insert(id, self.items.len());
            self.items.push(QueueItem { id, track });
            added.push(id);
        }
        if self.current_id.is_none() {
            self.current_id = self.items.first().map(|item| item.id);
        }
        if !added.is_empty() {
            self.add_ids_to_shuffle_cycles(&added);
        }
        added
    }

    pub fn insert_next(&mut self, tracks: Vec<Track>) {
        let mut ids = Vec::new();
        for track in tracks {
            let identity = track_identity(&track);
            let id = if let Some(id) = self.id_by_track.get(&identity).copied() {
                id
            } else {
                let id = self.allocate_id();
                self.id_by_track.insert(identity, id);
                self.items.push(QueueItem { id, track });
                id
            };
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        self.reindex();
        let Some(current) = self.current_id else {
            self.current_id = self.items.first().map(|item| item.id);
            return;
        };
        ids.retain(|id| *id != current);
        if ids.is_empty() {
            return;
        }

        if self.mode == PlaybackMode::Shuffle && !self.shuffle_activation_pending {
            self.insert_ids_after_shuffle_cursor(&ids);
        } else {
            self.move_ids_after_current(&ids);
        }
    }

    pub fn move_item(&mut self, from: usize, to: usize) -> PlaybackResult<()> {
        let len = self.items.len();
        if from >= len {
            return Err(PlaybackError::InvalidQueueIndex { index: from, len });
        }
        if to >= len {
            return Err(PlaybackError::InvalidQueueIndex { index: to, len });
        }
        if from != to {
            let item = self.items.remove(from);
            self.items.insert(to, item);
            self.reindex();
        }
        Ok(())
    }

    pub fn remove(&mut self, index: usize) -> PlaybackResult<Track> {
        let len = self.items.len();
        if index >= len {
            return Err(PlaybackError::InvalidQueueIndex { index, len });
        }
        let removed = self.items.remove(index);
        let removed_current = self.current_id == Some(removed.id);
        self.remove_id_from_shuffle(removed.id);
        self.reindex();
        if self.items.is_empty() {
            self.clear();
        } else if removed_current {
            self.current_id =
                if self.mode == PlaybackMode::Shuffle && !self.shuffle_activation_pending {
                    self.shuffle.current_id()
                } else {
                    self.items
                        .get(index.min(self.items.len() - 1))
                        .map(|item| item.id)
                };
        }
        Ok(removed.track)
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.index_by_id.clear();
        self.id_by_track.clear();
        self.current_id = None;
        self.shuffle.clear();
        self.shuffle_activation_pending = false;
    }

    pub fn set_mode(&mut self, mode: PlaybackMode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        self.shuffle_activation_pending = mode == PlaybackMode::Shuffle;
    }

    pub fn start_shuffled(&mut self) -> PlaybackResult<()> {
        if self.items.is_empty() {
            return Err(PlaybackError::EmptyQueue);
        }
        self.mode = PlaybackMode::Shuffle;
        self.shuffle_activation_pending = false;
        self.shuffle.clear();
        let first_cycle = self.random_cycle(None);
        self.shuffle.cycles.push_back(first_cycle);
        self.shuffle.active_cycle = 0;
        self.shuffle.position = 0;
        self.ensure_shuffle_lookahead();
        self.current_id = self.shuffle.current_id();
        Ok(())
    }

    pub fn select_index(&mut self, index: usize) -> PlaybackResult<()> {
        let Some(item) = self.items.get(index) else {
            return Err(PlaybackError::InvalidQueueIndex {
                index,
                len: self.items.len(),
            });
        };
        self.current_id = Some(item.id);
        if self.mode == PlaybackMode::Shuffle && !self.shuffle_activation_pending {
            self.jump_shuffle_to(item.id);
        }
        Ok(())
    }

    pub fn advance(&mut self) -> PlaybackResult<()> {
        let current = self.current_id.ok_or(PlaybackError::EmptyQueue)?;
        self.current_id = match self.mode {
            PlaybackMode::RepeatOne => Some(current),
            PlaybackMode::Sequential => self.ordered_neighbor(current, true),
            PlaybackMode::Shuffle => {
                self.activate_shuffle_from_current_if_needed(current);
                self.advance_shuffle()
            }
        };
        Ok(())
    }

    pub fn rewind(&mut self) -> PlaybackResult<()> {
        let current = self.current_id.ok_or(PlaybackError::EmptyQueue)?;
        self.current_id = match self.mode {
            PlaybackMode::RepeatOne => Some(current),
            PlaybackMode::Sequential => self.ordered_neighbor(current, false),
            PlaybackMode::Shuffle if self.shuffle_activation_pending => Some(current),
            PlaybackMode::Shuffle => self.rewind_shuffle().or(Some(current)),
        };
        Ok(())
    }

    fn restore_shuffle(
        &self,
        snapshot: Option<ShuffleQueueSnapshot>,
    ) -> PlaybackResult<ShuffleQueue> {
        let Some(snapshot) = snapshot else {
            return Ok(ShuffleQueue::default());
        };
        if snapshot.active_cycle >= snapshot.cycles.len() {
            return Err(PlaybackError::InvalidQueueState(
                "persisted shuffle cycle is out of bounds".to_owned(),
            ));
        }
        let ordered_ids = self.ordered_ids();
        let mut cycles = VecDeque::new();
        for order in snapshot.cycles {
            if !is_permutation(&order, &ordered_ids) {
                return Err(PlaybackError::InvalidQueueState(
                    "persisted shuffle cycle is not a queue permutation".to_owned(),
                ));
            }
            cycles.push_back(ShuffleCycle::new(order));
        }
        if snapshot.position >= cycles[snapshot.active_cycle].order.len() {
            return Err(PlaybackError::InvalidQueueState(
                "persisted shuffle position is out of bounds".to_owned(),
            ));
        }
        Ok(ShuffleQueue {
            cycles,
            active_cycle: snapshot.active_cycle,
            position: snapshot.position,
        })
    }

    fn allocate_id(&mut self) -> QueueItemId {
        let id = QueueItemId(self.next_internal_id);
        self.next_internal_id = self.next_internal_id.saturating_add(1);
        id
    }

    fn reindex(&mut self) {
        self.index_by_id.clear();
        self.id_by_track.clear();
        for (index, item) in self.items.iter().enumerate() {
            self.index_by_id.insert(item.id, index);
            self.id_by_track
                .insert(track_identity(&item.track), item.id);
        }
    }

    fn ordered_neighbor(&self, current: QueueItemId, forward: bool) -> Option<QueueItemId> {
        let index = self.index_by_id.get(&current).copied()?;
        let next = if forward {
            (index + 1) % self.items.len()
        } else if index == 0 {
            self.items.len() - 1
        } else {
            index - 1
        };
        self.items.get(next).map(|item| item.id)
    }

    fn activate_shuffle_from_current_if_needed(&mut self, current: QueueItemId) {
        if !self.shuffle_activation_pending && self.shuffle.current_id() == Some(current) {
            return;
        }
        self.shuffle.clear();
        let mut rest = self.ordered_ids();
        rest.retain(|id| *id != current);
        rest.shuffle(&mut self.rng);
        let mut current_cycle = Vec::with_capacity(self.items.len());
        current_cycle.push(current);
        current_cycle.extend(rest);
        self.shuffle
            .cycles
            .push_back(ShuffleCycle::new(current_cycle));
        self.shuffle.active_cycle = 0;
        self.shuffle.position = 0;
        self.shuffle_activation_pending = false;
        self.ensure_shuffle_lookahead();
    }

    fn random_cycle(&mut self, avoid_first: Option<QueueItemId>) -> ShuffleCycle {
        let ids = self.ordered_ids();
        if ids.len() <= 1 {
            return ShuffleCycle::new(ids);
        }
        let order = if let Some(avoid) = avoid_first {
            let mut eligible_first = ids
                .iter()
                .copied()
                .filter(|id| *id != avoid)
                .collect::<Vec<_>>();
            eligible_first.shuffle(&mut self.rng);
            let first = eligible_first[0];
            let mut rest = ids
                .into_iter()
                .filter(|id| *id != first)
                .collect::<Vec<_>>();
            rest.shuffle(&mut self.rng);
            let mut order = Vec::with_capacity(rest.len() + 1);
            order.push(first);
            order.extend(rest);
            order
        } else {
            let mut order = ids;
            order.shuffle(&mut self.rng);
            order
        };
        ShuffleCycle::new(order)
    }

    fn ensure_shuffle_lookahead(&mut self) {
        while self
            .shuffle
            .cycles
            .len()
            .saturating_sub(self.shuffle.active_cycle + 1)
            < SHUFFLE_FUTURE_CYCLES
        {
            let avoid = self
                .shuffle
                .cycles
                .back()
                .and_then(|cycle| cycle.order.last())
                .copied();
            let cycle = self.random_cycle(avoid);
            self.shuffle.cycles.push_back(cycle);
        }
    }

    fn advance_shuffle(&mut self) -> Option<QueueItemId> {
        let active_len = self.shuffle.active()?.order.len();
        if self.shuffle.position + 1 < active_len {
            self.shuffle.position += 1;
        } else {
            self.shuffle.active_cycle += 1;
            self.shuffle.position = 0;
            self.ensure_shuffle_lookahead();
            while self.shuffle.active_cycle > SHUFFLE_HISTORY_CYCLES {
                self.shuffle.cycles.pop_front();
                self.shuffle.active_cycle -= 1;
            }
        }
        self.shuffle.current_id()
    }

    fn rewind_shuffle(&mut self) -> Option<QueueItemId> {
        if self.shuffle.position > 0 {
            self.shuffle.position -= 1;
        } else if self.shuffle.active_cycle > 0 {
            self.shuffle.active_cycle -= 1;
            self.shuffle.position = self.shuffle.active()?.order.len().saturating_sub(1);
        } else {
            return None;
        }
        self.shuffle.current_id()
    }

    fn jump_shuffle_to(&mut self, id: QueueItemId) {
        for cycle_index in self.shuffle.active_cycle..self.shuffle.cycles.len() {
            let Some(position) = self.shuffle.cycles[cycle_index]
                .position_by_id
                .get(&id)
                .copied()
            else {
                continue;
            };
            if cycle_index > self.shuffle.active_cycle || position >= self.shuffle.position {
                self.shuffle.active_cycle = cycle_index;
                self.shuffle.position = position;
                return;
            }
        }
    }

    fn add_ids_to_shuffle_cycles(&mut self, ids: &[QueueItemId]) {
        if self.shuffle.cycles.is_empty() {
            return;
        }
        for cycle_index in self.shuffle.active_cycle..self.shuffle.cycles.len() {
            let minimum = if cycle_index == self.shuffle.active_cycle {
                (self.shuffle.position + 1).min(self.shuffle.cycles[cycle_index].order.len())
            } else {
                0
            };
            for id in ids {
                let maximum = self.shuffle.cycles[cycle_index].order.len();
                let position = self.rng.random_range(minimum..=maximum);
                self.shuffle.cycles[cycle_index].order.insert(position, *id);
            }
            self.shuffle.cycles[cycle_index].reindex();
        }
    }

    fn insert_ids_after_shuffle_cursor(&mut self, ids: &[QueueItemId]) {
        let Some(cycle) = self.shuffle.cycles.get_mut(self.shuffle.active_cycle) else {
            return;
        };
        let mut insert_at = self.shuffle.position + 1;
        for id in ids {
            if let Some(position) = cycle.position_by_id.get(id).copied() {
                if position > self.shuffle.position {
                    cycle.order.remove(position);
                    if position < insert_at {
                        insert_at -= 1;
                    }
                }
            }
            cycle.order.insert(insert_at, *id);
            insert_at += 1;
            cycle.reindex();
        }
    }

    fn move_ids_after_current(&mut self, ids: &[QueueItemId]) {
        let Some(current) = self.current_id else {
            return;
        };
        let Some(mut current_index) = self.index_by_id.get(&current).copied() else {
            return;
        };
        for id in ids {
            let Some(index) = self.items.iter().position(|item| item.id == *id) else {
                continue;
            };
            let item = self.items.remove(index);
            if index < current_index {
                current_index -= 1;
            }
            current_index += 1;
            self.items.insert(current_index, item);
        }
        self.reindex();
    }

    fn remove_id_from_shuffle(&mut self, id: QueueItemId) {
        for cycle_index in 0..self.shuffle.cycles.len() {
            let cycle = &mut self.shuffle.cycles[cycle_index];
            if let Some(position) = cycle.position_by_id.get(&id).copied() {
                cycle.order.remove(position);
                if cycle_index == self.shuffle.active_cycle
                    && position < self.shuffle.position
                    && self.shuffle.position > 0
                {
                    self.shuffle.position -= 1;
                }
                cycle.reindex();
            }
        }
        self.shuffle.cycles.retain(|cycle| !cycle.order.is_empty());
        if self.shuffle.cycles.is_empty() {
            self.shuffle.clear();
        } else {
            self.shuffle.active_cycle = self
                .shuffle
                .active_cycle
                .min(self.shuffle.cycles.len().saturating_sub(1));
            self.shuffle.position = self.shuffle.position.min(
                self.shuffle
                    .active()
                    .map_or(0, |cycle| cycle.order.len() - 1),
            );
        }
    }
}

fn track_identity(track: &Track) -> String {
    track.primary_view_id.value().to_owned()
}

fn deduplicate_tracks(tracks: Vec<Track>) -> Vec<Track> {
    let mut seen = HashMap::new();
    tracks
        .into_iter()
        .filter(|track| seen.insert(track_identity(track), ()).is_none())
        .collect()
}

fn is_permutation(left: &[QueueItemId], right: &[QueueItemId]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut left = left.to_vec();
    let mut right = right.to_vec();
    left.sort_unstable_by_key(|id| id.value());
    right.sort_unstable_by_key(|id| id.value());
    left == right
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(name: &str) -> Track {
        Track::from_path(format!("{name}.mp3").into())
    }

    fn titles(queue: &GlobalPlaybackQueue) -> Vec<&str> {
        queue.tracks().map(|track| track.title.as_str()).collect()
    }

    #[test]
    fn internal_ids_preserve_order_and_are_not_reused() {
        let mut queue = GlobalPlaybackQueue::seeded(1);
        queue.replace(vec![track("a"), track("b")], 0).unwrap();
        let first_ids = queue.ordered_ids();
        queue.remove(1).unwrap();
        queue.append(vec![track("c")]);

        assert_eq!(queue.ordered_ids()[0], first_ids[0]);
        assert!(queue.ordered_ids()[1].value() > first_ids[1].value());
        assert_eq!(titles(&queue), vec!["a", "c"]);
    }

    #[test]
    fn replacing_with_duplicates_keeps_the_selected_track() {
        let mut queue = GlobalPlaybackQueue::seeded(9);
        queue
            .replace(vec![track("a"), track("b"), track("a")], 2)
            .unwrap();

        assert_eq!(titles(&queue), vec!["a", "b"]);
        assert_eq!(queue.current_track().unwrap().title, "a");
    }

    #[test]
    fn play_next_deduplicates_new_tracks_within_the_same_request() {
        let mut queue = GlobalPlaybackQueue::seeded(10);
        queue.replace(vec![track("a")], 0).unwrap();

        queue.insert_next(vec![track("b"), track("b")]);

        assert_eq!(titles(&queue), vec!["a", "b"]);
        assert_eq!(queue.ordered_ids().len(), 2);
    }

    #[test]
    fn play_next_is_a_no_op_for_the_current_track() {
        let mut queue = GlobalPlaybackQueue::seeded(11);
        queue.replace(vec![track("a"), track("b")], 0).unwrap();
        queue.start_shuffled().unwrap();
        let before = queue.snapshot();
        let current = queue.current_track().unwrap().clone();

        queue.insert_next(vec![current]);

        assert_eq!(queue.snapshot(), before);
    }

    #[test]
    fn sequential_mode_is_a_global_cycle() {
        let mut queue = GlobalPlaybackQueue::seeded(2);
        queue
            .replace(vec![track("a"), track("b"), track("c")], 2)
            .unwrap();
        queue.advance().unwrap();
        assert_eq!(queue.current_track().unwrap().title, "a");
        queue.rewind().unwrap();
        assert_eq!(queue.current_track().unwrap().title, "c");
    }

    #[test]
    fn shuffle_materializes_multiple_complete_cycles() {
        let mut queue = GlobalPlaybackQueue::seeded(3);
        queue
            .replace(vec![track("a"), track("b"), track("c"), track("d")], 0)
            .unwrap();
        queue.start_shuffled().unwrap();
        let snapshot = queue.snapshot().shuffle.unwrap();

        assert_eq!(snapshot.cycles.len(), 3);
        for cycle in &snapshot.cycles {
            assert!(is_permutation(cycle, &queue.ordered_ids()));
        }
        for pair in snapshot.cycles.windows(2) {
            assert_ne!(pair[0].last(), pair[1].first());
        }
    }

    #[test]
    fn every_track_can_be_the_first_shuffled_track() {
        let mut observed = HashMap::new();
        for seed in 0..10_000 {
            let mut queue = GlobalPlaybackQueue::seeded(seed);
            queue
                .replace(vec![track("a"), track("b"), track("c")], 0)
                .unwrap();
            queue.start_shuffled().unwrap();
            observed.insert(queue.current_track().unwrap().title.clone(), ());
            if observed.len() == 3 {
                break;
            }
        }
        assert_eq!(observed.len(), 3);
    }

    #[test]
    fn shuffle_mode_switch_is_deferred_until_next() {
        let mut queue = GlobalPlaybackQueue::seeded(4);
        queue
            .replace(vec![track("a"), track("b"), track("c")], 0)
            .unwrap();
        queue.set_mode(PlaybackMode::Shuffle);

        assert!(queue.snapshot().shuffle.is_none());
        assert_eq!(queue.current_track().unwrap().title, "a");

        queue.advance().unwrap();
        let snapshot = queue.snapshot();
        assert!(!snapshot.shuffle_activation_pending);
        assert_eq!(snapshot.shuffle.unwrap().cycles.len(), 3);
        assert_ne!(queue.current_track().unwrap().title, "a");
    }

    #[test]
    fn repeat_one_does_not_advance_either_route() {
        let mut queue = GlobalPlaybackQueue::seeded(5);
        queue.replace(vec![track("a"), track("b")], 0).unwrap();
        queue.start_shuffled().unwrap();
        let current = queue.current_id();
        let shuffle = queue.snapshot().shuffle;
        queue.set_mode(PlaybackMode::RepeatOne);
        queue.advance().unwrap();

        assert_eq!(queue.current_id(), current);
        assert_eq!(queue.snapshot().shuffle, shuffle);
    }

    #[test]
    fn persisted_internal_order_and_shuffle_route_restore_exactly() {
        let mut queue = GlobalPlaybackQueue::seeded(6);
        let tracks = vec![track("a"), track("b"), track("c")];
        queue.replace(tracks.clone(), 0).unwrap();
        queue.start_shuffled().unwrap();
        queue.advance().unwrap();
        queue.advance().unwrap();
        let snapshot = queue.snapshot();
        let expected_current = queue.current_track().unwrap().title.clone();

        let mut restored = GlobalPlaybackQueue::seeded(999);
        restored.restore(tracks, snapshot.clone()).unwrap();

        assert_eq!(restored.snapshot(), snapshot);
        assert_eq!(restored.current_track().unwrap().title, expected_current);
        queue.advance().unwrap();
        restored.advance().unwrap();
        assert_eq!(restored.current_id(), queue.current_id());
    }

    #[test]
    fn play_next_in_shuffle_moves_the_existing_future_entry() {
        let mut queue = GlobalPlaybackQueue::seeded(7);
        queue
            .replace(vec![track("a"), track("b"), track("c")], 0)
            .unwrap();
        queue.start_shuffled().unwrap();
        let target_index = queue
            .tracks()
            .position(|track| {
                Some(track.title.as_str()) != queue.current_track().map(|t| t.title.as_str())
            })
            .unwrap();
        let target = queue.items[target_index].track.clone();
        let target_id = queue.items[target_index].id;

        queue.insert_next(vec![target]);
        queue.advance().unwrap();

        assert_eq!(queue.current_id(), Some(target_id));
        let active_cycle = &queue.snapshot().shuffle.unwrap().cycles[0];
        assert_eq!(
            active_cycle.iter().filter(|id| **id == target_id).count(),
            1
        );
    }

    #[test]
    fn append_adds_each_new_item_once_to_every_unplayed_cycle() {
        let mut queue = GlobalPlaybackQueue::seeded(8);
        queue.replace(vec![track("a"), track("b")], 0).unwrap();
        queue.start_shuffled().unwrap();
        let added = queue.append(vec![track("c")]);
        let snapshot = queue.snapshot().shuffle.unwrap();

        for cycle in snapshot.cycles {
            assert_eq!(cycle.iter().filter(|id| **id == added[0]).count(), 1);
        }
    }
}
