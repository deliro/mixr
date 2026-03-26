# mixr Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a TUI/CLI music-to-flash-drive filler with Elm architecture in Rust.

**Architecture:** Single binary, Elm pattern (Model + Msg + update + view). Background threads for IO (scanning, copying) communicate with main thread via `mpsc`. TUI mode (ratatui) when no args, CLI mode with plain text when args provided.

**Tech Stack:** Rust 2024 edition, clap (CLI), ratatui + crossterm (TUI), walkdir (FS traversal), fs4 (disk space), rand (shuffle)

---

## File Structure

```
src/
  main.rs       — clap parsing, mode selection, entry point
  types.rs      — ByteSize newtype, FileEntry, Config, Error, size parsing/formatting
  filters.rs    — extension, min-size, live filters; FilterSet combining them
  scanner.rs    — recursive FS walk, filtering, shuffle, budget packing
  copier.rs     — buffered file copy with progress, dest path generation, error classification
  app.rs        — Phase, Model, Msg, Effect, update function (Elm core)
  tui.rs        — ratatui event loop, view dispatch, setup wizard, progress views
  cli.rs        — plain text mode with line-by-line output
```

---

### Task 1: Project Setup + Types Module

**Files:**
- Modify: `Cargo.toml`
- Create: `src/types.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Update Cargo.toml with dependencies**

```toml
[package]
name = "mixr"
version = "0.1.0"
edition = "2024"

[dependencies]
clap = { version = "4", features = ["derive"] }
ratatui = "0.29"
crossterm = "0.28"
walkdir = "2"
fs4 = "0.13"
rand = "0.9"

[lints.rust]
warnings = "deny"
```

- [ ] **Step 2: Write tests for ByteSize parsing**

Create `src/types.rs`:

```rust
use std::fmt;
use std::path::PathBuf;

const KB: u64 = 1024;
const MB: u64 = 1024 * KB;
const GB: u64 = 1024 * MB;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ByteSize(pub u64);

#[derive(Debug, PartialEq)]
pub enum ParseSizeError {
    Empty,
    InvalidNumber(String),
    NegativeOrZero,
    UnknownUnit(String),
}

impl fmt::Display for ParseSizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "size string is empty"),
            Self::InvalidNumber(s) => write!(f, "invalid number: {s}"),
            Self::NegativeOrZero => write!(f, "size must be greater than zero"),
            Self::UnknownUnit(u) => write!(f, "unknown unit: {u}, expected G/M/K/B"),
        }
    }
}

impl std::error::Error for ParseSizeError {}

impl ByteSize {
    pub fn parse(s: &str) -> Result<Self, ParseSizeError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(ParseSizeError::Empty);
        }

        let idx = s
            .rfind(|c: char| c.is_ascii_digit() || c == '.')
            .ok_or(ParseSizeError::Empty)?;

        let (num_str, unit_str) = s.split_at(idx + 1);
        let num: f64 = num_str
            .parse()
            .map_err(|_| ParseSizeError::InvalidNumber(num_str.to_string()))?;

        if num <= 0.0 {
            return Err(ParseSizeError::NegativeOrZero);
        }

        let unit_lower = unit_str.trim().to_lowercase();
        let multiplier = match unit_lower.as_str() {
            "g" | "gb" => GB,
            "m" | "mb" => MB,
            "k" | "kb" => KB,
            "b" | "" => 1,
            other => return Err(ParseSizeError::UnknownUnit(other.to_string())),
        };

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let result = (num * multiplier as f64) as u64;
        Ok(Self(result))
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ByteSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.0;
        if bytes >= GB {
            write!(f, "{:.1}G", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            write!(f, "{:.1}M", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            write!(f, "{:.1}K", bytes as f64 / KB as f64)
        } else {
            write!(f, "{bytes}B")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_gigabytes() {
        assert_eq!(ByteSize::parse("8G").unwrap(), ByteSize(8 * GB));
        assert_eq!(ByteSize::parse("1.5GB").unwrap(), ByteSize((1.5 * GB as f64) as u64));
        assert_eq!(ByteSize::parse("8gb").unwrap(), ByteSize(8 * GB));
    }

    #[test]
    fn parse_megabytes() {
        assert_eq!(ByteSize::parse("500M").unwrap(), ByteSize(500 * MB));
        assert_eq!(ByteSize::parse("100mb").unwrap(), ByteSize(100 * MB));
    }

    #[test]
    fn parse_kilobytes() {
        assert_eq!(ByteSize::parse("900K").unwrap(), ByteSize(900 * KB));
    }

    #[test]
    fn parse_bytes() {
        assert_eq!(ByteSize::parse("1024B").unwrap(), ByteSize(1024));
        assert_eq!(ByteSize::parse("1024").unwrap(), ByteSize(1024));
    }

    #[test]
    fn parse_errors() {
        assert_eq!(ByteSize::parse(""), Err(ParseSizeError::Empty));
        assert_eq!(ByteSize::parse("0G"), Err(ParseSizeError::NegativeOrZero));
        assert_eq!(
            ByteSize::parse("5X"),
            Err(ParseSizeError::UnknownUnit("x".to_string()))
        );
    }

    #[test]
    fn format_sizes() {
        assert_eq!(format!("{}", ByteSize(8 * GB)), "8.0G");
        assert_eq!(format!("{}", ByteSize(500 * MB)), "500.0M");
        assert_eq!(format!("{}", ByteSize(900 * KB)), "900.0K");
        assert_eq!(format!("{}", ByteSize(512)), "512B");
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --lib types`
Expected: compilation succeeds, all tests pass (types are self-contained)

- [ ] **Step 4: Add remaining types (FileEntry, Config, Error)**

Append to `src/types.rs`:

```rust
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: ByteSize,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub max_size: Option<ByteSize>,
    pub min_file_size: Option<ByteSize>,
    pub no_live: bool,
    pub keep_names: bool,
    pub allowed_extensions: Vec<String>,
}

pub const DEFAULT_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "wav", "m4a", "aac", "wma"];

#[derive(Debug)]
pub enum Error {
    SourceNotFound(PathBuf),
    InvalidSize(ParseSizeError),
    ScanFailed { path: PathBuf, source: std::io::Error },
    ReadFailed { path: PathBuf, source: std::io::Error },
    WriteFailed { path: PathBuf, source: std::io::Error },
    CreateDirFailed { path: PathBuf, source: std::io::Error },
    DiskSpaceQuery { path: PathBuf, source: std::io::Error },
    Terminal(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceNotFound(p) => write!(f, "source not found: {}", p.display()),
            Self::InvalidSize(e) => write!(f, "invalid size: {e}"),
            Self::ScanFailed { path, source } => {
                write!(f, "scan failed at {}: {source}", path.display())
            }
            Self::ReadFailed { path, source } => {
                write!(f, "read failed {}: {source}", path.display())
            }
            Self::WriteFailed { path, source } => {
                write!(f, "write failed {}: {source}", path.display())
            }
            Self::CreateDirFailed { path, source } => {
                write!(f, "failed to create {}: {source}", path.display())
            }
            Self::DiskSpaceQuery { path, source } => {
                write!(f, "disk space query failed for {}: {source}", path.display())
            }
            Self::Terminal(e) => write!(f, "terminal error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<ParseSizeError> for Error {
    fn from(e: ParseSizeError) -> Self {
        Self::InvalidSize(e)
    }
}

pub fn format_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}
```

- [ ] **Step 5: Wire types module into main.rs**

Replace `src/main.rs`:

```rust
mod types;

fn main() {
    println!("mixr");
}
```

- [ ] **Step 6: Run all tests**

Run: `cargo test`
Expected: all tests pass, no warnings

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/types.rs src/main.rs
git commit -m "feat: add types module with ByteSize parsing and core types"
```

---

### Task 2: Filters Module

**Files:**
- Create: `src/filters.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write failing tests for extension filter**

Create `src/filters.rs`:

```rust
use std::path::Path;

use crate::types::ByteSize;

pub struct FilterSet {
    allowed_extensions: Vec<String>,
    min_size: Option<ByteSize>,
    no_live: bool,
}

impl FilterSet {
    pub fn new(
        allowed_extensions: Vec<String>,
        min_size: Option<ByteSize>,
        no_live: bool,
    ) -> Self {
        Self {
            allowed_extensions,
            min_size,
            no_live,
        }
    }

    pub fn matches(&self, path: &Path, size: u64) -> bool {
        self.matches_extension(path)
            && self.matches_min_size(size)
            && self.matches_live(path)
    }

    fn matches_extension(&self, path: &Path) -> bool {
        if self.allowed_extensions.is_empty() {
            return true;
        }
        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e.to_lowercase(),
            None => return false,
        };
        self.allowed_extensions.iter().any(|a| a == &ext)
    }

    fn matches_min_size(&self, size: u64) -> bool {
        match self.min_size {
            Some(min) => size >= min.as_u64(),
            None => true,
        }
    }

    fn matches_live(&self, path: &Path) -> bool {
        if !self.no_live {
            return true;
        }
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        let parent = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        !contains_live_word(&filename) && !contains_live_word(&parent)
    }
}

fn contains_live_word(s: &str) -> bool {
    s.split(|c: char| !c.is_alphanumeric())
        .any(|word| word == "live")
}

pub fn resolve_extensions(
    include: &Option<Vec<String>>,
    exclude: &Option<Vec<String>>,
    defaults: &[&str],
) -> Vec<String> {
    let base: Vec<String> = match include {
        Some(inc) => inc.iter().map(|s| s.to_lowercase()).collect(),
        None => defaults.iter().map(|s| (*s).to_string()).collect(),
    };
    match exclude {
        Some(exc) => {
            let exc_lower: Vec<String> = exc.iter().map(|s| s.to_lowercase()).collect();
            base.into_iter().filter(|e| !exc_lower.contains(e)).collect()
        }
        None => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn extension_filter_matches() {
        let f = FilterSet::new(vec!["mp3".to_string(), "flac".to_string()], None, false);
        assert!(f.matches(Path::new("/music/song.mp3"), 1000));
        assert!(f.matches(Path::new("/music/song.FLAC"), 1000));
        assert!(!f.matches(Path::new("/music/song.wav"), 1000));
        assert!(!f.matches(Path::new("/music/noext"), 1000));
    }

    #[test]
    fn empty_extensions_matches_all() {
        let f = FilterSet::new(vec![], None, false);
        assert!(f.matches(Path::new("/music/anything.xyz"), 1000));
    }

    #[test]
    fn min_size_filter() {
        let f = FilterSet::new(vec![], Some(ByteSize(1000)), false);
        assert!(f.matches(Path::new("/song.mp3"), 1000));
        assert!(f.matches(Path::new("/song.mp3"), 2000));
        assert!(!f.matches(Path::new("/song.mp3"), 999));
    }

    #[test]
    fn live_filter() {
        let f = FilterSet::new(vec![], None, true);
        assert!(!f.matches(Path::new("/music/song live.mp3"), 1000));
        assert!(!f.matches(Path::new("/music/Song (Live).mp3"), 1000));
        assert!(!f.matches(Path::new("/music/live/song.mp3"), 1000));
        assert!(!f.matches(Path::new("/music/Live At Wembley/song.mp3"), 1000));
        assert!(f.matches(Path::new("/music/olive/song.mp3"), 1000));
        assert!(f.matches(Path::new("/music/deliver.mp3"), 1000));
        assert!(f.matches(Path::new("/music/alive.mp3"), 1000));
    }

    #[test]
    fn live_filter_disabled() {
        let f = FilterSet::new(vec![], None, false);
        assert!(f.matches(Path::new("/music/live/song.mp3"), 1000));
    }

    #[test]
    fn resolve_extensions_defaults() {
        let result = resolve_extensions(&None, &None, &["mp3", "flac"]);
        assert_eq!(result, vec!["mp3", "flac"]);
    }

    #[test]
    fn resolve_extensions_include_overrides() {
        let result = resolve_extensions(
            &Some(vec!["ogg".to_string()]),
            &None,
            &["mp3", "flac"],
        );
        assert_eq!(result, vec!["ogg"]);
    }

    #[test]
    fn resolve_extensions_exclude() {
        let result = resolve_extensions(
            &None,
            &Some(vec!["flac".to_string()]),
            &["mp3", "flac", "ogg"],
        );
        assert_eq!(result, vec!["mp3", "ogg"]);
    }

    #[test]
    fn resolve_extensions_include_and_exclude() {
        let result = resolve_extensions(
            &Some(vec!["mp3".to_string(), "flac".to_string(), "ogg".to_string()]),
            &Some(vec!["flac".to_string()]),
            &["wav"],
        );
        assert_eq!(result, vec!["mp3", "ogg"]);
    }
}
```

- [ ] **Step 2: Wire module and run tests**

Add `mod filters;` to `src/main.rs`.

Run: `cargo test --lib filters`
Expected: all tests pass

- [ ] **Step 3: Commit**

```bash
git add src/filters.rs src/main.rs
git commit -m "feat: add filters module with extension, size, and live filters"
```

---

### Task 3: Scanner Module

**Files:**
- Create: `src/scanner.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write scanner implementation**

Create `src/scanner.rs`:

```rust
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use rand::seq::SliceRandom;
use rand::thread_rng;
use walkdir::WalkDir;

use crate::filters::FilterSet;
use crate::types::{ByteSize, FileEntry};

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

    for result in WalkDir::new(source).into_iter() {
        if shutdown.load(Ordering::Relaxed) {
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
        let size = match entry.metadata() {
            Ok(m) => m.len(),
            Err(_) => {
                let _ = tx.send(ScanMsg::FileFound {
                    path,
                    matched: false,
                });
                continue;
            }
        };

        let matched = filters.matches(&path, size);
        let _ = tx.send(ScanMsg::FileFound {
            path: path.clone(),
            matched,
        });

        if matched {
            entries.push(FileEntry {
                path,
                size: ByteSize(size),
            });
        }
    }

    entries.shuffle(&mut thread_rng());
    let selected = pack_into_budget(entries, budget);
    let _ = tx.send(ScanMsg::Complete(selected));
}

fn pack_into_budget(files: Vec<FileEntry>, budget: u64) -> Vec<FileEntry> {
    let mut selected = Vec::new();
    let mut remaining = budget;
    let mut consecutive_skips = 0;
    let max_skips = 10;

    for file in files {
        let size = file.size.as_u64();
        if size <= remaining {
            remaining -= size;
            selected.push(file);
            consecutive_skips = 0;
        } else {
            consecutive_skips += 1;
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

        fs::write(artist1.join("track1.mp3"), vec![0u8; 5000]).unwrap();
        fs::write(artist1.join("track2.flac"), vec![0u8; 8000]).unwrap();
        fs::write(artist2.join("song.mp3"), vec![0u8; 3000]).unwrap();
        fs::write(artist2.join("cover.jpg"), vec![0u8; 1000]).unwrap();
        fs::write(artist2.join("tiny.mp3"), vec![0u8; 100]).unwrap();
        fs::write(live_dir.join("concert.mp3"), vec![0u8; 6000]).unwrap();
    }

    #[test]
    fn scan_finds_matching_files() {
        let dir = tempfile::tempdir().unwrap();
        create_test_tree(dir.path());

        let filters = FilterSet::new(
            vec!["mp3".to_string(), "flac".to_string()],
            None,
            false,
        );
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

        let filters = FilterSet::new(
            vec!["mp3".to_string(), "flac".to_string()],
            None,
            true,
        );
        let (tx, rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));
        scan(dir.path(), &filters, u64::MAX, &tx, &shutdown);

        let messages: Vec<ScanMsg> = rx.try_iter().collect();
        let complete = messages.last().unwrap();
        match complete {
            ScanMsg::Complete(files) => {
                assert_eq!(files.len(), 4);
                assert!(!files.iter().any(|f| f.path.to_str().unwrap().contains("concert")));
            }
            _ => panic!("expected Complete"),
        }
    }

    #[test]
    fn scan_respects_min_size() {
        let dir = tempfile::tempdir().unwrap();
        create_test_tree(dir.path());

        let filters = FilterSet::new(
            vec!["mp3".to_string()],
            Some(ByteSize(1000)),
            false,
        );
        let (tx, rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));
        scan(dir.path(), &filters, u64::MAX, &tx, &shutdown);

        let messages: Vec<ScanMsg> = rx.try_iter().collect();
        let complete = messages.last().unwrap();
        match complete {
            ScanMsg::Complete(files) => {
                assert!(files.iter().all(|f| f.size.as_u64() >= 1000));
                assert!(!files.iter().any(|f| f.path.to_str().unwrap().contains("tiny")));
            }
            _ => panic!("expected Complete"),
        }
    }

    #[test]
    fn pack_into_budget_respects_limit() {
        let files = vec![
            FileEntry { path: PathBuf::from("a.mp3"), size: ByteSize(5000) },
            FileEntry { path: PathBuf::from("b.mp3"), size: ByteSize(3000) },
            FileEntry { path: PathBuf::from("c.mp3"), size: ByteSize(4000) },
        ];
        let selected = pack_into_budget(files, 8000);
        let total: u64 = selected.iter().map(|f| f.size.as_u64()).sum();
        assert!(total <= 8000);
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn pack_stops_after_consecutive_skips() {
        let files = vec![
            FileEntry { path: PathBuf::from("huge.mp3"), size: ByteSize(1_000_000) },
        ];
        let selected = pack_into_budget(files, 100);
        assert!(selected.is_empty());
    }
}
```

- [ ] **Step 2: Add tempfile dev-dependency**

Add to `Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Wire module and run tests**

Add `mod scanner;` to `src/main.rs`.

Run: `cargo test --lib scanner`
Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml src/scanner.rs src/main.rs
git commit -m "feat: add scanner module with recursive walk, filtering, and budget packing"
```

---

### Task 4: Copier Module

**Files:**
- Create: `src/copier.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write copier implementation**

Create `src/copier.rs`:

```rust
use std::fs;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

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
    tx: &Sender<CopyMsg>,
    shutdown: &Arc<AtomicBool>,
) {
    if let Err(e) = fs::create_dir_all(destination) {
        let _ = tx.send(CopyMsg::Error {
            index: 0,
            path: destination.to_path_buf(),
            error: e.to_string(),
            is_destination: true,
        });
        return;
    }

    for (index, entry) in files.iter().enumerate() {
        if shutdown.load(Ordering::Relaxed) {
            let _ = tx.send(CopyMsg::Aborted);
            return;
        }

        let dest_path = if keep_names {
            dest_path_keep_name(destination, &entry.path)
        } else {
            dest_path_numbered(destination, index, &entry.path)
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
    let mut buf = vec![0u8; BUF_SIZE];

    loop {
        if shutdown.load(Ordering::Relaxed) {
            drop(writer);
            let _ = cleanup_partial(dest);
            return Err((io::Error::new(io::ErrorKind::Interrupted, "shutdown"), true));
        }

        let bytes_read = reader.read(&mut buf).map_err(|e| (e, false))?;
        if bytes_read == 0 {
            break;
        }

        writer.write_all(&buf[..bytes_read]).map_err(|e| (e, true))?;
        let _ = tx.send(CopyMsg::Progress {
            bytes_written: bytes_read as u64,
        });
    }

    writer.flush().map_err(|e| (e, true))?;
    Ok(())
}

fn dest_path_numbered(destination: &Path, index: usize, source: &Path) -> PathBuf {
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");
    destination.join(format!("{:05}.{ext}", index + 1))
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
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");

    let mut counter = 1u32;
    loop {
        let candidate = destination.join(format!("({counter}) {stem}.{ext}"));
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
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
        fs::write(&f1, vec![1u8; 5000]).unwrap();
        fs::write(&f2, vec![2u8; 3000]).unwrap();
        vec![
            FileEntry { path: f1, size: ByteSize(5000) },
            FileEntry { path: f2, size: ByteSize(3000) },
        ]
    }

    #[test]
    fn copy_numbered() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let files = make_source_files(src.path());
        let (tx, rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));

        copy_files(&files, dst.path(), false, &tx, &shutdown);

        let messages: Vec<CopyMsg> = rx.try_iter().collect();
        assert!(matches!(messages.last().unwrap(), CopyMsg::Complete));

        let dest1 = dst.path().join("00001.mp3");
        let dest2 = dst.path().join("00002.flac");
        assert!(dest1.exists());
        assert!(dest2.exists());
        assert_eq!(fs::read(&dest1).unwrap(), vec![1u8; 5000]);
        assert_eq!(fs::read(&dest2).unwrap(), vec![2u8; 3000]);
    }

    #[test]
    fn copy_keep_names() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let files = make_source_files(src.path());
        let (tx, rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));

        copy_files(&files, dst.path(), true, &tx, &shutdown);

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
        fs::write(&f1, vec![1u8; 100]).unwrap();
        fs::write(dst.path().join("song.mp3"), vec![0u8; 50]).unwrap();

        let files = vec![FileEntry { path: f1, size: ByteSize(100) }];
        let (tx, _rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));

        copy_files(&files, dst.path(), true, &tx, &shutdown);

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

        copy_files(&files, dst.path(), false, &tx, &shutdown);

        let messages: Vec<CopyMsg> = rx.try_iter().collect();
        let has_error = messages.iter().any(|m| matches!(m, CopyMsg::Error { is_destination: false, .. }));
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

        copy_files(&files, dst.path(), false, &tx, &shutdown);

        assert!(!dst.path().join("00001.mp3").exists());
    }

    #[test]
    fn dest_path_numbered_format() {
        let dest = Path::new("/usb");
        assert_eq!(
            dest_path_numbered(dest, 0, Path::new("song.mp3")),
            PathBuf::from("/usb/00001.mp3")
        );
        assert_eq!(
            dest_path_numbered(dest, 99, Path::new("track.flac")),
            PathBuf::from("/usb/00100.flac")
        );
    }
}
```

- [ ] **Step 2: Wire module and run tests**

Add `mod copier;` to `src/main.rs`.

Run: `cargo test --lib copier`
Expected: all tests pass

- [ ] **Step 3: Commit**

```bash
git add src/copier.rs src/main.rs
git commit -m "feat: add copier module with buffered copy, progress, and error handling"
```

---

### Task 5: App Core (Elm Model + Msg + update)

**Files:**
- Create: `src/app.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Define Phase, Model, Msg, Effect**

Create `src/app.rs`:

```rust
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::copier::CopyMsg;
use crate::scanner::ScanMsg;
use crate::types::{ByteSize, Config, FileEntry};

const MAX_UPCOMING: usize = 3;
const MAX_HISTORY: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupField {
    Source,
    Destination,
    Size,
    MinSize,
    Extensions,
    Exclude,
    NoLive,
    KeepNames,
    Start,
}

impl SetupField {
    pub fn next(self) -> Self {
        match self {
            Self::Source => Self::Destination,
            Self::Destination => Self::Size,
            Self::Size => Self::MinSize,
            Self::MinSize => Self::Extensions,
            Self::Extensions => Self::Exclude,
            Self::Exclude => Self::NoLive,
            Self::NoLive => Self::KeepNames,
            Self::KeepNames => Self::Start,
            Self::Start => Self::Source,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Source => Self::Start,
            Self::Destination => Self::Source,
            Self::Size => Self::Destination,
            Self::MinSize => Self::Size,
            Self::Extensions => Self::MinSize,
            Self::Exclude => Self::Extensions,
            Self::NoLive => Self::Exclude,
            Self::KeepNames => Self::NoLive,
            Self::Start => Self::KeepNames,
        }
    }

    pub fn is_text(self) -> bool {
        matches!(
            self,
            Self::Source
                | Self::Destination
                | Self::Size
                | Self::MinSize
                | Self::Extensions
                | Self::Exclude
        )
    }

    pub fn is_checkbox(self) -> bool {
        matches!(self, Self::NoLive | Self::KeepNames)
    }
}

#[derive(Debug, Clone)]
pub struct SetupForm {
    pub source: String,
    pub destination: String,
    pub size: String,
    pub min_size: String,
    pub extensions: String,
    pub exclude: String,
    pub no_live: bool,
    pub keep_names: bool,
    pub focused: SetupField,
    pub error: Option<String>,
}

impl Default for SetupForm {
    fn default() -> Self {
        Self {
            source: String::new(),
            destination: String::new(),
            size: String::new(),
            min_size: String::new(),
            extensions: String::new(),
            exclude: String::new(),
            no_live: false,
            keep_names: false,
            focused: SetupField::Source,
            error: None,
        }
    }
}

impl SetupForm {
    pub fn focused_value_mut(&mut self) -> Option<&mut String> {
        match self.focused {
            SetupField::Source => Some(&mut self.source),
            SetupField::Destination => Some(&mut self.destination),
            SetupField::Size => Some(&mut self.size),
            SetupField::MinSize => Some(&mut self.min_size),
            SetupField::Extensions => Some(&mut self.extensions),
            SetupField::Exclude => Some(&mut self.exclude),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Queued,
    Copying,
    Done,
    Failed,
}

#[derive(Debug, Clone)]
pub struct FileItem {
    pub name: String,
    pub original_path: PathBuf,
    pub size: ByteSize,
    pub status: FileStatus,
}

#[derive(Debug)]
pub struct ScanState {
    pub files_found: usize,
    pub files_matched: usize,
    pub last_path: Option<PathBuf>,
    pub spinner_tick: usize,
}

#[derive(Debug)]
pub struct CopyState {
    pub config: Config,
    pub files: Vec<FileItem>,
    pub current_index: usize,
    pub total_bytes: u64,
    pub total_files: usize,
    pub copied_bytes: u64,
    pub current_file_copied: u64,
    pub current_file_size: u64,
    pub started_at: Instant,
    pub speed_bytes: Vec<(Instant, u64)>,
    pub errors: Vec<String>,
}

impl CopyState {
    pub fn speed(&self) -> f64 {
        let cutoff = Instant::now() - std::time::Duration::from_secs(3);
        let recent: Vec<_> = self
            .speed_bytes
            .iter()
            .filter(|(t, _)| *t >= cutoff)
            .collect();
        if recent.is_empty() {
            return 0.0;
        }
        let total: u64 = recent.iter().map(|(_, b)| b).sum();
        let elapsed = recent
            .last()
            .unwrap()
            .0
            .duration_since(recent.first().unwrap().0);
        let secs = elapsed.as_secs_f64().max(0.1);
        total as f64 / secs
    }

    pub fn upcoming(&self) -> impl Iterator<Item = &FileItem> {
        let start = self.current_index + 1;
        let end = (start + MAX_UPCOMING).min(self.files.len());
        self.files[start..end].iter().rev()
    }

    pub fn history(&self) -> impl Iterator<Item = &FileItem> {
        let end = self.current_index;
        let start = end.saturating_sub(MAX_HISTORY);
        self.files[start..end].iter().rev()
    }

    pub fn current(&self) -> Option<&FileItem> {
        self.files.get(self.current_index)
    }

    pub fn overall_progress(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        self.copied_bytes as f64 / self.total_bytes as f64
    }

    pub fn current_progress(&self) -> f64 {
        if self.current_file_size == 0 {
            return 0.0;
        }
        self.current_file_copied as f64 / self.current_file_size as f64
    }

    pub fn eta_secs(&self) -> Option<f64> {
        let speed = self.speed();
        if speed <= 0.0 {
            return None;
        }
        let remaining = self.total_bytes.saturating_sub(self.copied_bytes);
        Some(remaining as f64 / speed)
    }
}

pub enum Phase {
    Setup(SetupForm),
    Scanning {
        config: Config,
        state: ScanState,
    },
    Copying(CopyState),
    Done {
        total_files: usize,
        total_bytes: u64,
        errors: Vec<String>,
        elapsed: std::time::Duration,
    },
    FatalError(String),
}

pub struct Model {
    pub phase: Phase,
    pub terminal_size: (u16, u16),
    pub should_quit: bool,
    pub shutdown: Arc<AtomicBool>,
}

pub enum Msg {
    Key(crossterm::event::KeyEvent),
    Resize(u16, u16),
    Tick,
    Scan(ScanMsg),
    Copy(CopyMsg),
}

pub enum Effect {
    None,
    StartScan(Config),
    StartCopy {
        files: Vec<FileEntry>,
        config: Config,
    },
    Quit,
}

const SPINNER_CHARS: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

impl Model {
    pub fn new_tui() -> Self {
        Self {
            phase: Phase::Setup(SetupForm::default()),
            terminal_size: (80, 24),
            should_quit: false,
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn new_cli(config: Config) -> Self {
        Self {
            phase: Phase::Scanning {
                config,
                state: ScanState {
                    files_found: 0,
                    files_matched: 0,
                    last_path: None,
                    spinner_tick: 0,
                },
            },
            terminal_size: (80, 24),
            should_quit: false,
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn spinner_char(&self) -> char {
        if let Phase::Scanning { state, .. } = &self.phase {
            SPINNER_CHARS[state.spinner_tick % SPINNER_CHARS.len()]
        } else {
            ' '
        }
    }
}

pub fn update(model: &mut Model, msg: Msg) -> Effect {
    use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

    match msg {
        Msg::Resize(w, h) => {
            model.terminal_size = (w, h);
            Effect::None
        }

        Msg::Key(key) => {
            if key.kind != KeyEventKind::Press {
                return Effect::None;
            }

            if key.code == KeyCode::Char('c')
                && key.modifiers.contains(KeyModifiers::CONTROL)
            {
                model.shutdown.store(true, Ordering::Relaxed);
                model.should_quit = true;
                return Effect::Quit;
            }

            match &mut model.phase {
                Phase::Setup(form) => update_setup(form, key),
                Phase::Done { .. } | Phase::FatalError(_) => {
                    if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                        model.should_quit = true;
                        Effect::Quit
                    } else {
                        Effect::None
                    }
                }
                _ => Effect::None,
            }
        }

        Msg::Tick => {
            if let Phase::Scanning { state, .. } = &mut model.phase {
                state.spinner_tick = state.spinner_tick.wrapping_add(1);
            }
            if let Phase::Copying(cs) = &mut model.phase {
                let cutoff = Instant::now() - std::time::Duration::from_secs(5);
                cs.speed_bytes.retain(|(t, _)| *t >= cutoff);
            }
            Effect::None
        }

        Msg::Scan(scan_msg) => match scan_msg {
            ScanMsg::FileFound { path, matched } => {
                if let Phase::Scanning { state, .. } = &mut model.phase {
                    state.files_found += 1;
                    if matched {
                        state.files_matched += 1;
                    }
                    state.last_path = Some(path);
                }
                Effect::None
            }
            ScanMsg::Complete(files) => {
                if let Phase::Scanning { config, .. } = &mut model.phase {
                    let total_bytes: u64 = files.iter().map(|f| f.size.as_u64()).sum();
                    let total_files = files.len();
                    let items: Vec<FileItem> = files
                        .iter()
                        .map(|f| FileItem {
                            name: f
                                .path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("unknown")
                                .to_string(),
                            original_path: f.path.clone(),
                            size: f.size,
                            status: FileStatus::Queued,
                        })
                        .collect();

                    let config = config.clone();
                    let copy_state = CopyState {
                        config: config.clone(),
                        files: items,
                        current_index: 0,
                        total_bytes,
                        total_files,
                        copied_bytes: 0,
                        current_file_copied: 0,
                        current_file_size: 0,
                        started_at: Instant::now(),
                        speed_bytes: Vec::new(),
                        errors: Vec::new(),
                    };

                    if total_files == 0 {
                        model.phase = Phase::Done {
                            total_files: 0,
                            total_bytes: 0,
                            errors: vec![],
                            elapsed: std::time::Duration::ZERO,
                        };
                        return Effect::None;
                    }

                    model.phase = Phase::Copying(copy_state);
                    return Effect::StartCopy {
                        files,
                        config,
                    };
                }
                Effect::None
            }
            ScanMsg::Error(e) => {
                model.phase = Phase::FatalError(e);
                Effect::None
            }
        },

        Msg::Copy(copy_msg) => match copy_msg {
            CopyMsg::FileStart { index, name, original_path, size } => {
                if let Phase::Copying(cs) = &mut model.phase {
                    cs.current_index = index;
                    cs.current_file_copied = 0;
                    cs.current_file_size = size.as_u64();
                    if let Some(item) = cs.files.get_mut(index) {
                        item.status = FileStatus::Copying;
                        item.name = name;
                        item.original_path = original_path;
                    }
                }
                Effect::None
            }
            CopyMsg::Progress { bytes_written } => {
                if let Phase::Copying(cs) = &mut model.phase {
                    cs.current_file_copied += bytes_written;
                    cs.copied_bytes += bytes_written;
                    cs.speed_bytes.push((Instant::now(), bytes_written));
                }
                Effect::None
            }
            CopyMsg::FileDone { index } => {
                if let Phase::Copying(cs) = &mut model.phase {
                    if let Some(item) = cs.files.get_mut(index) {
                        item.status = FileStatus::Done;
                    }
                }
                Effect::None
            }
            CopyMsg::Error { index, path, error, is_destination } => {
                if let Phase::Copying(cs) = &mut model.phase {
                    let msg = format!("{}: {error}", path.display());
                    cs.errors.push(msg);
                    if let Some(item) = cs.files.get_mut(index) {
                        item.status = FileStatus::Failed;
                    }
                    if is_destination {
                        model.phase = Phase::FatalError(format!(
                            "Write error on {}: {error}",
                            path.display()
                        ));
                    }
                }
                Effect::None
            }
            CopyMsg::Complete => {
                if let Phase::Copying(cs) = &model.phase {
                    model.phase = Phase::Done {
                        total_files: cs.total_files,
                        total_bytes: cs.copied_bytes,
                        errors: cs.errors.clone(),
                        elapsed: cs.started_at.elapsed(),
                    };
                }
                Effect::None
            }
            CopyMsg::Aborted => {
                model.should_quit = true;
                Effect::Quit
            }
        },
    }
}

fn update_setup(
    form: &mut SetupForm,
    key: crossterm::event::KeyEvent,
) -> Effect {
    use crossterm::event::KeyCode;

    match key.code {
        KeyCode::Tab => {
            form.focused = form.focused.next();
            form.error = None;
            Effect::None
        }
        KeyCode::BackTab => {
            form.focused = form.focused.prev();
            form.error = None;
            Effect::None
        }
        KeyCode::Enter => {
            if form.focused == SetupField::Start {
                validate_and_start(form)
            } else {
                form.focused = form.focused.next();
                Effect::None
            }
        }
        KeyCode::Char(' ') if form.focused.is_checkbox() => {
            match form.focused {
                SetupField::NoLive => form.no_live = !form.no_live,
                SetupField::KeepNames => form.keep_names = !form.keep_names,
                _ => {}
            }
            Effect::None
        }
        KeyCode::Char(c) if form.focused.is_text() => {
            if let Some(val) = form.focused_value_mut() {
                val.push(c);
            }
            Effect::None
        }
        KeyCode::Backspace if form.focused.is_text() => {
            if let Some(val) = form.focused_value_mut() {
                val.pop();
            }
            Effect::None
        }
        _ => Effect::None,
    }
}

fn validate_and_start(form: &mut SetupForm) -> Effect {
    use std::path::Path;

    if form.source.is_empty() {
        form.error = Some("Source path is required".to_string());
        form.focused = SetupField::Source;
        return Effect::None;
    }
    if form.destination.is_empty() {
        form.error = Some("Destination path is required".to_string());
        form.focused = SetupField::Destination;
        return Effect::None;
    }

    let source = PathBuf::from(&form.source);
    if !source.exists() {
        form.error = Some(format!("Source not found: {}", source.display()));
        form.focused = SetupField::Source;
        return Effect::None;
    }

    let max_size = if form.size.is_empty() {
        None
    } else {
        match ByteSize::parse(&form.size) {
            Ok(s) => Some(s),
            Err(e) => {
                form.error = Some(format!("Invalid size: {e}"));
                form.focused = SetupField::Size;
                return Effect::None;
            }
        }
    };

    let min_file_size = if form.min_size.is_empty() {
        None
    } else {
        match ByteSize::parse(&form.min_size) {
            Ok(s) => Some(s),
            Err(e) => {
                form.error = Some(format!("Invalid min size: {e}"));
                form.focused = SetupField::MinSize;
                return Effect::None;
            }
        }
    };

    let include = if form.extensions.is_empty() {
        None
    } else {
        Some(parse_comma_list(&form.extensions))
    };
    let exclude = if form.exclude.is_empty() {
        None
    } else {
        Some(parse_comma_list(&form.exclude))
    };

    let allowed_extensions =
        crate::filters::resolve_extensions(&include, &exclude, crate::types::DEFAULT_EXTENSIONS);

    let config = Config {
        source,
        destination: PathBuf::from(&form.destination),
        max_size,
        min_file_size,
        no_live: form.no_live,
        keep_names: form.keep_names,
        allowed_extensions,
    };

    Effect::StartScan(config)
}

fn parse_comma_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|p| p.trim().to_lowercase().trim_start_matches('.').to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> Msg {
        Msg::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        })
    }

    #[test]
    fn setup_tab_navigation() {
        let mut model = Model::new_tui();
        update(&mut model, key(KeyCode::Tab));
        if let Phase::Setup(form) = &model.phase {
            assert_eq!(form.focused, SetupField::Destination);
        } else {
            panic!("wrong phase");
        }
    }

    #[test]
    fn setup_text_input() {
        let mut model = Model::new_tui();
        update(&mut model, key(KeyCode::Char('/')));
        update(&mut model, key(KeyCode::Char('m')));
        if let Phase::Setup(form) = &model.phase {
            assert_eq!(form.source, "/m");
        } else {
            panic!("wrong phase");
        }
    }

    #[test]
    fn setup_backspace() {
        let mut model = Model::new_tui();
        update(&mut model, key(KeyCode::Char('a')));
        update(&mut model, key(KeyCode::Char('b')));
        update(&mut model, key(KeyCode::Backspace));
        if let Phase::Setup(form) = &model.phase {
            assert_eq!(form.source, "a");
        } else {
            panic!("wrong phase");
        }
    }

    #[test]
    fn setup_checkbox_toggle() {
        let mut model = Model::new_tui();
        if let Phase::Setup(form) = &mut model.phase {
            form.focused = SetupField::NoLive;
        }
        update(&mut model, key(KeyCode::Char(' ')));
        if let Phase::Setup(form) = &model.phase {
            assert!(form.no_live);
        } else {
            panic!("wrong phase");
        }
    }

    #[test]
    fn setup_validation_empty_source() {
        let mut model = Model::new_tui();
        if let Phase::Setup(form) = &mut model.phase {
            form.focused = SetupField::Start;
        }
        let effect = update(&mut model, key(KeyCode::Enter));
        assert!(matches!(effect, Effect::None));
        if let Phase::Setup(form) = &model.phase {
            assert!(form.error.is_some());
        } else {
            panic!("wrong phase");
        }
    }

    #[test]
    fn scan_file_found_updates_state() {
        let config = Config {
            source: PathBuf::from("/src"),
            destination: PathBuf::from("/dst"),
            max_size: None,
            min_file_size: None,
            no_live: false,
            keep_names: false,
            allowed_extensions: vec![],
        };
        let mut model = Model::new_cli(config);

        update(
            &mut model,
            Msg::Scan(ScanMsg::FileFound {
                path: PathBuf::from("/src/a.mp3"),
                matched: true,
            }),
        );

        if let Phase::Scanning { state, .. } = &model.phase {
            assert_eq!(state.files_found, 1);
            assert_eq!(state.files_matched, 1);
        } else {
            panic!("wrong phase");
        }
    }

    #[test]
    fn scan_complete_transitions_to_copying() {
        let config = Config {
            source: PathBuf::from("/src"),
            destination: PathBuf::from("/dst"),
            max_size: None,
            min_file_size: None,
            no_live: false,
            keep_names: false,
            allowed_extensions: vec![],
        };
        let mut model = Model::new_cli(config);

        let files = vec![FileEntry {
            path: PathBuf::from("/src/a.mp3"),
            size: ByteSize(1000),
        }];

        let effect = update(&mut model, Msg::Scan(ScanMsg::Complete(files)));
        assert!(matches!(effect, Effect::StartCopy { .. }));
        assert!(matches!(model.phase, Phase::Copying(_)));
    }

    #[test]
    fn copy_progress_updates_bytes() {
        let config = Config {
            source: PathBuf::from("/src"),
            destination: PathBuf::from("/dst"),
            max_size: None,
            min_file_size: None,
            no_live: false,
            keep_names: false,
            allowed_extensions: vec![],
        };
        let mut model = Model::new_cli(config);
        let files = vec![FileEntry {
            path: PathBuf::from("/src/a.mp3"),
            size: ByteSize(1000),
        }];
        update(&mut model, Msg::Scan(ScanMsg::Complete(files)));
        update(
            &mut model,
            Msg::Copy(CopyMsg::FileStart {
                index: 0,
                name: "00001.mp3".to_string(),
                original_path: PathBuf::from("/src/a.mp3"),
                size: ByteSize(1000),
            }),
        );
        update(
            &mut model,
            Msg::Copy(CopyMsg::Progress { bytes_written: 500 }),
        );

        if let Phase::Copying(cs) = &model.phase {
            assert_eq!(cs.copied_bytes, 500);
            assert_eq!(cs.current_file_copied, 500);
        } else {
            panic!("wrong phase");
        }
    }

    #[test]
    fn copy_complete_transitions_to_done() {
        let config = Config {
            source: PathBuf::from("/src"),
            destination: PathBuf::from("/dst"),
            max_size: None,
            min_file_size: None,
            no_live: false,
            keep_names: false,
            allowed_extensions: vec![],
        };
        let mut model = Model::new_cli(config);
        let files = vec![FileEntry {
            path: PathBuf::from("/src/a.mp3"),
            size: ByteSize(1000),
        }];
        update(&mut model, Msg::Scan(ScanMsg::Complete(files)));
        update(&mut model, Msg::Copy(CopyMsg::Complete));

        assert!(matches!(model.phase, Phase::Done { .. }));
    }

    #[test]
    fn dest_error_transitions_to_fatal() {
        let config = Config {
            source: PathBuf::from("/src"),
            destination: PathBuf::from("/dst"),
            max_size: None,
            min_file_size: None,
            no_live: false,
            keep_names: false,
            allowed_extensions: vec![],
        };
        let mut model = Model::new_cli(config);
        let files = vec![FileEntry {
            path: PathBuf::from("/src/a.mp3"),
            size: ByteSize(1000),
        }];
        update(&mut model, Msg::Scan(ScanMsg::Complete(files)));
        update(
            &mut model,
            Msg::Copy(CopyMsg::Error {
                index: 0,
                path: PathBuf::from("/dst/00001.mp3"),
                error: "disk full".to_string(),
                is_destination: true,
            }),
        );

        assert!(matches!(model.phase, Phase::FatalError(_)));
    }

    #[test]
    fn ctrl_c_sets_quit() {
        let mut model = Model::new_tui();
        update(
            &mut model,
            Msg::Key(KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: KeyEventState::empty(),
            }),
        );
        assert!(model.should_quit);
        assert!(model.shutdown.load(Ordering::Relaxed));
    }
}
```

- [ ] **Step 2: Wire module and run tests**

Add `mod app;` to `src/main.rs`.

Run: `cargo test --lib app`
Expected: all tests pass

- [ ] **Step 3: Commit**

```bash
git add src/app.rs src/main.rs
git commit -m "feat: add Elm core with Model, Msg, Phase, update function"
```

---

### Task 6: TUI Module

**Files:**
- Create: `src/tui.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write TUI event loop and view dispatch**

Create `src/tui.rs`:

```rust
use std::sync::atomic::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Gauge, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{
    update, CopyState, Effect, FileStatus, Model, Msg, Phase, ScanState, SetupField, SetupForm,
};
use crate::copier;
use crate::filters::FilterSet;
use crate::scanner;
use crate::types::{format_duration, ByteSize, Config, Error};

const TICK_RATE: Duration = Duration::from_millis(50);

pub fn run() -> Result<(), Error> {
    let mut terminal =
        ratatui::init();
    let result = run_loop(&mut terminal);
    ratatui::restore();
    result
}

fn run_loop(terminal: &mut ratatui::DefaultTerminal) -> Result<(), Error> {
    let mut model = Model::new_tui();
    let (tx, rx) = mpsc::channel::<Msg>();
    let mut last_tick = Instant::now();

    loop {
        terminal
            .draw(|f| view(&model, f))
            .map_err(Error::Terminal)?;

        let timeout = TICK_RATE
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        if event::poll(timeout).map_err(Error::Terminal)? {
            match event::read().map_err(Error::Terminal)? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let effect = update(&mut model, Msg::Key(key));
                    handle_effect(effect, &tx, &model)?;
                }
                Event::Resize(w, h) => {
                    update(&mut model, Msg::Resize(w, h));
                }
                _ => {}
            }
        }

        while let Ok(msg) = rx.try_recv() {
            let effect = update(&mut model, msg);
            handle_effect(effect, &tx, &model)?;
        }

        if last_tick.elapsed() >= TICK_RATE {
            update(&mut model, Msg::Tick);
            last_tick = Instant::now();
        }

        if model.should_quit {
            break;
        }
    }

    Ok(())
}

fn handle_effect(
    effect: Effect,
    tx: &mpsc::Sender<Msg>,
    model: &Model,
) -> Result<(), Error> {
    match effect {
        Effect::None => {}
        Effect::Quit => {}
        Effect::StartScan(config) => {
            spawn_scanner(config, tx.clone(), model);
        }
        Effect::StartCopy { files, config } => {
            spawn_copier(files, config, tx.clone(), model);
        }
    }
    Ok(())
}

fn spawn_scanner(
    config: Config,
    tx: mpsc::Sender<Msg>,
    model: &Model,
) {
    let shutdown = Arc::clone(&model.shutdown);
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

    thread::spawn(move || {
        let scan_tx = {
            let tx = tx.clone();
            let (stx, srx) = mpsc::channel();
            thread::spawn(move || {
                for msg in srx {
                    if tx.send(Msg::Scan(msg)).is_err() {
                        break;
                    }
                }
            });
            stx
        };
        scanner::scan(&source, &filters, budget, &scan_tx, &shutdown);
    });
}

fn spawn_copier(
    files: Vec<crate::types::FileEntry>,
    config: Config,
    tx: mpsc::Sender<Msg>,
    model: &Model,
) {
    let shutdown = Arc::clone(&model.shutdown);
    let destination = config.destination.clone();
    let keep_names = config.keep_names;

    thread::spawn(move || {
        let copy_tx = {
            let tx = tx.clone();
            let (stx, srx) = mpsc::channel();
            thread::spawn(move || {
                for msg in srx {
                    if tx.send(Msg::Copy(msg)).is_err() {
                        break;
                    }
                }
            });
            stx
        };
        copier::copy_files(&files, &destination, keep_names, &copy_tx, &shutdown);
    });
}

fn view(model: &Model, frame: &mut Frame) {
    let area = frame.area();

    match &model.phase {
        Phase::Setup(form) => view_setup(form, frame, area),
        Phase::Scanning { config, state } => view_scanning(config, state, model, frame, area),
        Phase::Copying(cs) => view_copying(cs, frame, area),
        Phase::Done {
            total_files,
            total_bytes,
            errors,
            elapsed,
        } => view_done(*total_files, *total_bytes, errors, *elapsed, frame, area),
        Phase::FatalError(msg) => view_error(msg, frame, area),
    }
}

fn view_setup(form: &SetupForm, frame: &mut Frame, area: Rect) {
    let block = Block::bordered().title(" mixr ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas::<12>(inner);

    let label_width = 14;
    let fields: &[(SetupField, &str, &str)] = &[
        (SetupField::Source, "Source:", &form.source),
        (SetupField::Destination, "Destination:", &form.destination),
        (SetupField::Size, "Size:", &form.size),
        (SetupField::MinSize, "Min size:", &form.min_size),
        (SetupField::Extensions, "Extensions:", &form.extensions),
        (SetupField::Exclude, "Exclude:", &form.exclude),
    ];

    for (i, (field, label, value)) in fields.iter().enumerate() {
        let focused = form.focused == *field;
        let style = if focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        let cursor = if focused { "_" } else { "" };
        let line = Line::from(vec![
            Span::styled(format!("{label:<label_width$}"), style),
            Span::raw(format!("{value}{cursor}")),
        ]);
        frame.render_widget(Paragraph::new(line), chunks[i]);
    }

    let no_live_marker = if form.no_live { "x" } else { " " };
    let keep_marker = if form.keep_names { "x" } else { " " };
    let no_live_style = if form.focused == SetupField::NoLive {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let keep_style = if form.focused == SetupField::KeepNames {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let checkboxes = Line::from(vec![
        Span::styled(format!("[{no_live_marker}] No live"), no_live_style),
        Span::raw("   "),
        Span::styled(format!("[{keep_marker}] Keep names"), keep_style),
    ]);
    frame.render_widget(Paragraph::new(checkboxes), chunks[7]);

    let start_style = if form.focused == SetupField::Start {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    let start = Line::from(Span::styled(" [ Start ] ", start_style));
    frame.render_widget(
        Paragraph::new(start).alignment(Alignment::Center),
        chunks[9],
    );

    let help = if let Some(err) = &form.error {
        Line::from(Span::styled(err.as_str(), Style::default().fg(Color::Red)))
    } else {
        Line::from(Span::styled(
            "Tab: next  Shift+Tab: prev  Enter: go  Ctrl+C: quit",
            Style::default().fg(Color::DarkGray),
        ))
    };
    frame.render_widget(Paragraph::new(help), chunks[11]);
}

fn view_scanning(
    config: &Config,
    state: &ScanState,
    model: &Model,
    frame: &mut Frame,
    area: Rect,
) {
    let block = Block::bordered().title(" mixr - Scanning ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas::<4>(inner);

    let scanning = Line::from(format!(
        "Scanning {}...",
        config.source.display()
    ));
    frame.render_widget(Paragraph::new(scanning), chunks[0]);

    let stats = Line::from(format!(
        "{} {} found ({} matched)",
        model.spinner_char(),
        state.files_found,
        state.files_matched,
    ));
    frame.render_widget(Paragraph::new(stats), chunks[1]);

    if let Some(path) = &state.last_path {
        let path_str = path.display().to_string();
        let max_w = inner.width as usize;
        let display = if path_str.len() > max_w {
            format!("...{}", &path_str[path_str.len() - max_w + 3..])
        } else {
            path_str
        };
        let last = Line::from(Span::styled(display, Style::default().fg(Color::DarkGray)));
        frame.render_widget(Paragraph::new(last), chunks[2]);
    }
}

fn view_copying(cs: &CopyState, frame: &mut Frame, area: Rect) {
    let block = Block::bordered().title(" mixr - Copying ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas::<5>(inner);

    let file_area = chunks[0];
    render_file_list(cs, frame, file_area);

    let current_label = if let Some(cur) = cs.current() {
        format!("Current: {} ({})", cur.name, cur.size)
    } else {
        "Current:".to_string()
    };
    let current_gauge = Gauge::default()
        .label(Span::raw(current_label))
        .ratio(cs.current_progress().clamp(0.0, 1.0))
        .gauge_style(Style::default().fg(Color::Cyan));
    frame.render_widget(current_gauge, chunks[1]);

    let total_label = format!(
        "Total: {} / {} ({}/{})",
        ByteSize(cs.copied_bytes),
        ByteSize(cs.total_bytes),
        cs.files
            .iter()
            .filter(|f| f.status == FileStatus::Done)
            .count(),
        cs.total_files,
    );
    let total_gauge = Gauge::default()
        .label(Span::raw(total_label))
        .ratio(cs.overall_progress().clamp(0.0, 1.0))
        .gauge_style(Style::default().fg(Color::Green));
    frame.render_widget(total_gauge, chunks[2]);

    let speed = cs.speed();
    let speed_str = ByteSize(speed as u64);
    let elapsed = cs.started_at.elapsed();
    let elapsed_str = format_duration(elapsed);
    let eta_str = cs
        .eta_secs()
        .map(|s| format_duration(Duration::from_secs_f64(s)))
        .unwrap_or_else(|| "—".to_string());

    let status = Line::from(Span::styled(
        format!("{speed_str}/s  Elapsed: {elapsed_str}  ETA: {eta_str}"),
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(Paragraph::new(status), chunks[3]);

    let help = Line::from(Span::styled(
        "Ctrl+C to stop",
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(Paragraph::new(help), chunks[4]);
}

fn render_file_list(cs: &CopyState, frame: &mut Frame, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    for item in cs.upcoming() {
        let line = Line::from(Span::styled(
            format!("  {}", item.name),
            Style::default().fg(Color::DarkGray),
        ));
        lines.push(line);
    }

    if let Some(cur) = cs.current() {
        let line = Line::from(Span::styled(
            format!("> {} ({})", cur.name, cur.size),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));
        lines.push(line);
    }

    for item in cs.history() {
        let (prefix, style) = match item.status {
            FileStatus::Done => (
                "  ",
                Style::default().fg(Color::Green),
            ),
            FileStatus::Failed => (
                "  ",
                Style::default().fg(Color::Red),
            ),
            _ => ("  ", Style::default()),
        };
        let line = Line::from(Span::styled(
            format!("{prefix}{}", item.name),
            style,
        ));
        lines.push(line);
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

fn view_done(
    total_files: usize,
    total_bytes: u64,
    errors: &[String],
    elapsed: Duration,
    frame: &mut Frame,
    area: Rect,
) {
    let block = Block::bordered().title(" mixr - Done ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = vec![
        Line::from(Span::styled(
            "Done!",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!(
            "Copied {} files ({})",
            total_files,
            ByteSize(total_bytes),
        )),
        Line::from(format!("Time: {}", format_duration(elapsed))),
    ];

    if !errors.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("{} errors:", errors.len()),
            Style::default().fg(Color::Red),
        )));
        for err in errors.iter().take(10) {
            lines.push(Line::from(Span::styled(
                format!("  {err}"),
                Style::default().fg(Color::Red),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Press q to quit",
        Style::default().fg(Color::DarkGray),
    )));

    frame.render_widget(Paragraph::new(lines), inner);
}

fn view_error(msg: &str, frame: &mut Frame, area: Rect) {
    let block = Block::bordered().title(" mixr - Error ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(Span::styled(
            "Fatal error!",
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(msg.to_string()),
        Line::from(""),
        Line::from(Span::styled(
            "Press q to quit",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

```

- [ ] **Step 2: Wire module and verify compilation**

Add `mod tui;` to `src/main.rs`.

Run: `cargo check`
Expected: compiles without errors

- [ ] **Step 3: Commit**

```bash
git add src/tui.rs src/main.rs
git commit -m "feat: add TUI module with setup wizard, scanning, and copying views"
```

---

### Task 7: CLI Module

**Files:**
- Create: `src/cli.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write CLI mode implementation**

Create `src/cli.rs`:

```rust
use std::io::{self, Write};
use std::sync::atomic::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use crate::app::{update, Effect, Model, Msg, Phase};
use crate::copier::{self, CopyMsg};
use crate::filters::FilterSet;
use crate::scanner::{self, ScanMsg};
use crate::types::{format_duration, ByteSize, Config, Error};

pub fn run(config: Config) -> Result<bool, Error> {
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

    let started = Instant::now();
    let mut had_errors = false;

    loop {
        let msg = match rx.recv() {
            Ok(m) => m,
            Err(_) => break,
        };

        let effect = update(&mut model, msg);

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
                    if cs.current_file_copied == 0 {
                        let done_count = cs
                            .files
                            .iter()
                            .filter(|f| {
                                matches!(
                                    f.status,
                                    crate::app::FileStatus::Done | crate::app::FileStatus::Failed
                                )
                            })
                            .count();
                        let _ = writeln!(
                            stderr,
                            "[{:>width$}/{}]  {} <- {} ({})",
                            done_count + 1,
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
                let speed = if elapsed.as_secs_f64() > 0.0 {
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
            _ => {}
        }

        match effect {
            Effect::StartScan(_) => {}
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
            Effect::None => {}
        }
    }

    Ok(!had_errors)
}

```

- [ ] **Step 2: Wire module and verify compilation**

Add `mod cli;` to `src/main.rs`.

Run: `cargo check`
Expected: compiles without errors

- [ ] **Step 3: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat: add CLI mode with plain text progress output"
```

---

### Task 8: Main Entry Point with Clap

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Write clap arg parsing and mode selection**

Replace `src/main.rs`:

```rust
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
            let allowed_extensions =
                resolve_extensions(&args.include, &args.exclude, DEFAULT_EXTENSIONS);

            let config = Config {
                source,
                destination,
                max_size: args.size,
                min_file_size: args.min_size,
                no_live: args.no_live,
                keep_names: args.keep_names,
                allowed_extensions,
            };

            match cli::run(config) {
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
```

- [ ] **Step 2: Verify full build**

Run: `cargo build`
Expected: compiles without errors or warnings

- [ ] **Step 3: Run all tests**

Run: `cargo test`
Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: add clap entry point with TUI/CLI mode selection"
```

---

### Task 9: Integration Testing and Polish

**Files:**
- Modify: various files for fixes discovered during integration testing

- [ ] **Step 1: Test TUI mode launches**

Run: `cargo run`
Expected: TUI wizard appears with form fields, Tab navigates, Ctrl+C exits

- [ ] **Step 2: Test CLI mode with real files**

Create a temp test directory and run:

```bash
mkdir -p /tmp/mixr-test-src /tmp/mixr-test-dst
for i in $(seq 1 20); do dd if=/dev/urandom of="/tmp/mixr-test-src/song$i.mp3" bs=1K count=$((RANDOM % 100 + 10)) 2>/dev/null; done
cargo run -- /tmp/mixr-test-src /tmp/mixr-test-dst --size 500K
```

Expected: scans, copies files up to 500K, shows progress, exits with summary

- [ ] **Step 3: Test filters in CLI mode**

```bash
rm -rf /tmp/mixr-test-dst/*
mkdir -p "/tmp/mixr-test-src/Live At Wembley"
dd if=/dev/urandom of="/tmp/mixr-test-src/Live At Wembley/concert.mp3" bs=1K count=50 2>/dev/null
dd if=/dev/urandom of="/tmp/mixr-test-src/tiny.mp3" bs=100 count=1 2>/dev/null
cargo run -- /tmp/mixr-test-src /tmp/mixr-test-dst --no-live --min-size 1K --size 500K
```

Expected: concert.mp3 excluded (live), tiny.mp3 excluded (too small)

- [ ] **Step 4: Test keep-names mode**

```bash
rm -rf /tmp/mixr-test-dst/*
cargo run -- /tmp/mixr-test-src /tmp/mixr-test-dst --keep-names --size 500K
ls /tmp/mixr-test-dst/
```

Expected: files have original names, not numbered

- [ ] **Step 5: Fix any issues found during integration testing**

Address compilation errors, runtime issues, or UX problems discovered in steps 1-4.

- [ ] **Step 6: Clean up test artifacts**

```bash
rm -rf /tmp/mixr-test-src /tmp/mixr-test-dst
```

- [ ] **Step 7: Final test run**

Run: `cargo test`
Expected: all tests pass, no warnings

- [ ] **Step 8: Commit any fixes**

```bash
git add -A
git commit -m "fix: integration testing fixes and polish"
```
