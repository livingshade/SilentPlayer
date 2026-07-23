use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use player_core::Track;
use player_store_sqlite::LibraryStore;
use serde_json::Value;

#[test]
fn import_worker_emits_progress_and_skips_duplicate_files() {
    let source_dir = temp_dir("source");
    let media_root = temp_dir("media");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::create_dir_all(&media_root).unwrap();
    std::fs::copy(
        fixture("into_the_oceans_chorus.ogg"),
        source_dir.join("first.ogg"),
    )
    .unwrap();
    std::fs::copy(
        fixture("into_the_oceans_chorus.ogg"),
        source_dir.join("second.ogg"),
    )
    .unwrap();
    let db_path = temp_dir("db").join("library.sqlite3");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_player_library_worker"))
        .arg("import")
        .arg("--db")
        .arg(&db_path)
        .arg("--media-root")
        .arg(&media_root)
        .arg("--folder")
        .arg(&source_dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let events = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(events.first().unwrap()["event"], "started");
    let finished = events
        .iter()
        .find(|event| event["event"] == "finished")
        .unwrap();
    assert_eq!(finished["operation"], "import");
    assert_eq!(finished["total"], 2);
    assert_eq!(finished["duplicates_skipped"], 1);

    let store = LibraryStore::open(&db_path).unwrap();
    assert_eq!(store.tracks().unwrap().len(), 1);

    std::fs::remove_dir_all(source_dir).ok();
    std::fs::remove_dir_all(media_root).ok();
    std::fs::remove_dir_all(db_path.parent().unwrap()).ok();
}

#[test]
fn audit_worker_hashes_and_merges_duplicate_audio() {
    let media_root = temp_dir("audit_media");
    std::fs::create_dir_all(&media_root).unwrap();
    let first_path = media_root.join("first.ogg");
    let second_path = media_root.join("second.ogg");
    std::fs::copy(fixture("into_the_oceans_chorus.ogg"), &first_path).unwrap();
    std::fs::copy(fixture("into_the_oceans_chorus.ogg"), &second_path).unwrap();
    let db_path = temp_dir("audit_db").join("library.sqlite3");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();

    {
        let mut first = Track::from_path(first_path.clone());
        first.title = "First".to_owned();
        let mut second = Track::from_path(second_path.clone());
        second.title = "Second".to_owned();
        LibraryStore::open(&db_path)
            .unwrap()
            .upsert_tracks(&[first, second])
            .unwrap();
    }

    let output = Command::new(env!("CARGO_BIN_EXE_player_library_worker"))
        .arg("audit")
        .arg("--db")
        .arg(&db_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let events = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let finished = events
        .iter()
        .find(|event| event["event"] == "finished")
        .unwrap();
    assert_eq!(finished["operation"], "audit");
    assert_eq!(finished["tracks_scanned"], 2);
    assert_eq!(finished["duplicate_groups"], 1);
    assert_eq!(finished["tracks_merged"], 1);

    let tracks = LibraryStore::open(&db_path).unwrap().tracks().unwrap();
    assert_eq!(tracks.len(), 1);
    assert!(tracks[0].audio_hash.is_some());

    std::fs::remove_dir_all(media_root).ok();
    std::fs::remove_dir_all(db_path.parent().unwrap()).ok();
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("test-assets")
        .join("audio")
        .join(name)
}

fn temp_dir(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("player_library_worker_{prefix}_{nonce}"))
}
