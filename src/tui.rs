use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Gauge, List, ListItem, Paragraph, Wrap};

use crate::app::{
    CopyState, Effect, FileStatus, MAX_HISTORY, MAX_UPCOMING, Model, Msg, Phase, ScanState,
    SetupField, SetupForm, dest_existing_prefix_len, field_is_invalid, format_ext_list, update,
};
use crate::copier;
use crate::filters::FilterSet;
use crate::i18n::Locale;
use crate::scanner;
use crate::types::{ByteSize, CbrBitrate, Config, Encoding, Error, VbrQuality, format_duration};

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
                    handle_effect(effect, &tx, &model);
                }
                Event::Resize(w, h) => {
                    update(&mut model, Msg::Resize(w, h));
                }
                _ => {}
            }
        }

        while let Ok(msg) = rx.try_recv() {
            let effect = update(&mut model, msg);
            handle_effect(effect, &tx, &model);
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

fn handle_effect(effect: Effect, tx: &mpsc::Sender<Msg>, model: &Model) {
    match effect {
        Effect::None | Effect::Quit => {}
        Effect::StartScan(config) => {
            spawn_scanner(&config, tx.clone(), model);
        }
        Effect::StartCopy { files, config } => {
            spawn_copier(files, &config, tx.clone(), model);
        }
    }
}

fn spawn_scanner(config: &Config, tx: mpsc::Sender<Msg>, model: &Model) {
    let shutdown = Arc::clone(&model.shutdown);
    let budget = match &config.max_size {
        Some(s) => s.as_u64(),
        None => fs4::available_space(&config.destination).unwrap_or(u64::MAX),
    };
    let filters = FilterSet::new(
        config.allowed_extensions.clone(),
        config.min_file_size,
        config.min_duration,
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
    config: &Config,
    tx: mpsc::Sender<Msg>,
    model: &Model,
) {
    let shutdown = Arc::clone(&model.shutdown);
    let config = config.clone();

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
        copier::copy_files(&files, &config, &copy_tx, &shutdown);
    });
}

fn view(model: &Model, frame: &mut Frame) {
    let area = frame.area();
    let locale = model.locale;

    match &model.phase {
        Phase::Setup(form) => view_setup(form, locale, frame, area),
        Phase::Scanning { config, state } => {
            view_scanning(config, state, model, locale, frame, area);
        }
        Phase::Copying(cs) => {
            let confirm_quit = model
                .ctrl_c_at
                .is_some_and(|t| t.elapsed().as_millis() < 1000_u128);
            view_copying(cs, locale, confirm_quit, frame, area);
        }
        Phase::Done {
            total_files,
            total_bytes,
            errors,
            elapsed,
        } => view_done(
            *total_files,
            *total_bytes,
            errors,
            *elapsed,
            locale,
            frame,
            area,
        ),
        Phase::FatalError(msg) => view_error(msg, locale, frame, area),
    }
}

#[allow(clippy::too_many_lines)]
fn view_setup(form: &SetupForm, locale: &Locale, frame: &mut Frame, area: Rect) {
    let block = Block::bordered().title(" mixr ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::vertical([
        Constraint::Length(1), // 0: Source
        Constraint::Length(1), // 1: Destination
        Constraint::Length(1), // 2: Size
        Constraint::Length(1), // 3: MinSize
        Constraint::Length(1), // 4: MinDuration
        Constraint::Length(1), // 5: Extensions
        Constraint::Length(1), // 6: Exclude
        Constraint::Length(1), // 7: Encoding
        Constraint::Length(1), // 8: Bitrate/Quality
        Constraint::Length(1), // 9: NoLive
        Constraint::Length(1), // 10: KeepNames
        Constraint::Length(1), // 11: Overwrite
        Constraint::Length(1), // 12: blank
        Constraint::Length(1), // 13: Start
        Constraint::Min(0),    // 14: spacer
        Constraint::Length(1), // 15: help
    ])
    .areas::<16>(inner);

    let label_width = 14_usize;
    let fields: &[(SetupField, &str, &str)] = &[
        (SetupField::Source, locale.source, &form.source),
        (
            SetupField::Destination,
            locale.destination,
            &form.destination,
        ),
        (SetupField::Size, locale.size, &form.size),
        (SetupField::MinSize, locale.min_size, &form.min_size),
        (
            SetupField::MinDuration,
            locale.min_duration,
            &form.min_duration,
        ),
        (SetupField::Extensions, locale.extensions, &form.extensions),
        (SetupField::Exclude, locale.exclude, &form.exclude),
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

        let dim = Style::default().fg(Color::DarkGray);

        if value.is_empty() && !focused {
            spans.push(Span::styled(field.placeholder(locale), dim));
        } else if focused {
            let chars: Vec<char> = value.chars().collect();
            let cursor_pos = form.cursor.min(chars.len());

            let style_for_char = |ci: usize| -> Style {
                if invalid {
                    Style::default().fg(Color::Red)
                } else if let Some(split) = dest_split {
                    let byte_pos = chars
                        .get(..ci)
                        .map_or(0_usize, |slice| slice.iter().collect::<String>().len());
                    if byte_pos >= split {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default()
                    }
                } else {
                    Style::default()
                }
            };

            if chars.is_empty() {
                spans.push(Span::styled(
                    " ".to_string(),
                    Style::default().bg(Color::White).fg(Color::Black),
                ));
                spans.push(Span::styled(format!(" {}", field.placeholder(locale)), dim));
            } else {
                let cursor_style = Style::default().bg(Color::White).fg(Color::Black);
                let mut run = String::new();
                let mut run_style = style_for_char(0_usize);

                for (i, &ch) in chars.iter().enumerate() {
                    if i == cursor_pos {
                        if !run.is_empty() {
                            spans.push(Span::styled(std::mem::take(&mut run), run_style));
                        }
                        spans.push(Span::styled(ch.to_string(), cursor_style));
                        run_style = style_for_char(i.saturating_add(1));
                        continue;
                    }
                    let style = style_for_char(i);
                    if style != run_style && !run.is_empty() {
                        spans.push(Span::styled(std::mem::take(&mut run), run_style));
                        run_style = style;
                    }
                    run.push(ch);
                }
                if !run.is_empty() {
                    spans.push(Span::styled(run, run_style));
                }
                if cursor_pos >= chars.len() {
                    spans.push(Span::styled(" ".to_string(), cursor_style));
                }
            }

            if field.is_ext() && !value.is_empty() {
                let formatted = format_ext_list(value);
                if !formatted.is_empty() {
                    spans.push(Span::styled(format!("  \u{2192} {formatted}"), dim));
                }
            }
        } else if let Some(split) = dest_split {
            if split >= value.len() {
                spans.push(Span::raw((*value).to_string()));
            } else {
                let existing: String = value.chars().take(split).collect();
                let new_part: String = value.chars().skip(split).collect();
                spans.push(Span::raw(existing));
                spans.push(Span::styled(new_part, Style::default().fg(Color::Yellow)));
            }
        } else if invalid {
            spans.push(Span::styled(
                (*value).to_string(),
                Style::default().fg(Color::Red),
            ));
        } else {
            spans.push(Span::raw((*value).to_string()));
            if field.is_ext() && !value.is_empty() {
                let formatted = format_ext_list(value);
                if !formatted.is_empty() {
                    spans.push(Span::styled(format!("  \u{2192} {formatted}"), dim));
                }
            }
        }

        if let Some(chunk) = chunks.get(i) {
            frame.render_widget(Paragraph::new(Line::from(spans)), *chunk);
        }
    }

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
    let encoding_line = Line::from(vec![
        Span::styled(
            format!("{:<label_width$}", locale.encoding_label),
            encoding_style,
        ),
        Span::styled(format!("< {encoding_value} >"), encoding_style),
    ]);
    if let Some(chunk) = chunks.get(7) {
        frame.render_widget(Paragraph::new(encoding_line), *chunk);
    }

    let bitrate_info = match form.encoding {
        Encoding::Cbr => Some((
            locale.bitrate_label,
            format!("< {} kbps >", form.cbr_bitrate.as_kbps()),
        )),
        Encoding::Vbr => {
            let q = match form.vbr_quality {
                VbrQuality::High => locale.quality_high,
                VbrQuality::Medium => locale.quality_medium,
                VbrQuality::Low => locale.quality_low,
            };
            Some((locale.quality_label, format!("< {q} >")))
        }
        Encoding::Keep => None,
    };
    if let Some((label, value)) = bitrate_info {
        let bitrate_style = if form.focused == SetupField::Bitrate {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        let bitrate_line = Line::from(vec![
            Span::styled(format!("{label:<label_width$}"), bitrate_style),
            Span::styled(value, bitrate_style),
        ]);
        if let Some(chunk) = chunks.get(8) {
            frame.render_widget(Paragraph::new(bitrate_line), *chunk);
        }
    }

    let checkboxes: &[(SetupField, &str, bool, usize)] = &[
        (SetupField::NoLive, locale.no_live, form.no_live, 9_usize),
        (
            SetupField::KeepNames,
            locale.keep_names,
            form.keep_names,
            10_usize,
        ),
        (
            SetupField::Overwrite,
            locale.overwrite,
            form.overwrite,
            11_usize,
        ),
    ];
    for &(field, label, checked, row) in checkboxes {
        let focused = form.focused == field;
        let style = if focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        let marker = if checked { "x" } else { " " };
        let line = Line::from(vec![
            Span::styled(format!("{label:<label_width$}"), style),
            Span::styled(format!("[{marker}]"), style),
        ]);
        if let Some(chunk) = chunks.get(row) {
            frame.render_widget(Paragraph::new(line), *chunk);
        }
    }

    let start_style = if form.focused == SetupField::Start {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    let start = Line::from(Span::styled(format!(" [ {} ] ", locale.start), start_style));
    if let Some(chunk) = chunks.get(13) {
        frame.render_widget(Paragraph::new(start).alignment(Alignment::Center), *chunk);
    }

    let help = if let Some(err) = &form.error {
        Line::from(Span::styled(err.as_str(), Style::default().fg(Color::Red)))
    } else {
        Line::from(Span::styled(
            locale.help_setup,
            Style::default().fg(Color::DarkGray),
        ))
    };
    if let Some(chunk) = chunks.get(15) {
        frame.render_widget(Paragraph::new(help), *chunk);
    }

    if form.dropdown.visible && !form.dropdown.entries.is_empty() && form.focused.is_path() {
        let field_row = match form.focused {
            SetupField::Destination => 1_usize,
            _ => 0_usize,
        };
        let field_chunk_y = chunks.get(field_row).map_or(inner.y, |c| c.y);
        let dropdown_y = field_chunk_y.saturating_add(1);
        #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
        let label_w_u16 = label_width as u16;
        let dropdown_x = inner.x.saturating_add(label_w_u16);
        let dropdown_w = inner.width.saturating_sub(label_w_u16);
        let visible_count = (form
            .dropdown
            .entries
            .len()
            .saturating_sub(form.dropdown.scroll_offset))
        .min(crate::app::MAX_DROPDOWN);
        #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
        let dropdown_h = (visible_count as u16).saturating_add(2);
        let dropdown_rect = Rect::new(dropdown_x, dropdown_y, dropdown_w, dropdown_h);

        let items: Vec<ListItem> = form
            .dropdown
            .entries
            .iter()
            .skip(form.dropdown.scroll_offset)
            .take(crate::app::MAX_DROPDOWN)
            .enumerate()
            .map(|(i, entry)| {
                let idx = i.saturating_add(form.dropdown.scroll_offset);
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
    locale: &Locale,
    frame: &mut Frame,
    area: Rect,
) {
    let block = Block::bordered().title(format!(" mixr - {} ", locale.scanning));
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
        "{} {}...",
        locale.scanning,
        config.source.display()
    ));
    if let Some(chunk) = chunks.first() {
        frame.render_widget(Paragraph::new(scanning), *chunk);
    }

    let stats = Line::from(format!(
        "{} {} {} ({} {})",
        model.spinner_char(),
        state.files_found,
        locale.found,
        state.files_matched,
        locale.matched,
    ));
    if let Some(chunk) = chunks.get(1) {
        frame.render_widget(Paragraph::new(stats), *chunk);
    }

    if let Some(path) = &state.last_path {
        let path_str = path.display().to_string();
        #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
        let max_w = inner.width as usize;
        let display = if path_str.len() > max_w {
            let skip = path_str.len().saturating_sub(max_w).saturating_add(3);
            let tail: String = path_str.chars().skip(skip).collect();
            format!("...{tail}")
        } else {
            path_str
        };
        let last = Line::from(Span::styled(display, Style::default().fg(Color::DarkGray)));
        if let Some(chunk) = chunks.get(2) {
            frame.render_widget(Paragraph::new(last), *chunk);
        }
    }
}

fn encoding_label(cs: &CopyState, locale: &Locale) -> String {
    match cs.config.encoding {
        Encoding::Keep => locale.keep_original.to_string(),
        Encoding::Cbr => {
            let br = cs
                .config
                .cbr_bitrate
                .unwrap_or(CbrBitrate::Kbps192)
                .as_kbps();
            format!("CBR {br} kbps")
        }
        Encoding::Vbr => {
            let q = match cs.config.vbr_quality.unwrap_or(VbrQuality::Medium) {
                VbrQuality::High => locale.quality_high,
                VbrQuality::Medium => locale.quality_medium,
                VbrQuality::Low => locale.quality_low,
            };
            format!("VBR {q}")
        }
    }
}

#[allow(clippy::too_many_lines)]
fn view_copying(
    cs: &CopyState,
    locale: &Locale,
    confirm_quit: bool,
    frame: &mut Frame,
    area: Rect,
) {
    let enc = encoding_label(cs, locale);
    let block = Block::bordered().title(format!(" mixr - {} \u{2014} {enc} ", locale.copying));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
    let total_file_rows = (MAX_UPCOMING.saturating_add(1).saturating_add(MAX_HISTORY)) as u16;
    #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
    let error_rows: u16 = if cs.errors.is_empty() {
        0_u16
    } else {
        cs.errors.len().min(3_usize).saturating_add(1) as u16
    };
    let chunks = Layout::vertical([
        Constraint::Length(total_file_rows),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(error_rows),
        Constraint::Length(1),
    ])
    .areas::<7>(inner);

    render_file_list(cs, locale, frame, chunks[0_usize]);

    let current_pct = format!(
        "{:.0}%",
        cs.current_progress().clamp(0.0_f64, 1.0_f64) * 100.0_f64
    );
    let current_label = if let Some(cur) = cs.current() {
        format!(
            "{}: {} ({}) {current_pct}",
            locale.current, cur.name, cur.size
        )
    } else {
        format!("{}: {current_pct}", locale.current)
    };
    let current_gauge = Gauge::default()
        .label(Span::styled(
            current_label,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .ratio(cs.current_progress().clamp(0.0_f64, 1.0_f64))
        .gauge_style(
            Style::default()
                .fg(Color::Rgb(0, 150, 180))
                .bg(Color::Rgb(40, 40, 40)),
        );
    if let Some(chunk) = chunks.get(2) {
        frame.render_widget(current_gauge, *chunk);
    }

    let done_count = cs
        .files
        .iter()
        .filter(|f| f.status == FileStatus::Done)
        .count();
    let total_pct = format!(
        "{:.0}%",
        cs.overall_progress().clamp(0.0_f64, 1.0_f64) * 100.0_f64
    );
    let total_label = format!(
        "{}: {} / {} ({}/{}) {total_pct}",
        locale.total,
        ByteSize(cs.copied_bytes),
        ByteSize(cs.total_bytes),
        done_count,
        cs.total_files,
    );
    let total_gauge = Gauge::default()
        .label(Span::styled(
            total_label,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .ratio(cs.overall_progress().clamp(0.0_f64, 1.0_f64))
        .gauge_style(
            Style::default()
                .fg(Color::Rgb(0, 150, 0))
                .bg(Color::Rgb(40, 40, 40)),
        );
    if let Some(chunk) = chunks.get(3) {
        frame.render_widget(total_gauge, *chunk);
    }

    let speed = cs.speed();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::as_conversions
    )]
    let speed_u64 = speed as u64;
    let speed_str = ByteSize(speed_u64);
    let elapsed = cs.started_at.elapsed();
    let elapsed_str = format_duration(elapsed);
    let eta_str = cs.eta_secs().map_or_else(
        || "\u{2014}".to_string(),
        |s| format_duration(Duration::from_secs_f64(s)),
    );

    let status = Line::from(Span::styled(
        format!(
            "{speed_str}/s  {}: {elapsed_str}  {}: {eta_str}",
            locale.elapsed, locale.eta
        ),
        Style::default().fg(Color::DarkGray),
    ));
    if let Some(chunk) = chunks.get(4) {
        frame.render_widget(Paragraph::new(status), *chunk);
    }

    if !cs.errors.is_empty() {
        let max_shown = 3_usize;
        let start = cs.errors.len().saturating_sub(max_shown);
        let mut error_lines = vec![Line::from(Span::styled(
            format!("{} ({}):", locale.errors, cs.errors.len()),
            Style::default().fg(Color::Red).add_modifier(Modifier::DIM),
        ))];
        error_lines.extend(cs.errors.iter().skip(start).map(|err| {
            Line::from(Span::styled(
                format!("  {err}"),
                Style::default().fg(Color::Red).add_modifier(Modifier::DIM),
            ))
        }));
        if let Some(chunk) = chunks.get(5) {
            frame.render_widget(Paragraph::new(error_lines), *chunk);
        }
    }

    let help = if confirm_quit {
        Line::from(Span::styled(
            "Press Ctrl+C again to stop",
            Style::default().fg(Color::Red),
        ))
    } else {
        Line::from(Span::styled(
            locale.ctrl_c_stop,
            Style::default().fg(Color::DarkGray),
        ))
    };
    if let Some(chunk) = chunks.get(6) {
        frame.render_widget(Paragraph::new(help), *chunk);
    }
}

const HISTORY_GREENS: &[Color] = &[
    Color::Rgb(0, 220, 0),
    Color::Rgb(0, 180, 0),
    Color::Rgb(0, 140, 0),
    Color::Rgb(0, 100, 0),
];

fn render_file_list(cs: &CopyState, locale: &Locale, frame: &mut Frame, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    let upcoming: Vec<_> = cs.upcoming().collect();
    lines.extend(
        std::iter::repeat_with(|| Line::from("")).take(MAX_UPCOMING.saturating_sub(upcoming.len())),
    );
    let spinner = cs.spinner_char();
    lines.extend(upcoming.iter().rev().map(|item| {
        let display_name = item
            .original_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&item.name);
        let (suffix, style) = match item.status {
            FileStatus::Reading => (
                format!("   {spinner} {}", locale.preparing),
                Style::default().fg(Color::Yellow),
            ),
            FileStatus::Converting => (
                format!("   {spinner} {}", locale.converting),
                Style::default().fg(Color::Yellow),
            ),
            _ => (String::new(), Style::default().fg(Color::DarkGray)),
        };
        Line::from(vec![
            Span::styled(
                format!("  {display_name}"),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(suffix, style),
        ])
    }));

    if let Some(cur) = cs.current() {
        let cur_display = cur
            .original_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&cur.name);
        let size_info = if cur.converting {
            let written = ByteSize(cs.current_file_copied);
            format!(
                " ({} \u{2192} {}) [{}]",
                cur.size,
                written,
                locale.converting.trim_end_matches('\u{2026}')
            )
        } else {
            format!(" ({})", cur.size)
        };
        lines.push(Line::from(Span::styled(
            format!("> {cur_display}{size_info}"),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )));
    } else {
        lines.push(Line::from(""));
    }

    let history: Vec<_> = cs.history().collect();
    lines.extend(
        history
            .iter()
            .zip(HISTORY_GREENS.iter())
            .map(|(item, &color)| {
                let hist_display = item
                    .original_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&item.name);
                let style = match item.status {
                    FileStatus::Done => Style::default().fg(color),
                    FileStatus::Failed => Style::default().fg(Color::Red),
                    _ => Style::default(),
                };
                let size_suffix = if item.converting && item.written_bytes > 0_u64 {
                    format!(" ({} \u{2192} {})", item.size, ByteSize(item.written_bytes))
                } else {
                    String::new()
                };
                Line::from(Span::styled(
                    format!("  {hist_display}{size_suffix}"),
                    style,
                ))
            }),
    );
    lines.extend(
        std::iter::repeat_with(|| Line::from("")).take(MAX_HISTORY.saturating_sub(history.len())),
    );

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

fn view_done(
    total_files: usize,
    total_bytes: u64,
    errors: &[String],
    elapsed: Duration,
    locale: &Locale,
    frame: &mut Frame,
    area: Rect,
) {
    let block = Block::bordered().title(format!(" mixr - {} ", locale.done));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = vec![
        Line::from(Span::styled(
            locale.done,
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!(
            "{} {} ({})",
            locale.copied_files,
            total_files,
            ByteSize(total_bytes),
        )),
        Line::from(format!("{}: {}", locale.time, format_duration(elapsed))),
    ];

    if !errors.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("{} {}:", errors.len(), locale.errors),
            Style::default().fg(Color::Red),
        )));
        lines.extend(errors.iter().take(10_usize).map(|err| {
            Line::from(Span::styled(
                format!("  {err}"),
                Style::default().fg(Color::Red),
            ))
        }));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        locale.press_q,
        Style::default().fg(Color::DarkGray),
    )));

    frame.render_widget(Paragraph::new(lines), inner);
}

fn view_error(msg: &str, locale: &Locale, frame: &mut Frame, area: Rect) {
    let block = Block::bordered().title(format!(" mixr - {} ", locale.fatal_error));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(Span::styled(
            locale.fatal_error,
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(msg.to_string()),
        Line::from(""),
        Line::from(Span::styled(
            locale.press_q,
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
