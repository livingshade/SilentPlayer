use super::*;
use std::collections::{BTreeMap, BTreeSet};

mod analysis;
mod artwork;
mod playback;
mod playlists;
mod schema;
mod tracks;

fn analyzed_album_track(
    path: &str,
    album: &str,
    album_artist: &str,
    track_number: u32,
    integrated_lufs: f32,
) -> Track {
    let mut track = Track::from_path(path.into());
    track.album = Some(album.to_owned());
    track.album_artist = Some(album_artist.to_owned());
    track.track_number = Some(track_number);
    track.duration_ms = Some(60_000);
    track.loudness = Some(LoudnessInfo::track(integrated_lufs, -1.0));
    track
}

fn assert_playlist_paths(store: &LibraryStore, playlist: &str, expected: &[&PathBuf]) {
    let actual = store
        .playlist_tracks(playlist)
        .unwrap()
        .into_iter()
        .map(|entry| entry.track.path)
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        expected
            .iter()
            .map(|path| (*path).clone())
            .collect::<Vec<_>>()
    );
}

fn artwork_image(picture_index: u32, data: Vec<u8>) -> ArtworkImage {
    ArtworkImage {
        picture_index,
        mime_type: Some("image/png".to_owned()),
        picture_type: "CoverFront".to_owned(),
        description: None,
        data,
    }
}

fn artwork_asset_count(store: &LibraryStore) -> i64 {
    store
        .conn
        .query_row("SELECT COUNT(*) FROM artwork_assets", [], |row| row.get(0))
        .unwrap()
}

struct TestRng(u64);

impl TestRng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        self.0
    }

    fn usize(&mut self, upper: usize) -> usize {
        assert!(upper > 0);
        (self.next() as usize) % upper
    }

    fn bool(&mut self) -> bool {
        self.usize(2) == 0
    }
}
