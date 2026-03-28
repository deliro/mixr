use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use rand::rng;
use rand::seq::SliceRandom;
use walkdir::WalkDir;

use crate::filters::FilterSet;
use crate::types::{ByteSize, FileEntry};

#[allow(dead_code)]
pub enum ScanMsg {
    FileFound { path: PathBuf, matched: bool },
    Complete(Vec<FileEntry>),
    Error(String),
}

pub fn scan(
    source: &Path,
    filters: &FilterSet,
    budget: u64,
    tx: &Sender<ScanMsg>,
    shutdown: &Arc<AtomicBool>,
) {
    let mut entries: Vec<FileEntry> = Vec::new();

    for result in WalkDir::new(source) {
        if shutdown.load(Ordering::Acquire) {
            return;
        }

        let entry = match result {
            Ok(e) => e,
            Err(e) => {
                let path = e.path().unwrap_or(Path::new("unknown")).to_path_buf();
                let _ = tx.send(ScanMsg::FileFound {
                    path,
                    matched: false,
                });
                continue;
            }
        };

        if entry.file_type().is_dir() {
            continue;
        }

        let path = entry.path().to_path_buf();
        let Ok(meta) = entry.metadata() else {
            let _ = tx.send(ScanMsg::FileFound {
                path,
                matched: false,
            });
            continue;
        };
        let size = meta.len();

        let matched = filters.matches(&path, size);
        let _ = tx.send(ScanMsg::FileFound {
            path: path.clone(),
            matched,
        });

        if matched {
            entries.push(FileEntry {
                path,
                size: ByteSize(size),
                duration: None,
                bitrate_kbps: None,
            });
        }
    }

    entries.shuffle(&mut rng());
    let selected = pack_into_budget(entries, budget);
    let _ = tx.send(ScanMsg::Complete(selected));
}

fn pack_into_budget(files: Vec<FileEntry>, budget: u64) -> Vec<FileEntry> {
    let mut selected = Vec::new();
    let mut remaining = budget;
    let mut consecutive_skips = 0_u32;
    let max_skips = 10_u32;

    for file in files {
        let size = file.size.as_u64();
        if size <= remaining {
            remaining = remaining.saturating_sub(size);
            selected.push(file);
            consecutive_skips = 0_u32;
        } else {
            consecutive_skips = consecutive_skips.saturating_add(1);
            if consecutive_skips >= max_skips {
                break;
            }
        }
    }

    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;

    fn create_test_tree(dir: &Path) {
        let artist1 = dir.join("Artist1").join("Album1");
        let artist2 = dir.join("Artist2");
        let live_dir = dir.join("Artist3").join("Live At Somewhere");
        fs::create_dir_all(&artist1).unwrap();
        fs::create_dir_all(&artist2).unwrap();
        fs::create_dir_all(&live_dir).unwrap();

        fs::write(artist1.join("track1.mp3"), vec![0_u8; 5000]).unwrap();
        fs::write(artist1.join("track2.flac"), vec![0_u8; 8000]).unwrap();
        fs::write(artist2.join("song.mp3"), vec![0_u8; 3000]).unwrap();
        fs::write(artist2.join("cover.jpg"), vec![0_u8; 1000]).unwrap();
        fs::write(artist2.join("tiny.mp3"), vec![0_u8; 100]).unwrap();
        fs::write(live_dir.join("concert.mp3"), vec![0_u8; 6000]).unwrap();
    }

    #[test]
    fn scan_finds_matching_files() {
        let dir = tempfile::tempdir().unwrap();
        create_test_tree(dir.path());

        let filters = FilterSet::new(vec!["mp3".to_string(), "flac".to_string()], None, false);
        let (tx, rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));
        scan(dir.path(), &filters, u64::MAX, &tx, &shutdown);

        let mut messages: Vec<ScanMsg> = rx.try_iter().collect();
        let complete = messages.pop().unwrap();
        match complete {
            ScanMsg::Complete(files) => {
                assert_eq!(files.len(), 5);
            }
            _ => panic!("expected Complete"),
        }
    }

    #[test]
    fn scan_respects_live_filter() {
        let dir = tempfile::tempdir().unwrap();
        create_test_tree(dir.path());

        let filters = FilterSet::new(vec!["mp3".to_string(), "flac".to_string()], None, true);
        let (tx, rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));
        scan(dir.path(), &filters, u64::MAX, &tx, &shutdown);

        let messages: Vec<ScanMsg> = rx.try_iter().collect();
        let complete = messages.last().unwrap();
        match complete {
            ScanMsg::Complete(files) => {
                assert_eq!(files.len(), 4);
                assert!(
                    !files
                        .iter()
                        .any(|f| f.path.to_str().unwrap().contains("concert"))
                );
            }
            _ => panic!("expected Complete"),
        }
    }

    #[test]
    fn scan_respects_min_size() {
        let dir = tempfile::tempdir().unwrap();
        create_test_tree(dir.path());

        let filters = FilterSet::new(vec!["mp3".to_string()], Some(ByteSize(1000)), false);
        let (tx, rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));
        scan(dir.path(), &filters, u64::MAX, &tx, &shutdown);

        let messages: Vec<ScanMsg> = rx.try_iter().collect();
        let complete = messages.last().unwrap();
        match complete {
            ScanMsg::Complete(files) => {
                assert!(files.iter().all(|f| f.size.as_u64() >= 1000));
                assert!(
                    !files
                        .iter()
                        .any(|f| f.path.to_str().unwrap().contains("tiny"))
                );
            }
            _ => panic!("expected Complete"),
        }
    }

    #[test]
    fn pack_into_budget_respects_limit() {
        let files = vec![
            FileEntry {
                path: PathBuf::from("a.mp3"),
                size: ByteSize(5000),
                duration: None,
                bitrate_kbps: None,
            },
            FileEntry {
                path: PathBuf::from("b.mp3"),
                size: ByteSize(3000),
                duration: None,
                bitrate_kbps: None,
            },
            FileEntry {
                path: PathBuf::from("c.mp3"),
                size: ByteSize(4000),
                duration: None,
                bitrate_kbps: None,
            },
        ];
        let selected = pack_into_budget(files, 8000);
        let total: u64 = selected.iter().map(|f| f.size.as_u64()).sum();
        assert!(total <= 8000);
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn pack_stops_after_consecutive_skips() {
        let files = vec![FileEntry {
            path: PathBuf::from("huge.mp3"),
            size: ByteSize(1_000_000),
            duration: None,
            bitrate_kbps: None,
        }];
        let selected = pack_into_budget(files, 100);
        assert!(selected.is_empty());
    }
}
