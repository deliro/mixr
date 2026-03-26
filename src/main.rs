mod app;
mod cli;
mod copier;
mod filters;
mod i18n;
mod scanner;
mod tui;
mod types;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use filters::resolve_extensions;
use types::{ByteSize, Config, DEFAULT_EXTENSIONS};

#[derive(Parser)]
#[command(
    name = "mixr",
    version,
    about = "Fill your flash drive with random music"
)]
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

    #[arg(long)]
    overwrite: bool,
}

fn parse_byte_size(s: &str) -> Result<ByteSize, String> {
    ByteSize::parse(s).map_err(|e| e.to_string())
}

fn main() -> ExitCode {
    let locale = i18n::detect();
    let args = Args::parse();

    match (args.source, args.destination) {
        (Some(source), Some(destination)) => {
            if !source.exists() {
                eprintln!("{}: {}", locale.err_source_not_found, source.display());
                return ExitCode::FAILURE;
            }

            let allowed_extensions = resolve_extensions(
                args.include.as_ref(),
                args.exclude.as_ref(),
                DEFAULT_EXTENSIONS,
            );

            let config = Config {
                source,
                destination,
                max_size: args.size,
                min_file_size: args.min_size,
                no_live: args.no_live,
                keep_names: args.keep_names,
                overwrite: args.overwrite,
                allowed_extensions,
            };

            match cli::run(&config, locale) {
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
            eprintln!("{}", locale.err_both_required);
            eprintln!("{}", locale.err_usage);
            eprintln!("{}", locale.err_run_tui);
            ExitCode::FAILURE
        }
    }
}
