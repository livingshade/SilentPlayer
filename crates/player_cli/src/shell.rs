use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use player_ffi::SilentAppClient;

use crate::command::{resolve_track, CliContext};
use crate::error::{CliError, CliResult};

pub fn run_playback_shell(context: &CliContext, startup: Vec<String>) -> CliResult<()> {
    let mut client = context.open_client()?;
    if !startup.is_empty() {
        let (paths, first) = resolve_queue(&mut client, &startup)?;
        context.emit(&client.play_queue(&paths, &first)?)?;
    }

    if !context.quiet {
        println!("Silent playback shell. Type `help` for commands and `quit` to exit.");
    }
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut line = String::new();
    loop {
        if let Err(error) = client.poll() {
            eprintln!("silent: playback poll failed: {error}");
        }
        if !context.quiet {
            eprint!("silent> ");
            io::stderr().flush()?;
        }
        line.clear();
        if input.read_line(&mut line)? == 0 {
            break;
        }
        let arguments = match split_command_line(&line) {
            Ok(arguments) => arguments,
            Err(error) => {
                eprintln!("silent: {error}");
                continue;
            }
        };
        if arguments.is_empty() {
            continue;
        }
        if matches!(arguments[0].as_str(), "quit" | "exit") {
            break;
        }
        if let Err(error) = run_shell_command(context, &mut client, arguments) {
            eprintln!("silent: {error}");
        }
    }
    client.stop()?;
    Ok(())
}

fn run_shell_command(
    context: &CliContext,
    client: &mut SilentAppClient,
    mut args: Vec<String>,
) -> CliResult<()> {
    let command = args.remove(0);
    match command.as_str() {
        "help" | "-h" | "--help" => {
            print_shell_help();
            Ok(())
        }
        "play" => {
            let selector = exactly_one(args, "play requires <path-or-view-id>")?;
            let selected = resolve_track(client, &selector)?;
            context.emit(&client.play_path(selected.path)?)
        }
        "load" => {
            if args.is_empty() {
                return Err(CliError::usage(
                    "load requires one or more paths or view ids",
                ));
            }
            let (paths, first) = resolve_queue(client, &args)?;
            context.emit(&client.play_queue(&paths, &first)?)
        }
        "pause" => {
            ensure_no_args(&args, "pause")?;
            context.emit(&client.pause()?)
        }
        "resume" => {
            ensure_no_args(&args, "resume")?;
            context.emit(&client.resume()?)
        }
        "stop" => {
            ensure_no_args(&args, "stop")?;
            context.emit(&client.stop()?)
        }
        "next" => {
            ensure_no_args(&args, "next")?;
            context.emit(&client.next_track()?)
        }
        "previous" => {
            ensure_no_args(&args, "previous")?;
            context.emit(&client.previous_track()?)
        }
        "seek" => {
            let position = exactly_one(args, "seek requires <milliseconds|mm:ss>")?;
            context.emit(&client.seek(parse_position(&position)?)?)
        }
        "status" => {
            ensure_no_args(&args, "status")?;
            context.emit(&client.poll()?)
        }
        "queue" => {
            ensure_no_args(&args, "queue")?;
            context.emit(&client.queue()?)
        }
        "repeat" => {
            let mode = exactly_one(args, "repeat requires <off|one|all>")?;
            if !matches!(mode.as_str(), "off" | "one" | "all") {
                return Err(CliError::usage("repeat requires <off|one|all>"));
            }
            context.emit(&client.set_repeat_mode(&mode)?)
        }
        "shuffle" => {
            let enabled = parse_on_off(&exactly_one(args, "shuffle requires <on|off>")?)?;
            context.emit(&client.set_shuffle(enabled)?)
        }
        "lifecycle" => run_lifecycle(context, client, args),
        _ => Err(CliError::usage(format!(
            "unknown playback command `{command}`; type `help`"
        ))),
    }
}

fn run_lifecycle(
    context: &CliContext,
    client: &mut SilentAppClient,
    mut args: Vec<String>,
) -> CliResult<()> {
    if args.is_empty() {
        return Err(CliError::usage(
            "lifecycle requires interruption-begin, interruption-end, or output-disconnected",
        ));
    }
    let event = args.remove(0);
    match event.as_str() {
        "interruption-begin" => {
            ensure_no_args(&args, "lifecycle interruption-begin")?;
            context.emit(&client.audio_interruption_began()?)
        }
        "interruption-end" => {
            let should_resume = parse_on_off(&exactly_one(
                args,
                "lifecycle interruption-end requires <on|off>",
            )?)?;
            context.emit(&client.audio_interruption_ended(should_resume)?)
        }
        "output-disconnected" => {
            ensure_no_args(&args, "lifecycle output-disconnected")?;
            context.emit(&client.audio_output_disconnected()?)
        }
        _ => Err(CliError::usage(format!(
            "unknown lifecycle event `{event}`"
        ))),
    }
}

fn resolve_queue(
    client: &mut SilentAppClient,
    selectors: &[String],
) -> CliResult<(Vec<PathBuf>, PathBuf)> {
    let mut paths = Vec::with_capacity(selectors.len());
    for selector in selectors {
        paths.push(resolve_track(client, selector)?.path);
    }
    let first = paths
        .first()
        .cloned()
        .ok_or_else(|| CliError::usage("queue cannot be empty"))?;
    Ok((paths, first))
}

fn parse_position(value: &str) -> CliResult<u64> {
    if let Some((minutes, seconds)) = value.split_once(':') {
        let minutes = minutes
            .parse::<u64>()
            .map_err(|_| CliError::usage("seek position must be milliseconds or mm:ss"))?;
        let seconds = seconds
            .parse::<u64>()
            .map_err(|_| CliError::usage("seek position must be milliseconds or mm:ss"))?;
        if seconds >= 60 {
            return Err(CliError::usage("seek seconds must be below 60"));
        }
        return Ok(minutes
            .saturating_mul(60_000)
            .saturating_add(seconds.saturating_mul(1_000)));
    }
    value
        .parse()
        .map_err(|_| CliError::usage("seek position must be milliseconds or mm:ss"))
}

fn parse_on_off(value: &str) -> CliResult<bool> {
    match value {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(CliError::usage("expected `on` or `off`")),
    }
}

fn exactly_one(args: Vec<String>, message: &str) -> CliResult<String> {
    if args.len() == 1 {
        Ok(args[0].clone())
    } else {
        Err(CliError::usage(message))
    }
}

fn ensure_no_args(args: &[String], command: &str) -> CliResult<()> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(CliError::usage(format!(
            "{command} does not accept arguments"
        )))
    }
}

fn split_command_line(input: &str) -> CliResult<Vec<String>> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    for character in input.trim().chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if let Some(expected) = quote {
            if character == expected {
                quote = None;
            } else {
                current.push(character);
            }
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            value if value.is_whitespace() => {
                if !current.is_empty() {
                    arguments.push(std::mem::take(&mut current));
                }
            }
            value => current.push(value),
        }
    }
    if escaped {
        return Err(CliError::usage("unfinished escape at end of command"));
    }
    if quote.is_some() {
        return Err(CliError::usage("unterminated quote"));
    }
    if !current.is_empty() {
        arguments.push(current);
    }
    Ok(arguments)
}

fn print_shell_help() {
    println!(
        "\
Playback shell commands:
  play <path-or-view-id>              Play the selected track in library order
  load <selector>...                  Load and play an explicit queue
  pause | resume | stop
  next | previous
  seek <milliseconds|mm:ss>
  status | queue
  repeat off|one|all
  shuffle on|off
  lifecycle interruption-begin
  lifecycle interruption-end on|off
  lifecycle output-disconnected
  help | quit"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_parser_preserves_quoted_paths() {
        let parsed = split_command_line(r#"play "/Music/My Song.flac""#).unwrap();
        assert_eq!(parsed, ["play", "/Music/My Song.flac"]);
    }

    #[test]
    fn seek_supports_milliseconds_and_clock_time() {
        assert_eq!(parse_position("1234").unwrap(), 1234);
        assert_eq!(parse_position("2:03").unwrap(), 123_000);
        assert!(parse_position("1:99").is_err());
    }

    #[test]
    fn boolean_inputs_are_exact() {
        assert!(parse_on_off("on").unwrap());
        assert!(!parse_on_off("off").unwrap());
        assert!(parse_on_off("true").is_err());
        assert!(parse_on_off("1").is_err());
    }
}
