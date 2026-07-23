use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use player_analysis_ebur128::ANALYSIS_VERSION;
use player_core::Track;
use player_store_sqlite::LibraryStore;
use serde_json::Value;

#[test]
fn worker_emits_progress_and_updates_loudness_cache() {
    let db_path = temp_db_path("worker");
    let audio_root = workspace_root().join("test-assets").join("audio");
    let tracks = [
        Track::from_path(audio_root.join("into_the_oceans_chorus.ogg")),
        Track::from_path(audio_root.join("funk_room_reverb.ogg")),
    ];

    let mut store = LibraryStore::open(&db_path).unwrap();
    store.upsert_tracks(&tracks).unwrap();
    drop(store);

    let output = Command::new(env!("CARGO_BIN_EXE_player_analyzer"))
        .arg("--db")
        .arg(&db_path)
        .output()
        .expect("run analyzer worker");

    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let events = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(events.first().unwrap()["event"], "started");
    assert_eq!(events.first().unwrap()["total"], 2);
    assert!(events
        .iter()
        .any(|event| event["event"] == "track_finished"));
    let album_finished = events
        .iter()
        .find(|event| event["event"] == "album_finished")
        .expect("album finished event");
    assert!(album_finished.get("album_tracks_updated").is_some());
    assert!(album_finished.get("album_skipped").is_some());
    assert!(album_finished.get("tracks_updated").is_none());
    assert!(album_finished.get("skipped").is_none());
    let finished = events
        .iter()
        .find(|event| event["event"] == "finished")
        .expect("finished event");
    assert_eq!(finished["analyzed"], 2);
    assert_eq!(finished["failed"], 0);

    let store = LibraryStore::open(&db_path).unwrap();
    for track in tracks {
        let loaded = store.track_by_path(track.path).unwrap().unwrap();
        let loudness = loaded.loudness.unwrap();
        assert_eq!(loudness.analysis_version, ANALYSIS_VERSION);
    }

    fs::remove_file(db_path).ok();
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn temp_db_path(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("player_analyzer_{prefix}_{nonce}.sqlite3"))
}
