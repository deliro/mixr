use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;

use mixr::cli;
use mixr::filters::resolve_extensions;
use mixr::i18n;
use mixr::tui;
use mixr::types::{ByteSize, Config, DEFAULT_EXTENSIONS, Encoding, VbrQuality, parse_duration};

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

    #[arg(long, value_parser = parse_duration_arg)]
    min_duration: Option<Duration>,

    #[arg(long, default_value = "keep")]
    encoding: String,

    #[arg(long)]
    bitrate: Option<u16>,

    #[arg(long)]
    quality: Option<String>,
}

fn parse_byte_size(s: &str) -> Result<ByteSize, String> {
    ByteSize::parse(s).map_err(|e| e.to_string())
}

fn parse_duration_arg(s: &str) -> Result<Duration, String> {
    parse_duration(s).map_err(|e| e.to_string())
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

            let encoding = match args.encoding.as_str() {
                "keep" => Encoding::Keep,
                "cbr" => Encoding::Cbr,
                "vbr" => Encoding::Vbr,
                other => {
                    eprintln!("Unknown encoding: {other}. Use: keep, cbr, vbr");
                    return ExitCode::FAILURE;
                }
            };

            let cbr_bitrate = if encoding == Encoding::Cbr {
                if let Some(br) = args.bitrate {
                    Some(br)
                } else {
                    eprintln!("{}", locale.err_bitrate_required);
                    return ExitCode::FAILURE;
                }
            } else {
                None
            };

            let vbr_quality = if encoding == Encoding::Vbr {
                Some(match args.quality.as_deref() {
                    Some("high") => VbrQuality::High,
                    Some("medium") | None => VbrQuality::Medium,
                    Some("low") => VbrQuality::Low,
                    Some(other) => {
                        eprintln!("Unknown quality: {other}. Use: high, medium, low");
                        return ExitCode::FAILURE;
                    }
                })
            } else {
                None
            };

            let config = Config {
                source,
                destination,
                max_size: args.size,
                min_file_size: args.min_size,
                min_duration: args.min_duration,
                no_live: args.no_live,
                keep_names: args.keep_names,
                overwrite: args.overwrite,
                allowed_extensions,
                encoding,
                cbr_bitrate,
                vbr_quality,
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
