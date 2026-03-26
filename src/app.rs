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

    pub fn is_path(self) -> bool {
        matches!(self, Self::Source | Self::Destination)
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
    pub extensions: String,
    pub exclude: String,
    pub no_live: bool,
    pub keep_names: bool,
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
            extensions: String::new(),
            exclude: String::new(),
            no_live: false,
            keep_names: false,
            focused: SetupField::Source,
            error: None,
            dropdown: Dropdown::default(),
            cursor: 0,
        }
    }
}

impl SetupForm {
    pub fn focused_value(&self) -> Option<&str> {
        match self.focused {
            SetupField::Source => Some(&self.source),
            SetupField::Destination => Some(&self.destination),
            SetupField::Size => Some(&self.size),
            SetupField::MinSize => Some(&self.min_size),
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
            SetupField::Extensions => Some(&mut self.extensions),
            SetupField::Exclude => Some(&mut self.exclude),
            _ => None,
        }
    }

    pub fn sync_cursor(&mut self) {
        let len = self.focused_value().map(|v| v.len()).unwrap_or(0);
        self.cursor = self.cursor.min(len);
    }

    fn insert_char(&mut self, c: char) {
        let cursor = self.cursor;
        if let Some(val) = self.focused_value_mut() {
            let byte_idx = char_to_byte_idx(val, cursor);
            val.insert(byte_idx, c);
        }
        self.cursor = cursor + 1;
    }

    fn delete_char_before_cursor(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let cursor = self.cursor;
        if let Some(val) = self.focused_value_mut() {
            let byte_idx = char_to_byte_idx(val, cursor);
            let prev = char_to_byte_idx(val, cursor - 1);
            val.drain(prev..byte_idx);
        }
        self.cursor = cursor - 1;
    }

    fn delete_word_before_cursor(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let new_cursor = self.word_boundary_left();
        let byte_start = self
            .focused_value()
            .map(|v| char_to_byte_idx(v, new_cursor))
            .unwrap_or(0);
        let byte_end = self
            .focused_value()
            .map(|v| char_to_byte_idx(v, self.cursor))
            .unwrap_or(0);
        if let Some(v) = self.focused_value_mut() {
            v.drain(byte_start..byte_end);
        }
        self.cursor = new_cursor;
    }

    fn word_boundary_left(&self) -> usize {
        let val = match self.focused_value() {
            Some(v) => v,
            None => return 0,
        };
        let chars: Vec<char> = val.chars().collect();
        if self.cursor == 0 {
            return 0;
        }
        let mut pos = self.cursor;
        while pos > 0 && is_word_separator(chars[pos - 1]) {
            pos -= 1;
        }
        while pos > 0 && !is_word_separator(chars[pos - 1]) {
            pos -= 1;
        }
        pos
    }

    fn word_boundary_right(&self) -> usize {
        let val = match self.focused_value() {
            Some(v) => v,
            None => return 0,
        };
        let chars: Vec<char> = val.chars().collect();
        let len = chars.len();
        if self.cursor >= len {
            return len;
        }
        let mut pos = self.cursor;
        while pos < len && !is_word_separator(chars[pos]) {
            pos += 1;
        }
        while pos < len && is_word_separator(chars[pos]) {
            pos += 1;
        }
        pos
    }
}

fn is_word_separator(c: char) -> bool {
    c == '/' || c == ' ' || c == '.' || c == '-' || c == '_'
}

fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
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
}

impl CopyState {
    pub fn speed(&self) -> f64 {
        let window = std::time::Duration::from_secs(3);
        let now = Instant::now();
        let cutoff = now - window;
        let total: u64 = self
            .speed_bytes
            .iter()
            .filter(|(t, _)| *t >= cutoff)
            .map(|(_, b)| b)
            .sum();
        if total == 0 {
            return 0.0;
        }
        total as f64 / window.as_secs_f64()
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

        Msg::Scan(scan_msg) => handle_scan(model, scan_msg),

        Msg::Copy(copy_msg) => handle_copy(model, copy_msg),
    }
}

fn handle_scan(model: &mut Model, scan_msg: ScanMsg) -> Effect {
    match scan_msg {
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
                })
                .collect();

            if total_files == 0 {
                model.phase = Phase::Done {
                    total_files: 0,
                    total_bytes: 0,
                    errors: vec![],
                    elapsed: std::time::Duration::ZERO,
                };
                return Effect::None;
            }

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
            if let Phase::Copying(cs) = &mut model.phase
                && let Some(item) = cs.files.get_mut(index)
            {
                item.status = FileStatus::Done;
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
                model.phase = Phase::FatalError(format!(
                    "Write error on {}: {error}",
                    path.display()
                ));
            }
            Effect::None
        }
        CopyMsg::Complete => {
            let (total_files, copied_bytes, errors, elapsed) = match &model.phase {
                Phase::Copying(cs) => (
                    cs.total_files,
                    cs.copied_bytes,
                    cs.errors.clone(),
                    cs.started_at.elapsed(),
                ),
                _ => return Effect::None,
            };
            model.phase = Phase::Done {
                total_files,
                total_bytes: copied_bytes,
                errors,
                elapsed,
            };
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
        SetupField::Size | SetupField::MinSize => ByteSize::parse(value).is_err(),
        SetupField::Source | SetupField::Destination => {
            let expanded = expand_path(value);
            let path = std::path::Path::new(&expanded);
            !path.is_dir()
        }
        _ => false,
    }
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

    let value = if raw_value.is_empty() {
        let mut home = expand_path("~");
        if !home.ends_with('/') {
            home.push('/');
        }
        home
    } else {
        expand_path(&raw_value)
    };

    let (parent, prefix) = if value.ends_with('/') || value.ends_with(std::path::MAIN_SEPARATOR) {
        (value.as_str(), "")
    } else {
        let path = std::path::Path::new(value.as_str());
        match (
            path.parent().and_then(|p| p.to_str()),
            path.file_name().and_then(|n| n.to_str()),
        ) {
            (Some(parent), Some(name)) => {
                let p = if parent.is_empty() { "/" } else { parent };
                (p, name)
            }
            _ => {
                form.dropdown = Dropdown::default();
                return;
            }
        }
    };

    let prefix_lower = prefix.to_lowercase();

    let mut entries: Vec<String> = std::fs::read_dir(parent)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if !is_dir {
                return None;
            }
            let name = e.file_name().to_str()?.to_string();
            if !name.to_lowercase().starts_with(&prefix_lower) {
                return None;
            }
            Some(format!("{name}/"))
        })
        .collect();

    entries.sort_by_key(|a| a.to_lowercase());

    form.dropdown.visible = !entries.is_empty();
    form.dropdown.entries = entries;
    form.dropdown.selected = 0;
    form.dropdown.scroll_offset = 0;
}

fn apply_autocomplete(form: &mut SetupForm) {
    if !form.dropdown.visible || form.dropdown.entries.is_empty() {
        return;
    }

    let selected = form
        .dropdown
        .entries
        .get(form.dropdown.selected)
        .cloned();

    let Some(entry) = selected else { return };

    let raw_value = match form.focused {
        SetupField::Source => form.source.clone(),
        SetupField::Destination => form.destination.clone(),
        _ => return,
    };

    let value = if raw_value.is_empty() {
        let mut home = expand_path("~");
        if !home.ends_with('/') {
            home.push('/');
        }
        home
    } else {
        expand_path(&raw_value)
    };

    let parent = if value.ends_with('/') || value.ends_with(std::path::MAIN_SEPARATOR) {
        value.clone()
    } else {
        let path = std::path::Path::new(value.as_str());
        match path.parent().and_then(|p| p.to_str()) {
            Some("") => "/".to_string(),
            Some(p) => {
                let mut s = p.to_string();
                if !s.ends_with('/') {
                    s.push('/');
                }
                s
            }
            None => return,
        }
    };

    let new_value = format!("{parent}{entry}");
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

fn update_setup(
    form: &mut SetupForm,
    key: crossterm::event::KeyEvent,
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
                form.focused = form.focused.prev();
                form.error = None;
                form.dropdown.visible = false;
                form.sync_cursor();
            }
            Effect::None
        }
        KeyCode::Down => {
            if form.dropdown.visible {
                let max = form.dropdown.entries.len().saturating_sub(1);
                form.dropdown.selected = (form.dropdown.selected + 1).min(max);
                if form.dropdown.selected >= form.dropdown.scroll_offset + MAX_DROPDOWN {
                    form.dropdown.scroll_offset =
                        form.dropdown.selected.saturating_sub(MAX_DROPDOWN - 1);
                }
            } else {
                form.focused = form.focused.next();
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
            let len = form.focused_value().map(|v| v.chars().count()).unwrap_or(0);
            form.cursor = (form.cursor + 1).min(len);
            Effect::None
        }
        KeyCode::Char('a') if ctrl && form.focused.is_text() => {
            form.cursor = 0;
            Effect::None
        }
        KeyCode::Char('e') if ctrl && form.focused.is_text() => {
            let len = form.focused_value().map(|v| v.chars().count()).unwrap_or(0);
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
        KeyCode::Tab => {
            if form.focused.is_path() {
                if !form.dropdown.visible {
                    refresh_dropdown(form);
                } else {
                    apply_autocomplete(form);
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
                validate_and_start(form)
            } else {
                form.dropdown.visible = false;
                form.focused = form.focused.next();
                form.sync_cursor();
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
            form.insert_char(c);
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

fn validate_and_start(form: &mut SetupForm) -> Effect {
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

    let source = PathBuf::from(expand_path(&form.source));
    if !source.is_dir() {
        form.error = Some(format!("Source is not a directory: {}", source.display()));
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
        destination: PathBuf::from(expand_path(&form.destination)),
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
