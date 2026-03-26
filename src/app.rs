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
        if recent.len() < 2 {
            return 0.0;
        }
        let total: u64 = recent.iter().map(|(_, b)| b).sum();
        let elapsed = match (recent.last(), recent.first()) {
            (Some(last), Some(first)) => last.0.duration_since(first.0),
            _ => return 0.0,
        };
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
            if let Phase::Copying(cs) = &mut model.phase {
                if let Some(item) = cs.files.get_mut(index) {
                    item.status = FileStatus::Done;
                }
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
