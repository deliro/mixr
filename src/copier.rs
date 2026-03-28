use std::fs;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use crate::types::{ByteSize, FileEntry};

const BUF_SIZE: usize = 64 * 1024;

pub enum CopyMsg {
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

pub fn copy_files(
    files: &[FileEntry],
    destination: &Path,
    keep_names: bool,
    overwrite: bool,
    tx: &Sender<CopyMsg>,
    shutdown: &Arc<AtomicBool>,
) {
    if let Err(e) = fs::create_dir_all(destination) {
        let _ = tx.send(CopyMsg::Error {
            index: 0_usize,
            path: destination.to_path_buf(),
            error: e.to_string(),
            is_destination: true,
        });
        return;
    }

    let mut counter = 1_usize;
    for (index, entry) in files.iter().enumerate() {
        if shutdown.load(Ordering::Acquire) {
            let _ = tx.send(CopyMsg::Aborted);
            return;
        }

        let dest_path = if keep_names {
            if overwrite {
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
            let (path, next) = dest_path_numbered(destination, counter, &entry.path, overwrite);
            counter = next;
            path
        };

        let name = dest_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let _ = tx.send(CopyMsg::FileStart {
            index,
            name,
            original_path: entry.path.clone(),
            size: entry.size,
        });

        match copy_single(&entry.path, &dest_path, tx, shutdown) {
            Ok(()) => {
                let _ = tx.send(CopyMsg::FileDone { index });
            }
            Err((error, is_destination)) => {
                if is_destination {
                    let _ = cleanup_partial(&dest_path);
                }
                let _ = tx.send(CopyMsg::Error {
                    index,
                    path: entry.path.clone(),
                    error: error.to_string(),
                    is_destination,
                });
                if is_destination {
                    return;
                }
            }
        }
    }

    let _ = tx.send(CopyMsg::Complete);
}

fn copy_single(
    source: &Path,
    dest: &Path,
    tx: &Sender<CopyMsg>,
    shutdown: &Arc<AtomicBool>,
) -> Result<(), (io::Error, bool)> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| (e, true))?;
    }

    let src_file = fs::File::open(source).map_err(|e| (e, false))?;
    let dest_file = fs::File::create(dest).map_err(|e| (e, true))?;

    let mut reader = BufReader::with_capacity(BUF_SIZE, src_file);
    let mut writer = BufWriter::with_capacity(BUF_SIZE, dest_file);
    let mut buf = vec![0_u8; BUF_SIZE];

    loop {
        if shutdown.load(Ordering::Acquire) {
            drop(writer);
            let _ = cleanup_partial(dest);
            return Err((io::Error::new(io::ErrorKind::Interrupted, "shutdown"), true));
        }

        let bytes_read = reader.read(&mut buf).map_err(|e| (e, false))?;
        if bytes_read == 0_usize {
            break;
        }

        writer
            .write_all(buf.get(..bytes_read).unwrap_or(&buf))
            .map_err(|e| (e, true))?;
        #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
        let written = bytes_read as u64;
        let _ = tx.send(CopyMsg::Progress {
            bytes_written: written,
        });
    }

    writer.flush().map_err(|e| (e, true))?;
    Ok(())
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
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn make_source_files(dir: &Path) -> Vec<FileEntry> {
        let f1 = dir.join("song1.mp3");
        let f2 = dir.join("song2.flac");
        fs::write(&f1, vec![1_u8; 5000]).unwrap();
        fs::write(&f2, vec![2_u8; 3000]).unwrap();
        vec![
            FileEntry {
                path: f1,
                size: ByteSize(5000),
            },
            FileEntry {
                path: f2,
                size: ByteSize(3000),
            },
        ]
    }

    #[test]
    fn copy_numbered() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let files = make_source_files(src.path());
        let (tx, rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));

        copy_files(&files, dst.path(), false, false, &tx, &shutdown);

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
        let (tx, rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));

        copy_files(&files, dst.path(), true, false, &tx, &shutdown);

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
        }];
        let (tx, _rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));

        copy_files(&files, dst.path(), true, false, &tx, &shutdown);

        assert!(dst.path().join("(1) song.mp3").exists());
    }

    #[test]
    fn copy_skips_missing_source() {
        let dst = tempfile::tempdir().unwrap();
        let files = vec![FileEntry {
            path: PathBuf::from("/nonexistent/song.mp3"),
            size: ByteSize(100),
        }];
        let (tx, rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));

        copy_files(&files, dst.path(), false, false, &tx, &shutdown);

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
        let (tx, _rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(true));

        copy_files(&files, dst.path(), false, false, &tx, &shutdown);

        assert!(!dst.path().join("00001.mp3").exists());
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
}
