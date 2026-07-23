use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use player_analysis_ebur128::analyze_path;
use player_ffi::SilentAppClient;
use player_library_fs::{LibraryScanner, ScanOptions};
use serde_json::{json, Value};

use crate::error::{CliError, CliResult};
use crate::output::{emit, OutputMode};
use crate::shell::run_playback_shell;

#[derive(Clone, Debug)]
pub(crate) struct CliContext {
    pub(crate) db_path: Option<PathBuf>,
    pub(crate) media_root: Option<PathBuf>,
    pub(crate) output: OutputMode,
    pub(crate) quiet: bool,
    pub(crate) yes: bool,
}

impl CliContext {
    pub(crate) fn open_client(&self) -> CliResult<SilentAppClient> {
        let (db_path, media_root) = self.configured_paths()?;
        let metadata = fs::metadata(db_path).map_err(|error| {
            CliError::operation(format!(
                "cannot open database {}: {error}; use `library import` or `library package import` to create it",
                db_path.display()
            ))
        })?;
        if !metadata.is_file() {
            return Err(CliError::operation(format!(
                "database path {} is not a file",
                db_path.display()
            )));
        }
        self.open_client_at(db_path, media_root)
    }

    fn open_creatable_client(&self) -> CliResult<SilentAppClient> {
        let (db_path, media_root) = self.configured_paths()?;
        if db_path.exists() && !db_path.is_file() {
            return Err(CliError::operation(format!(
                "database path {} is not a file",
                db_path.display()
            )));
        }
        if let Some(parent) = db_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            if !parent.is_dir() {
                return Err(CliError::operation(format!(
                    "database parent {} is not an existing directory",
                    parent.display()
                )));
            }
        }
        self.open_client_at(db_path, media_root)
    }

    fn configured_paths(&self) -> CliResult<(&Path, &Path)> {
        let db_path = self.db_path.as_deref().ok_or_else(|| {
            CliError::usage("stateful commands require global option `--db <path>`")
        })?;
        let media_root = self.media_root.as_deref().ok_or_else(|| {
            CliError::usage("stateful commands require global option `--media-root <path>`")
        })?;
        if media_root.exists() && !media_root.is_dir() {
            return Err(CliError::operation(format!(
                "media root {} is not a directory",
                media_root.display()
            )));
        }
        Ok((db_path, media_root))
    }

    fn open_client_at(&self, db_path: &Path, media_root: &Path) -> CliResult<SilentAppClient> {
        SilentAppClient::open(db_path, media_root).map_err(Into::into)
    }

    pub(crate) fn emit(&self, value: &Value) -> CliResult<()> {
        emit(value, self.output, self.quiet)
    }
}

pub fn run(args: Vec<String>) -> CliResult<()> {
    let (context, args) = extract_global_options(args)?;
    if args.is_empty() || is_help(&args[0]) {
        print_cli_help();
        return Ok(());
    }

    let domain = &args[0];
    let rest = args[1..].to_vec();
    match domain.as_str() {
        "library" => run_library(&context, rest),
        "track" => run_track(&context, rest),
        "favorites" => run_favorites(&context, rest),
        "playlist" => run_playlist(&context, rest),
        "history" => run_history(&context, rest),
        "user" => run_user(&context, rest),
        "playback" => run_playback(&context, rest),
        _ => Err(CliError::usage(format!(
            "unknown CLI domain `{domain}`; run `silent --cli --help`"
        ))),
    }
}

fn extract_global_options(args: Vec<String>) -> CliResult<(CliContext, Vec<String>)> {
    let mut context = CliContext {
        db_path: None,
        media_root: None,
        output: OutputMode::Table,
        quiet: false,
        yes: false,
    };
    let mut seen = BTreeSet::new();
    let mut args = args.into_iter().peekable();
    while let Some(argument) = args.peek() {
        if !argument.starts_with('-') || is_help(argument) {
            break;
        }
        let argument = args.next().expect("peeked argument must exist");
        if !seen.insert(argument.clone()) {
            return Err(CliError::usage(format!(
                "global option `{argument}` may only be provided once"
            )));
        }
        match argument.as_str() {
            "--db" => {
                context.db_path = Some(PathBuf::from(required_value("--db", args.next())?));
            }
            "--media-root" => {
                context.media_root =
                    Some(PathBuf::from(required_value("--media-root", args.next())?));
            }
            "--output" => {
                context.output = OutputMode::parse(&required_value("--output", args.next())?)?;
            }
            "--quiet" => context.quiet = true,
            "--yes" => context.yes = true,
            _ => {
                return Err(CliError::usage(format!(
                    "unknown global option `{argument}`"
                )))
            }
        }
    }
    Ok((context, args.collect()))
}

fn run_library(context: &CliContext, mut args: Vec<String>) -> CliResult<()> {
    let Some(command) = take_first(&mut args) else {
        print_library_help();
        return Ok(());
    };
    if is_help(&command) {
        print_library_help();
        return Ok(());
    }
    match command.as_str() {
        "scan" => {
            let root = one_path(args, "library scan requires <folder>")?;
            let scanner = LibraryScanner::new(ScanOptions {
                follow_symlinks: false,
                include_hidden: false,
            });
            let tracks = scanner.scan(&root)?;
            let value = Value::Array(
                tracks
                    .into_iter()
                    .map(|track| {
                        json!({
                            "id": track.id.value().to_string(),
                            "title": track.title,
                            "path": track.path,
                        })
                    })
                    .collect(),
            );
            context.emit(&value)
        }
        "list" => {
            ensure_empty(&args, "library list")?;
            let mut client = context.open_client()?;
            context.emit(&client.library()?)
        }
        "search" => {
            let (query, limit) = parse_search_args(args)?;
            let mut client = context.open_client()?;
            context.emit(&client.search(&query, limit)?)
        }
        "import" => import_paths(context, args),
        "package" => run_library_package(context, args),
        "zero" => {
            ensure_empty(&args, "library zero")?;
            require_confirmation(context, "library zero")?;
            let mut client = context.open_client()?;
            context.emit(&client.zero_out_library()?)
        }
        "audit" => {
            ensure_empty(&args, "library audit")?;
            let mut client = context.open_client()?;
            context.emit(&client.audit_database()?)
        }
        "analyze" => {
            ensure_empty(&args, "library analyze")?;
            let mut client = context.open_client()?;
            context.emit(&client.analyze()?)
        }
        _ => Err(CliError::usage(format!(
            "unknown library command `{command}`"
        ))),
    }
}

fn run_library_package(context: &CliContext, mut args: Vec<String>) -> CliResult<()> {
    let action = take_first(&mut args)
        .ok_or_else(|| CliError::usage("library package requires `export` or `import`"))?;
    let path = one_path(
        args,
        "library package export/import requires <package-directory>",
    )?;
    match action.as_str() {
        "export" => export_library_package(context, path),
        "import" => import_library_package(context, path),
        _ => Err(CliError::usage(
            "library package requires `export` or `import`",
        )),
    }
}

fn export_library_package(context: &CliContext, destination: PathBuf) -> CliResult<()> {
    if destination.exists() {
        if !destination.is_dir() {
            return Err(CliError::operation(format!(
                "package destination {} is not a directory",
                destination.display()
            )));
        }
        if !context.yes {
            return Err(CliError::usage(format!(
                "{} already exists; pass --yes to allow updating it",
                destination.display()
            )));
        }
    }
    let mut client = context.open_client()?;
    context.emit(&client.export_library(destination)?)
}

fn import_library_package(context: &CliContext, source: PathBuf) -> CliResult<()> {
    if !source.is_dir() {
        return Err(CliError::operation(format!(
            "library package {} is not a directory",
            source.display()
        )));
    }
    require_confirmation(context, "library package import")?;
    let mut client = context.open_creatable_client()?;
    context.emit(&client.import_library(source)?)
}

fn import_paths(context: &CliContext, paths: Vec<String>) -> CliResult<()> {
    if paths.is_empty() {
        return Err(CliError::usage(
            "library import requires one or more files or folders",
        ));
    }
    let mut files = Vec::new();
    let mut folders = Vec::new();
    for value in paths {
        let path = PathBuf::from(value);
        let metadata = fs::metadata(&path).map_err(|error| {
            CliError::operation(format!("cannot inspect {}: {error}", path.display()))
        })?;
        if metadata.is_dir() {
            folders.push(path);
        } else if metadata.is_file() {
            files.push(path);
        } else {
            return Err(CliError::usage(format!(
                "{} is neither a file nor a folder",
                path.display()
            )));
        }
    }

    match (folders.as_slice(), files.as_slice()) {
        ([folder], []) => {
            let mut client = context.open_creatable_client()?;
            context.emit(&client.import_folder(folder)?)
        }
        ([], files) => {
            let mut client = context.open_creatable_client()?;
            context.emit(&client.import_files(files)?)
        }
        _ => Err(CliError::usage(
            "library import accepts either one folder or one or more files, not a mixture",
        )),
    }
}

fn run_track(context: &CliContext, mut args: Vec<String>) -> CliResult<()> {
    let Some(command) = take_first(&mut args) else {
        print_track_help();
        return Ok(());
    };
    if is_help(&command) {
        print_track_help();
        return Ok(());
    }
    match command.as_str() {
        "show" => {
            let selector = one_value(args, "track show requires <path-or-view-id>")?;
            let mut client = context.open_client()?;
            let selected = resolve_track(&mut client, &selector)?;
            let details = client.track_details(&selected.path)?;
            let diagnostics = track_diagnostics(&details);
            context.emit(
                &json!({"track": selected.track, "details": details, "diagnostics": diagnostics}),
            )
        }
        "edit" => edit_track(context, args),
        "metadata" => set_track_metadata(context, args),
        "notes" => set_track_notes(context, args),
        "rate" => rate_track(context, args),
        "artwork" => run_track_artwork(context, args),
        "album-artwork" => set_album_artwork(context, args),
        "lyrics" => set_track_lyrics(context, args),
        "export" => export_track(context, args),
        "analyze" => analyze_track(context, args),
        _ => Err(CliError::usage(format!(
            "unknown track command `{command}`"
        ))),
    }
}

fn edit_track(context: &CliContext, mut args: Vec<String>) -> CliResult<()> {
    let selector = take_first(&mut args)
        .ok_or_else(|| CliError::usage("track edit requires <path-or-view-id>"))?;
    let options = parse_named_values(
        args,
        &[
            "--name",
            "--title",
            "--artist",
            "--album",
            "--notes",
            "--artwork",
            "--lyrics",
        ],
        "track edit",
    )?;
    if options.is_empty() {
        return Err(CliError::usage(
            "track edit requires at least one edit option",
        ));
    }

    let mut client = context.open_client()?;
    let selected = resolve_track(&mut client, &selector)?;
    let details = client.track_details(&selected.path)?;
    let edit = json!({
        "view_name": options.get("--name").cloned().or_else(|| json_string(&details, "view_name")),
        "title": match options.get("--title") {
            Some(title) => title.clone(),
            None => required_json_string(&details, "display_title")?,
        },
        "artist": options.get("--artist").cloned().or_else(|| json_string(&details, "display_artist")),
        "album": options.get("--album").cloned().or_else(|| json_string(&details, "display_album")),
        "notes": options.get("--notes").cloned().or_else(|| json_string(&details, "notes")),
        "artwork_path": options.get("--artwork"),
        "lyrics_path": options.get("--lyrics"),
    });
    context.emit(&client.edit_track_view(&selected.path, &edit)?)
}

fn set_track_metadata(context: &CliContext, mut args: Vec<String>) -> CliResult<()> {
    let action =
        take_first(&mut args).ok_or_else(|| CliError::usage("track metadata requires `set`"))?;
    if action != "set" {
        return Err(CliError::usage("track metadata requires `set`"));
    }
    let selector = take_first(&mut args)
        .ok_or_else(|| CliError::usage("track metadata set requires <path-or-view-id>"))?;
    let options = parse_named_values(
        args,
        &["--title", "--artist", "--album"],
        "track metadata set",
    )?;
    if options.len() != 3 {
        return Err(CliError::usage(
            "track metadata set requires exactly --title, --artist, and --album",
        ));
    }
    let mut client = context.open_client()?;
    let selected = resolve_track(&mut client, &selector)?;
    let title = required_named_value(&options, "--title")?;
    let artist = required_named_value(&options, "--artist")?;
    let album = required_named_value(&options, "--album")?;
    context.emit(&client.set_track_metadata(&selected.path, &title, &artist, &album)?)
}

fn set_track_notes(context: &CliContext, mut args: Vec<String>) -> CliResult<()> {
    let action =
        take_first(&mut args).ok_or_else(|| CliError::usage("track notes requires `set`"))?;
    if action != "set" {
        return Err(CliError::usage("track notes requires `set`"));
    }
    let selector = take_first(&mut args)
        .ok_or_else(|| CliError::usage("track notes set requires <path-or-view-id> <notes>"))?;
    if args.is_empty() {
        return Err(CliError::usage(
            "track notes set requires <path-or-view-id> <notes>",
        ));
    }
    let notes = args.join(" ");
    let mut client = context.open_client()?;
    let selected = resolve_track(&mut client, &selector)?;
    context.emit(&client.set_track_notes(&selected.path, &notes)?)
}

fn rate_track(context: &CliContext, args: Vec<String>) -> CliResult<()> {
    if args.len() != 2 {
        return Err(CliError::usage(
            "track rate requires <path-or-view-id> <1..10|clear>",
        ));
    }
    let rating = match args[1].as_str() {
        "clear" => 0,
        value => value
            .parse::<i32>()
            .map_err(|_| CliError::usage("rating must be 1 through 10, or `clear`"))?,
    };
    if rating != 0 && !(1..=10).contains(&rating) {
        return Err(CliError::usage("rating must be 1 through 10, or `clear`"));
    }
    let mut client = context.open_client()?;
    let selected = resolve_track(&mut client, &args[0])?;
    context.emit(&client.set_track_rating(&selected.path, rating)?)
}

fn run_track_artwork(context: &CliContext, mut args: Vec<String>) -> CliResult<()> {
    let action =
        take_first(&mut args).ok_or_else(|| CliError::usage("track artwork requires `set`"))?;
    match action.as_str() {
        "set" => {
            if args.len() != 2 {
                return Err(CliError::usage(
                    "track artwork set requires <path-or-view-id> <image>",
                ));
            }
            let mut client = context.open_client()?;
            let selected = resolve_track(&mut client, &args[0])?;
            context.emit(&client.set_track_artwork(&selected.path, PathBuf::from(&args[1]))?)
        }
        _ => Err(CliError::usage("track artwork requires `set`")),
    }
}

fn set_album_artwork(context: &CliContext, mut args: Vec<String>) -> CliResult<()> {
    let action = take_first(&mut args)
        .ok_or_else(|| CliError::usage("track album-artwork requires `set`"))?;
    if action != "set" || args.len() != 2 {
        return Err(CliError::usage(
            "track album-artwork set requires <path-or-view-id> <image>",
        ));
    }
    let mut client = context.open_client()?;
    let selected = resolve_track(&mut client, &args[0])?;
    context.emit(&client.set_album_artwork(&selected.path, PathBuf::from(&args[1]))?)
}

fn set_track_lyrics(context: &CliContext, mut args: Vec<String>) -> CliResult<()> {
    let action =
        take_first(&mut args).ok_or_else(|| CliError::usage("track lyrics requires `set`"))?;
    if action != "set" || args.len() != 2 {
        return Err(CliError::usage(
            "track lyrics set requires <path-or-view-id> <lyrics-file>",
        ));
    }
    let mut client = context.open_client()?;
    let selected = resolve_track(&mut client, &args[0])?;
    context.emit(&client.set_track_lyrics(&selected.path, PathBuf::from(&args[1]))?)
}

fn export_track(context: &CliContext, args: Vec<String>) -> CliResult<()> {
    if args.len() != 2 {
        return Err(CliError::usage(
            "track export requires <path-or-view-id> <destination>",
        ));
    }
    let destination = PathBuf::from(&args[1]);
    if destination.exists() && !context.yes {
        return Err(CliError::usage(format!(
            "{} already exists; pass --yes to overwrite it",
            destination.display()
        )));
    }
    let mut client = context.open_client()?;
    let selected = resolve_track(&mut client, &args[0])?;
    context.emit(&client.export_track_view(&selected.path, destination)?)
}

fn analyze_track(context: &CliContext, args: Vec<String>) -> CliResult<()> {
    let input = one_path(args, "track analyze requires <audio-file>")?;
    let report = analyze_path(&input)?;
    context.emit(&json!({
        "path": report.path,
        "sample_rate_hz": report.sample_rate_hz,
        "channels": report.channels,
        "duration_seconds": report.duration_seconds,
        "integrated_lufs": report.integrated_lufs,
        "true_peak_dbtp": report.true_peak_dbtp,
    }))
}

fn run_favorites(context: &CliContext, mut args: Vec<String>) -> CliResult<()> {
    let Some(command) = take_first(&mut args) else {
        print_favorites_help();
        return Ok(());
    };
    if is_help(&command) {
        print_favorites_help();
        return Ok(());
    }
    match command.as_str() {
        "list" => {
            ensure_empty(&args, "favorites list")?;
            let mut client = context.open_client()?;
            context.emit(&client.favorites()?)
        }
        "add" | "remove" => {
            let selector = one_value(args, "favorites add/remove requires <path-or-view-id>")?;
            let mut client = context.open_client()?;
            let selected = resolve_track(&mut client, &selector)?;
            context.emit(&client.set_favorite(&selected.path, command == "add")?)
        }
        _ => Err(CliError::usage(format!(
            "unknown favorites command `{command}`"
        ))),
    }
}

fn run_playlist(context: &CliContext, mut args: Vec<String>) -> CliResult<()> {
    let Some(command) = take_first(&mut args) else {
        print_playlist_help();
        return Ok(());
    };
    if is_help(&command) {
        print_playlist_help();
        return Ok(());
    }
    match command.as_str() {
        "list" => {
            ensure_empty(&args, "playlist list")?;
            let mut client = context.open_client()?;
            context.emit(&client.playlists()?)
        }
        "show" => {
            let name = one_value(args, "playlist show requires <name>")?;
            let mut client = context.open_client()?;
            context.emit(&client.playlist_tracks(&name)?)
        }
        "create" => {
            let name = one_value(args, "playlist create requires <name>")?;
            let mut client = context.open_client()?;
            context.emit(&client.create_playlist(&name)?)
        }
        "rename" => {
            if args.len() != 2 {
                return Err(CliError::usage(
                    "playlist rename requires <old-name> <new-name>",
                ));
            }
            let mut client = context.open_client()?;
            context.emit(&client.rename_playlist(&args[0], &args[1])?)
        }
        "delete" | "clear" => {
            let name = one_value(args, "playlist delete/clear requires <name>")?;
            require_confirmation(context, &format!("playlist {command}"))?;
            let mut client = context.open_client()?;
            let value = if command == "delete" {
                client.delete_playlist(&name)?
            } else {
                client.clear_playlist(&name)?
            };
            context.emit(&value)
        }
        "add" | "remove" => {
            if args.len() != 2 {
                return Err(CliError::usage(
                    "playlist add/remove requires <name> <path-or-view-id>",
                ));
            }
            let mut client = context.open_client()?;
            let selected = resolve_track(&mut client, &args[1])?;
            let value = if command == "add" {
                client.add_to_playlist(&args[0], &selected.path)?
            } else {
                client.remove_from_playlist(&args[0], &selected.path)?
            };
            context.emit(&value)
        }
        "move" => move_playlist_track(context, args),
        "sort" => {
            if args.len() != 2 {
                return Err(CliError::usage(
                    "playlist sort requires <name> <manual|title|artist|album|rating>",
                ));
            }
            if !matches!(
                args[1].as_str(),
                "manual" | "title" | "artist" | "album" | "rating"
            ) {
                return Err(CliError::usage(
                    "playlist sort requires <name> <manual|title|artist|album|rating>",
                ));
            }
            let mut client = context.open_client()?;
            context.emit(&client.sort_playlist(&args[0], &args[1])?)
        }
        "artwork" => set_playlist_artwork(context, args),
        _ => Err(CliError::usage(format!(
            "unknown playlist command `{command}`"
        ))),
    }
}

fn move_playlist_track(context: &CliContext, args: Vec<String>) -> CliResult<()> {
    if args.len() != 3 {
        return Err(CliError::usage(
            "playlist move requires <name> <path-or-view-id> <up|down>",
        ));
    }
    let delta = match args[2].as_str() {
        "up" => -1,
        "down" => 1,
        _ => {
            return Err(CliError::usage(
                "playlist move direction must be `up` or `down`",
            ))
        }
    };
    let mut client = context.open_client()?;
    let selected = resolve_track(&mut client, &args[1])?;
    context.emit(&client.move_playlist_track(&args[0], &selected.path, delta)?)
}

fn set_playlist_artwork(context: &CliContext, mut args: Vec<String>) -> CliResult<()> {
    let action =
        take_first(&mut args).ok_or_else(|| CliError::usage("playlist artwork requires `set`"))?;
    if action != "set" || args.len() != 2 {
        return Err(CliError::usage(
            "playlist artwork set requires <name> <image>",
        ));
    }
    let mut client = context.open_client()?;
    context.emit(&client.set_playlist_artwork(&args[0], PathBuf::from(&args[1]))?)
}

fn run_history(context: &CliContext, mut args: Vec<String>) -> CliResult<()> {
    let Some(command) = take_first(&mut args) else {
        print_history_help();
        return Ok(());
    };
    if is_help(&command) {
        print_history_help();
        return Ok(());
    }
    match command.as_str() {
        "list" => {
            let limit = parse_required_limit(args)?;
            let mut client = context.open_client()?;
            context.emit(&client.history(limit)?)
        }
        _ => Err(CliError::usage(format!(
            "unknown history command `{command}`"
        ))),
    }
}

fn run_user(context: &CliContext, args: Vec<String>) -> CliResult<()> {
    if args.len() == 1 && args[0] == "show" {
        let mut client = context.open_client()?;
        return context.emit(&client.user_data()?);
    }
    if args.first().is_some_and(|value| is_help(value)) || args.is_empty() {
        println!("Usage: silent --cli [options] user show");
        return Ok(());
    }
    Err(CliError::usage("user supports only `show`"))
}

fn run_playback(context: &CliContext, mut args: Vec<String>) -> CliResult<()> {
    let Some(command) = take_first(&mut args) else {
        print_playback_help();
        return Ok(());
    };
    if is_help(&command) {
        print_playback_help();
        return Ok(());
    }
    match command.as_str() {
        "shell" => run_playback_shell(context, args),
        _ => Err(CliError::usage("playback supports only `shell`")),
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SelectedTrack {
    pub(crate) path: PathBuf,
    pub(crate) track: Value,
}

pub(crate) fn resolve_track(
    client: &mut SilentAppClient,
    selector: &str,
) -> CliResult<SelectedTrack> {
    let library = client.library()?;
    let tracks = library
        .as_array()
        .ok_or_else(|| CliError::operation("library returned an invalid track list"))?;
    let selector_path = PathBuf::from(selector);
    let canonical_selector = selector_path.canonicalize().ok();
    let mut matches = tracks
        .iter()
        .filter(|track| {
            let exact = ["path", "view_id", "id"]
                .iter()
                .any(|key| track.get(key).and_then(Value::as_str) == Some(selector));
            if exact {
                return true;
            }
            let Some(canonical_selector) = &canonical_selector else {
                return false;
            };
            track
                .get("path")
                .and_then(Value::as_str)
                .and_then(|path| Path::new(path).canonicalize().ok())
                .as_ref()
                == Some(canonical_selector)
        })
        .cloned()
        .collect::<Vec<_>>();
    match matches.len() {
        0 => Err(CliError::operation(format!(
            "track not found for selector `{selector}`"
        ))),
        1 => {
            let track = matches.remove(0);
            let path = track
                .get("path")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .ok_or_else(|| CliError::operation("selected track has no path"))?;
            Ok(SelectedTrack { path, track })
        }
        count => Err(CliError::operation(format!(
            "selector `{selector}` matched {count} tracks; use an exact path or view id"
        ))),
    }
}

fn parse_search_args(mut args: Vec<String>) -> CliResult<(String, usize)> {
    let mut limit = None;
    let mut query = Vec::new();
    while let Some(value) = take_first(&mut args) {
        if value == "--limit" {
            if limit.is_some() {
                return Err(CliError::usage(
                    "library search option `--limit` may only be provided once",
                ));
            }
            let parsed = take_first(&mut args)
                .ok_or_else(|| CliError::usage("--limit requires a value"))?
                .parse()
                .map_err(|_| CliError::usage("--limit must be a positive integer"))?;
            limit = Some(positive_limit(parsed)?);
        } else {
            query.push(value);
        }
    }
    if query.is_empty() {
        return Err(CliError::usage("library search requires <query>"));
    }
    let limit = limit
        .ok_or_else(|| CliError::usage("library search requires explicit option `--limit <n>`"))?;
    Ok((query.join(" "), limit))
}

fn parse_required_limit(args: Vec<String>) -> CliResult<usize> {
    if args.len() == 2 && args[0] == "--limit" {
        let limit = args[1]
            .parse()
            .map_err(|_| CliError::usage("--limit must be a positive integer"))?;
        return positive_limit(limit);
    }
    Err(CliError::usage("history list requires `--limit <n>`"))
}

fn positive_limit(limit: usize) -> CliResult<usize> {
    if limit == 0 {
        Err(CliError::usage("--limit must be greater than zero"))
    } else {
        Ok(limit)
    }
}

fn parse_named_values(
    mut args: Vec<String>,
    allowed: &[&str],
    command: &str,
) -> CliResult<BTreeMap<String, String>> {
    let mut parsed = BTreeMap::new();
    while let Some(option) = take_first(&mut args) {
        if !allowed.contains(&option.as_str()) {
            return Err(CliError::usage(format!(
                "unknown {command} option `{option}`"
            )));
        }
        let value = take_first(&mut args)
            .ok_or_else(|| CliError::usage(format!("{option} requires a value")))?;
        if parsed.insert(option.clone(), value).is_some() {
            return Err(CliError::usage(format!(
                "{command} option `{option}` may only be provided once"
            )));
        }
    }
    Ok(parsed)
}

fn required_named_value(values: &BTreeMap<String, String>, name: &str) -> CliResult<String> {
    values
        .get(name)
        .cloned()
        .ok_or_else(|| CliError::usage(format!("missing required option `{name}`")))
}

fn json_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn required_json_string(value: &Value, key: &str) -> CliResult<String> {
    json_string(value, key)
        .ok_or_else(|| CliError::operation(format!("track details have no string `{key}` field")))
}

fn track_diagnostics(details: &Value) -> Vec<Value> {
    let mut diagnostics = Vec::new();
    let mut push = |severity: &str, title: &str, detail: &str| {
        diagnostics.push(json!({
            "severity": severity,
            "title": title,
            "detail": detail,
        }));
    };
    if blank_json_string(details, "view_id") {
        push(
            "error",
            "Missing view id",
            "Playback may still use the file path, but view identity and export lineage are incomplete.",
        );
    }
    if blank_json_string(details, "primary_view_id") {
        push(
            "error",
            "Missing primary view",
            "This view cannot be traced back to an imported primary view.",
        );
    }
    if blank_json_string(details, "audio_hash") {
        push(
            "warning",
            "Missing audio hash",
            "Playback can continue from the file path, but deduplication and primary identity are limited.",
        );
    }
    if blank_json_string(details, "format_name") {
        push(
            "warning",
            "Unknown format",
            "Playback can still try the current file. Export uses the current audio bytes.",
        );
    }
    if blank_json_string(details, "quality_profile") {
        push(
            "info",
            "Quality profile not set",
            "This is expected for imported primary views until transcode profiles are implemented.",
        );
    }
    match details.get("is_primary_view").and_then(Value::as_bool) {
        Some(false) if blank_json_string(details, "transform_spec") => {
            push(
                "warning",
                "Missing transform spec",
                "The view can play, but the derived-view recipe has not been recorded.",
            );
        }
        None => {
            push(
                "error",
                "Missing primary-view state",
                "Track details do not identify whether this is a primary or derived view.",
            );
        }
        _ => {}
    }
    if details.get("artwork_path").is_none_or(Value::is_null) {
        push(
            "info",
            "No artwork",
            "A placeholder cover is shown. Playback is unaffected.",
        );
    }
    if blank_json_string(details, "lyrics_text") {
        push(
            "info",
            "No lyrics",
            "Lyrics are optional and do not affect playback.",
        );
    }
    diagnostics
}

fn blank_json_string(value: &Value, key: &str) -> bool {
    value
        .get(key)
        .and_then(Value::as_str)
        .is_none_or(|value| value.trim().is_empty())
}

fn require_confirmation(context: &CliContext, operation: &str) -> CliResult<()> {
    if context.yes {
        Ok(())
    } else {
        Err(CliError::usage(format!(
            "{operation} changes or removes library data; pass --yes to confirm"
        )))
    }
}

fn one_path(args: Vec<String>, message: &str) -> CliResult<PathBuf> {
    one_value(args, message).map(PathBuf::from)
}

fn one_value(args: Vec<String>, message: &str) -> CliResult<String> {
    if args.len() == 1 {
        Ok(args[0].clone())
    } else {
        Err(CliError::usage(message))
    }
}

fn ensure_empty(args: &[String], command: &str) -> CliResult<()> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(CliError::usage(format!(
            "{command} does not accept additional arguments"
        )))
    }
}

fn required_value(flag: &str, value: Option<String>) -> CliResult<String> {
    match value {
        Some(value) if !value.is_empty() && !value.starts_with('-') => Ok(value),
        _ => Err(CliError::usage(format!("{flag} requires a value"))),
    }
}

fn take_first(args: &mut Vec<String>) -> Option<String> {
    if args.is_empty() {
        None
    } else {
        Some(args.remove(0))
    }
}

fn is_help(value: &str) -> bool {
    value == "--help" || value == "-h"
}

fn print_cli_help() {
    println!(
        "\
Usage:
  silent --cli [global options] <domain> <command> [arguments]

Global options:
  --db <path>           Required database for stateful commands
  --media-root <path>   Required managed music directory for stateful commands
  --output table|json   Output format (table unless explicitly changed)
  --quiet               Suppress successful output
  --yes                 Confirm destructive or overwriting operations

Global options must appear before the domain and may only be specified once.

Domains:
  library       Scan, import, query, migrate, audit, and analyze the library
  track         Inspect and edit music views, artwork, lyrics, rating, and export
  favorites     List, add, and remove favorites
  playlist      Full playlist and playlist-artwork management
  history       List playback history
  user          Show local user data locations
  playback      Interactive playback shell

Run `silent --cli <domain> --help` for domain commands."
    );
}

fn print_library_help() {
    println!(
        "\
Usage:
  silent --cli [options] library scan <folder>
  silent --cli [options] library list
  silent --cli [options] library search <query> --limit <n>
  silent --cli [options] library import <file>... | <folder>
  silent --cli [options] library package export <directory>
  silent --cli [options] library package import <directory>
  silent --cli [options] library zero
  silent --cli [options] library audit
  silent --cli [options] library analyze

Package import and zero require global option --yes before `library`."
    );
}

fn print_track_help() {
    println!(
        "\
Usage:
  silent --cli [options] track show <path-or-view-id>
  silent --cli [options] track edit <selector> [--name <v>] [--title <v>] [--artist <v>] [--album <v>] [--notes <v>] [--artwork <file>] [--lyrics <file>]
  silent --cli [options] track metadata set <selector> --title <v> --artist <v> --album <v>
  silent --cli [options] track notes set <selector> <notes>
  silent --cli [options] track rate <selector> <1..10|clear>
  silent --cli [options] track artwork set <selector> <image>
  silent --cli [options] track album-artwork set <selector> <image>
  silent --cli [options] track lyrics set <selector> <lyrics-file>
  silent --cli [options] track export <selector> <destination>
  silent --cli [options] track analyze <audio-file>"
    );
}

fn print_favorites_help() {
    println!("Usage: silent --cli [options] favorites list|add <selector>|remove <selector>");
}

fn print_playlist_help() {
    println!(
        "\
Usage:
  silent --cli [options] playlist list
  silent --cli [options] playlist show|create|delete|clear <name>
  silent --cli [options] playlist rename <old-name> <new-name>
  silent --cli [options] playlist add|remove <name> <selector>
  silent --cli [options] playlist move <name> <selector> <up|down>
  silent --cli [options] playlist sort <name> <manual|title|artist|album|rating>
  silent --cli [options] playlist artwork set <name> <image>"
    );
}

fn print_history_help() {
    println!(
        "\
Usage:
  silent --cli [options] history list --limit <n>"
    );
}

fn print_playback_help() {
    println!(
        "\
Usage:
  silent --cli [options] playback shell [path-or-view-id]...

Pause, resume, next, previous, seek, repeat, and shuffle are available inside
the interactive shell so one process owns the audio session."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_options_stop_at_the_domain_boundary() {
        let (context, remaining) = extract_global_options(vec![
            "--output".to_owned(),
            "json".to_owned(),
            "--yes".to_owned(),
            "library".to_owned(),
            "list".to_owned(),
            "--output".to_owned(),
            "table".to_owned(),
        ])
        .unwrap();
        assert_eq!(context.output, OutputMode::Json);
        assert!(context.yes);
        assert_eq!(remaining, ["library", "list", "--output", "table"]);
    }

    #[test]
    fn search_collects_unquoted_query_words() {
        let (query, limit) = parse_search_args(vec![
            "miles".to_owned(),
            "davis".to_owned(),
            "--limit".to_owned(),
            "7".to_owned(),
        ])
        .unwrap();
        assert_eq!(query, "miles davis");
        assert_eq!(limit, 7);
    }

    #[test]
    fn duplicate_and_implicit_options_are_rejected() {
        assert!(extract_global_options(vec![
            "--output".to_owned(),
            "json".to_owned(),
            "--output".to_owned(),
            "table".to_owned(),
        ])
        .is_err());
        assert!(parse_search_args(vec!["miles".to_owned()]).is_err());
        assert!(parse_required_limit(Vec::new()).is_err());
        assert!(parse_required_limit(vec!["--limit".to_owned(), "0".to_owned()]).is_err());
    }
}
