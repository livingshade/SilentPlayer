mod command;
mod error;
mod output;
mod shell;

use std::env;
use std::process;

use error::{CliError, CliResult};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    if let Err(error) = run(env::args().skip(1).collect()) {
        eprintln!("silent: {error}");
        process::exit(error.exit_code());
    }
}

fn run(args: Vec<String>) -> CliResult<()> {
    match args.as_slice() {
        [] => {
            print_root_help();
            Ok(())
        }
        [flag] if flag == "--version" || flag == "-V" => {
            println!("silent {VERSION}");
            Ok(())
        }
        [flag] if flag == "--help" || flag == "-h" => {
            print_root_help();
            Ok(())
        }
        [flag, rest @ ..] if flag == "--cli" => command::run(rest.to_vec()),
        _ => Err(CliError::usage(
            "complex commands must start with `silent --cli`; run `silent --help`",
        )),
    }
}

fn print_root_help() {
    println!(
        "\
Silent local music player

Usage:
  silent --version
  silent --help
  silent --cli [global options] <domain> <command> [arguments]

The macOS app, iPhone app, and CLI are peer targets backed by the same Rust
application behavior. Run `silent --cli --help` for the complete CLI."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_rejects_complex_commands_without_cli_boundary() {
        let error = run(vec!["library".to_owned(), "list".to_owned()]).unwrap_err();
        assert_eq!(error.exit_code(), 2);
        assert!(error.to_string().contains("silent --cli"));
    }
}
