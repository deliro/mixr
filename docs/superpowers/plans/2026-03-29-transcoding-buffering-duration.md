# Transcoding, Double Buffering, Min Duration — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add on-the-fly MP3 transcoding (CBR/VBR), double-buffered copy pipeline, and min-duration filtering to mixr.

**Architecture:** Scanner probes audio metadata (duration/bitrate) via symphonia during scan. Copier uses a reader→writer pipeline connected by bounded sync_channel: reader thread reads or transcodes files one ahead, writer thread writes to destination. Transcoding uses symphonia (decode any format) + mp3lame-encoder (encode to MP3, statically linked). New TUI form fields (Encoding, Bitrate/Quality, Min duration) with conditional visibility.

**Tech Stack:** symphonia (audio decoding/probing), mp3lame-encoder (MP3 encoding, bundles LAME C source), sync_channel for double buffering.

**Spec:** `docs/superpowers/specs/2026-03-29-transcoding-buffering-duration-design.md`

---

## File Structure

### New files
- `src/probe.rs` — audio metadata probing (duration, bitrate) via symphonia. Used by scanner during scan.
- `src/transcoder.rs` — decode any audio format to PCM via symphonia, encode to MP3 via lame. Used by copier reader thread.

### Modified files
- `Cargo.toml` — add symphonia, mp3lame-encoder dependencies
- `src/types.rs` — new types (Encoding, VbrQuality, Duration parsing), updated FileEntry and Config
- `src/filters.rs` — add min_duration to FilterSet
- `src/scanner.rs` — call probe for duration/bitrate during scan
- `src/copier.rs` — double buffering (reader/writer threads, sync_channel, 1MB buffer), transcoding integration
- `src/app.rs` — new SetupField variants, conditional field visibility, budget estimation, FileStatus::Reading/Converting, CopyMsg::Preparing
- `src/tui.rs` — conditional field rendering, preparing…/converting… status labels
- `src/cli.rs` — new CLI args, [converting]/[reencoding] log markers
- `src/main.rs` — wire new CLI args into Config
- `src/i18n.rs` — new locale strings for both EN and RU

---

### Task 1: Duration Parsing (types.rs)

**Files:**
- Modify: `src/types.rs`

- [ ] **Step 1: Write failing tests for duration parsing**

Add to the `tests` module in `src/types.rs`:

```rust
#[test]
fn parse_duration_bare_seconds() {
    assert_eq!(parse_duration("30").unwrap().as_secs(), 30);
    assert_eq!(parse_duration("120").unwrap().as_secs(), 120);
}

#[test]
fn parse_duration_with_suffix() {
    assert_eq!(parse_duration("30s").unwrap().as_secs(), 30);
    assert_eq!(parse_duration("2m").unwrap().as_secs(), 120);
    assert_eq!(parse_duration("2m30s").unwrap().as_secs(), 150);
}

#[test]
fn parse_duration_colon_format() {
    assert_eq!(parse_duration("2:30").unwrap().as_secs(), 150);
    assert_eq!(parse_duration("0:45").unwrap().as_secs(), 45);
    assert_eq!(parse_duration("10:00").unwrap().as_secs(), 600);
}

#[test]
fn parse_duration_errors() {
    assert!(parse_duration("").is_err());
    assert!(parse_duration("abc").is_err());
    assert!(parse_duration("0").is_err());
    assert!(parse_duration("-5").is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test parse_duration -- --nocapture 2>&1`
Expected: FAIL — `parse_duration` not found

- [ ] **Step 3: Implement duration parsing**

Add `ParseDurationError` enum and `parse_duration` function to `src/types.rs`:

```rust
pub fn parse_duration(s: &str) -> Result<Duration, ParseDurationError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(ParseDurationError::Empty);
    }

    let total_secs = if let Some((min_str, sec_str)) = s.split_once(':') {
        let mins = min_str
            .parse::<u64>()
            .map_err(|_| ParseDurationError::Invalid(s.to_string()))?;
        let secs = sec_str
            .parse::<u64>()
            .map_err(|_| ParseDurationError::Invalid(s.to_string()))?;
        mins.saturating_mul(60).saturating_add(secs)
    } else if let Some(idx) = s.find('m') {
        let min_str = s.get(..idx).ok_or_else(|| ParseDurationError::Invalid(s.to_string()))?;
        let rest = s.get(idx.saturating_add(1)..).unwrap_or("");
        let rest = rest.strip_suffix('s').unwrap_or(rest);
        let mins = min_str
            .parse::<u64>()
            .map_err(|_| ParseDurationError::Invalid(s.to_string()))?;
        let secs = if rest.is_empty() {
            0_u64
        } else {
            rest.parse::<u64>()
                .map_err(|_| ParseDurationError::Invalid(s.to_string()))?
        };
        mins.saturating_mul(60).saturating_add(secs)
    } else {
        let num_str = s.strip_suffix('s').unwrap_or(s);
        num_str
            .parse::<u64>()
            .map_err(|_| ParseDurationError::Invalid(s.to_string()))?
    };

    if total_secs == 0 {
        return Err(ParseDurationError::Zero);
    }

    Ok(Duration::from_secs(total_secs))
}
```

Also add `ParseDurationError` enum (same as `ParseSizeError` pattern) with `Empty`, `Invalid(String)`, `Zero` variants, `Display` and `Error` impls.

Note: `.get()` is used instead of `&s[..idx]` to satisfy `clippy::string_slice`. `.unwrap_or("")` on `Option<&str>` is fine — `clippy::unwrap_used` only fires on `.unwrap()`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test parse_duration -- --nocapture 2>&1`
Expected: All 4 tests PASS

- [ ] **Step 5: Clippy + fmt**

Run: `cargo fmt && cargo clippy -- -D warnings 2>&1`
Expected: Clean

- [ ] **Step 6: Commit**

```bash
git add src/types.rs
git commit -m "feat: duration parsing (30s, 2m, 2:30, 2m30s)"
```

---

### Task 2: Encoding Types + Updated Config/FileEntry (types.rs)

**Files:**
- Modify: `src/types.rs`
- Modify: `src/copier.rs` (update FileEntry usage in tests)
- Modify: `src/scanner.rs` (update FileEntry construction)

- [ ] **Step 1: Add Encoding and VbrQuality enums**

Add to `src/types.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encoding {
    #[default]
    Keep,
    Cbr,
    Vbr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VbrQuality {
    High,
    Medium,
    Low,
}

impl VbrQuality {
    pub fn avg_bitrate_kbps(self) -> u16 {
        match self {
            Self::High => 245,
            Self::Medium => 190,
            Self::Low => 130,
        }
    }

    pub fn lame_quality(self) -> u8 {
        match self {
            Self::High => 0,
            Self::Medium => 2,
            Self::Low => 6,
        }
    }
}
```

- [ ] **Step 2: Update FileEntry with duration and bitrate**

Change `FileEntry` in `src/types.rs`:

```rust
pub struct FileEntry {
    pub path: PathBuf,
    pub size: ByteSize,
    pub duration: Option<Duration>,
    pub bitrate_kbps: Option<u32>,
}
```

- [ ] **Step 3: Update Config with new fields**

Change `Config` in `src/types.rs`:

```rust
pub struct Config {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub max_size: Option<ByteSize>,
    pub min_file_size: Option<ByteSize>,
    pub min_duration: Option<Duration>,
    pub no_live: bool,
    pub keep_names: bool,
    pub overwrite: bool,
    pub allowed_extensions: Vec<String>,
    pub encoding: Encoding,
    pub cbr_bitrate: Option<u16>,
    pub vbr_quality: Option<VbrQuality>,
}
```

- [ ] **Step 4: Fix all compilation errors**

Update every place that constructs `FileEntry` (scanner.rs, copier.rs tests) to include `duration: None, bitrate_kbps: None`.

Update every place that constructs `Config` (app.rs `validate_and_start`, main.rs, cli.rs tests if any) to include `min_duration: None, encoding: Encoding::Keep, cbr_bitrate: None, vbr_quality: None`.

- [ ] **Step 5: Add `From<ParseDurationError> for Error`**

```rust
impl From<ParseDurationError> for Error {
    fn from(e: ParseDurationError) -> Self {
        Self::InvalidDuration(e)
    }
}
```

Add `InvalidDuration(ParseDurationError)` variant to `Error` enum and its `Display` impl.

- [ ] **Step 6: Run full test suite**

Run: `cargo test 2>&1`
Expected: All 37+ tests pass

- [ ] **Step 7: Commit**

```bash
git add src/types.rs src/scanner.rs src/copier.rs src/app.rs src/main.rs
git commit -m "feat: Encoding/VbrQuality types, updated FileEntry and Config"
```

---

### Task 3: Add Dependencies (Cargo.toml)

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Look up symphonia and mp3lame-encoder via context7**

Use context7 MCP to resolve library IDs and query docs for:
- `symphonia` — verify crate name, features needed (mp3, flac, ogg, wav, aac, alac decoders)
- `mp3lame-encoder` — verify crate name, API for Builder/encode/flush, static linking behavior

- [ ] **Step 2: Add dependencies to Cargo.toml**

```toml
[dependencies]
# ... existing deps ...
symphonia = { version = "0.5", features = ["mp3", "flac", "ogg", "wav", "aac", "alac", "isomp4", "pcm"] }
mp3lame-encoder = "0.2"
```

Note: verify exact versions and feature flags from context7 docs. The `isomp4` feature enables M4A/AAC in MP4 container.

- [ ] **Step 3: Verify it compiles**

Run: `cargo check 2>&1`
Expected: Compiles (may take a while first time to download and compile LAME C source)

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add symphonia and mp3lame-encoder"
```

---

### Task 4: Audio Metadata Probing (probe.rs)

**Files:**
- Create: `src/probe.rs`
- Modify: `src/main.rs` (add `mod probe;`)

- [ ] **Step 1: Write failing test for WAV duration probing**

Create `src/probe.rs` with test:

```rust
use std::path::Path;
use std::time::Duration;

pub struct AudioMeta {
    pub duration: Option<Duration>,
    pub bitrate_kbps: Option<u32>,
}

pub fn probe(_path: &Path) -> AudioMeta {
    AudioMeta {
        duration: None,
        bitrate_kbps: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_wav(path: &Path, sample_rate: u32, channels: u16, duration_secs: u32) {
        let bits_per_sample: u16 = 16;
        let byte_rate = sample_rate
            .saturating_mul(u32::from(channels))
            .saturating_mul(u32::from(bits_per_sample) / 8);
        let block_align = channels.saturating_mul(bits_per_sample / 8);
        let data_size = byte_rate.saturating_mul(duration_secs);
        let file_size = 36_u32.saturating_add(data_size);

        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(b"RIFF").unwrap();
        f.write_all(&file_size.to_le_bytes()).unwrap();
        f.write_all(b"WAVE").unwrap();
        f.write_all(b"fmt ").unwrap();
        f.write_all(&16_u32.to_le_bytes()).unwrap();
        f.write_all(&1_u16.to_le_bytes()).unwrap(); // PCM
        f.write_all(&channels.to_le_bytes()).unwrap();
        f.write_all(&sample_rate.to_le_bytes()).unwrap();
        f.write_all(&byte_rate.to_le_bytes()).unwrap();
        f.write_all(&block_align.to_le_bytes()).unwrap();
        f.write_all(&bits_per_sample.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&data_size.to_le_bytes()).unwrap();
        let silence = vec![0_u8; data_size as usize];
        f.write_all(&silence).unwrap();
    }

    #[test]
    fn probe_wav_duration() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wav");
        create_wav(&path, 44100, 2, 5);
        let meta = probe(&path);
        let dur = meta.duration.unwrap();
        assert!(dur.as_secs() >= 4 && dur.as_secs() <= 6);
    }

    #[test]
    fn probe_nonexistent_returns_none() {
        let meta = probe(Path::new("/nonexistent/file.mp3"));
        assert!(meta.duration.is_none());
        assert!(meta.bitrate_kbps.is_none());
    }
}
```

- [ ] **Step 2: Add `mod probe;` to `src/main.rs`**

- [ ] **Step 3: Run tests to verify probe_wav_duration fails**

Run: `cargo test probe_ -- --nocapture 2>&1`
Expected: `probe_nonexistent_returns_none` PASS, `probe_wav_duration` FAIL (duration is None)

- [ ] **Step 4: Implement probe function**

```rust
use std::fs::File;
use std::path::Path;
use std::time::Duration;

use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub struct AudioMeta {
    pub duration: Option<Duration>,
    pub bitrate_kbps: Option<u32>,
}

pub fn probe(path: &Path) -> AudioMeta {
    probe_inner(path).unwrap_or(AudioMeta {
        duration: None,
        bitrate_kbps: None,
    })
}

fn probe_inner(path: &Path) -> Option<AudioMeta> {
    let file = File::open(path).ok()?;
    let file_size = file.metadata().ok()?.len();
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;

    let track = probed.format.default_track()?;
    let params = &track.codec_params;

    let duration = params
        .time_base
        .zip(params.n_frames)
        .map(|(tb, frames)| {
            let time = tb.calc_time(frames);
            #[allow(clippy::cast_precision_loss, clippy::as_conversions)]
            let secs = time.seconds as f64 + time.frac;
            Duration::from_secs_f64(secs)
        });

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        clippy::as_conversions
    )]
    let bitrate_kbps = duration.and_then(|d| {
        let secs = d.as_secs_f64();
        if secs > 0.0_f64 {
            Some((file_size.saturating_mul(8) as f64 / secs / 1000.0_f64) as u32)
        } else {
            None
        }
    });

    Some(AudioMeta {
        duration,
        bitrate_kbps,
    })
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test probe_ -- --nocapture 2>&1`
Expected: Both PASS

- [ ] **Step 6: Clippy + fmt**

Run: `cargo fmt && cargo clippy -- -D warnings 2>&1`
Expected: Clean

- [ ] **Step 7: Commit**

```bash
git add src/probe.rs src/main.rs
git commit -m "feat: audio metadata probing via symphonia (duration, bitrate)"
```

---

### Task 5: Min-Duration Filter (filters.rs)

**Files:**
- Modify: `src/filters.rs`

- [ ] **Step 1: Write failing tests**

Add to `tests` module in `src/filters.rs`:

```rust
#[test]
fn min_duration_filter() {
    let fs = FilterSet::new(vec![], None, None, false);
    assert!(fs.matches(Path::new("song.mp3"), 1000, Some(Duration::from_secs(120))));

    let fs = FilterSet::new(vec![], None, Some(Duration::from_secs(60)), false);
    assert!(fs.matches(Path::new("song.mp3"), 1000, Some(Duration::from_secs(120))));
    assert!(!fs.matches(Path::new("sample.mp3"), 1000, Some(Duration::from_secs(10))));
}

#[test]
fn min_duration_none_passes() {
    let fs = FilterSet::new(vec![], None, Some(Duration::from_secs(60)), false);
    assert!(fs.matches(Path::new("unknown.bin"), 1000, None));
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test min_duration -- --nocapture 2>&1`
Expected: FAIL — `matches` signature doesn't accept duration

- [ ] **Step 3: Implement min_duration filter**

Update `FilterSet`:

```rust
pub struct FilterSet {
    allowed_extensions: Vec<String>,
    min_size: Option<ByteSize>,
    min_duration: Option<Duration>,
    no_live: bool,
}

impl FilterSet {
    pub fn new(
        allowed_extensions: Vec<String>,
        min_size: Option<ByteSize>,
        min_duration: Option<Duration>,
        no_live: bool,
    ) -> Self {
        Self {
            allowed_extensions,
            min_size,
            min_duration,
            no_live,
        }
    }

    pub fn matches(&self, path: &Path, size: u64, duration: Option<Duration>) -> bool {
        self.matches_extension(path)
            && self.matches_min_size(size)
            && self.matches_min_duration(duration)
            && self.matches_live(path)
    }

    fn matches_min_duration(&self, duration: Option<Duration>) -> bool {
        match (self.min_duration, duration) {
            (Some(min), Some(d)) => d >= min,
            _ => true,
        }
    }
    // ... existing methods unchanged
}
```

- [ ] **Step 4: Fix all callers of FilterSet::new and matches**

In `scanner.rs`, `tui.rs`, `cli.rs` — add `min_duration` parameter to `FilterSet::new` calls and `duration` parameter to `.matches()` calls. For now pass `None` for duration in scanner (will be wired in Task 6).

- [ ] **Step 5: Run full test suite**

Run: `cargo test 2>&1`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add src/filters.rs src/scanner.rs src/tui.rs src/cli.rs
git commit -m "feat: min-duration filter in FilterSet"
```

---

### Task 6: Scanner Integration (scanner.rs)

**Files:**
- Modify: `src/scanner.rs`

- [ ] **Step 1: Write failing test**

Add to `tests` module in `src/scanner.rs`:

```rust
#[test]
fn scan_populates_duration() {
    let dir = tempfile::tempdir().unwrap();
    let wav_path = dir.path().join("test.wav");
    crate::probe::tests::create_wav(&wav_path, 44100, 2, 3);

    let (tx, rx) = mpsc::channel();
    let shutdown = Arc::new(AtomicBool::new(false));
    let filters = FilterSet::new(vec!["wav".to_string()], None, None, false);

    scan(dir.path(), &filters, u64::MAX, &tx, &shutdown);

    let mut found_duration = false;
    for msg in rx.iter() {
        if let ScanMsg::Complete(files) = msg {
            if let Some(f) = files.first() {
                found_duration = f.duration.is_some();
            }
            break;
        }
    }
    assert!(found_duration);
}
```

Note: the `create_wav` helper needs to be `pub` in `probe::tests` module, or extracted to a shared test helper. Simplest: make it `pub(crate)` within `#[cfg(test)]`.

- [ ] **Step 2: Run to verify fail**

Run: `cargo test scan_populates_duration -- --nocapture 2>&1`
Expected: FAIL — duration is None

- [ ] **Step 3: Integrate probe into scan function**

In `scanner.rs`, after collecting metadata for each file, call `probe::probe()`:

```rust
use crate::probe;

// Inside scan(), after getting file metadata:
let meta = probe::probe(&path);

// When constructing FileEntry:
entries.push(FileEntry {
    path,
    size: ByteSize(size),
    duration: meta.duration,
    bitrate_kbps: meta.bitrate_kbps,
});
```

Pass `meta.duration` to `filters.matches()`:

```rust
let matched = filters.matches(&path, size, meta.duration);
```

Only probe files that match extension filter first (to avoid probing non-audio files). Restructure the filter check:

```rust
// Check extension first (cheap)
if !filters.matches_extension(&path) {
    let _ = tx.send(ScanMsg::FileFound { path, matched: false });
    continue;
}

// Probe audio metadata (reads headers)
let meta = probe::probe(&path);

// Check remaining filters
let matched = filters.matches_size_duration_live(&path, size, meta.duration);
```

This requires splitting `FilterSet::matches` into two stages. Alternative: just probe all files that pass the extension filter. Simpler approach — probe in scanner for all files, pass to `matches`:

```rust
let meta = if filters.matches_extension(&path) {
    probe::probe(&path)
} else {
    probe::AudioMeta { duration: None, bitrate_kbps: None }
};
let matched = filters.matches(&path, size, meta.duration);
```

This avoids probing non-audio files while keeping the filter API simple.

- [ ] **Step 4: Run full test suite**

Run: `cargo test 2>&1`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add src/scanner.rs src/probe.rs
git commit -m "feat: probe audio duration/bitrate during scan"
```

---

### Task 7: i18n Strings (i18n.rs)

**Files:**
- Modify: `src/i18n.rs`

- [ ] **Step 1: Add new fields to Locale struct**

```rust
pub struct Locale {
    // ... existing fields ...

    // New setup fields
    pub min_duration: &'static str,
    pub encoding_label: &'static str,
    pub bitrate_label: &'static str,
    pub quality_label: &'static str,

    // Placeholders
    pub ph_min_duration: &'static str,

    // Encoding options
    pub keep_original: &'static str,

    // VBR quality labels
    pub quality_high: &'static str,
    pub quality_medium: &'static str,
    pub quality_low: &'static str,

    // Errors
    pub err_invalid_duration: &'static str,
    pub err_bitrate_required: &'static str,

    // Copy statuses
    pub preparing: &'static str,
    pub converting: &'static str,
}
```

- [ ] **Step 2: Add English strings**

In `EN` static:

```rust
min_duration: "Min duration",
encoding_label: "Encoding",
bitrate_label: "Bitrate",
quality_label: "Quality",
ph_min_duration: "30s, 2m, 2:30",
keep_original: "Keep original",
quality_high: "High (~245kbps)",
quality_medium: "Medium (~190kbps)",
quality_low: "Low (~130kbps)",
err_invalid_duration: "Invalid duration format",
err_bitrate_required: "Bitrate is required for CBR",
preparing: "preparing\u{2026}",
converting: "converting\u{2026}",
```

- [ ] **Step 3: Add Russian strings**

In `RU` static:

```rust
min_duration: "\u{41c}\u{438}\u{43d}. \u{434}\u{43b}\u{438}\u{442}.",
encoding_label: "\u{41a}\u{43e}\u{434}\u{438}\u{440}\u{43e}\u{432}\u{430}\u{43d}\u{438}\u{435}",
bitrate_label: "\u{411}\u{438}\u{442}\u{440}\u{435}\u{439}\u{442}",
quality_label: "\u{41a}\u{430}\u{447}\u{435}\u{441}\u{442}\u{432}\u{43e}",
ph_min_duration: "30s, 2m, 2:30",
keep_original: "\u{411}\u{435}\u{437} \u{438}\u{437}\u{43c}\u{435}\u{43d}\u{435}\u{43d}\u{438}\u{439}",
quality_high: "\u{412}\u{44b}\u{441}\u{43e}\u{43a}\u{43e}\u{435} (~245kbps)",
quality_medium: "\u{421}\u{440}\u{435}\u{434}\u{43d}\u{435}\u{435} (~190kbps)",
quality_low: "\u{41d}\u{438}\u{437}\u{43a}\u{43e}\u{435} (~130kbps)",
err_invalid_duration: "\u{41d}\u{435}\u{43a}\u{43e}\u{440}\u{440}\u{435}\u{43a}\u{442}\u{43d}\u{430}\u{44f} \u{434}\u{43b}\u{438}\u{442}\u{435}\u{43b}\u{44c}\u{43d}\u{43e}\u{441}\u{442}\u{44c}",
err_bitrate_required: "\u{411}\u{438}\u{442}\u{440}\u{435}\u{439}\u{442} \u{43e}\u{431}\u{44f}\u{437}\u{430}\u{442}\u{435}\u{43b}\u{435}\u{43d} \u{434}\u{43b}\u{44f} CBR",
preparing: "\u{43f}\u{43e}\u{434}\u{433}\u{43e}\u{442}\u{43e}\u{432}\u{43a}\u{430}\u{2026}",
converting: "\u{43a}\u{43e}\u{43d}\u{432}\u{435}\u{440}\u{442}\u{430}\u{446}\u{438}\u{44f}\u{2026}",
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check 2>&1`
Expected: Compiles

- [ ] **Step 5: Commit**

```bash
git add src/i18n.rs
git commit -m "i18n: add strings for encoding, duration, quality (en/ru)"
```

---

### Task 8: Double Buffering (copier.rs)

**Files:**
- Modify: `src/copier.rs`

This is the largest change. The copier is rewritten from single-thread copy to a reader→writer pipeline.

- [ ] **Step 1: Define internal pipe message type and increase buffer**

```rust
const BUF_SIZE: usize = 1024 * 1024; // 1MB

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
```

- [ ] **Step 2: Add CopyMsg::Preparing variant**

```rust
pub enum CopyMsg {
    // ... existing variants ...
    Preparing {
        index: usize,
        converting: bool,
    },
}
```

- [ ] **Step 3: Write the writer thread function**

```rust
fn writer_thread(
    pipe_rx: mpsc::Receiver<PipeMsg>,
    progress_tx: Sender<CopyMsg>,
    shutdown: Arc<AtomicBool>,
) {
    let mut writer: Option<BufWriter<std::fs::File>> = None;

    for msg in pipe_rx {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

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
                if let Some(parent) = dest_path.parent() {
                    if let Err(e) = fs::create_dir_all(parent) {
                        let _ = progress_tx.send(CopyMsg::Error {
                            index,
                            path: dest_path,
                            error: e.to_string(),
                            is_destination: true,
                        });
                        shutdown.store(true, Ordering::Relaxed);
                        break;
                    }
                }
                match fs::File::create(&dest_path) {
                    Ok(f) => {
                        writer = Some(BufWriter::with_capacity(BUF_SIZE, f));
                        let _ = progress_tx.send(CopyMsg::FileStart {
                            index,
                            name,
                            original_path,
                            size,
                        });
                    }
                    Err(e) => {
                        let _ = progress_tx.send(CopyMsg::Error {
                            index,
                            path: dest_path,
                            error: e.to_string(),
                            is_destination: true,
                        });
                        shutdown.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
            PipeMsg::Chunk(data) => {
                if let Some(w) = writer.as_mut() {
                    #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
                    let len = data.len() as u64;
                    if let Err(e) = w.write_all(&data) {
                        let _ = progress_tx.send(CopyMsg::Error {
                            index: 0,
                            path: PathBuf::new(),
                            error: e.to_string(),
                            is_destination: true,
                        });
                        shutdown.store(true, Ordering::Relaxed);
                        break;
                    }
                    let _ = progress_tx.send(CopyMsg::Progress {
                        bytes_written: len,
                    });
                }
            }
            PipeMsg::EndFile { index } => {
                if let Some(w) = writer.as_mut() {
                    let _ = w.flush();
                }
                writer = None;
                let _ = progress_tx.send(CopyMsg::FileDone { index });
            }
            PipeMsg::SkipFile { index, path, error } => {
                let _ = progress_tx.send(CopyMsg::Error {
                    index,
                    path,
                    error,
                    is_destination: false,
                });
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
```

- [ ] **Step 4: Write the reader thread function (copy-only, no transcoding yet)**

```rust
fn reader_thread(
    files: Vec<FileEntry>,
    destination: PathBuf,
    keep_names: bool,
    overwrite: bool,
    pipe_tx: mpsc::SyncSender<PipeMsg>,
    shutdown: Arc<AtomicBool>,
) {
    let mut counter = 1_usize;

    for (index, entry) in files.iter().enumerate() {
        if shutdown.load(Ordering::Relaxed) {
            let _ = pipe_tx.send(PipeMsg::Abort);
            return;
        }

        let _ = pipe_tx.send(PipeMsg::Preparing {
            index,
            converting: false,
        });

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
                dest_path_keep_name(&destination, &entry.path)
            }
        } else {
            let (path, next) =
                dest_path_numbered(&destination, counter, &entry.path, overwrite);
            counter = next;
            path
        };

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

        let mut reader = BufReader::with_capacity(BUF_SIZE, src_file);
        let mut buf = vec![0_u8; BUF_SIZE];

        loop {
            if shutdown.load(Ordering::Relaxed) {
                let _ = pipe_tx.send(PipeMsg::Abort);
                return;
            }

            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = buf.get(..n).unwrap_or(&buf).to_vec();
                    if pipe_tx.send(PipeMsg::Chunk(chunk)).is_err() {
                        return;
                    }
                }
                Err(e) => {
                    let _ = pipe_tx.send(PipeMsg::SkipFile {
                        index,
                        path: entry.path.clone(),
                        error: e.to_string(),
                    });
                    break;
                }
            }
        }

        let _ = pipe_tx.send(PipeMsg::EndFile { index });
    }

    let _ = pipe_tx.send(PipeMsg::Done);
}
```

- [ ] **Step 5: Rewrite copy_files to spawn reader and writer threads**

```rust
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

    let (pipe_tx, pipe_rx) = mpsc::sync_channel::<PipeMsg>(4);
    let progress_tx = tx.clone();
    let writer_shutdown = Arc::clone(shutdown);

    let writer_handle = thread::spawn(move || {
        writer_thread(pipe_rx, progress_tx, writer_shutdown);
    });

    let files_owned: Vec<FileEntry> = files.to_vec();
    let dest_owned = destination.to_path_buf();
    let reader_shutdown = Arc::clone(shutdown);

    reader_thread(
        files_owned,
        dest_owned,
        keep_names,
        overwrite,
        pipe_tx,
        reader_shutdown,
    );

    let _ = writer_handle.join();
}
```

Note: `FileEntry` needs `Clone` derive. Add it in types.rs.

Note: `copy_files` signature stays the same for now. The encoding params will be added in Task 10.

- [ ] **Step 6: Run existing copier tests**

Run: `cargo test copier -- --nocapture 2>&1`
Expected: All 6 copier tests pass (same behavior, new architecture)

- [ ] **Step 7: Clippy + fmt**

Run: `cargo fmt && cargo clippy -- -D warnings 2>&1`

- [ ] **Step 8: Commit**

```bash
git add src/copier.rs src/types.rs
git commit -m "feat: double buffering with reader/writer pipeline (1MB buffer)"
```

---

### Task 9: Transcoding Pipeline (transcoder.rs)

**Files:**
- Create: `src/transcoder.rs`
- Modify: `src/main.rs` (add `mod transcoder;`)

- [ ] **Step 1: Look up mp3lame-encoder API via context7**

Use context7 MCP to verify the exact API for `mp3lame-encoder`:
- Builder construction
- Setting channels, sample rate
- Setting CBR bitrate vs VBR quality
- Encoding interleaved PCM samples
- Flushing encoder

- [ ] **Step 2: Write failing test for WAV→MP3 transcoding**

Create `src/transcoder.rs`:

```rust
use std::path::Path;

use crate::types::{Encoding, VbrQuality};

pub struct TranscodeConfig {
    pub encoding: Encoding,
    pub cbr_bitrate: Option<u16>,
    pub vbr_quality: Option<VbrQuality>,
}

pub fn transcode(
    _source: &Path,
    _config: &TranscodeConfig,
    _on_chunk: &mut dyn FnMut(&[u8]),
) -> Result<(), String> {
    Err("not implemented".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcode_wav_to_cbr_mp3() {
        let dir = tempfile::tempdir().unwrap();
        let wav_path = dir.path().join("test.wav");
        crate::probe::tests::create_wav(&wav_path, 44100, 2, 2);

        let config = TranscodeConfig {
            encoding: Encoding::Cbr,
            cbr_bitrate: Some(128),
            vbr_quality: None,
        };

        let mut output = Vec::new();
        transcode(&wav_path, &config, &mut |chunk| {
            output.extend_from_slice(chunk);
        })
        .unwrap();

        assert!(!output.is_empty());
        // MP3 files start with 0xFF 0xFB (or 0xFF 0xE2, etc.) or ID3 tag
        assert!(output.len() > 100);
    }

    #[test]
    fn transcode_wav_to_vbr_mp3() {
        let dir = tempfile::tempdir().unwrap();
        let wav_path = dir.path().join("test.wav");
        crate::probe::tests::create_wav(&wav_path, 44100, 2, 2);

        let config = TranscodeConfig {
            encoding: Encoding::Vbr,
            cbr_bitrate: None,
            vbr_quality: Some(VbrQuality::Medium),
        };

        let mut output = Vec::new();
        transcode(&wav_path, &config, &mut |chunk| {
            output.extend_from_slice(chunk);
        })
        .unwrap();

        assert!(!output.is_empty());
    }
}
```

- [ ] **Step 3: Run to verify fail**

Run: `cargo test transcode_ -- --nocapture 2>&1`
Expected: FAIL — "not implemented"

- [ ] **Step 4: Implement transcode function**

```rust
use std::fs::File;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::types::{Encoding, VbrQuality};

pub struct TranscodeConfig {
    pub encoding: Encoding,
    pub cbr_bitrate: Option<u16>,
    pub vbr_quality: Option<VbrQuality>,
}

pub fn transcode(
    source: &Path,
    config: &TranscodeConfig,
    on_chunk: &mut dyn FnMut(&[u8]),
) -> Result<(), String> {
    let file = File::open(source).map_err(|e| e.to_string())?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = source.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| e.to_string())?;

    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or("no audio track")?;

    let track_id = track.id;
    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or("unknown sample rate")?;
    let channels = track
        .codec_params
        .channels
        .ok_or("unknown channels")?
        .count();

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| e.to_string())?;

    // Build lame encoder
    // NOTE: exact API depends on mp3lame-encoder crate version — verify via context7
    let mut encoder = mp3lame_encoder::Builder::new()
        .ok_or("failed to create lame encoder")?;
    encoder
        .set_num_channels(channels as u8)
        .map_err(|e| format!("{e:?}"))?;
    encoder
        .set_sample_rate(sample_rate)
        .map_err(|e| format!("{e:?}"))?;

    match config.encoding {
        Encoding::Cbr => {
            if let Some(br) = config.cbr_bitrate {
                encoder
                    .set_brate(mp3lame_encoder::Bitrate::from(br))
                    .map_err(|e| format!("{e:?}"))?;
            }
        }
        Encoding::Vbr => {
            if let Some(q) = config.vbr_quality {
                encoder
                    .set_quality(mp3lame_encoder::Quality::from(q.lame_quality()))
                    .map_err(|e| format!("{e:?}"))?;
            }
        }
        Encoding::Keep => return Err("transcode called with Keep encoding".to_string()),
    }

    let mut encoder = encoder.build().map_err(|e| format!("{e:?}"))?;

    let mut sample_buf: Option<SampleBuffer<i16>> = None;
    let mut mp3_buf = vec![0_u8; BUF_SIZE];

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(e) => return Err(e.to_string()),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = decoder.decode(&packet).map_err(|e| e.to_string())?;
        let spec = *decoded.spec();
        let duration = decoded.capacity();

        let buf = sample_buf.get_or_insert_with(|| {
            SampleBuffer::<i16>::new(duration as u64, spec)
        });

        buf.copy_interleaved_ref(decoded);
        let samples = buf.samples();

        let encoded = encoder
            .encode(
                mp3lame_encoder::InterleavedPcm(samples),
                &mut mp3_buf,
            )
            .map_err(|e| format!("{e:?}"))?;

        if encoded > 0 {
            if let Some(data) = mp3_buf.get(..encoded) {
                on_chunk(data);
            }
        }
    }

    // Flush remaining
    let flushed = encoder
        .flush::<mp3lame_encoder::FlushNoGap>(&mut mp3_buf)
        .map_err(|e| format!("{e:?}"))?;

    if flushed > 0 {
        if let Some(data) = mp3_buf.get(..flushed) {
            on_chunk(data);
        }
    }

    Ok(())
}

const BUF_SIZE: usize = 8192;
```

**IMPORTANT:** The exact mp3lame-encoder API (Builder, set_brate, Bitrate::from, Quality::from, InterleavedPcm, FlushNoGap) MUST be verified against context7 docs during implementation. The code above is approximate — method names and type conversions may differ.

- [ ] **Step 5: Run tests**

Run: `cargo test transcode_ -- --nocapture 2>&1`
Expected: Both tests PASS

- [ ] **Step 6: Clippy + fmt**

Run: `cargo fmt && cargo clippy -- -D warnings 2>&1`

- [ ] **Step 7: Commit**

```bash
git add src/transcoder.rs src/main.rs
git commit -m "feat: audio transcoding pipeline (symphonia decode + lame encode)"
```

---

### Task 10: Copier Transcoding Integration (copier.rs)

**Files:**
- Modify: `src/copier.rs`

- [ ] **Step 1: Update copy_files signature to accept Config**

Change `copy_files` to accept `&Config` instead of individual params:

```rust
pub fn copy_files(
    files: &[FileEntry],
    config: &Config,
    tx: &Sender<CopyMsg>,
    shutdown: &Arc<AtomicBool>,
)
```

Update all callers (tui.rs `spawn_copier`, cli.rs copy spawning) to pass `&config`.

- [ ] **Step 2: Add transcoding decision logic to reader thread**

In `reader_thread`, for each file, decide whether to copy or transcode:

```rust
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
                return true; // non-MP3 always needs conversion
            }
            // MP3: check bitrate threshold
            let threshold = match config.encoding {
                Encoding::Cbr => u32::from(config.cbr_bitrate.unwrap_or(0)),
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
```

- [ ] **Step 3: Add transcode path to reader thread**

When `needs_transcode` is true, use `transcoder::transcode()` instead of raw file read:

```rust
if needs_transcode(entry, &config) {
    let _ = pipe_tx.send(PipeMsg::Preparing { index, converting: true });

    // Change extension to .mp3 in dest_path
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

    let tc_config = TranscodeConfig {
        encoding: config.encoding,
        cbr_bitrate: config.cbr_bitrate,
        vbr_quality: config.vbr_quality,
    };

    let pipe = &pipe_tx;
    match transcoder::transcode(&entry.path, &tc_config, &mut |chunk| {
        let _ = pipe.send(PipeMsg::Chunk(chunk.to_vec()));
    }) {
        Ok(()) => {
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
} else {
    // existing copy path
    let _ = pipe_tx.send(PipeMsg::Preparing { index, converting: false });
    // ... existing read logic ...
}
```

- [ ] **Step 4: Write integration test**

```rust
#[test]
fn copy_with_transcode_wav_to_mp3() {
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let wav_path = src.path().join("song.wav");
    crate::probe::tests::create_wav(&wav_path, 44100, 2, 1);

    let files = vec![FileEntry {
        path: wav_path,
        size: ByteSize(176_400),
        duration: Some(Duration::from_secs(1)),
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
        cbr_bitrate: Some(128),
        vbr_quality: None,
    };

    let (tx, rx) = mpsc::channel();
    let shutdown = Arc::new(AtomicBool::new(false));

    copy_files(&files, &config, &tx, &shutdown);

    let messages: Vec<CopyMsg> = rx.try_iter().collect();
    assert!(messages.iter().any(|m| matches!(m, CopyMsg::Complete)));
    assert!(dst.path().join("song.mp3").exists()); // .wav → .mp3
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test copier -- --nocapture 2>&1`
Expected: All tests pass including new transcoding test

- [ ] **Step 6: Commit**

```bash
git add src/copier.rs src/tui.rs src/cli.rs
git commit -m "feat: integrate transcoding into copier pipeline"
```

---

### Task 11: App Form Fields + Logic (app.rs)

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add new SetupField variants**

```rust
pub enum SetupField {
    Source,
    Destination,
    MinSize,
    MinDuration,   // NEW
    Extensions,
    Exclude,
    Encoding,      // NEW
    Bitrate,       // NEW (conditional)
    NoLive,
    KeepNames,
    Overwrite,
    Start,
}
```

Remove `Size` (it was `max_size` which per spec doesn't exist as a TUI field).

- [ ] **Step 2: Add new fields to SetupForm**

```rust
pub struct SetupForm {
    pub source: String,
    pub destination: String,
    pub min_size: String,
    pub min_duration: String,       // NEW
    pub extensions: String,
    pub exclude: String,
    pub encoding: Encoding,          // NEW
    pub cbr_bitrate_idx: usize,      // NEW — index into BITRATE_OPTIONS
    pub vbr_quality: VbrQuality,     // NEW
    pub no_live: bool,
    pub keep_names: bool,
    pub overwrite: bool,
    pub focused: SetupField,
    pub error: Option<String>,
    pub dropdown: Dropdown,
    pub cursor: usize,
}
```

Add constants:

```rust
pub const BITRATE_OPTIONS: &[u16] = &[128, 160, 192, 224, 256, 320];
```

- [ ] **Step 3: Update field navigation (next/prev) with conditional skip**

The `Bitrate` field is only active when `encoding != Keep`. Navigation must skip it when hidden:

```rust
impl SetupField {
    pub fn next(self, encoding: Encoding) -> Self {
        let candidate = match self {
            Self::Source => Self::Destination,
            Self::Destination => Self::MinSize,
            Self::MinSize => Self::MinDuration,
            Self::MinDuration => Self::Extensions,
            Self::Extensions => Self::Exclude,
            Self::Exclude => Self::Encoding,
            Self::Encoding => Self::Bitrate,
            Self::Bitrate => Self::NoLive,
            Self::NoLive => Self::KeepNames,
            Self::KeepNames => Self::Overwrite,
            Self::Overwrite => Self::Start,
            Self::Start => Self::Start,
        };
        if candidate == Self::Bitrate && encoding == Encoding::Keep {
            Self::NoLive
        } else {
            candidate
        }
    }
    // Similar for prev()
}
```

- [ ] **Step 4: Update SetupField trait methods**

```rust
pub fn is_text(self) -> bool {
    matches!(
        self,
        Self::Source | Self::Destination | Self::MinSize | Self::MinDuration | Self::Extensions | Self::Exclude
    )
}

pub fn is_checkbox(self) -> bool {
    matches!(self, Self::NoLive | Self::KeepNames | Self::Overwrite)
}

pub fn is_dropdown_field(self) -> bool {
    matches!(self, Self::Encoding | Self::Bitrate)
}

pub fn placeholder(self, locale: &Locale) -> &'static str {
    match self {
        // ... existing ...
        Self::MinDuration => locale.ph_min_duration,
        _ => "",
    }
}
```

- [ ] **Step 5: Handle Encoding/Bitrate field input**

Encoding field cycles through Keep/CBR/VBR on Left/Right or Space. Bitrate field cycles through options on Left/Right or Space.

Add to `update_setup`:

```rust
KeyCode::Left | KeyCode::Right if form.focused == SetupField::Encoding => {
    form.encoding = match (key.code, form.encoding) {
        (KeyCode::Right, Encoding::Keep) => Encoding::Cbr,
        (KeyCode::Right, Encoding::Cbr) => Encoding::Vbr,
        (KeyCode::Right, Encoding::Vbr) => Encoding::Vbr,
        (KeyCode::Left, Encoding::Vbr) => Encoding::Cbr,
        (KeyCode::Left, Encoding::Cbr) => Encoding::Keep,
        (KeyCode::Left, Encoding::Keep) => Encoding::Keep,
    };
    Effect::None
}
KeyCode::Left | KeyCode::Right if form.focused == SetupField::Bitrate => {
    match form.encoding {
        Encoding::Cbr => {
            let max = BITRATE_OPTIONS.len().saturating_sub(1);
            form.cbr_bitrate_idx = match key.code {
                KeyCode::Right => (form.cbr_bitrate_idx.saturating_add(1)).min(max),
                KeyCode::Left => form.cbr_bitrate_idx.saturating_sub(1),
                _ => form.cbr_bitrate_idx,
            };
        }
        Encoding::Vbr => {
            form.vbr_quality = match (key.code, form.vbr_quality) {
                (KeyCode::Right, VbrQuality::High) => VbrQuality::Medium,
                (KeyCode::Right, VbrQuality::Medium) => VbrQuality::Low,
                (KeyCode::Left, VbrQuality::Low) => VbrQuality::Medium,
                (KeyCode::Left, VbrQuality::Medium) => VbrQuality::High,
                (_, q) => q,
            };
        }
        Encoding::Keep => {}
    }
    Effect::None
}
```

- [ ] **Step 6: Update validate_and_start**

Add min_duration parsing and encoding validation:

```rust
let min_duration = if form.min_duration.is_empty() {
    None
} else {
    match parse_duration(&form.min_duration) {
        Ok(d) => Some(d),
        Err(_) => {
            form.error = Some(locale.err_invalid_duration.to_string());
            form.focused = SetupField::MinDuration;
            return Effect::None;
        }
    }
};

let (encoding, cbr_bitrate, vbr_quality) = match form.encoding {
    Encoding::Keep => (Encoding::Keep, None, None),
    Encoding::Cbr => {
        let br = BITRATE_OPTIONS
            .get(form.cbr_bitrate_idx)
            .copied()
            .unwrap_or(192);
        (Encoding::Cbr, Some(br), None)
    }
    Encoding::Vbr => (Encoding::Vbr, None, Some(form.vbr_quality)),
};
```

Include these in the `Config` construction.

- [ ] **Step 7: Update field_is_invalid for MinDuration**

```rust
SetupField::MinDuration => {
    !value.is_empty() && parse_duration(value).is_err()
}
```

- [ ] **Step 8: Add budget estimation for transcoded files**

In `handle_scan` → `ScanMsg::Complete`, when building `CopyState`, compute estimated sizes:

```rust
fn estimated_output_size(entry: &FileEntry, config: &Config) -> u64 {
    if config.encoding == Encoding::Keep {
        return entry.size.as_u64();
    }
    // Check if this file would be transcoded
    let ext = entry.path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let threshold = match config.encoding {
        Encoding::Cbr => u32::from(config.cbr_bitrate.unwrap_or(0)),
        Encoding::Vbr => u32::from(config.vbr_quality.unwrap_or(VbrQuality::Medium).avg_bitrate_kbps()),
        Encoding::Keep => return entry.size.as_u64(),
    };
    let needs_convert = ext != "mp3" || entry.bitrate_kbps.is_some_and(|br| br > threshold);
    if !needs_convert {
        return entry.size.as_u64();
    }
    // Estimate: duration × target_bitrate / 8
    let target_kbps = match config.encoding {
        Encoding::Cbr => u64::from(config.cbr_bitrate.unwrap_or(192)),
        Encoding::Vbr => u64::from(config.vbr_quality.unwrap_or(VbrQuality::Medium).avg_bitrate_kbps()),
        Encoding::Keep => return entry.size.as_u64(),
    };
    entry.duration
        .map(|d| d.as_secs().saturating_mul(target_kbps).saturating_mul(1000) / 8)
        .unwrap_or(entry.size.as_u64())
}
```

Use this in `pack_into_budget` (scanner.rs) or when computing `total_bytes` in `CopyState`.

- [ ] **Step 9: Handle CopyMsg::Preparing in update**

```rust
CopyMsg::Preparing { index, converting } => {
    if let Phase::Copying(cs) = &mut model.phase {
        if let Some(file) = cs.files.get_mut(index) {
            file.status = if converting {
                FileStatus::Converting
            } else {
                FileStatus::Reading
            };
        }
    }
    Effect::None
}
```

- [ ] **Step 10: Add FileStatus::Reading and FileStatus::Converting**

```rust
pub enum FileStatus {
    Queued,
    Reading,      // NEW
    Converting,   // NEW
    Copying,
    Done,
    Failed,
}
```

- [ ] **Step 11: Run full test suite**

Run: `cargo test 2>&1`
Expected: All tests pass (update existing tests for new Config fields)

- [ ] **Step 12: Commit**

```bash
git add src/app.rs src/types.rs
git commit -m "feat: encoding/duration form fields, budget estimation, Reading/Converting statuses"
```

---

### Task 12: TUI Rendering (tui.rs)

**Files:**
- Modify: `src/tui.rs`

- [ ] **Step 1: Update view_setup for new fields**

Add Min duration as a text field after Min size. Add Encoding as a cycling selector. Add conditional Bitrate/Quality field.

In the `fields` array, add:

```rust
(SetupField::MinDuration, locale.min_duration, &form.min_duration),
```

For Encoding field (after text fields, before checkboxes), render as a selector:

```rust
let encoding_label = format!("{:<label_width$}", locale.encoding_label);
let encoding_value = match form.encoding {
    Encoding::Keep => locale.keep_original,
    Encoding::Cbr => "CBR",
    Encoding::Vbr => "VBR",
};
let encoding_style = if form.focused == SetupField::Encoding {
    Style::default().fg(Color::Yellow)
} else {
    Style::default()
};
let line = Line::from(vec![
    Span::styled(encoding_label, encoding_style),
    Span::styled(format!("\u{25C0} {encoding_value} \u{25B6}"), encoding_style),
]);
```

- [ ] **Step 2: Render conditional Bitrate/Quality field**

Only render when `form.encoding != Encoding::Keep`:

```rust
if form.encoding != Encoding::Keep {
    let (label, value) = match form.encoding {
        Encoding::Cbr => {
            let br = BITRATE_OPTIONS
                .get(form.cbr_bitrate_idx)
                .copied()
                .unwrap_or(192);
            (locale.bitrate_label, format!("\u{25C0} {br} kbps \u{25B6}"))
        }
        Encoding::Vbr => {
            let q = match form.vbr_quality {
                VbrQuality::High => locale.quality_high,
                VbrQuality::Medium => locale.quality_medium,
                VbrQuality::Low => locale.quality_low,
            };
            (locale.quality_label, format!("\u{25C0} {q} \u{25B6}"))
        }
        Encoding::Keep => unreachable!(),
    };
    let style = if form.focused == SetupField::Bitrate {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let line = Line::from(vec![
        Span::styled(format!("{label:<label_width$}"), style),
        Span::styled(value, style),
    ]);
    // render in appropriate chunk
}
```

- [ ] **Step 3: Update layout chunk count**

The layout needs to account for the conditional Bitrate row. Total rows = existing + 2 (MinDuration, Encoding) + 1 conditional (Bitrate). Recalculate the `chunks` layout constraints.

- [ ] **Step 4: Render preparing/converting status in file list**

In `render_file_list`, update upcoming rendering to show status labels:

```rust
for item in upcoming.iter().rev() {
    let (suffix, style) = match item.status {
        FileStatus::Reading => (
            format!("   {}", locale.preparing),
            Style::default().fg(Color::Yellow),
        ),
        FileStatus::Converting => (
            format!("   {}", locale.converting),
            Style::default().fg(Color::Yellow),
        ),
        _ => (String::new(), Style::default().fg(Color::DarkGray)),
    };
    lines.push(Line::from(vec![
        Span::styled(format!("  {}", item.name), Style::default().fg(Color::DarkGray)),
        Span::styled(suffix, style),
    ]));
}
```

- [ ] **Step 5: Verify visually**

Run: `cargo run` (TUI mode)
Verify: new fields appear, Encoding cycles on Left/Right, Bitrate appears/disappears with encoding selection.

- [ ] **Step 6: Clippy + fmt**

Run: `cargo fmt && cargo clippy -- -D warnings 2>&1`

- [ ] **Step 7: Commit**

```bash
git add src/tui.rs
git commit -m "feat: TUI rendering for encoding/duration fields, preparing/converting labels"
```

---

### Task 13: CLI Args + Main Wiring (cli.rs, main.rs)

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Add new CLI args to Args struct**

In `src/main.rs`, update `Args`:

```rust
#[derive(Parser)]
struct Args {
    source: Option<PathBuf>,
    destination: Option<PathBuf>,
    #[arg(long, value_parser = parse_byte_size)]
    size: Option<ByteSize>,
    #[arg(long, value_parser = parse_byte_size)]
    min_size: Option<ByteSize>,
    #[arg(long, value_parser = parse_duration_arg)]
    min_duration: Option<Duration>,
    #[arg(long, default_value = "keep")]
    encoding: String,
    #[arg(long)]
    bitrate: Option<u16>,
    #[arg(long, default_value = "medium")]
    quality: Option<String>,
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

fn parse_duration_arg(s: &str) -> Result<Duration, String> {
    parse_duration(s).map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Parse encoding args in main**

```rust
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
    match args.bitrate {
        Some(br) => Some(br),
        None => {
            eprintln!("{}", locale.err_bitrate_required);
            return ExitCode::FAILURE;
        }
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
```

Include in Config construction.

- [ ] **Step 3: Add log markers to cli.rs**

In `Phase::Copying` log output, add conversion marker:

```rust
Phase::Copying(cs) => {
    if let Some(cur) = cs.current() {
        let idx = cs.current_index;
        if last_printed_index != Some(idx)
            && matches!(cur.status, FileStatus::Copying)
        {
            last_printed_index = Some(idx);
            let marker = match cur.conversion_status {
                Some(ConversionKind::Converting) => " [converting]",
                Some(ConversionKind::Reencoding) => " [reencoding]",
                None => "",
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
```

This requires adding a `conversion_status` field to `FileItem`. Set it in the `Preparing` message handler based on whether it's converting and whether the source is MP3 (reencoding) vs non-MP3 (converting).

- [ ] **Step 4: Pass min_duration to FilterSet in scanner spawning**

In both `tui.rs` and `cli.rs`, update `FilterSet::new` calls to include `config.min_duration`.

- [ ] **Step 5: Run full test suite**

Run: `cargo test 2>&1`
Expected: All tests pass

- [ ] **Step 6: Clippy + fmt**

Run: `cargo fmt && cargo clippy -- -D warnings 2>&1`

- [ ] **Step 7: End-to-end test**

Manual test with CLI:
```bash
# Copy with transcoding
cargo run -- /path/to/music /tmp/test-output --encoding cbr --bitrate 192 --min-duration 30s

# Check output files are MP3, verify duration filter worked
ls -la /tmp/test-output/
```

- [ ] **Step 8: Commit**

```bash
git add src/main.rs src/cli.rs src/tui.rs src/app.rs
git commit -m "feat: CLI args for encoding/duration, log markers [converting]/[reencoding]"
```

---

## Cross-Task Notes

### Clippy Compatibility
All code must respect these active deny lints:
- `unwrap_used` / `expect_used` — use `?`, `.ok_or()`, `.unwrap_or()` only
- `indexing_slicing` — use `.get()` instead of `[]`
- `as_conversions` — use `From`/`TryFrom`, annotate with `#[allow]` where unavoidable (e.g., `as f64`)
- `arithmetic_side_effects` — use `saturating_*` ops
- `cast_*` — annotate with `#[allow]` where needed

### Test Audio Files
The `create_wav` helper in `probe::tests` generates valid WAV files for testing. Make it `pub(crate)` so scanner and copier tests can reuse it.

### Feature Flags
Consider gating symphonia/lame behind a cargo feature flag if binary size increase is unacceptable. This is optional — evaluate after measuring.
