use ratatui::{prelude::*, widgets::*};
use crate::app::{App, Tab, Focus, SearchType};

// -----------------------------------------------------------------
// Palette
// -----------------------------------------------------------------
const ACCENT: Color = Color::Cyan;
const ACCENT2: Color = Color::Magenta;
const MUTED: Color = Color::DarkGray;
const WARN: Color = Color::LightYellow;
const OK: Color = Color::LightGreen;
const SEL_BG: Color = Color::Rgb(30, 38, 46);

pub fn render(f: &mut Frame, app: &mut App) {
    // [header 1] [body] [player 5]
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(4),
            Constraint::Length(5),
        ])
        .split(f.size());

    render_header(f, app, chunks[0]);
    render_body(f, app, chunks[1]);
    render_player(f, app, chunks[2]);
}

// -----------------------------------------------------------------
// Header — single slim line: brand on the left, account on the right
// -----------------------------------------------------------------
fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(14)])
        .split(area);

    let brand = Paragraph::new(Line::from(vec![
        Span::styled(" ♪ rusty tube ", Style::default().bg(ACCENT).fg(Color::Black).add_modifier(Modifier::BOLD)),
    ]));
    f.render_widget(brand, cols[0]);

    let account = if app.client.is_authenticated() {
        Span::styled("● connected", Style::default().fg(OK))
    } else {
        Span::styled("○ guest", Style::default().fg(WARN))
    };
    f.render_widget(Paragraph::new(account).alignment(Alignment::Right), cols[1]);
}

// -----------------------------------------------------------------
// Body — narrow sidebar + content panel
// -----------------------------------------------------------------
fn render_body(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(15), Constraint::Min(20)])
        .split(area);

    render_sidebar(f, app, cols[0]);
    render_content(f, app, cols[1]);
}

fn render_sidebar(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Sidebar;
    let border = if focused { ACCENT2 } else { MUTED };

    let items: Vec<ListItem> = Tab::all()
        .iter()
        .map(|tab| {
            let active = app.active_tab == *tab;
            let style = if active {
                Style::default().fg(ACCENT2).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", tab.icon()), style),
                Span::styled(tab.name(), style),
            ]))
        })
        .collect();

    let widget = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border)),
        )
        .highlight_style(Style::default().bg(SEL_BG).fg(ACCENT2).add_modifier(Modifier::BOLD))
        .highlight_symbol("❯ ");

    let mut state = ListState::default();
    if focused {
        state.select(Some(app.active_tab as usize));
    }
    f.render_stateful_widget(widget, area, &mut state);
}

fn render_content(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = matches!(app.focus, Focus::Main | Focus::SearchInput | Focus::LoginInput);
    let border = if focused { ACCENT } else { MUTED };

    let block = Block::default()
        .title(Span::styled(
            format!(" {} ", app.active_tab.name()),
            Style::default().fg(border).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border));

    let inner = block.inner(area);
    f.render_widget(block, area);

    match app.active_tab {
        Tab::Search => render_search(f, app, inner),
        Tab::Library => render_track_tab(
            f, app, inner,
            app.library_songs.clone(),
            "Guest mode — log in to see your liked songs.",
            "No liked songs yet.",
            ["TITLE", "ARTIST", "TIME"],
            app.client.is_authenticated(),
        ),
        Tab::Playlists => render_playlists(f, app, inner),
        Tab::History => render_track_tab(
            f, app, inner,
            app.history_tracks.clone(),
            "Guest mode — log in to see your history.",
            "No history yet.",
            ["TITLE", "ARTIST", "TIME"],
            app.client.is_authenticated(),
        ),
        Tab::Radio => render_radio(f, app, inner),
        Tab::Login => render_login(f, app, inner),
    }
}

// -----------------------------------------------------------------
// Tab renderers
// -----------------------------------------------------------------
fn render_search(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(2)])
        .split(area);

    // Search box with an inline filter indicator in the title.
    let active_input = app.focus == Focus::SearchInput;
    let box_border = if active_input { WARN } else { MUTED };
    let (songs_style, pl_style) = match app.search_type {
        SearchType::Songs => (
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            Style::default().fg(MUTED),
        ),
        SearchType::Playlists => (
            Style::default().fg(MUTED),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
    };
    let title = Line::from(vec![
        Span::styled(" search ", Style::default().fg(box_border)),
        Span::styled("songs", songs_style),
        Span::styled(" · ", Style::default().fg(MUTED)),
        Span::styled("playlists ", pl_style),
        Span::styled("[tab] ", Style::default().fg(MUTED)),
    ]);
    let cursor = if active_input { "▏" } else { "" };
    let field = Paragraph::new(format!(" {}{}", app.search_input, cursor)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(box_border))
            .title(title),
    );
    f.render_widget(field, rows[0]);

    match app.search_type {
        SearchType::Songs => track_table(
            f, app, rows[1], &app.searched_songs, ["TITLE", "ARTIST", "TIME"],
        ),
        SearchType::Playlists => playlist_table(
            f, app, rows[1], &app.searched_playlists, ["PLAYLIST", "AUTHOR", "TRACKS"],
        ),
    }
}

fn render_playlists(f: &mut Frame, app: &App, area: Rect) {
    if !app.client.is_authenticated() {
        placeholder(f, area, "Guest mode — log in to see your playlists.", WARN);
        return;
    }

    if app.active_playlist_id.is_some() {
        if app.playlist_tracks.is_empty() {
            placeholder(f, area, "Loading tracks…", Color::Gray);
        } else {
            track_table(f, app, area, &app.playlist_tracks, ["TITLE", "ARTIST", "TIME"]);
        }
    } else if app.library_playlists.is_empty() {
        placeholder(f, area, "No playlists yet.", Color::Gray);
    } else {
        playlist_table(f, app, area, &app.library_playlists, ["PLAYLIST", "AUTHOR", "TRACKS"]);
    }
}

fn render_radio(f: &mut Frame, app: &App, area: Rect) {
    if app.recommendation_tracks.is_empty() {
        placeholder(
            f, area,
            "Highlight or play a track, then press R to start a radio of similar songs.",
            Color::Gray,
        );
    } else {
        track_table(f, app, area, &app.recommendation_tracks, ["TITLE", "ARTIST", "TIME"]);
    }
}

#[allow(clippy::too_many_arguments)]
fn render_track_tab(
    f: &mut Frame,
    app: &App,
    area: Rect,
    tracks: Vec<crate::yt::Track>,
    locked_msg: &str,
    empty_msg: &str,
    headers: [&str; 3],
    authed: bool,
) {
    if !authed {
        placeholder(f, area, locked_msg, WARN);
    } else if tracks.is_empty() {
        placeholder(f, area, empty_msg, Color::Gray);
    } else {
        track_table(f, app, area, &tracks, headers);
    }
}

fn render_login(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(3), Constraint::Length(1)])
        .split(area);

    let lines = vec![
        Line::from(Span::styled("Browser login (recommended)", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from("  1. Ctrl+O — open music.youtube.com"),
        Line::from("  2. Sign in there (skip if already signed in)"),
        Line::from("  3. Return and press Enter to import your session"),
        Line::from(""),
        Line::from(Span::styled("Manual: paste your Cookie header below, then Enter.", Style::default().fg(MUTED))),
    ];
    f.render_widget(Paragraph::new(lines).style(Style::default().fg(Color::Gray)), rows[0]);

    let active = app.focus == Focus::LoginInput;
    let border = if active { WARN } else { MUTED };
    let body = if app.login_input.is_empty() {
        " paste cookie, or just press Enter to import".to_string()
    } else {
        format!(" {}", "•".repeat(app.login_input.len().min(40)))
    };
    let input = Paragraph::new(body).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border))
            .title(Span::styled(" Ctrl+O browser · Enter import ", Style::default().fg(border))),
    );
    f.render_widget(input, rows[1]);

    if !app.login_status_msg.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(app.login_status_msg.clone(), Style::default().fg(WARN)),
            ])),
            rows[2],
        );
    }
}

// -----------------------------------------------------------------
// Shared table helpers
// -----------------------------------------------------------------
fn track_table(f: &mut Frame, app: &App, area: Rect, tracks: &[crate::yt::Track], headers: [&str; 3]) {
    let rows: Vec<Row> = tracks
        .iter()
        .map(|t| {
            Row::new(vec![
                Cell::from(t.title.clone()),
                Cell::from(Span::styled(t.artist.clone(), Style::default().fg(Color::Gray))),
                Cell::from(Span::styled(t.duration.clone(), Style::default().fg(MUTED))),
            ])
        })
        .collect();
    render_table(f, app, area, rows, headers, [
        Constraint::Percentage(55),
        Constraint::Percentage(35),
        Constraint::Length(6),
    ]);
}

fn playlist_table(f: &mut Frame, app: &App, area: Rect, items: &[crate::yt::Playlist], headers: [&str; 3]) {
    let rows: Vec<Row> = items
        .iter()
        .map(|p| {
            Row::new(vec![
                Cell::from(p.title.clone()),
                Cell::from(Span::styled(p.author.clone(), Style::default().fg(Color::Gray))),
                Cell::from(Span::styled(p.song_count.clone(), Style::default().fg(MUTED))),
            ])
        })
        .collect();
    render_table(f, app, area, rows, headers, [
        Constraint::Percentage(55),
        Constraint::Percentage(30),
        Constraint::Length(8),
    ]);
}

fn render_table(
    f: &mut Frame,
    app: &App,
    area: Rect,
    rows: Vec<Row>,
    headers: [&str; 3],
    widths: [Constraint; 3],
) {
    let table = Table::new(rows, widths)
        .header(
            Row::new(headers.to_vec())
                .style(Style::default().fg(MUTED).add_modifier(Modifier::BOLD))
                .bottom_margin(0),
        )
        .column_spacing(2)
        .highlight_style(Style::default().bg(SEL_BG).fg(ACCENT).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    let mut state = TableState::default();
    if app.focus == Focus::Main {
        state.select(Some(app.selected_index));
    }
    f.render_stateful_widget(table, area, &mut state);
}

fn placeholder(f: &mut Frame, area: Rect, msg: &str, color: Color) {
    let text = Paragraph::new(msg)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(color));
    // Vertically center-ish by padding from the top.
    let pad = area.height.saturating_sub(2) / 2;
    let inner = Rect { y: area.y + pad, height: area.height.saturating_sub(pad), ..area };
    f.render_widget(text, inner);
}

// -----------------------------------------------------------------
// Player
// -----------------------------------------------------------------
fn render_player(f: &mut Frame, app: &App, area: Rect) {
    let p = app.player.lock().unwrap();
    let current_track = p.current_track.clone();
    let is_paused = p.is_paused();
    let elapsed = p.elapsed_seconds();
    let is_loading = p.is_loading;
    let volume = p.volume();
    drop(p);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    // Row 1 — state + track
    let (title, artist, duration_str) = match &current_track {
        Some(t) => (t.title.clone(), t.artist.clone(), t.duration.clone()),
        None => ("Nothing playing".to_string(), String::new(), "0:00".to_string()),
    };
    let state = if is_loading {
        Span::styled("⠿ ", Style::default().fg(WARN).add_modifier(Modifier::BOLD))
    } else if current_track.is_none() {
        Span::styled("■ ", Style::default().fg(MUTED))
    } else if is_paused {
        Span::styled("⏸ ", Style::default().fg(WARN).add_modifier(Modifier::BOLD))
    } else {
        Span::styled("▶ ", Style::default().fg(OK).add_modifier(Modifier::BOLD))
    };
    let mut spans = vec![
        state,
        Span::styled(title, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
    ];
    if !artist.is_empty() {
        spans.push(Span::styled("  —  ", Style::default().fg(MUTED)));
        spans.push(Span::styled(artist, Style::default().fg(Color::LightBlue)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), layout[0]);

    // Row 2 — progress bar
    let total = parse_duration_to_seconds(&duration_str);
    f.render_widget(
        Paragraph::new(progress_line(elapsed, total, &duration_str, layout[1].width)),
        layout[1],
    );

    // Row 3 — compact controls
    f.render_widget(
        Paragraph::new(controls_line(app, volume)).alignment(Alignment::Center),
        layout[2],
    );
}

/// Builds a btop-style progress bar: `1:23 ━━━━━━╸──────── 3:45`.
fn progress_line(elapsed: u64, total: u64, total_str: &str, width: u16) -> Line<'static> {
    let elapsed_str = format_seconds_to_duration(elapsed);
    let total_disp = if total > 0 { total_str.to_string() } else { "--:--".to_string() };

    let reserved = elapsed_str.len() + total_disp.len() + 2; // two spaces
    let bar_w = (width as usize).saturating_sub(reserved).max(1);

    let ratio = if total > 0 { (elapsed as f64 / total as f64).clamp(0.0, 1.0) } else { 0.0 };
    let filled = ((bar_w as f64) * ratio).round() as usize;
    let filled = filled.min(bar_w);

    let bar_filled: String = "━".repeat(filled);
    let bar_empty: String = "─".repeat(bar_w - filled);

    Line::from(vec![
        Span::styled(elapsed_str, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled(bar_filled, Style::default().fg(ACCENT)),
        Span::styled(bar_empty, Style::default().fg(MUTED)),
        Span::raw(" "),
        Span::styled(total_disp, Style::default().fg(MUTED)),
    ])
}

fn controls_line(app: &App, volume: f32) -> Line<'static> {
    let key = |k: &str| Span::styled(k.to_string(), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD));
    let lbl = |l: &str| Span::styled(l.to_string(), Style::default().fg(Color::Gray));
    let sep = || Span::styled("  ", Style::default());

    let repeat_active = app.loop_mode != crate::app::LoopMode::Off;
    let repeat_style = if repeat_active {
        Style::default().fg(OK).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };

    Line::from(vec![
        key("␣"), lbl(" play"), sep(),
        key("n/p"), lbl(" skip"), sep(),
        key("←→"), lbl(" seek"), sep(),
        key("↑↓"), Span::styled(format!(" {:.0}%", volume * 100.0), Style::default().fg(Color::Gray)), sep(),
        key("r"), Span::styled(format!(" {}", app.loop_mode.label()), repeat_style), sep(),
        key("R"), lbl(" radio"), sep(),
        Span::styled("q", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)), lbl(" quit"),
    ])
}

// -----------------------------------------------------------------
// Duration helpers
// -----------------------------------------------------------------
fn parse_duration_to_seconds(duration: &str) -> u64 {
    let parts: Vec<&str> = duration.trim().split(':').collect();
    match parts.as_slice() {
        [m, s] => m.parse::<u64>().unwrap_or(0) * 60 + s.parse::<u64>().unwrap_or(0),
        [h, m, s] => {
            h.parse::<u64>().unwrap_or(0) * 3600
                + m.parse::<u64>().unwrap_or(0) * 60
                + s.parse::<u64>().unwrap_or(0)
        }
        _ => 0,
    }
}

fn format_seconds_to_duration(seconds: u64) -> String {
    let (h, m, s) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, s)
    } else {
        format!("{}:{:02}", m, s)
    }
}
