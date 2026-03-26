mod app;
mod cli;
mod copier;
mod filters;
mod scanner;
mod tui;
mod types;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use filters::resolve_extensions;
use types::{ByteSize, Config, DEFAULT_EXTENSIONS};

#[derive(Parser)]
#[command(name = "mixr", version, about = "Fill your flash drive with random music")]
struct Args {
    source: Option<PathBuf>,

    destination: Option<PathBuf>,

    #[arg(long, value_parser = parse_byte_size)]
    size: Option<ByteSize>,

    #[arg(long, value_parser = parse_byte_size)]
    min_size: Option<ByteSize>,

    #[arg(long)]
    no_live: bool,

    #[arg(long, value_delimiter = ',')]
    include: Option<Vec<String>>,

    #[arg(long, value_delimiter = ',')]
    exclude: Option<Vec<String>>,

    #[arg(long)]
    keep_names: bool,
}

fn parse_byte_size(s: &str) -> Result<ByteSize, String> {
    ByteSize::parse(s).map_err(|e| e.to_string())
}

fn main() -> ExitCode {
    let args = Args::parse();

    match (args.source, args.destination) {
        (Some(source), Some(destination)) => {
            if !source.exists() {
                eprintln!("Error: source not found: {}", source.display());
                return ExitCode::FAILURE;
            }

            let allowed_extensions =
                resolve_extensions(args.include.as_ref(), args.exclude.as_ref(), DEFAULT_EXTENSIONS);

            let config = Config {
                source,
                destination,
                max_size: args.size,
                min_file_size: args.min_size,
                no_live: args.no_live,
                keep_names: args.keep_names,
                allowed_extensions,
            };

            match cli::run(&config) {
                Ok(true) => ExitCode::SUCCESS,
                Ok(false) => ExitCode::FAILURE,
                Err(e) => {
                    eprintln!("Error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        (None, None) => match tui::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("Error: {e}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("Error: both SOURCE and DESTINATION are required in CLI mode");
            eprintln!("Usage: mixr [OPTIONS] <SOURCE> <DESTINATION>");
            eprintln!("Run without arguments for interactive TUI mode");
            ExitCode::FAILURE
        }
    }
}
