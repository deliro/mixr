use std::fs;
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::mpsc::Sender;
use std::thread;

use crate::types::{ByteSize, Config, Encoding, FileEntry, VbrQuality};

const BUF_SIZE: usize = 1024 * 1024;
const PIPE_CAPACITY: usize = 16_usize;
const TRANSCODE_BUF_THRESHOLD: usize = 256 * 1024;

pub enum CopyMsg {
    Preparing {
        index: usize,
        converting: bool,
    },
    FileStart {
        index: usize,
        name: String,
        original_path: PathBuf,
        size: ByteSize,
    },
    Progress {
        bytes_written: u64,
    },
    FileDone {
        index: usize,
    },
    Error {
        index: usize,
        path: PathBuf,
        error: String,
        is_destination: bool,
    },
    Complete,
    Aborted,
}

enum PipeMsg {
    Preparing {
        index: usize,
        converting: bool,
    },
    StartFile {
        index: usize,
        dest_path: PathBuf,
        name: String,
        original_path: PathBuf,
        size: ByteSize,
    },
    Chunk(Vec<u8>),
    EndFile {
        index: usize,
    },
    SkipFile {
        index: usize,
        path: PathBuf,
        error: String,
    },
    Done,
    Abort,
}

pub fn copy_files(
    files: &[FileEntry],
    config: &Config,
    tx: &Sender<CopyMsg>,
    shutdown: &Arc<AtomicBool>,
) {
    if let Err(e) = fs::create_dir_all(&config.destination) {
        let _ = tx.send(CopyMsg::Error {
            index: 0_usize,
            path: config.destination.clone(),
            error: e.to_string(),
            is_destination: true,
        });
        return;
    }

    let (pipe_tx, pipe_rx) = mpsc::sync_channel::<PipeMsg>(PIPE_CAPACITY);

    let progress_tx = tx.clone();
    let writer_shutdown = Arc::clone(shutdown);

    let writer_handle = thread::spawn(move || {
        writer_thread(pipe_rx, &progress_tx, &writer_shutdown);
    });

    reader_thread(files, config, &pipe_tx, shutdown);

    drop(pipe_tx);
    let _ = writer_handle.join();
}

fn needs_transcode(entry: &FileEntry, config: &Config) -> bool {
    match config.encoding {
        Encoding::Keep => false,
        Encoding::Cbr | Encoding::Vbr => {
            let ext = entry
                .path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if ext != "mp3" {
                return true;
            }
            let threshold = match config.encoding {
                Encoding::Cbr => u32::from(config.cbr_bitrate.unwrap_or(0_u16)),
                Encoding::Vbr => u32::from(
                    config
                        .vbr_quality
                        .unwrap_or(VbrQuality::Medium)
                        .avg_bitrate_kbps(),
                ),
                Encoding::Keep => return false,
            };
            entry.bitrate_kbps.is_some_and(|br| br > threshold)
        }
    }
}

fn reader_thread(
    files: &[FileEntry],
    config: &Config,
    pipe_tx: &mpsc::SyncSender<PipeMsg>,
    shutdown: &Arc<AtomicBool>,
) {
    let mut counter = 1_usize;
    let destination = &config.destination;

    for (index, entry) in files.iter().enumerate() {
        if shutdown.load(Ordering::Relaxed) {
            let _ = pipe_tx.send(PipeMsg::Abort);
            return;
        }

        let dest_path = if config.keep_names {
            if config.overwrite {
                destination.join(
                    entry
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown"),
                )
            } else {
                dest_path_keep_name(destination, &entry.path)
            }
        } else {
            let (path, next) =
                dest_path_numbered(destination, counter, &entry.path, config.overwrite);
            counter = next;
            path
        };

        if needs_transcode(entry, config) {
            transcode_file(index, entry, &dest_path, config, pipe_tx);
        } else {
            let _ = pipe_tx.send(PipeMsg::Preparing {
                index,
                converting: false,
            });

            let name = dest_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            let src_file = match fs::File::open(&entry.path) {
                Ok(f) => f,
                Err(e) => {
                    let _ = pipe_tx.send(PipeMsg::SkipFile {
                        index,
                        path: entry.path.clone(),
                        error: e.to_string(),
                    });
                    continue;
                }
            };

            let _ = pipe_tx.send(PipeMsg::StartFile {
                index,
                dest_path,
                name,
                original_path: entry.path.clone(),
                size: entry.size,
            });

            if !read_file_chunks(src_file, index, &entry.path, pipe_tx) {
                continue;
            }

            let _ = pipe_tx.send(PipeMsg::EndFile { index });
        }
    }

    let _ = pipe_tx.send(PipeMsg::Done);
}

fn transcode_file(
    index: usize,
    entry: &FileEntry,
    dest_path: &Path,
    config: &Config,
    pipe_tx: &mpsc::SyncSender<PipeMsg>,
) {
    let _ = pipe_tx.send(PipeMsg::Preparing {
        index,
        converting: true,
    });

    let dest_path = dest_path.with_extension("mp3");
    let name = dest_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let _ = pipe_tx.send(PipeMsg::StartFile {
        index,
        dest_path,
        name,
        original_path: entry.path.clone(),
        size: entry.size,
    });

    let tc_config = crate::transcoder::TranscodeConfig {
        encoding: config.encoding,
        cbr_bitrate: config.cbr_bitrate,
        vbr_quality: config.vbr_quality,
    };

    let mut transcode_buf: Vec<u8> = Vec::with_capacity(TRANSCODE_BUF_THRESHOLD);
    let result = crate::transcoder::transcode(&entry.path, &tc_config, &mut |chunk| {
        transcode_buf.extend_from_slice(chunk);
        if transcode_buf.len() >= TRANSCODE_BUF_THRESHOLD {
            let full_buf = std::mem::replace(
                &mut transcode_buf,
                Vec::with_capacity(TRANSCODE_BUF_THRESHOLD),
            );
            let _ = pipe_tx.send(PipeMsg::Chunk(full_buf));
        }
    });
    match result {
        Ok(()) => {
            if !transcode_buf.is_empty() {
                let _ = pipe_tx.send(PipeMsg::Chunk(transcode_buf));
            }
            let _ = pipe_tx.send(PipeMsg::EndFile { index });
        }
        Err(e) => {
            let _ = pipe_tx.send(PipeMsg::SkipFile {
                index,
                path: entry.path.clone(),
                error: e,
            });
        }
    }
}

fn read_file_chunks(
    mut src_file: fs::File,
    index: usize,
    path: &Path,
    pipe_tx: &mpsc::SyncSender<PipeMsg>,
) -> bool {
    let mut buf = vec![0_u8; BUF_SIZE];
    loop {
        match src_file.read(&mut buf) {
            Ok(0_usize) => return true,
            Ok(n) => {
                let chunk = buf.get(..n).unwrap_or(&buf).to_vec();
                if pipe_tx.send(PipeMsg::Chunk(chunk)).is_err() {
                    return false;
                }
            }
            Err(e) => {
                let _ = pipe_tx.send(PipeMsg::SkipFile {
                    index,
                    path: path.to_path_buf(),
                    error: e.to_string(),
                });
                return false;
            }
        }
    }
}

struct WriterState<'a> {
    writer: Option<BufWriter<fs::File>>,
    current_dest: Option<PathBuf>,
    progress_tx: &'a Sender<CopyMsg>,
    shutdown: &'a Arc<AtomicBool>,
}

impl<'a> WriterState<'a> {
    fn new(progress_tx: &'a Sender<CopyMsg>, shutdown: &'a Arc<AtomicBool>) -> Self {
        Self {
            writer: None,
            current_dest: None,
            progress_tx,
            shutdown,
        }
    }

    fn fatal_dest_error(&mut self, index: usize, path: PathBuf, error: String) {
        let _ = self.progress_tx.send(CopyMsg::Error {
            index,
            path,
            error,
            is_destination: true,
        });
        self.shutdown.store(true, Ordering::Relaxed);
    }

    fn handle_start(
        &mut self,
        index: usize,
        dest_path: PathBuf,
        name: String,
        original_path: PathBuf,
        size: ByteSize,
    ) -> bool {
        if let Some(parent) = dest_path.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            self.fatal_dest_error(index, dest_path, e.to_string());
            return false;
        }

        match fs::File::create(&dest_path) {
            Ok(f) => {
                self.writer = Some(BufWriter::with_capacity(BUF_SIZE, f));
                self.current_dest = Some(dest_path);
                let _ = self.progress_tx.send(CopyMsg::FileStart {
                    index,
                    name,
                    original_path,
                    size,
                });
                true
            }
            Err(e) => {
                self.fatal_dest_error(index, dest_path, e.to_string());
                false
            }
        }
    }

    fn handle_chunk(&mut self, data: &[u8]) -> bool {
        if let Some(ref mut w) = self.writer {
            if let Err(e) = w.write_all(data) {
                if let Some(dest) = self.current_dest.take() {
                    let _ = cleanup_partial(&dest);
                    self.fatal_dest_error(0_usize, dest, e.to_string());
                }
                return false;
            }
            #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
            let written = data.len() as u64;
            let _ = self.progress_tx.send(CopyMsg::Progress {
                bytes_written: written,
            });
        }
        true
    }

    fn handle_end(&mut self, index: usize) -> bool {
        if let Some(mut w) = self.writer.take()
            && let Err(e) = w.flush()
        {
            if let Some(dest) = self.current_dest.take() {
                let _ = cleanup_partial(&dest);
                self.fatal_dest_error(index, dest, e.to_string());
            }
            return false;
        }
        self.current_dest = None;
        let _ = self.progress_tx.send(CopyMsg::FileDone { index });
        true
    }

    fn handle_skip(&mut self, index: usize, path: PathBuf, error: String) {
        self.writer = None;
        if let Some(dest) = self.current_dest.take() {
            let _ = cleanup_partial(&dest);
        }
        let _ = self.progress_tx.send(CopyMsg::Error {
            index,
            path,
            error,
            is_destination: false,
        });
    }
}

#[allow(clippy::needless_pass_by_value)]
fn writer_thread(
    pipe_rx: mpsc::Receiver<PipeMsg>,
    progress_tx: &Sender<CopyMsg>,
    shutdown: &Arc<AtomicBool>,
) {
    let mut state = WriterState::new(progress_tx, shutdown);

    for msg in &pipe_rx {
        match msg {
            PipeMsg::Preparing { index, converting } => {
                let _ = progress_tx.send(CopyMsg::Preparing { index, converting });
            }
            PipeMsg::StartFile {
                index,
                dest_path,
                name,
                original_path,
                size,
            } => {
                if !state.handle_start(index, dest_path, name, original_path, size) {
                    break;
                }
            }
            PipeMsg::Chunk(data) => {
                if !state.handle_chunk(&data) {
                    break;
                }
            }
            PipeMsg::EndFile { index } => {
                if !state.handle_end(index) {
                    break;
                }
            }
            PipeMsg::SkipFile { index, path, error } => {
                state.handle_skip(index, path, error);
            }
            PipeMsg::Done => {
                let _ = progress_tx.send(CopyMsg::Complete);
                break;
            }
            PipeMsg::Abort => {
                let _ = progress_tx.send(CopyMsg::Aborted);
                break;
            }
        }
    }
}

fn dest_path_numbered(
    destination: &Path,
    start: usize,
    source: &Path,
    overwrite: bool,
) -> (PathBuf, usize) {
    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("bin");
    let num = (start..)
        .take(100_000_usize)
        .find(|&n| overwrite || !destination.join(format!("{n:05}.{ext}")).exists())
        .unwrap_or(start);
    (
        destination.join(format!("{num:05}.{ext}")),
        num.saturating_add(1),
    )
}

fn dest_path_keep_name(destination: &Path, source: &Path) -> PathBuf {
    let filename = source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    let base = destination.join(filename);
    if !base.exists() {
        return base;
    }

    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("bin");

    (1_u32..=u32::MAX)
        .map(|counter| destination.join(format!("({counter}) {stem}.{ext}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or_else(|| destination.join(format!("{stem}.{ext}")))
}

fn cleanup_partial(path: &Path) -> io::Result<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn keep_config(src: &Path, dst: &Path, keep_names: bool, overwrite: bool) -> Config {
        Config {
            source: src.to_path_buf(),
            destination: dst.to_path_buf(),
            max_size: None,
            min_file_size: None,
            min_duration: None,
            no_live: false,
            keep_names,
            overwrite,
            allowed_extensions: vec![],
            encoding: Encoding::Keep,
            cbr_bitrate: None,
            vbr_quality: None,
        }
    }

    fn make_source_files(dir: &Path) -> Vec<FileEntry> {
        let f1 = dir.join("song1.mp3");
        let f2 = dir.join("song2.flac");
        fs::write(&f1, vec![1_u8; 5000]).unwrap();
        fs::write(&f2, vec![2_u8; 3000]).unwrap();
        vec![
            FileEntry {
                path: f1,
                size: ByteSize(5000),
                duration: None,
                bitrate_kbps: None,
            },
            FileEntry {
                path: f2,
                size: ByteSize(3000),
                duration: None,
                bitrate_kbps: None,
            },
        ]
    }

    #[test]
    fn copy_numbered() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let files = make_source_files(src.path());
        let config = keep_config(src.path(), dst.path(), false, false);
        let (tx, rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));

        copy_files(&files, &config, &tx, &shutdown);

        let messages: Vec<CopyMsg> = rx.try_iter().collect();
        assert!(matches!(messages.last().unwrap(), CopyMsg::Complete));

        let dest1 = dst.path().join("00001.mp3");
        let dest2 = dst.path().join("00002.flac");
        assert!(dest1.exists());
        assert!(dest2.exists());
        assert_eq!(fs::read(&dest1).unwrap(), vec![1_u8; 5000]);
        assert_eq!(fs::read(&dest2).unwrap(), vec![2_u8; 3000]);
    }

    #[test]
    fn copy_keep_names() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let files = make_source_files(src.path());
        let config = keep_config(src.path(), dst.path(), true, false);
        let (tx, rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));

        copy_files(&files, &config, &tx, &shutdown);

        let messages: Vec<CopyMsg> = rx.try_iter().collect();
        assert!(matches!(messages.last().unwrap(), CopyMsg::Complete));
        assert!(dst.path().join("song1.mp3").exists());
        assert!(dst.path().join("song2.flac").exists());
    }

    #[test]
    fn copy_deduplicates_names() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let f1 = src.path().join("song.mp3");
        fs::write(&f1, vec![1_u8; 100]).unwrap();
        fs::write(dst.path().join("song.mp3"), vec![0_u8; 50]).unwrap();

        let files = vec![FileEntry {
            path: f1,
            size: ByteSize(100),
            duration: None,
            bitrate_kbps: None,
        }];
        let config = keep_config(src.path(), dst.path(), true, false);
        let (tx, _rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));

        copy_files(&files, &config, &tx, &shutdown);

        assert!(dst.path().join("(1) song.mp3").exists());
    }

    #[test]
    fn copy_skips_missing_source() {
        let dst = tempfile::tempdir().unwrap();
        let files = vec![FileEntry {
            path: PathBuf::from("/nonexistent/song.mp3"),
            size: ByteSize(100),
            duration: None,
            bitrate_kbps: None,
        }];
        let config = keep_config(Path::new("/nonexistent"), dst.path(), false, false);
        let (tx, rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));

        copy_files(&files, &config, &tx, &shutdown);

        let messages: Vec<CopyMsg> = rx.try_iter().collect();
        let has_error = messages.iter().any(|m| {
            matches!(
                m,
                CopyMsg::Error {
                    is_destination: false,
                    ..
                }
            )
        });
        let has_complete = messages.iter().any(|m| matches!(m, CopyMsg::Complete));
        assert!(has_error);
        assert!(has_complete);
    }

    #[test]
    fn copy_respects_shutdown() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let files = make_source_files(src.path());
        let config = keep_config(src.path(), dst.path(), false, false);
        let (tx, _rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(true));

        copy_files(&files, &config, &tx, &shutdown);

        assert!(!dst.path().join("00001.mp3").exists());
    }

    #[test]
    fn copy_with_transcode_wav_to_mp3() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let wav_path = src.path().join("song.wav");
        crate::probe::tests::create_wav(&wav_path, 44100_u32, 2_u16, 1_u32);

        let files = vec![FileEntry {
            path: wav_path,
            size: ByteSize(176_400),
            duration: Some(std::time::Duration::from_secs(1)),
            bitrate_kbps: Some(1411),
        }];

        let config = Config {
            source: src.path().to_path_buf(),
            destination: dst.path().to_path_buf(),
            max_size: None,
            min_file_size: None,
            min_duration: None,
            no_live: false,
            keep_names: true,
            overwrite: false,
            allowed_extensions: vec![],
            encoding: Encoding::Cbr,
            cbr_bitrate: Some(128_u16),
            vbr_quality: None,
        };

        let (tx, rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));
        copy_files(&files, &config, &tx, &shutdown);

        let messages: Vec<CopyMsg> = rx.try_iter().collect();
        assert!(messages.iter().any(|m| matches!(m, CopyMsg::Complete)));
        assert!(dst.path().join("song.mp3").exists());
    }

    #[test]
    fn dest_path_numbered_format() {
        let dest = Path::new("/usb");
        assert_eq!(
            dest_path_numbered(dest, 1_usize, Path::new("song.mp3"), true).0,
            PathBuf::from("/usb/00001.mp3")
        );
        assert_eq!(
            dest_path_numbered(dest, 100_usize, Path::new("track.flac"), true).0,
            PathBuf::from("/usb/00100.flac")
        );
    }

    #[test]
    fn copy_keeps_mp3_below_threshold() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let mp3_path = src.path().join("song.mp3");
        fs::write(&mp3_path, vec![1_u8; 1000]).unwrap();

        let files = vec![FileEntry {
            path: mp3_path,
            size: ByteSize(1000),
            duration: Some(std::time::Duration::from_secs(10)),
            bitrate_kbps: Some(128),
        }];

        let config = Config {
            source: src.path().to_path_buf(),
            destination: dst.path().to_path_buf(),
            max_size: None,
            min_file_size: None,
            min_duration: None,
            no_live: false,
            keep_names: true,
            overwrite: false,
            allowed_extensions: vec![],
            encoding: Encoding::Cbr,
            cbr_bitrate: Some(192),
            vbr_quality: None,
        };

        let (tx, rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));
        copy_files(&files, &config, &tx, &shutdown);

        let messages: Vec<CopyMsg> = rx.try_iter().collect();
        assert!(messages.iter().any(|m| matches!(m, CopyMsg::Complete)));
        assert!(dst.path().join("song.mp3").exists());
        let content = fs::read(dst.path().join("song.mp3")).unwrap();
        assert_eq!(content, vec![1_u8; 1000]);
    }
}
