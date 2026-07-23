use std::path::{Path, PathBuf};
use std::time::Duration;

use player_audio_rodio::RodioBackend;
use player_core::{GainDecision, Track};
use player_engine::AudioRenderSettings;

#[test]
#[ignore = "requires a working default audio output device"]
fn plays_downloaded_fixture_quietly() {
    let fixture = workspace_root()
        .join("test-assets")
        .join("audio")
        .join("funk_room_reverb.ogg");
    assert!(fixture.exists(), "missing fixture: {}", fixture.display());

    let track = Track::from_path(fixture);
    let gain = GainDecision::ready(-50.0, false);
    let mut backend = RodioBackend::open_default().unwrap();

    backend
        .play_track_blocking(
            &track,
            AudioRenderSettings::new(0, gain),
            Some(Duration::from_millis(120)),
        )
        .unwrap();
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}
