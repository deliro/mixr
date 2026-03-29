use std::io::{self, Write};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;

use crate::app::{Effect, FileStatus, Model, Msg, Phase, update};
use crate::copier;
use crate::filters::FilterSet;
use crate::i18n::Locale;
use crate::scanner;
use crate::types::{ByteSize, Config, Error, format_duration};

#[allow(clippy::too_many_lines, clippy::unnecessary_wraps)]
pub fn run(config: &Config, locale: &'static Locale) -> Result<bool, Error> {
    let mut model = Model::new_cli(config.clone(), locale);
    let (tx, rx) = mpsc::channel::<Msg>();
    let mut stderr = io::stderr().lock();

    let budget = match &config.max_size {
        Some(s) => s.as_u64(),
        None => fs4::available_space(&config.destination).unwrap_or(u64::MAX),
    };

    let filters = FilterSet::new(
        config.allowed_extensions.clone(),
        config.min_file_size,
        config.min_duration,
        config.no_live,
    );

    let source = config.source.clone();
    let needs_probe =
        config.min_duration.is_some() || config.encoding != crate::types::Encoding::Keep;
    let scan_tx = tx.clone();
    let scan_shutdown = Arc::clone(&model.shutdown);
    thread::spawn(move || {
        let (stx, srx) = mpsc::channel();
        thread::spawn(move || {
            for msg in srx {
                if scan_tx.send(Msg::Scan(msg)).is_err() {
                    break;
                }
            }
        });
        scanner::scan(&source, &filters, budget, needs_probe, &stx, &scan_shutdown);
    });

    let _ = write!(
        stderr,
        "{} {}... ",
        locale.scanning,
        config.source.display()
    );
    let _ = stderr.flush();

    let mut had_errors = false;
    let mut last_printed_index: Option<usize> = None;

    while let Ok(msg) = rx.recv() {
        let effect = update(&mut model, msg);

        match effect {
            Effect::StartCopy { files, config } => {
                let _ = writeln!(stderr);
                let total: u64 = files.iter().map(|f| f.size.as_u64()).sum();
                let _ = writeln!(
                    stderr,
                    "{} {} ({}) -> {}",
                    locale.cli_copying,
                    files.len(),
                    ByteSize(total),
                    config.destination.display(),
                );
                let _ = writeln!(stderr);

                let shutdown = Arc::clone(&model.shutdown);
                let copy_tx = tx.clone();
                thread::spawn(move || {
                    let (stx, srx) = mpsc::channel();
                    thread::spawn(move || {
                        for msg in srx {
                            if copy_tx.send(Msg::Copy(msg)).is_err() {
                                break;
                            }
                        }
                    });
                    copier::copy_files(&files, &config, &stx, &shutdown);
                });
            }
            Effect::Quit => break,
            Effect::StartScan(_) | Effect::None => {}
        }

        match &model.phase {
            Phase::Scanning { state, .. } => {
                let _ = write!(
                    stderr,
                    "\r{} {}... {} {}, {} {}",
                    locale.scanning,
                    config.source.display(),
                    state.files_found,
                    locale.found,
                    state.files_matched,
                    locale.matched,
                );
                let _ = stderr.flush();
            }
            Phase::Copying(cs) => {
                if let Some(cur) = cs.current() {
                    let idx = cs.current_index;
                    if last_printed_index != Some(idx) && matches!(cur.status, FileStatus::Copying)
                    {
                        last_printed_index = Some(idx);
                        let marker = if cur.converting {
                            let orig_ext = cur
                                .original_path
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("")
                                .to_lowercase();
                            if orig_ext == "mp3" {
                                " [reencoding]"
                            } else {
                                " [converting]"
                            }
                        } else {
                            ""
                        };
                        let _ = writeln!(
                            stderr,
                            "[{:>width$}/{}]  {} <- {} ({}){marker}",
                            idx.saturating_add(1),
                            cs.total_files,
                            cur.name,
                            cur.original_path.display(),
                            cur.size,
                            width = cs.total_files.to_string().len(),
                        );
                        let _ = stderr.flush();
                    }
                }
            }
            Phase::Done {
                total_files,
                total_bytes,
                errors,
                elapsed,
            } => {
                if !errors.is_empty() {
                    let log_path = config.destination.join("mixr-errors.log");
                    let _ = std::fs::write(&log_path, errors.join("\n"));
                }
                let _ = writeln!(stderr);
                #[allow(
                    clippy::cast_precision_loss,
                    clippy::as_conversions,
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss
                )]
                let speed = if elapsed.as_secs_f64() > 0.0_f64 {
                    ByteSize((*total_bytes as f64 / elapsed.as_secs_f64()) as u64)
                } else {
                    ByteSize(0)
                };
                let _ = writeln!(
                    stderr,
                    "{}: {} files, {} in {}, {speed}/s, {} {}",
                    locale.cli_done,
                    total_files,
                    ByteSize(*total_bytes),
                    format_duration(*elapsed),
                    errors.len(),
                    locale.cli_errors,
                );
                had_errors = !errors.is_empty();
                break;
            }
            Phase::FatalError(msg) => {
                let _ = writeln!(stderr, "\n{}: {msg}", locale.cli_fatal);
                had_errors = true;
                break;
            }
            Phase::Setup(_) => {}
        }
    }

    Ok(!had_errors)
}
