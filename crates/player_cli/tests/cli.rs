use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use player_core::{LoudnessInfo, Track};
use player_store_sqlite::LibraryStore;

#[test]
fn scan_lists_supported_visible_audio_files() {
    let root = make_temp_dir("scan");
    fs::create_dir(root.join("nested")).unwrap();
    fs::create_dir(root.join(".hidden")).unwrap();
    fs::write(root.join("z.mp3"), []).unwrap();
    fs::write(root.join("nested").join("a.FLAC"), []).unwrap();
    fs::write(root.join(".hidden").join("secret.wav"), []).unwrap();
    fs::write(root.join("cover.png"), []).unwrap();

    let output = player_cli()
        .arg("scan")
        .arg(&root)
        .output()
        .expect("run scan command");
    fs::remove_dir_all(&root).unwrap();

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("found 2 tracks"), "{stdout}");
    assert!(stdout.contains("a.FLAC"), "{stdout}");
    assert!(stdout.contains("z.mp3"), "{stdout}");
    assert!(!stdout.contains("secret.wav"), "{stdout}");
}

#[test]
fn analyze_reports_downloaded_fixture_loudness() {
    let fixture = fixture("into_the_oceans_chorus.ogg");
    let output = player_cli()
        .arg("analyze")
        .arg(&fixture)
        .output()
        .expect("run analyze command");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("analysis sample_rate=44100Hz"), "{stdout}");
    assert!(stdout.contains("channels=2"), "{stdout}");
    assert!(stdout.contains("integrated_lufs="), "{stdout}");
    assert!(stdout.contains("true_peak_dbtp="), "{stdout}");
}

#[test]
fn import_library_and_analyze_library_cache_downloaded_fixtures() {
    let db_path = temp_db_path("library");
    let audio_root = workspace_root().join("test-assets").join("audio");

    let import = player_cli()
        .arg("import")
        .arg(&audio_root)
        .arg("--db")
        .arg(&db_path)
        .output()
        .expect("run import command");
    assert!(
        import.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&import.stderr)
    );
    let stdout = String::from_utf8_lossy(&import.stdout);
    assert!(stdout.contains("imported 3 tracks"), "{stdout}");

    let library = player_cli()
        .arg("library")
        .arg("--db")
        .arg(&db_path)
        .output()
        .expect("run library command");
    assert!(
        library.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&library.stderr)
    );
    let stdout = String::from_utf8_lossy(&library.stdout);
    assert!(stdout.contains("library 3 tracks"), "{stdout}");
    assert!(stdout.contains("into_the_oceans_chorus"), "{stdout}");

    let analyze = player_cli()
        .arg("analyze-library")
        .arg("--db")
        .arg(&db_path)
        .output()
        .expect("run analyze-library command");
    assert!(
        analyze.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&analyze.stderr)
    );
    let stdout = String::from_utf8_lossy(&analyze.stdout);
    assert!(stdout.contains("analysis analyzed=3 failed=0"), "{stdout}");

    let analyze_again = player_cli()
        .arg("analyze-library")
        .arg("--db")
        .arg(&db_path)
        .output()
        .expect("rerun analyze-library command");
    fs::remove_file(&db_path).ok();
    assert!(
        analyze_again.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&analyze_again.stderr)
    );
    let stdout = String::from_utf8_lossy(&analyze_again.stdout);
    assert!(stdout.contains("analysis analyzed=0 failed=0"), "{stdout}");
}

#[test]
fn analyze_albums_updates_cached_album_loudness() {
    let db_path = temp_db_path("albums");
    let mut store = LibraryStore::open(&db_path).unwrap();
    store
        .upsert_tracks(&[
            cached_album_track("/music/a.ogg", -20.0, -3.0, 10_000),
            cached_album_track("/music/b.ogg", -10.0, -2.0, 30_000),
        ])
        .unwrap();
    drop(store);

    let output = player_cli()
        .arg("analyze-albums")
        .arg("--db")
        .arg(&db_path)
        .output()
        .expect("run analyze-albums command");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("album-analysis albums=1 tracks=2 skipped=0"),
        "{stdout}"
    );

    let store = LibraryStore::open(&db_path).unwrap();
    let loaded = store.track_by_path("/music/a.ogg").unwrap().unwrap();
    fs::remove_file(&db_path).ok();
    let loudness = loaded.loudness.unwrap();
    assert!(loudness.album_integrated_lufs.is_some());
    assert_eq!(loudness.album_true_peak_dbtp, Some(-2.0));
}

#[test]
fn collection_and_artwork_commands_work_against_library_db() {
    let db_path = temp_db_path("collections");
    let audio_root = workspace_root().join("test-assets").join("audio");
    let fixture = fixture("into_the_oceans_chorus.ogg");

    let import = player_cli()
        .arg("import")
        .arg(&audio_root)
        .arg("--db")
        .arg(&db_path)
        .output()
        .expect("run import command");
    assert!(
        import.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&import.stderr)
    );

    let search = player_cli()
        .arg("search")
        .arg("oceans")
        .arg("--db")
        .arg(&db_path)
        .output()
        .expect("run search command");
    assert!(
        search.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&search.stderr)
    );
    let stdout = String::from_utf8_lossy(&search.stdout);
    assert!(stdout.contains("search"), "{stdout}");
    assert!(stdout.contains("into_the_oceans_chorus"), "{stdout}");

    assert_command_ok(
        player_cli()
            .arg("playlist-create")
            .arg("Mix")
            .arg("--db")
            .arg(&db_path)
            .output()
            .expect("run playlist-create command"),
    );
    assert_command_ok(
        player_cli()
            .arg("playlist-add")
            .arg("Mix")
            .arg(&fixture)
            .arg("--db")
            .arg(&db_path)
            .output()
            .expect("run playlist-add command"),
    );
    let playlist = player_cli()
        .arg("playlist")
        .arg("Mix")
        .arg("--db")
        .arg(&db_path)
        .output()
        .expect("run playlist command");
    assert!(
        playlist.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&playlist.stderr)
    );
    let stdout = String::from_utf8_lossy(&playlist.stdout);
    assert!(stdout.contains("playlist Mix tracks=1"), "{stdout}");
    assert!(stdout.contains("into_the_oceans_chorus"), "{stdout}");

    assert_command_ok(
        player_cli()
            .arg("favorite")
            .arg(&fixture)
            .arg("--db")
            .arg(&db_path)
            .output()
            .expect("run favorite command"),
    );
    let favorites = player_cli()
        .arg("favorites")
        .arg("--db")
        .arg(&db_path)
        .output()
        .expect("run favorites command");
    assert!(
        favorites.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&favorites.stderr)
    );
    let stdout = String::from_utf8_lossy(&favorites.stdout);
    assert!(stdout.contains("favorites 1 tracks"), "{stdout}");

    assert_command_ok(
        player_cli()
            .arg("history-add")
            .arg(&fixture)
            .arg("--db")
            .arg(&db_path)
            .arg("--position-ms")
            .arg("123")
            .arg("--completed")
            .output()
            .expect("run history-add command"),
    );
    let history = player_cli()
        .arg("history")
        .arg("--db")
        .arg(&db_path)
        .arg("--limit")
        .arg("5")
        .output()
        .expect("run history command");
    assert!(
        history.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&history.stderr)
    );
    let stdout = String::from_utf8_lossy(&history.stdout);
    assert!(stdout.contains("history 1 entries"), "{stdout}");
    assert!(stdout.contains("123ms"), "{stdout}");

    let extract_artwork = player_cli()
        .arg("extract-artwork")
        .arg(&fixture)
        .arg("--db")
        .arg(&db_path)
        .output()
        .expect("run extract-artwork command");
    assert!(
        extract_artwork.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&extract_artwork.stderr)
    );
    let stdout = String::from_utf8_lossy(&extract_artwork.stdout);
    assert!(stdout.contains("artwork saved=0"), "{stdout}");

    let artwork = player_cli()
        .arg("artwork")
        .arg(&fixture)
        .arg("--db")
        .arg(&db_path)
        .output()
        .expect("run artwork command");
    fs::remove_file(&db_path).ok();
    assert!(
        artwork.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&artwork.stderr)
    );
    let stdout = String::from_utf8_lossy(&artwork.stdout);
    assert!(stdout.contains("artwork 0 images"), "{stdout}");
}

#[test]
#[ignore = "requires a working default audio output device"]
fn play_with_manual_loudness_prints_expected_gain_without_analysis() {
    let fixture = fixture("funk_room_reverb.ogg");
    let output = player_cli()
        .arg("play")
        .arg(&fixture)
        .arg("--measured-lufs")
        .arg("-20")
        .arg("--true-peak-dbtp")
        .arg("-6")
        .arg("--target-lufs")
        .arg("-80")
        .arg("--stop-after-ms")
        .arg("50")
        .output()
        .expect("run play command");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("gain_db=-60.00"), "{stdout}");
    assert!(stdout.contains("linear_gain=0.0010"), "{stdout}");
}

#[test]
#[ignore = "requires a working default audio output device and performs EBU R128 analysis first"]
fn play_analyze_uses_downloaded_fixture_and_audio_backend() {
    let fixture = fixture("into_the_oceans_chorus.ogg");
    let output = player_cli()
        .arg("play")
        .arg(&fixture)
        .arg("--analyze")
        .arg("--stop-after-ms")
        .arg("120")
        .output()
        .expect("run play --analyze command");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("analysis integrated_lufs="), "{stdout}");
    assert!(stdout.contains("normalize status=Ready"), "{stdout}");
}

fn player_cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_player_cli"))
}

fn assert_command_ok(output: std::process::Output) {
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn fixture(name: &str) -> PathBuf {
    let path = workspace_root()
        .join("test-assets")
        .join("audio")
        .join(name);
    assert!(path.exists(), "missing fixture: {}", path.display());
    path
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn make_temp_dir(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("player_cli_{prefix}_{nonce}"));
    fs::create_dir(&path).unwrap();
    path
}

fn temp_db_path(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("player_cli_{prefix}_{nonce}.sqlite3"))
}

fn cached_album_track(
    path: &str,
    integrated_lufs: f32,
    true_peak_dbtp: f32,
    duration_ms: u64,
) -> Track {
    let mut track = Track::from_path(path.into());
    track.album = Some("Album".to_owned());
    track.album_artist = Some("Band".to_owned());
    track.duration_ms = Some(duration_ms);
    track.loudness = Some(LoudnessInfo {
        integrated_lufs,
        true_peak_dbtp,
        album_integrated_lufs: None,
        album_true_peak_dbtp: None,
        analysis_version: 1,
    });
    track
}
