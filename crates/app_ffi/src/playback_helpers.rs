use std::collections::HashSet;
use std::path::Path;

use domain::{PlaybackMode, RepeatMode, Track};
use errors::{PlayerError, PlayerResult};
use store_sqlite::LibraryStore;

pub(super) fn library_playback_plan(
    store: &LibraryStore,
    start_path: Option<&Path>,
) -> PlayerResult<(Vec<Track>, usize)> {
    let tracks = store.tracks()?;
    if tracks.is_empty() {
        return Err(PlayerError::invalid_input("library is empty"));
    }

    let start_index = match start_path {
        Some(path) => tracks
            .iter()
            .position(|track| track.path == path)
            .ok_or_else(|| {
                PlayerError::store(format!("track is not in library: {}", path.display()))
            })?,
        None => 0,
    };
    Ok((tracks, start_index))
}

pub(super) fn parse_repeat_mode(value: &str) -> PlayerResult<RepeatMode> {
    match value {
        "off" => Ok(RepeatMode::Off),
        "one" => Ok(RepeatMode::One),
        "all" => Ok(RepeatMode::All),
        other => Err(PlayerError::metadata(format!(
            "unknown repeat mode: {other}"
        ))),
    }
}

pub(super) fn repeat_mode_name(repeat_mode: RepeatMode) -> &'static str {
    match repeat_mode {
        RepeatMode::Off => "off",
        RepeatMode::One => "one",
        RepeatMode::All => "all",
    }
}

pub(super) fn parse_playback_mode(value: &str) -> PlayerResult<PlaybackMode> {
    PlaybackMode::parse(value).map_err(|error| PlayerError::metadata(error.to_string()))
}

pub(super) fn is_valid_queue_order(order: &[usize], queue_len: usize) -> bool {
    if order.len() != queue_len {
        return false;
    }
    let unique = order.iter().copied().collect::<HashSet<_>>();
    unique.len() == queue_len && unique.iter().all(|index| *index < queue_len)
}

pub(super) fn moved_queue_index(current: Option<usize>, from: usize, to: usize) -> Option<usize> {
    current.map(|current| {
        if current == from {
            to
        } else if from < current && to >= current {
            current - 1
        } else if from > current && to <= current {
            current + 1
        } else {
            current
        }
    })
}
