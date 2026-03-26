use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Gauge, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{
    dest_existing_prefix_len, field_is_invalid, update, CopyState, Effect, FileStatus, Model, Msg,
    Phase, ScanState, SetupField, SetupForm,
};
use crate::copier;
use crate::filters::FilterSet;
use crate::scanner;
use crate::types::{format_duration, ByteSize, Config, Error};

const TICK_RATE: Duration = Duration::from_millis(50);

pub fn run() -> Result<(), Error> {
    let mut terminal = ratatui::init();
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

fn spawn_scanner(config: Config, tx: mpsc::Sender<Msg>, model: &Model) {
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
        let label_style = if focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        let invalid = field_is_invalid(*field, value);

        let dest_split = if *field == SetupField::Destination && !value.is_empty() {
            Some(dest_existing_prefix_len(value))
        } else {
            None
        };

        let mut spans = vec![Span::styled(format!("{label:<label_width$}"), label_style)];

        if focused {
            let chars: Vec<char> = value.chars().collect();
            let cursor_pos = form.cursor.min(chars.len());

            let style_for_char = |ci: usize| -> Style {
                if invalid {
                    Style::default().fg(Color::Red)
                } else if let Some(split) = dest_split {
                    let byte_pos = chars[..ci].iter().collect::<String>().len();
                    if byte_pos >= split {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default()
                    }
                } else {
                    Style::default()
                }
            };

            let mut run_start = 0;
            while run_start < chars.len() {
                if run_start == cursor_pos {
                    spans.push(Span::styled(
                        chars[cursor_pos].to_string(),
                        Style::default().bg(Color::White).fg(Color::Black),
                    ));
                    run_start += 1;
                    continue;
                }
                let style = style_for_char(run_start);
                let mut run_end = run_start + 1;
                while run_end < chars.len()
                    && run_end != cursor_pos
                    && style_for_char(run_end) == style
                {
                    run_end += 1;
                }
                let text: String = chars[run_start..run_end].iter().collect();
                spans.push(Span::styled(text, style));
                run_start = run_end;
            }
            if cursor_pos >= chars.len() {
                spans.push(Span::styled(
                    " ".to_string(),
                    Style::default().bg(Color::White).fg(Color::Black),
                ));
            }
        } else if let Some(split) = dest_split {
            if split >= value.len() {
                spans.push(Span::raw(value.to_string()));
            } else {
                spans.push(Span::raw(value[..split].to_string()));
                spans.push(Span::styled(
                    value[split..].to_string(),
                    Style::default().fg(Color::Yellow),
                ));
            }
        } else if invalid {
            spans.push(Span::styled(value.to_string(), Style::default().fg(Color::Red)));
        } else {
            spans.push(Span::raw(value.to_string()));
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), chunks[i]);
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
            "\u{2191}\u{2193}: navigate  Tab: complete  Enter: go  Ctrl+C: quit",
            Style::default().fg(Color::DarkGray),
        ))
    };
    frame.render_widget(Paragraph::new(help), chunks[11]);

    if form.dropdown.visible && !form.dropdown.entries.is_empty() && form.focused.is_path() {
        let field_row = match form.focused {
            SetupField::Source => 0,
            SetupField::Destination => 1,
            _ => 0,
        };
        let dropdown_y = chunks[field_row].y + 1;
        let dropdown_x = inner.x + label_width as u16;
        let dropdown_w = inner.width.saturating_sub(label_width as u16);
        let visible_count =
            (form.dropdown.entries.len() - form.dropdown.scroll_offset).min(crate::app::MAX_DROPDOWN);
        let dropdown_h = visible_count as u16 + 2;
        let dropdown_rect = Rect::new(dropdown_x, dropdown_y, dropdown_w, dropdown_h);

        let items: Vec<ListItem> = form
            .dropdown
            .entries
            .iter()
            .skip(form.dropdown.scroll_offset)
            .take(crate::app::MAX_DROPDOWN)
            .enumerate()
            .map(|(i, entry)| {
                let idx = i + form.dropdown.scroll_offset;
                let style = if idx == form.dropdown.selected {
                    Style::default().bg(Color::Yellow).fg(Color::Black)
                } else {
                    Style::default()
                };
                ListItem::new(Span::styled(entry.as_str(), style))
            })
            .collect();

        let list = List::new(items).block(Block::bordered());
        frame.render_widget(Clear, dropdown_rect);
        frame.render_widget(list, dropdown_rect);
    }
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

    let scanning = Line::from(format!("Scanning {}...", config.source.display()));
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
            FileStatus::Done => ("  ", Style::default().fg(Color::Green)),
            FileStatus::Failed => ("  ", Style::default().fg(Color::Red)),
            _ => ("  ", Style::default()),
        };
        let line = Line::from(Span::styled(format!("{prefix}{}", item.name), style));
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
