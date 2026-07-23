use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

#[test]
fn root_commands_are_simple_and_complex_commands_require_cli_boundary() {
    let root = make_temp_dir("root");
    let version = silent()
        .current_dir(&root)
        .arg("--version")
        .output()
        .expect("run --version");
    assert_command_ok(&version);
    assert!(
        String::from_utf8_lossy(&version.stdout).starts_with("silent "),
        "{}",
        String::from_utf8_lossy(&version.stdout)
    );
    assert!(!root.join("player_library.sqlite3").exists());
    assert!(!root.join("UserData").exists());

    let rejected = silent()
        .current_dir(&root)
        .args(["library", "list"])
        .output()
        .expect("run command without --cli");
    fs::remove_dir_all(root).ok();
    assert_eq!(rejected.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("silent --cli"));
}

#[test]
fn stateful_commands_require_explicit_paths_and_never_create_on_read() {
    let root = make_temp_dir("explicit_paths");
    let db_path = root.join("missing.sqlite3");
    let media_root = root.join("Music");

    let missing_options = silent()
        .current_dir(&root)
        .args(["--cli", "library", "list"])
        .output()
        .expect("reject missing paths");
    assert_eq!(missing_options.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing_options.stderr).contains("--db"));

    let missing_database = silent_cli(&db_path, &media_root)
        .args(["library", "list"])
        .output()
        .expect("reject missing database");
    assert_eq!(missing_database.status.code(), Some(1));
    assert!(!db_path.exists());

    let misplaced_global = silent()
        .args(["--cli", "library", "list", "--db"])
        .arg(&db_path)
        .arg("--media-root")
        .arg(&media_root)
        .output()
        .expect("reject global options after domain");
    assert_eq!(misplaced_global.status.code(), Some(2));
    assert!(!db_path.exists());

    fs::remove_dir_all(root).ok();
}

#[test]
fn scan_lists_supported_visible_audio_files_as_json() {
    let root = make_temp_dir("scan");
    fs::create_dir(root.join("nested")).unwrap();
    fs::create_dir(root.join(".hidden")).unwrap();
    fs::write(root.join("z.mp3"), []).unwrap();
    fs::write(root.join("nested").join("a.FLAC"), []).unwrap();
    fs::write(root.join(".hidden").join("secret.wav"), []).unwrap();
    fs::write(root.join("cover.png"), []).unwrap();

    let output = silent()
        .args(["--cli", "--output", "json", "library", "scan"])
        .arg(&root)
        .output()
        .expect("run library scan");
    fs::remove_dir_all(&root).unwrap();
    assert_command_ok(&output);
    let tracks = json_output(&output);
    let tracks = tracks.as_array().unwrap();
    assert_eq!(tracks.len(), 2);
    let paths = tracks
        .iter()
        .filter_map(|track| track["path"].as_str())
        .collect::<Vec<_>>();
    assert!(paths.iter().any(|path| path.ends_with("a.FLAC")));
    assert!(paths.iter().any(|path| path.ends_with("z.mp3")));
    assert!(!paths.iter().any(|path| path.ends_with("secret.wav")));
}

#[test]
fn managed_import_search_analysis_and_audit_share_app_semantics() {
    let root = make_temp_dir("library");
    let db_path = root.join("silent.sqlite3");
    let media_root = root.join("Music");
    let audio_root = workspace_root().join("test-assets").join("audio");

    let import = silent_cli(&db_path, &media_root)
        .args(["--output", "json", "library", "import"])
        .arg(&audio_root)
        .output()
        .expect("run managed import");
    assert_command_ok(&import);
    let summary = json_output(&import);
    assert_eq!(summary["imported"], 3);
    assert_eq!(summary["copied"], 3);

    let list = silent_cli(&db_path, &media_root)
        .args(["--output", "json", "library", "list"])
        .output()
        .expect("list library");
    assert_command_ok(&list);
    let tracks = json_output(&list);
    assert_eq!(tracks.as_array().unwrap().len(), 3);
    for track in tracks.as_array().unwrap() {
        assert!(
            Path::new(track["path"].as_str().unwrap()).starts_with(&media_root),
            "{track}"
        );
        assert!(track["view_id"].as_str().unwrap().starts_with("audio:"));
    }

    let search = silent_cli(&db_path, &media_root)
        .args([
            "--output", "json", "library", "search", "oceans", "--limit", "5",
        ])
        .output()
        .expect("search library");
    assert_command_ok(&search);
    assert!(!json_output(&search).as_array().unwrap().is_empty());

    let analyze = silent_cli(&db_path, &media_root)
        .args(["--output", "json", "library", "analyze"])
        .output()
        .expect("analyze library");
    assert_command_ok(&analyze);
    let summary = json_output(&analyze);
    assert_eq!(summary["tracks_analyzed"], 3);
    assert_eq!(summary["track_failures"], 0);

    let audit = silent_cli(&db_path, &media_root)
        .args(["--output", "json", "library", "audit"])
        .output()
        .expect("audit library");
    assert_command_ok(&audit);
    assert_eq!(json_output(&audit)["tracks_scanned"], 3);
    fs::remove_dir_all(root).ok();
}

#[test]
fn music_view_collections_and_playlist_commands_cover_app_mutations() {
    let root = make_temp_dir("mutations");
    let db_path = root.join("silent.sqlite3");
    let media_root = root.join("Music");
    let fixture = fixture("into_the_oceans_chorus.ogg");
    import_one(&db_path, &media_root, &fixture);
    let primary = first_track(&db_path, &media_root);
    let primary_view = primary["view_id"].as_str().unwrap().to_owned();

    let details = silent_cli(&db_path, &media_root)
        .args(["--output", "json", "track", "show", &primary_view])
        .output()
        .expect("show track details");
    assert_command_ok(&details);
    let details = json_output(&details);
    assert_eq!(details["details"]["view_id"], primary_view);
    assert!(details["diagnostics"].is_array());

    let edit = silent_cli(&db_path, &media_root)
        .args([
            "--output",
            "json",
            "track",
            "edit",
            &primary_view,
            "--name",
            "CLI View",
            "--title",
            "CLI Title",
            "--notes",
            "CLI notes",
        ])
        .output()
        .expect("edit track view");
    assert_command_ok(&edit);
    let edited = json_output(&edit);
    assert_eq!(edited["title"], "CLI Title");
    assert_eq!(edited["view_kind"], "derived");
    let edited_view = edited["view_id"].as_str().unwrap().to_owned();

    let rate = silent_cli(&db_path, &media_root)
        .args(["--output", "json", "track", "rate", &edited_view, "8"])
        .output()
        .expect("rate track");
    assert_command_ok(&rate);
    assert_eq!(json_output(&rate)["rating"], 8);

    assert_command_ok(
        &silent_cli(&db_path, &media_root)
            .args(["favorites", "add", &edited_view])
            .output()
            .expect("favorite track"),
    );
    let favorites = silent_cli(&db_path, &media_root)
        .args(["--output", "json", "favorites", "list"])
        .output()
        .expect("list favorites");
    assert_command_ok(&favorites);
    assert_eq!(json_output(&favorites).as_array().unwrap().len(), 1);

    assert_command_ok(
        &silent_cli(&db_path, &media_root)
            .args(["playlist", "create", "CLI Mix"])
            .output()
            .expect("create playlist"),
    );
    assert_command_ok(
        &silent_cli(&db_path, &media_root)
            .args(["playlist", "add", "CLI Mix", &edited_view])
            .output()
            .expect("add playlist track"),
    );
    assert_command_ok(
        &silent_cli(&db_path, &media_root)
            .args(["playlist", "rename", "CLI Mix", "Renamed Mix"])
            .output()
            .expect("rename playlist"),
    );
    assert_command_ok(
        &silent_cli(&db_path, &media_root)
            .args(["playlist", "sort", "Renamed Mix", "rating"])
            .output()
            .expect("sort playlist"),
    );
    assert_command_ok(
        &silent_cli(&db_path, &media_root)
            .args(["playlist", "move", "Renamed Mix", &edited_view, "up"])
            .output()
            .expect("move playlist track"),
    );
    let playlist = silent_cli(&db_path, &media_root)
        .args(["--output", "json", "playlist", "show", "Renamed Mix"])
        .output()
        .expect("show playlist");
    assert_command_ok(&playlist);
    assert_eq!(json_output(&playlist).as_array().unwrap().len(), 1);

    let history = silent_cli(&db_path, &media_root)
        .args(["--output", "json", "history", "list", "--limit", "5"])
        .output()
        .expect("list history");
    assert_command_ok(&history);
    assert!(json_output(&history).as_array().unwrap().is_empty());

    let user = silent_cli(&db_path, &media_root)
        .args(["--output", "json", "user", "show"])
        .output()
        .expect("show user");
    assert_command_ok(&user);
    assert_eq!(json_output(&user)["display_name"], "Local User");

    assert_command_ok(
        &silent_cli(&db_path, &media_root)
            .args(["favorites", "remove", &edited_view])
            .output()
            .expect("remove favorite"),
    );
    assert_command_ok(
        &silent_cli(&db_path, &media_root)
            .args(["playlist", "remove", "Renamed Mix", &edited_view])
            .output()
            .expect("remove playlist track"),
    );
    assert_command_ok(
        &silent_cli(&db_path, &media_root)
            .args(["--yes", "playlist", "clear", "Renamed Mix"])
            .output()
            .expect("clear playlist"),
    );
    assert_command_ok(
        &silent_cli(&db_path, &media_root)
            .args(["--yes", "playlist", "delete", "Renamed Mix"])
            .output()
            .expect("delete playlist"),
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn artwork_lyrics_materialization_and_library_package_roundtrip() {
    let root = make_temp_dir("portable");
    let db_path = root.join("silent.sqlite3");
    let media_root = root.join("Music");
    let fixture = fixture("into_the_oceans_chorus.ogg");
    import_one(&db_path, &media_root, &fixture);
    let primary = first_track(&db_path, &media_root);
    let selector = primary["view_id"].as_str().unwrap();

    let image = root.join("cover.png");
    fs::write(
        &image,
        [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0],
    )
    .unwrap();
    let lyrics = root.join("words.lrc");
    fs::write(&lyrics, "[00:00.00]Silent CLI").unwrap();

    let metadata = silent_cli(&db_path, &media_root)
        .args([
            "--output",
            "json",
            "track",
            "metadata",
            "set",
            selector,
            "--title",
            "CLI Title",
            "--artist",
            "CLI Artist",
            "--album",
            "CLI Album",
        ])
        .output()
        .expect("set track metadata");
    assert_command_ok(&metadata);
    let album_selector = json_output(&metadata)["view_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let album_artwork = silent_cli(&db_path, &media_root)
        .args([
            "--output",
            "json",
            "track",
            "album-artwork",
            "set",
            &album_selector,
        ])
        .arg(&image)
        .output()
        .expect("set album artwork");
    assert_command_ok(&album_artwork);

    let artwork = silent_cli(&db_path, &media_root)
        .args([
            "--output",
            "json",
            "track",
            "artwork",
            "set",
            &album_selector,
        ])
        .arg(&image)
        .output()
        .expect("set track artwork");
    assert_command_ok(&artwork);
    let artwork_view = json_output(&artwork)["view_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let lyrics_result = silent_cli(&db_path, &media_root)
        .args(["--output", "json", "track", "lyrics", "set", &artwork_view])
        .arg(&lyrics)
        .output()
        .expect("set lyrics");
    assert_command_ok(&lyrics_result);
    let lyrics_view = json_output(&lyrics_result)["view_id"]
        .as_str()
        .unwrap()
        .to_owned();

    assert_command_ok(
        &silent_cli(&db_path, &media_root)
            .args(["playlist", "create", "Covered"])
            .output()
            .expect("create covered playlist"),
    );
    assert_command_ok(
        &silent_cli(&db_path, &media_root)
            .args(["playlist", "add", "Covered", selector])
            .output()
            .expect("add covered playlist track"),
    );
    assert_command_ok(
        &silent_cli(&db_path, &media_root)
            .args(["playlist", "artwork", "set", "Covered"])
            .arg(&image)
            .output()
            .expect("set playlist artwork"),
    );

    let exported = root.join("Exported.ogg");
    let materialize = silent_cli(&db_path, &media_root)
        .args(["--output", "json", "track", "export", &lyrics_view])
        .arg(&exported)
        .output()
        .expect("materialize track");
    assert_command_ok(&materialize);
    assert!(exported.exists());
    assert!(exported.with_extension("lrc").exists());
    assert_eq!(json_output(&materialize)["view_kind"], "primary");

    let package = root.join("Library.silentlibrary");
    let package_export = silent_cli(&db_path, &media_root)
        .args(["--output", "json", "library", "package", "export"])
        .arg(&package)
        .output()
        .expect("export package");
    assert_command_ok(&package_export);
    assert!(package.join("manifest.json").exists());

    let refused = silent_cli(&db_path, &media_root)
        .args(["library", "zero"])
        .output()
        .expect("refuse zero without confirmation");
    assert_eq!(refused.status.code(), Some(2));

    assert_command_ok(
        &silent_cli(&db_path, &media_root)
            .args(["--yes", "library", "zero"])
            .output()
            .expect("zero library"),
    );
    assert_command_ok(
        &silent_cli(&db_path, &media_root)
            .args(["--yes", "library", "package", "import"])
            .arg(&package)
            .output()
            .expect("import package"),
    );
    assert!(!library_tracks(&db_path, &media_root).is_empty());
    fs::remove_dir_all(root).ok();
}

#[test]
fn playback_shell_supports_state_commands_without_opening_audio() {
    let root = make_temp_dir("shell");
    let db_path = root.join("silent.sqlite3");
    let media_root = root.join("Music");
    import_one(
        &db_path,
        &media_root,
        &fixture("into_the_oceans_chorus.ogg"),
    );
    let mut child = silent_cli(&db_path, &media_root)
        .args(["--quiet", "playback", "shell"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start playback shell");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(
            b"repeat all\nshuffle on\nlifecycle interruption-begin\nlifecycle interruption-end off\nlifecycle output-disconnected\nstatus\nquit\n",
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    fs::remove_dir_all(root).ok();
    assert_command_ok(&output);
    assert!(
        output.stderr.is_empty(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires a working default audio output device"]
fn playback_shell_plays_and_seeks_a_managed_track() {
    let root = make_temp_dir("real_playback");
    let db_path = root.join("silent.sqlite3");
    let media_root = root.join("Music");
    import_one(
        &db_path,
        &media_root,
        &fixture("into_the_oceans_chorus.ogg"),
    );
    let selector = first_track(&db_path, &media_root)["view_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut child = silent_cli(&db_path, &media_root)
        .args(["--quiet", "playback", "shell"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start playback shell");
    let stdin = child.stdin.as_mut().unwrap();
    writeln!(stdin, "play {selector}").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    stdin.write_all(b"seek 50\nquit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    fs::remove_dir_all(root).ok();
    assert_command_ok(&output);
    assert!(
        output.stderr.is_empty(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn silent() -> Command {
    Command::new(env!("CARGO_BIN_EXE_silent"))
}

fn silent_cli(db_path: &Path, media_root: &Path) -> Command {
    let mut command = silent();
    command
        .args(["--cli", "--db"])
        .arg(db_path)
        .arg("--media-root")
        .arg(media_root);
    command
}

fn import_one(db_path: &Path, media_root: &Path, fixture: &Path) {
    let output = silent_cli(db_path, media_root)
        .args(["--output", "json", "library", "import"])
        .arg(fixture)
        .output()
        .expect("import fixture");
    assert_command_ok(&output);
    assert_eq!(json_output(&output)["imported"], 1);
}

fn first_track(db_path: &Path, media_root: &Path) -> Value {
    library_tracks(db_path, media_root)
        .into_iter()
        .next()
        .expect("library track")
}

fn library_tracks(db_path: &Path, media_root: &Path) -> Vec<Value> {
    let output = silent_cli(db_path, media_root)
        .args(["--output", "json", "library", "list"])
        .output()
        .expect("list library");
    assert_command_ok(&output);
    json_output(&output).as_array().unwrap().clone()
}

fn json_output(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "invalid JSON stdout: {error}\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn assert_command_ok(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
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
    let path = std::env::temp_dir().join(format!("silent_cli_{prefix}_{nonce}"));
    fs::create_dir(&path).unwrap();
    path
}
