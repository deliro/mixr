use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::copier::CopyMsg;
use crate::i18n::{self, Locale};
use crate::scanner::ScanMsg;
use crate::types::{ByteSize, CbrBitrate, Config, Encoding, FileEntry, VbrQuality, parse_duration};

pub const MAX_UPCOMING: usize = 3;
pub const MAX_HISTORY: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupField {
    Source,
    Destination,
    Size,
    MinSize,
    MinDuration,
    Extensions,
    Exclude,
    Encoding,
    Bitrate,
    NoLive,
    KeepNames,
    Overwrite,
    Start,
}

impl SetupField {
    pub fn next(self, encoding: Encoding) -> Self {
        let n = match self {
            Self::Source => Self::Destination,
            Self::Destination => Self::Size,
            Self::Size => Self::MinSize,
            Self::MinSize => Self::MinDuration,
            Self::MinDuration => Self::Extensions,
            Self::Extensions => Self::Exclude,
            Self::Exclude => Self::Encoding,
            Self::Encoding => Self::Bitrate,
            Self::Bitrate => Self::NoLive,
            Self::NoLive => Self::KeepNames,
            Self::KeepNames => Self::Overwrite,
            Self::Overwrite => Self::Start,
            Self::Start => Self::Source,
        };
        if n == Self::Bitrate && encoding == Encoding::Keep {
            Self::NoLive
        } else {
            n
        }
    }

    pub fn prev(self, encoding: Encoding) -> Self {
        let n = match self {
            Self::Source => Self::Start,
            Self::Destination => Self::Source,
            Self::Size => Self::Destination,
            Self::MinSize => Self::Size,
            Self::MinDuration => Self::MinSize,
            Self::Extensions => Self::MinDuration,
            Self::Exclude => Self::Extensions,
            Self::Encoding => Self::Exclude,
            Self::Bitrate => Self::Encoding,
            Self::NoLive => Self::Bitrate,
            Self::KeepNames => Self::NoLive,
            Self::Overwrite => Self::KeepNames,
            Self::Start => Self::Overwrite,
        };
        if n == Self::Bitrate && encoding == Encoding::Keep {
            Self::Encoding
        } else {
            n
        }
    }

    pub fn is_text(self) -> bool {
        matches!(
            self,
            Self::Source
                | Self::Destination
                | Self::Size
                | Self::MinSize
                | Self::MinDuration
                | Self::Extensions
                | Self::Exclude
        )
    }

    pub fn is_checkbox(self) -> bool {
        matches!(self, Self::NoLive | Self::KeepNames | Self::Overwrite)
    }

    pub fn is_path(self) -> bool {
        matches!(self, Self::Source | Self::Destination)
    }

    pub fn is_ext(self) -> bool {
        matches!(self, Self::Extensions | Self::Exclude)
    }

    pub fn placeholder(self, locale: &Locale) -> &'static str {
        match self {
            Self::Source => locale.ph_source,
            Self::Destination => locale.ph_destination,
            Self::Size => locale.ph_size,
            Self::MinSize => locale.ph_min_size,
            Self::MinDuration => locale.ph_min_duration,
            Self::Extensions => locale.ph_extensions,
            Self::Exclude => locale.ph_exclude,
            _ => "",
        }
    }
}

pub const MAX_DROPDOWN: usize = 8;

#[derive(Debug, Clone, Default)]
pub struct Dropdown {
    pub entries: Vec<String>,
    pub selected: usize,
    pub scroll_offset: usize,
    pub visible: bool,
}

#[derive(Debug, Clone)]
pub struct SetupForm {
    pub source: String,
    pub destination: String,
    pub size: String,
    pub min_size: String,
    pub min_duration: String,
    pub extensions: String,
    pub exclude: String,
    pub encoding: Encoding,
    pub cbr_bitrate: CbrBitrate,
    pub vbr_quality: VbrQuality,
    pub no_live: bool,
    pub keep_names: bool,
    pub overwrite: bool,
    pub focused: SetupField,
    pub error: Option<String>,
    pub dropdown: Dropdown,
    pub cursor: usize,
}

impl Default for SetupForm {
    fn default() -> Self {
        Self {
            source: String::new(),
            destination: String::new(),
            size: String::new(),
            min_size: String::new(),
            min_duration: String::new(),
            extensions: String::new(),
            exclude: String::new(),
            encoding: Encoding::Keep,
            cbr_bitrate: CbrBitrate::Kbps320,
            vbr_quality: VbrQuality::High,
            no_live: false,
            keep_names: false,
            overwrite: false,
            focused: SetupField::Source,
            error: None,
            dropdown: Dropdown::default(),
            cursor: 0_usize,
        }
    }
}

impl SetupForm {
    fn from_config(config: &Config, _locale: &Locale) -> Self {
        Self {
            source: config.source.to_str().unwrap_or("").to_string(),
            destination: config.destination.to_str().unwrap_or("").to_string(),
            size: config.max_size.map_or_else(String::new, |s| format!("{s}")),
            min_size: config
                .min_file_size
                .map_or_else(String::new, |s| format!("{s}")),
            min_duration: config.min_duration.map_or_else(String::new, |d| {
                let secs = d.as_secs();
                let m = secs / 60_u64;
                let s = secs % 60_u64;
                if m > 0_u64 && s > 0_u64 {
                    format!("{m}m{s}s")
                } else if m > 0_u64 {
                    format!("{m}m")
                } else {
                    format!("{secs}s")
                }
            }),
            extensions: config.allowed_extensions.join(", "),
            exclude: String::new(),
            encoding: config.encoding,
            cbr_bitrate: config.cbr_bitrate.unwrap_or(CbrBitrate::Kbps320),
            vbr_quality: config.vbr_quality.unwrap_or(VbrQuality::High),
            no_live: config.no_live,
            keep_names: config.keep_names,
            overwrite: config.overwrite,
            focused: SetupField::Source,
            error: None,
            dropdown: Dropdown::default(),
            cursor: 0_usize,
        }
    }

    pub fn focused_value(&self) -> Option<&str> {
        match self.focused {
            SetupField::Source => Some(&self.source),
            SetupField::Destination => Some(&self.destination),
            SetupField::Size => Some(&self.size),
            SetupField::MinSize => Some(&self.min_size),
            SetupField::MinDuration => Some(&self.min_duration),
            SetupField::Extensions => Some(&self.extensions),
            SetupField::Exclude => Some(&self.exclude),
            _ => None,
        }
    }

    pub fn focused_value_mut(&mut self) -> Option<&mut String> {
        match self.focused {
            SetupField::Source => Some(&mut self.source),
            SetupField::Destination => Some(&mut self.destination),
            SetupField::Size => Some(&mut self.size),
            SetupField::MinSize => Some(&mut self.min_size),
            SetupField::MinDuration => Some(&mut self.min_duration),
            SetupField::Extensions => Some(&mut self.extensions),
            SetupField::Exclude => Some(&mut self.exclude),
            _ => None,
        }
    }

    pub fn sync_cursor(&mut self) {
        let len = self.focused_value().map_or(0_usize, |v| v.chars().count());
        self.cursor = len;
    }

    fn insert_char(&mut self, c: char) {
        let cursor = self.cursor;
        if let Some(val) = self.focused_value_mut() {
            let byte_idx = char_to_byte_idx(val, cursor);
            val.insert(byte_idx, c);
        }
        self.cursor = cursor.saturating_add(1);
    }

    fn delete_char_before_cursor(&mut self) {
        if self.cursor == 0_usize {
            return;
        }
        let cursor = self.cursor;
        if let Some(val) = self.focused_value_mut() {
            let byte_idx = char_to_byte_idx(val, cursor);
            let prev = char_to_byte_idx(val, cursor.saturating_sub(1));
            val.drain(prev..byte_idx);
        }
        self.cursor = cursor.saturating_sub(1);
    }

    fn delete_word_before_cursor(&mut self) {
        if self.cursor == 0_usize {
            return;
        }
        let new_cursor = self.word_boundary_left();
        let byte_start = self
            .focused_value()
            .map_or(0_usize, |v| char_to_byte_idx(v, new_cursor));
        let byte_end = self
            .focused_value()
            .map_or(0_usize, |v| char_to_byte_idx(v, self.cursor));
        if let Some(v) = self.focused_value_mut() {
            v.drain(byte_start..byte_end);
        }
        self.cursor = new_cursor;
    }

    fn word_boundary_left(&self) -> usize {
        let Some(val) = self.focused_value() else {
            return 0_usize;
        };
        let before: Vec<char> = val.chars().take(self.cursor).collect();
        let skipped_separators = before
            .iter()
            .rev()
            .take_while(|c| is_word_separator(**c))
            .count();
        let skipped_word = before
            .iter()
            .rev()
            .skip(skipped_separators)
            .take_while(|c| !is_word_separator(**c))
            .count();
        self.cursor
            .saturating_sub(skipped_separators)
            .saturating_sub(skipped_word)
    }

    fn word_boundary_right(&self) -> usize {
        let Some(val) = self.focused_value() else {
            return 0_usize;
        };
        let after_cursor = val.chars().skip(self.cursor);
        let skipped_word = after_cursor
            .clone()
            .take_while(|c| !is_word_separator(*c))
            .count();
        let skipped_separators = after_cursor
            .skip(skipped_word)
            .take_while(|c| is_word_separator(*c))
            .count();
        self.cursor
            .saturating_add(skipped_word)
            .saturating_add(skipped_separators)
    }
}

fn is_word_separator(c: char) -> bool {
    c == '/' || c == '\\' || c == ' ' || c == '.' || c == '-' || c == '_'
}

fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map_or(s.len(), |(i, _)| i)
}

fn expand_tilde(s: &str) -> Option<String> {
    if !s.starts_with('~') {
        return None;
    }
    #[cfg(unix)]
    {
        let home = std::env::var("HOME").ok()?;
        Some(s.replacen('~', &home, 1))
    }
    #[cfg(windows)]
    {
        let home = std::env::var("USERPROFILE").ok()?;
        Some(s.replacen('~', &home, 1))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Queued,
    Reading,
    Converting,
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
    pub converting: bool,
    pub written_bytes: u64,
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
    #[allow(dead_code)]
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
    pub spinner_tick: usize,
}

impl CopyState {
    #[allow(clippy::cast_precision_loss, clippy::as_conversions)]
    pub fn speed(&self) -> f64 {
        let window = std::time::Duration::from_secs(10);
        let now = Instant::now();
        let cutoff = now.checked_sub(window).unwrap_or(now);
        let total: u64 = self
            .speed_bytes
            .iter()
            .filter(|(t, _)| *t >= cutoff)
            .map(|(_, b)| b)
            .sum();
        if total == 0_u64 {
            return 0.0_f64;
        }
        total as f64 / window.as_secs_f64()
    }

    pub fn spinner_char(&self) -> char {
        let len = SPINNER_CHARS.len();
        let idx = if len == 0_usize {
            0_usize
        } else {
            self.spinner_tick.checked_rem(len).unwrap_or(0_usize)
        };
        SPINNER_CHARS.get(idx).copied().unwrap_or(' ')
    }

    pub fn upcoming(&self) -> impl Iterator<Item = &FileItem> {
        self.files
            .iter()
            .skip(self.current_index.saturating_add(1))
            .take(MAX_UPCOMING)
            .rev()
    }

    pub fn history(&self) -> impl Iterator<Item = &FileItem> {
        let start = self.current_index.saturating_sub(MAX_HISTORY);
        self.files
            .iter()
            .skip(start)
            .take(self.current_index.saturating_sub(start))
            .rev()
    }

    pub fn current(&self) -> Option<&FileItem> {
        self.files.get(self.current_index)
    }

    #[allow(clippy::cast_precision_loss, clippy::as_conversions)]
    pub fn overall_progress(&self) -> f64 {
        if self.total_bytes == 0_u64 {
            return 0.0_f64;
        }
        self.copied_bytes as f64 / self.total_bytes as f64
    }

    #[allow(clippy::cast_precision_loss, clippy::as_conversions)]
    pub fn current_progress(&self) -> f64 {
        if self.current_file_size == 0_u64 {
            return 0.0_f64;
        }
        self.current_file_copied as f64 / self.current_file_size as f64
    }

    #[allow(clippy::cast_precision_loss, clippy::as_conversions)]
    pub fn eta_secs(&self) -> Option<f64> {
        let speed = self.speed();
        if speed <= 0.0_f64 {
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
    pub locale: &'static Locale,
    pub ctrl_c_at: Option<Instant>,
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

const SPINNER_CHARS: &[char] = &[
    '\u{280B}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283C}', '\u{2834}', '\u{2826}', '\u{2827}',
    '\u{2807}', '\u{280F}',
];

impl Model {
    pub fn new_tui() -> Self {
        Self {
            phase: Phase::Setup(SetupForm::default()),
            terminal_size: (80, 24),
            should_quit: false,
            shutdown: Arc::new(AtomicBool::new(false)),
            locale: i18n::detect(),
            ctrl_c_at: None,
        }
    }

    pub fn new_cli(config: Config, locale: &'static Locale) -> Self {
        Self {
            phase: Phase::Scanning {
                config,
                state: ScanState {
                    files_found: 0_usize,
                    files_matched: 0_usize,
                    last_path: None,
                    spinner_tick: 0_usize,
                },
            },
            terminal_size: (80, 24),
            should_quit: false,
            shutdown: Arc::new(AtomicBool::new(false)),
            locale,
            ctrl_c_at: None,
        }
    }

    pub fn spinner_char(&self) -> char {
        if let Phase::Scanning { state, .. } = &self.phase {
            let len = SPINNER_CHARS.len();
            let idx = if len == 0_usize {
                0_usize
            } else {
                state.spinner_tick.checked_rem(len).unwrap_or(0_usize)
            };
            SPINNER_CHARS.get(idx).copied().unwrap_or(' ')
        } else {
            ' '
        }
    }
}

#[allow(clippy::too_many_lines)]
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

            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                if let Phase::Scanning { config, .. } = &model.phase {
                    model.shutdown.store(true, Ordering::Release);
                    let form = SetupForm::from_config(config, model.locale);
                    model.phase = Phase::Setup(form);
                    model.shutdown = Arc::new(AtomicBool::new(false));
                    model.ctrl_c_at = None;
                    return Effect::None;
                }
                if let Phase::Copying(_) = &model.phase {
                    let now = Instant::now();
                    if model
                        .ctrl_c_at
                        .is_some_and(|t| now.duration_since(t).as_millis() < 1000_u128)
                    {
                        model.shutdown.store(true, Ordering::Release);
                        model.should_quit = true;
                        return Effect::Quit;
                    }
                    model.ctrl_c_at = Some(now);
                    return Effect::None;
                }
                model.shutdown.store(true, Ordering::Release);
                model.should_quit = true;
                return Effect::Quit;
            }

            let locale = model.locale;
            match &mut model.phase {
                Phase::Setup(form) => {
                    let effect = update_setup(form, key, locale);
                    if let Effect::StartScan(ref config) = effect {
                        model.phase = Phase::Scanning {
                            config: config.clone(),
                            state: ScanState {
                                files_found: 0_usize,
                                files_matched: 0_usize,
                                last_path: None,
                                spinner_tick: 0_usize,
                            },
                        };
                    }
                    effect
                }
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
                cs.spinner_tick = cs.spinner_tick.wrapping_add(1);
                let twelve_secs = std::time::Duration::from_secs(12);
                let cutoff = Instant::now()
                    .checked_sub(twelve_secs)
                    .unwrap_or(Instant::now());
                cs.speed_bytes.retain(|(t, _)| *t >= cutoff);
            }
            Effect::None
        }

        Msg::Scan(scan_msg) => handle_scan(model, scan_msg),

        Msg::Copy(copy_msg) => handle_copy(model, copy_msg),
    }
}

fn handle_scan(model: &mut Model, scan_msg: ScanMsg) -> Effect {
    match scan_msg {
        ScanMsg::FileFound { path, matched } => {
            if let Phase::Scanning { state, .. } = &mut model.phase {
                state.files_found = state.files_found.saturating_add(1);
                if matched {
                    state.files_matched = state.files_matched.saturating_add(1);
                }
                state.last_path = Some(path);
            }
            Effect::None
        }
        ScanMsg::Complete(files) => {
            let config = match &model.phase {
                Phase::Scanning { config, .. } => config.clone(),
                _ => return Effect::None,
            };

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
                    converting: false,
                    written_bytes: 0_u64,
                })
                .collect();

            if total_files == 0_usize {
                model.phase = Phase::Done {
                    total_files: 0_usize,
                    total_bytes: 0_u64,
                    errors: vec![],
                    elapsed: std::time::Duration::ZERO,
                };
                return Effect::None;
            }

            let copy_state = CopyState {
                config: config.clone(),
                files: items,
                current_index: 0_usize,
                total_bytes,
                total_files,
                copied_bytes: 0_u64,
                current_file_copied: 0_u64,
                current_file_size: 0_u64,
                started_at: Instant::now(),
                speed_bytes: Vec::new(),
                errors: Vec::new(),
                spinner_tick: 0_usize,
            };

            model.phase = Phase::Copying(copy_state);
            Effect::StartCopy { files, config }
        }
        ScanMsg::Error(e) => {
            model.phase = Phase::FatalError(e);
            Effect::None
        }
    }
}

fn handle_copy(model: &mut Model, copy_msg: CopyMsg) -> Effect {
    match copy_msg {
        CopyMsg::FileStart {
            index,
            name,
            original_path,
            size,
        } => {
            if let Phase::Copying(cs) = &mut model.phase {
                cs.current_index = index;
                cs.current_file_copied = 0_u64;
                cs.current_file_size = size.as_u64();
                if let Some(item) = cs.files.get_mut(index) {
                    item.status = FileStatus::Copying;
                    item.name.clone_from(&name);
                    item.original_path.clone_from(&original_path);
                }
            }
            Effect::None
        }
        CopyMsg::Progress { bytes_written } => {
            if let Phase::Copying(cs) = &mut model.phase {
                cs.current_file_copied = cs.current_file_copied.saturating_add(bytes_written);
                cs.copied_bytes = cs.copied_bytes.saturating_add(bytes_written);
                cs.speed_bytes.push((Instant::now(), bytes_written));
            }
            Effect::None
        }
        CopyMsg::FileDone { index } => {
            if let Phase::Copying(cs) = &mut model.phase
                && let Some(item) = cs.files.get_mut(index)
            {
                item.status = FileStatus::Done;
                item.written_bytes = cs.current_file_copied;
            }
            Effect::None
        }
        CopyMsg::Error {
            index,
            path,
            error,
            is_destination,
        } => {
            if let Phase::Copying(cs) = &mut model.phase {
                let msg = format!("{}: {error}", path.display());
                cs.errors.push(msg);
                if let Some(item) = cs.files.get_mut(index) {
                    item.status = FileStatus::Failed;
                }
            }
            if is_destination {
                model.phase =
                    Phase::FatalError(format!("Write error on {}: {error}", path.display()));
            }
            Effect::None
        }
        CopyMsg::Complete => {
            let (total_files, copied_bytes, errors, elapsed, destination) = match &model.phase {
                Phase::Copying(cs) => (
                    cs.total_files,
                    cs.copied_bytes,
                    cs.errors.clone(),
                    cs.started_at.elapsed(),
                    cs.config.destination.clone(),
                ),
                _ => return Effect::None,
            };
            if !errors.is_empty() {
                let log_path = destination.join("mixr-errors.log");
                let _ = std::fs::write(&log_path, errors.join("\n"));
            }
            model.phase = Phase::Done {
                total_files,
                total_bytes: copied_bytes,
                errors,
                elapsed,
            };
            Effect::None
        }
        CopyMsg::Preparing { index, converting } => {
            if let Phase::Copying(cs) = &mut model.phase
                && let Some(file) = cs.files.get_mut(index)
            {
                file.converting = converting;
                file.status = if converting {
                    FileStatus::Converting
                } else {
                    FileStatus::Reading
                };
            }
            Effect::None
        }
        CopyMsg::Aborted => {
            model.should_quit = true;
            Effect::Quit
        }
    }
}

pub fn expand_path(value: &str) -> String {
    expand_tilde(value).unwrap_or_else(|| value.to_string())
}

pub fn field_is_invalid(field: SetupField, value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    match field {
        SetupField::MinDuration => parse_duration(value).is_err(),
        SetupField::Size | SetupField::MinSize => ByteSize::parse(value).is_err(),
        SetupField::Source => {
            let resolved = resolve_path_value(value);
            let path = std::path::Path::new(&resolved);
            !path.is_dir() || std::fs::read_dir(path).is_err()
        }
        SetupField::Destination => {
            let resolved = resolve_path_value(value);
            !is_writable_or_creatable(&resolved)
        }
        _ => false,
    }
}

fn is_writable_or_creatable(path: &str) -> bool {
    std::path::Path::new(path)
        .ancestors()
        .find(|a| a.is_dir())
        .is_some_and(is_dir_writable)
}

fn is_dir_writable(path: &std::path::Path) -> bool {
    let test_path = path.join(".mixr_write_test");
    match std::fs::File::create(&test_path) {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_path);
            true
        }
        Err(_) => false,
    }
}

pub fn dest_existing_prefix_len(value: &str) -> usize {
    if value.is_empty() {
        return 0_usize;
    }
    let resolved = resolve_path_value(value);
    let path = std::path::Path::new(&resolved);
    if path.is_dir() {
        return value.len();
    }
    path.ancestors()
        .skip(1)
        .find(|a| a.is_dir())
        .and_then(|parent| parent.to_str())
        .map_or(0_usize, |parent_str| {
            let offset = resolved.len().saturating_sub(value.len());
            parent_str.len().saturating_sub(offset)
        })
}

fn navigate_to_volumes(form: &mut SetupForm) {
    #[cfg(target_os = "macos")]
    let base = Some("/Volumes/".to_string());
    #[cfg(target_os = "linux")]
    let base = std::env::var("USER")
        .ok()
        .and_then(|user| {
            [format!("/media/{user}/"), format!("/run/media/{user}/")]
                .into_iter()
                .find(|p| std::path::Path::new(p).is_dir())
        })
        .or_else(|| {
            std::path::Path::new("/mnt/")
                .is_dir()
                .then(|| "/mnt/".to_string())
        });
    #[cfg(windows)]
    let base: Option<String> = {
        #[allow(clippy::as_conversions)]
        let drives: Vec<String> = (b'A'..=b'Z')
            .filter(|&letter| std::path::Path::new(&format!("{}:\\", letter as char)).exists())
            .map(|letter| format!("{}:{}", letter as char, std::path::MAIN_SEPARATOR))
            .collect();
        if !drives.is_empty() {
            match form.focused {
                SetupField::Source => form.source.clear(),
                SetupField::Destination => form.destination.clear(),
                _ => {}
            }
            form.cursor = 0_usize;
            form.dropdown.entries = drives;
            form.dropdown.visible = true;
            form.dropdown.selected = 0_usize;
            form.dropdown.scroll_offset = 0_usize;
            return;
        }
        None
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    let base: Option<String> = None;

    if let Some(path) = base {
        match form.focused {
            SetupField::Source => form.source.clone_from(&path),
            SetupField::Destination => form.destination.clone_from(&path),
            _ => {}
        }
        form.cursor = path.chars().count();
        refresh_dropdown(form);
    }
}

#[cfg(not(windows))]
fn resolve_path_value(raw: &str) -> String {
    if raw.is_empty() {
        let mut h = expand_path("~");
        if !h.ends_with('/') {
            h.push('/');
        }
        h
    } else if raw.starts_with('/') || raw.starts_with('~') {
        expand_path(raw)
    } else {
        let mut h = expand_path("~");
        if !h.ends_with('/') {
            h.push('/');
        }
        format!("{h}{raw}")
    }
}

#[cfg(windows)]
fn resolve_path_value(raw: &str) -> String {
    if raw.is_empty() {
        let mut h = expand_path("~");
        if !h.ends_with('/') && !h.ends_with('\\') {
            h.push('\\');
        }
        h
    } else if raw.starts_with('~') {
        expand_path(raw)
    } else if is_windows_absolute(raw) {
        raw.to_string()
    } else {
        let mut h = expand_path("~");
        if !h.ends_with('/') && !h.ends_with('\\') {
            h.push('\\');
        }
        format!("{h}{raw}")
    }
}

#[cfg(windows)]
fn is_windows_absolute(path: &str) -> bool {
    matches!(path.as_bytes(), [letter, b':', ..] if letter.is_ascii_alphabetic())
}

fn refresh_dropdown(form: &mut SetupForm) {
    if !form.focused.is_path() {
        form.dropdown.visible = false;
        return;
    }

    let raw_value = match form.focused {
        SetupField::Source => form.source.clone(),
        SetupField::Destination => form.destination.clone(),
        _ => {
            form.dropdown.visible = false;
            return;
        }
    };

    let value = resolve_path_value(&raw_value);

    let (parent, prefix) = if value.ends_with('/') || value.ends_with(std::path::MAIN_SEPARATOR) {
        (value.as_str(), "")
    } else {
        let path = std::path::Path::new(value.as_str());
        if let (Some(parent), Some(name)) = (
            path.parent().and_then(|p| p.to_str()),
            path.file_name().and_then(|n| n.to_str()),
        ) {
            let p = if parent.is_empty() { "/" } else { parent };
            (p, name)
        } else {
            form.dropdown = Dropdown::default();
            return;
        }
    };

    let prefix_lower = prefix.to_lowercase();

    let mut entries: Vec<String> = std::fs::read_dir(parent)
        .into_iter()
        .flatten()
        .filter_map(std::result::Result::ok)
        .filter_map(|e| {
            let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
            if !is_dir {
                return None;
            }
            let name = e.file_name().to_str()?.to_string();
            if !name.to_lowercase().starts_with(&prefix_lower) {
                return None;
            }
            Some(format!("{name}{}", std::path::MAIN_SEPARATOR))
        })
        .collect();

    entries.sort_by_key(|a| a.to_lowercase());

    form.dropdown.visible = !entries.is_empty();
    form.dropdown.entries = entries;
    form.dropdown.selected = 0_usize;
    form.dropdown.scroll_offset = 0_usize;
}

fn apply_autocomplete(form: &mut SetupForm) {
    if !form.dropdown.visible || form.dropdown.entries.is_empty() {
        return;
    }

    let selected = form.dropdown.entries.get(form.dropdown.selected).cloned();

    let Some(entry) = selected else { return };

    let raw_value = match form.focused {
        SetupField::Source => form.source.clone(),
        SetupField::Destination => form.destination.clone(),
        _ => return,
    };

    let new_value = if raw_value.is_empty() {
        entry
    } else {
        let value = resolve_path_value(&raw_value);
        let parent = if value.ends_with('/') || value.ends_with(std::path::MAIN_SEPARATOR) {
            value
        } else {
            let path = std::path::Path::new(value.as_str());
            match path.parent().and_then(|p| p.to_str()) {
                Some("") | None => {
                    #[cfg(windows)]
                    {
                        String::new()
                    }
                    #[cfg(not(windows))]
                    {
                        "/".to_string()
                    }
                }
                Some(p) => {
                    let mut s = p.to_string();
                    if !s.ends_with('/') && !s.ends_with(std::path::MAIN_SEPARATOR) {
                        s.push(std::path::MAIN_SEPARATOR);
                    }
                    s
                }
            }
        };
        format!("{parent}{entry}")
    };
    let new_cursor = new_value.chars().count();

    match form.focused {
        SetupField::Source => form.source = new_value,
        SetupField::Destination => form.destination = new_value,
        _ => {}
    }

    form.cursor = new_cursor;
    form.dropdown.visible = false;
    refresh_dropdown(form);
}

#[allow(clippy::too_many_lines)]
fn update_setup(
    form: &mut SetupForm,
    key: crossterm::event::KeyEvent,
    locale: &'static Locale,
) -> Effect {
    use crossterm::event::{KeyCode, KeyModifiers};

    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Up => {
            if form.dropdown.visible {
                form.dropdown.selected = form.dropdown.selected.saturating_sub(1);
                if form.dropdown.selected < form.dropdown.scroll_offset {
                    form.dropdown.scroll_offset = form.dropdown.selected;
                }
            } else {
                form.focused = form.focused.prev(form.encoding);
                form.error = None;
                form.dropdown.visible = false;
                form.sync_cursor();
            }
            Effect::None
        }
        KeyCode::Down => {
            if form.dropdown.visible {
                let max = form.dropdown.entries.len().saturating_sub(1);
                form.dropdown.selected = (form.dropdown.selected.saturating_add(1)).min(max);
                if form.dropdown.selected
                    >= form.dropdown.scroll_offset.saturating_add(MAX_DROPDOWN)
                {
                    form.dropdown.scroll_offset = form
                        .dropdown
                        .selected
                        .saturating_sub(MAX_DROPDOWN.saturating_sub(1));
                }
            } else {
                form.focused = form.focused.next(form.encoding);
                form.error = None;
                form.dropdown.visible = false;
                form.sync_cursor();
            }
            Effect::None
        }
        KeyCode::Left if alt && form.focused.is_text() => {
            form.cursor = form.word_boundary_left();
            Effect::None
        }
        KeyCode::Right if alt && form.focused.is_text() => {
            form.cursor = form.word_boundary_right();
            Effect::None
        }
        KeyCode::Left if form.focused.is_text() => {
            form.cursor = form.cursor.saturating_sub(1);
            Effect::None
        }
        KeyCode::Right if form.focused.is_text() => {
            let len = form.focused_value().map_or(0_usize, |v| v.chars().count());
            form.cursor = (form.cursor.saturating_add(1)).min(len);
            Effect::None
        }
        KeyCode::Char('a') if ctrl && form.focused.is_text() => {
            form.cursor = 0_usize;
            Effect::None
        }
        KeyCode::Char('e') if ctrl && form.focused.is_text() => {
            let len = form.focused_value().map_or(0_usize, |v| v.chars().count());
            form.cursor = len;
            Effect::None
        }
        KeyCode::Char('w') if ctrl && form.focused.is_text() => {
            form.delete_word_before_cursor();
            if form.focused.is_path() {
                refresh_dropdown(form);
            }
            Effect::None
        }
        KeyCode::Char('d') if ctrl && form.focused.is_path() => {
            navigate_to_volumes(form);
            Effect::None
        }
        KeyCode::Tab => {
            if form.focused.is_path() {
                if form.dropdown.visible {
                    apply_autocomplete(form);
                } else {
                    refresh_dropdown(form);
                }
            }
            Effect::None
        }
        KeyCode::Esc => {
            form.dropdown.visible = false;
            Effect::None
        }
        KeyCode::Enter => {
            if form.focused == SetupField::Start {
                validate_and_start(form, locale)
            } else if form.dropdown.visible {
                apply_autocomplete(form);
                form.dropdown.visible = false;
                Effect::None
            } else {
                form.focused = form.focused.next(form.encoding);
                form.sync_cursor();
                Effect::None
            }
        }
        KeyCode::Left if form.focused == SetupField::Encoding => {
            form.encoding = match form.encoding {
                Encoding::Keep => Encoding::Vbr,
                Encoding::Cbr => Encoding::Keep,
                Encoding::Vbr => Encoding::Cbr,
            };
            Effect::None
        }
        KeyCode::Right | KeyCode::Char(' ') if form.focused == SetupField::Encoding => {
            form.encoding = match form.encoding {
                Encoding::Keep => Encoding::Cbr,
                Encoding::Cbr => Encoding::Vbr,
                Encoding::Vbr => Encoding::Keep,
            };
            Effect::None
        }
        KeyCode::Left if form.focused == SetupField::Bitrate => {
            match form.encoding {
                Encoding::Cbr => form.cbr_bitrate = form.cbr_bitrate.prev(),
                Encoding::Vbr => form.vbr_quality = form.vbr_quality.prev(),
                Encoding::Keep => {}
            }
            Effect::None
        }
        KeyCode::Right if form.focused == SetupField::Bitrate => {
            match form.encoding {
                Encoding::Cbr => form.cbr_bitrate = form.cbr_bitrate.next(),
                Encoding::Vbr => form.vbr_quality = form.vbr_quality.next(),
                Encoding::Keep => {}
            }
            Effect::None
        }
        KeyCode::Char(' ') if form.focused.is_checkbox() => {
            match form.focused {
                SetupField::NoLive => form.no_live = !form.no_live,
                SetupField::KeepNames => form.keep_names = !form.keep_names,
                SetupField::Overwrite => form.overwrite = !form.overwrite,
                _ => {}
            }
            Effect::None
        }
        KeyCode::Char(c) if form.focused.is_text() => {
            form.insert_char(c);
            if form.focused.is_path() {
                refresh_dropdown(form);
            }
            Effect::None
        }
        KeyCode::Backspace if alt && form.focused.is_text() => {
            form.delete_word_before_cursor();
            if form.focused.is_path() {
                refresh_dropdown(form);
            }
            Effect::None
        }
        KeyCode::Backspace if form.focused.is_text() => {
            form.delete_char_before_cursor();
            if form.focused.is_path() {
                refresh_dropdown(form);
            }
            Effect::None
        }
        _ => Effect::None,
    }
}

fn validate_and_start(form: &mut SetupForm, locale: &Locale) -> Effect {
    if form.source.is_empty() {
        form.error = Some(locale.err_source_required.to_string());
        form.focused = SetupField::Source;
        return Effect::None;
    }
    if form.destination.is_empty() {
        form.error = Some(locale.err_dest_required.to_string());
        form.focused = SetupField::Destination;
        return Effect::None;
    }

    let source = PathBuf::from(resolve_path_value(&form.source));
    if !source.is_dir() {
        form.error = Some(format!(
            "{}: {}",
            locale.err_source_not_dir,
            source.display()
        ));
        form.focused = SetupField::Source;
        return Effect::None;
    }

    let max_size = if form.size.is_empty() {
        None
    } else {
        match ByteSize::parse(&form.size) {
            Ok(s) => Some(s),
            Err(e) => {
                form.error = Some(format!("{}: {e}", locale.err_invalid_size));
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
                form.error = Some(format!("{}: {e}", locale.err_invalid_min_size));
                form.focused = SetupField::MinSize;
                return Effect::None;
            }
        }
    };

    let min_duration = if form.min_duration.is_empty() {
        None
    } else if let Ok(d) = parse_duration(&form.min_duration) {
        Some(d)
    } else {
        form.error = Some(locale.err_invalid_duration.to_string());
        form.focused = SetupField::MinDuration;
        return Effect::None;
    };

    let include = if form.extensions.is_empty() {
        None
    } else {
        Some(parse_ext_list(&form.extensions))
    };
    let exclude = if form.exclude.is_empty() {
        None
    } else {
        Some(parse_ext_list(&form.exclude))
    };

    let allowed_extensions = crate::filters::resolve_extensions(
        include.as_ref(),
        exclude.as_ref(),
        crate::types::DEFAULT_EXTENSIONS,
    );

    let (cbr_bitrate, vbr_quality) = match form.encoding {
        Encoding::Keep => (None, None),
        Encoding::Cbr => (Some(form.cbr_bitrate), None),
        Encoding::Vbr => (None, Some(form.vbr_quality)),
    };

    let config = Config {
        source,
        destination: PathBuf::from(resolve_path_value(&form.destination)),
        max_size,
        min_file_size,
        min_duration,
        no_live: form.no_live,
        keep_names: form.keep_names,
        overwrite: form.overwrite,
        allowed_extensions,
        encoding: form.encoding,
        cbr_bitrate,
        vbr_quality,
    };

    Effect::StartScan(config)
}

pub fn parse_ext_list(s: &str) -> Vec<String> {
    s.split([',', ' '])
        .map(|p| {
            p.trim()
                .to_lowercase()
                .trim_start_matches('*')
                .trim_start_matches('.')
                .to_string()
        })
        .filter(|p| !p.is_empty())
        .collect()
}

pub fn format_ext_list(s: &str) -> String {
    let parsed = parse_ext_list(s);
    if parsed.is_empty() {
        return String::new();
    }
    parsed
        .iter()
        .map(|e| format!(".{e}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use rstest::rstest;

    fn key(code: KeyCode) -> Msg {
        Msg::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        })
    }

    #[test]
    fn setup_arrow_navigation() {
        let mut model = Model::new_tui();
        update(&mut model, key(KeyCode::Down));
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
            min_duration: None,
            no_live: false,
            keep_names: false,
            overwrite: false,
            allowed_extensions: vec![],
            encoding: Encoding::Keep,
            cbr_bitrate: None,
            vbr_quality: None,
        };
        let mut model = Model::new_cli(config, &i18n::EN);

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
            min_duration: None,
            no_live: false,
            keep_names: false,
            overwrite: false,
            allowed_extensions: vec![],
            encoding: Encoding::Keep,
            cbr_bitrate: None,
            vbr_quality: None,
        };
        let mut model = Model::new_cli(config, &i18n::EN);

        let files = vec![FileEntry {
            path: PathBuf::from("/src/a.mp3"),
            size: ByteSize(1000),
            duration: None,
            bitrate_kbps: None,
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
            min_duration: None,
            no_live: false,
            keep_names: false,
            overwrite: false,
            allowed_extensions: vec![],
            encoding: Encoding::Keep,
            cbr_bitrate: None,
            vbr_quality: None,
        };
        let mut model = Model::new_cli(config, &i18n::EN);
        let files = vec![FileEntry {
            path: PathBuf::from("/src/a.mp3"),
            size: ByteSize(1000),
            duration: None,
            bitrate_kbps: None,
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
            min_duration: None,
            no_live: false,
            keep_names: false,
            overwrite: false,
            allowed_extensions: vec![],
            encoding: Encoding::Keep,
            cbr_bitrate: None,
            vbr_quality: None,
        };
        let mut model = Model::new_cli(config, &i18n::EN);
        let files = vec![FileEntry {
            path: PathBuf::from("/src/a.mp3"),
            size: ByteSize(1000),
            duration: None,
            bitrate_kbps: None,
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
            min_duration: None,
            no_live: false,
            keep_names: false,
            overwrite: false,
            allowed_extensions: vec![],
            encoding: Encoding::Keep,
            cbr_bitrate: None,
            vbr_quality: None,
        };
        let mut model = Model::new_cli(config, &i18n::EN);
        let files = vec![FileEntry {
            path: PathBuf::from("/src/a.mp3"),
            size: ByteSize(1000),
            duration: None,
            bitrate_kbps: None,
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
        assert!(model.shutdown.load(Ordering::Acquire));
    }

    #[rstest]
    #[case(Encoding::Keep, SetupField::NoLive)]
    #[case(Encoding::Cbr, SetupField::Bitrate)]
    #[case(Encoding::Vbr, SetupField::Bitrate)]
    fn setup_field_next_skips_bitrate_when_keep(
        #[case] encoding: Encoding,
        #[case] expected: SetupField,
    ) {
        assert_eq!(SetupField::Encoding.next(encoding), expected);
    }

    #[rstest]
    #[case(Encoding::Keep, SetupField::Encoding)]
    #[case(Encoding::Cbr, SetupField::Bitrate)]
    #[case(Encoding::Vbr, SetupField::Bitrate)]
    fn setup_field_prev_skips_bitrate_when_keep(
        #[case] encoding: Encoding,
        #[case] expected: SetupField,
    ) {
        assert_eq!(SetupField::NoLive.prev(encoding), expected);
    }

    #[rstest]
    #[case("", false)]
    #[case("30s", false)]
    #[case("2m", false)]
    #[case("2:30", false)]
    #[case("abc", true)]
    #[case("0", true)]
    fn field_is_invalid_min_duration(#[case] input: &str, #[case] invalid: bool) {
        assert_eq!(field_is_invalid(SetupField::MinDuration, input), invalid);
    }

    #[test]
    fn copy_preparing_sets_status() {
        let config = Config {
            source: PathBuf::from("/src"),
            destination: PathBuf::from("/dst"),
            max_size: None,
            min_file_size: None,
            min_duration: None,
            no_live: false,
            keep_names: false,
            overwrite: false,
            allowed_extensions: vec![],
            encoding: Encoding::Keep,
            cbr_bitrate: None,
            vbr_quality: None,
        };

        let locale = &crate::i18n::EN;
        let mut model = Model::new_cli(config.clone(), locale);

        let files = vec![FileEntry {
            path: PathBuf::from("/src/a.mp3"),
            size: ByteSize(1000),
            duration: None,
            bitrate_kbps: None,
        }];
        let items: Vec<FileItem> = files
            .iter()
            .map(|f| FileItem {
                name: "a.mp3".to_string(),
                original_path: f.path.clone(),
                size: f.size,
                status: FileStatus::Queued,
                converting: false,
                written_bytes: 0_u64,
            })
            .collect();
        model.phase = Phase::Copying(CopyState {
            config,
            files: items,
            current_index: 0_usize,
            total_bytes: 1000_u64,
            total_files: 1_usize,
            copied_bytes: 0_u64,
            current_file_copied: 0_u64,
            current_file_size: 1000_u64,
            started_at: std::time::Instant::now(),
            speed_bytes: vec![],
            errors: vec![],
            spinner_tick: 0_usize,
        });

        let effect = update(
            &mut model,
            Msg::Copy(CopyMsg::Preparing {
                index: 0_usize,
                converting: true,
            }),
        );
        assert!(matches!(effect, Effect::None));
        if let Phase::Copying(cs) = &model.phase {
            assert!(matches!(
                cs.files.first().map(|f| f.status),
                Some(FileStatus::Converting)
            ));
            assert!(cs.files.first().map_or(false, |f| f.converting));
        } else {
            panic!("Expected Copying phase");
        }
    }
}
