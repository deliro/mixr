use std::io::{self, Write};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use crate::app::{update, Effect, FileStatus, Model, Msg, Phase};
use crate::copier;
use crate::filters::FilterSet;
use crate::scanner;
use crate::types::{format_duration, ByteSize, Config, Error};

#[allow(clippy::too_many_lines, clippy::unnecessary_wraps)]
pub fn run(config: &Config) -> Result<bool, Error> {
    let mut model = Model::new_cli(config.clone());
    let (tx, rx) = mpsc::channel::<Msg>();
    let mut stderr = io::stderr().lock();

    let budget = match &config.max_size {
        Some(s) => s.as_u64(),
        None => fs4::available_space(&config.destination).unwrap_or(u64::MAX),
    };

    let filters = FilterSet::new(
        config.allowed_extensions.clone(),
        config.min_file_size,
        config.no_live,
    );

    let source = config.source.clone();
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
        scanner::scan(&source, &filters, budget, &stx, &scan_shutdown);
    });

    let _ = write!(
        stderr,
        "Scanning {}... ",
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
                    "Copying {} files ({}) to {}",
                    files.len(),
                    ByteSize(total),
                    config.destination.display(),
                );
                let _ = writeln!(stderr);

                let shutdown = Arc::clone(&model.shutdown);
                let destination = config.destination.clone();
                let keep_names = config.keep_names;
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
                    copier::copy_files(&files, &destination, keep_names, &stx, &shutdown);
                });
            }
            Effect::Quit => break,
            Effect::StartScan(_) | Effect::None => {}
        }

        match &model.phase {
            Phase::Scanning { state, .. } => {
                let _ = write!(
                    stderr,
                    "\rScanning {}... {} found, {} matched",
                    config.source.display(),
                    state.files_found,
                    state.files_matched,
                );
                let _ = stderr.flush();
            }
            Phase::Copying(cs) => {
                if let Some(cur) = cs.current() {
                    let idx = cs.current_index;
                    if last_printed_index != Some(idx)
                        && matches!(cur.status, FileStatus::Copying)
                    {
                        last_printed_index = Some(idx);
                        let _ = writeln!(
                            stderr,
                            "[{:>width$}/{}]  {} <- {} ({})",
                            idx.saturating_add(1),
                            cs.total_files,
                            cur.name,
                            cur.original_path.display(),
                            cur.size,
                            width = cs.total_files.to_string().len(),
                        );
                    }
                }
            }
            Phase::Done {
                total_files,
                total_bytes,
                errors,
                elapsed,
            } => {
                let _ = writeln!(stderr);
                #[allow(clippy::cast_precision_loss, clippy::as_conversions, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let speed = if elapsed.as_secs_f64() > 0.0_f64 {
                    ByteSize((*total_bytes as f64 / elapsed.as_secs_f64()) as u64)
                } else {
                    ByteSize(0)
                };
                let _ = writeln!(
                    stderr,
                    "Done: {total_files} files, {} copied in {}, {speed}/s, {} errors",
                    ByteSize(*total_bytes),
                    format_duration(*elapsed),
                    errors.len(),
                );
                had_errors = !errors.is_empty();
                break;
            }
            Phase::FatalError(msg) => {
                let _ = writeln!(stderr, "\nFatal error: {msg}");
                had_errors = true;
                break;
            }
            Phase::Setup(_) => {}
        }
    }

    Ok(!had_errors)
}
