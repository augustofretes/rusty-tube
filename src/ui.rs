use ratatui::{prelude::*, widgets::*};
use crate::app::{App, Tab, Focus, SearchType};

pub fn render(f: &mut Frame, app: &mut App) {
    // Colors
    let active_color = Color::Cyan;
    let sidebar_focus_color = Color::Magenta;
    let main_focus_color = Color::Cyan;
    let muted_color = Color::DarkGray;
    let alert_color = Color::LightYellow;
    let success_color = Color::LightGreen;

    // Define top-level layout: [Header (3 lines)] -> [Main Area (remaining except player)] -> [Player (7 lines)]
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(7),
        ])
        .split(f.size());

    // -------------------------------------------------------------
    // HEADER SECTION
    // -------------------------------------------------------------
    let user_status = if app.client.is_authenticated() {
        Span::styled("● Google Account Connected", Style::default().fg(success_color).add_modifier(Modifier::BOLD))
    } else {
        Span::styled("○ Running in Guest Mode", Style::default().fg(alert_color))
    };

    let header_widget = Paragraph::new(Line::from(vec![
        Span::styled(" YTM-TUI PLAYER ", Style::default().bg(active_color).fg(Color::Black).add_modifier(Modifier::BOLD)),
        Span::raw(" │ "),
        user_status,
    ]))
    .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(muted_color)));
    
    f.render_widget(header_widget, chunks[0]);

    // -------------------------------------------------------------
    // MAIN AREA (SIDEBAR + CONTENT PANEL)
    // -------------------------------------------------------------
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // Sidebar width
            Constraint::Percentage(75), // Content width
        ])
        .split(chunks[1]);

    // Render Sidebar (Tabs)
    let sidebar_border_color = if app.focus == Focus::Sidebar { sidebar_focus_color } else { muted_color };
    let tabs_list: Vec<ListItem> = Tab::all()
        .iter()
        .map(|tab| {
            let label = tab.name();
            let style = if app.active_tab == *tab {
                Style::default().fg(sidebar_focus_color).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Span::styled(label, style))
        })
        .collect();

    let sidebar_widget = List::new(tabs_list)
        .block(
            Block::default()
                .title(" NAVIGATION ")
                .title_style(Style::default().fg(sidebar_border_color).add_modifier(Modifier::BOLD))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(sidebar_border_color))
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(40, 20, 40))
                .fg(sidebar_focus_color)
                .add_modifier(Modifier::BOLD)
        )
        .highlight_symbol("❯ ");

    // Ensure selection index matches tab index in Sidebar mode
    let mut sidebar_state = ListState::default();
    if app.focus == Focus::Sidebar {
        sidebar_state.select(Some(app.active_tab as usize));
    } else {
        sidebar_state.select(None);
    }
    
    f.render_stateful_widget(sidebar_widget, main_chunks[0], &mut sidebar_state);

    // Prepare Content Panel border block
    let content_border_color = if app.focus == Focus::Main || app.focus == Focus::SearchInput || app.focus == Focus::LoginInput {
        main_focus_color
    } else {
        muted_color
    };
    
    let content_block = Block::default()
        .title(format!(" {} ", app.active_tab.name().to_uppercase()))
        .title_style(Style::default().fg(content_border_color).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(content_border_color));

    // Get the inner area where content elements will be drawn
    let inner_area = content_block.inner(main_chunks[1]);
    
    // Render the outer panel borders first
    f.render_widget(content_block, main_chunks[1]);

    // Now render tab content inside the pre-calculated inner area
    match app.active_tab {
        Tab::Search => {
            // Layout: [Search input (3 lines)] -> [Results list/table]
            let search_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(2)])
                .split(inner_area);

            // Search input field
            let search_border = if app.focus == Focus::SearchInput { Color::Yellow } else { muted_color };
            let search_text = format!(" {}|", app.search_input); // cursor representation
            let search_field = Paragraph::new(search_text)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(search_border))
                        .title(" Query Input (Press Esc to focus results, '/' to search) ")
                        .title_style(Style::default().fg(search_border))
                );
            f.render_widget(search_field, search_layout[0]);

            // Draw results table based on search filter type
            match app.search_type {
                SearchType::Songs => {
                    let rows: Vec<Row> = app.searched_songs
                        .iter()
                        .map(|song| Row::new(vec![
                            Cell::from(song.title.clone()),
                            Cell::from(song.artist.clone()),
                            Cell::from(song.duration.clone()),
                        ]))
                        .collect();
                    
                    let table = Table::new(
                        rows,
                        [Constraint::Percentage(50), Constraint::Percentage(40), Constraint::Percentage(10)]
                    )
                    .header(
                        Row::new(vec!["SONG TITLE", "ARTIST", "DURATION"])
                            .style(Style::default().fg(active_color).add_modifier(Modifier::BOLD))
                    )
                    .block(Block::default().title(" Song Results (Press Tab to filter Playlists) ").title_style(Style::default().fg(muted_color)))
                    .highlight_style(Style::default().bg(Color::Rgb(20, 40, 40)).fg(active_color).add_modifier(Modifier::BOLD))
                    .highlight_symbol("▶ ");

                    let mut table_state = TableState::default();
                    if app.focus == Focus::Main {
                        table_state.select(Some(app.selected_index));
                    }
                    f.render_stateful_widget(table, search_layout[1], &mut table_state);
                }
                SearchType::Playlists => {
                    let rows: Vec<Row> = app.searched_playlists
                        .iter()
                        .map(|pl| Row::new(vec![
                            Cell::from(pl.title.clone()),
                            Cell::from(pl.author.clone()),
                            Cell::from(pl.song_count.clone()),
                        ]))
                        .collect();
                    
                    let table = Table::new(
                        rows,
                        [Constraint::Percentage(50), Constraint::Percentage(35), Constraint::Percentage(15)]
                    )
                    .header(
                        Row::new(vec!["PLAYLIST TITLE", "AUTHOR", "SONG COUNT / VIEWS"])
                            .style(Style::default().fg(active_color).add_modifier(Modifier::BOLD))
                    )
                    .block(Block::default().title(" Playlist Results (Press Tab to filter Songs) ").title_style(Style::default().fg(muted_color)))
                    .highlight_style(Style::default().bg(Color::Rgb(20, 40, 40)).fg(active_color).add_modifier(Modifier::BOLD))
                    .highlight_symbol("▶ ");

                    let mut table_state = TableState::default();
                    if app.focus == Focus::Main {
                        table_state.select(Some(app.selected_index));
                    }
                    f.render_stateful_widget(table, search_layout[1], &mut table_state);
                }
            }
        }
        Tab::Library => {
            if !app.client.is_authenticated() {
                let text = Paragraph::new("\n\n  Guest Mode: Library items are locked.\n  Please log in using your Google account cookie in the Login tab.")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(alert_color));
                f.render_widget(text, inner_area);
            } else if app.library_songs.is_empty() {
                let text = Paragraph::new("\n\n  Library is empty or loading...")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::Gray));
                f.render_widget(text, inner_area);
            } else {
                let rows: Vec<Row> = app.library_songs
                    .iter()
                    .map(|song| Row::new(vec![
                        Cell::from(song.title.clone()),
                        Cell::from(song.artist.clone()),
                        Cell::from(song.duration.clone()),
                    ]))
                    .collect();
                
                let table = Table::new(
                    rows,
                    [Constraint::Percentage(50), Constraint::Percentage(40), Constraint::Percentage(10)]
                )
                .header(
                    Row::new(vec!["LIKED SONG TITLE", "ARTIST", "DURATION"])
                        .style(Style::default().fg(active_color).add_modifier(Modifier::BOLD))
                )
                .highlight_style(Style::default().bg(Color::Rgb(20, 40, 40)).fg(active_color).add_modifier(Modifier::BOLD))
                .highlight_symbol("▶ ");

                let mut table_state = TableState::default();
                if app.focus == Focus::Main {
                    table_state.select(Some(app.selected_index));
                }
                f.render_stateful_widget(table, inner_area, &mut table_state);
            }
        }
        Tab::Playlists => {
            if !app.client.is_authenticated() {
                let text = Paragraph::new("\n\n  Guest Mode: Playlists are locked.\n  Please log in using your Google account cookie in the Login tab.")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(alert_color));
                f.render_widget(text, inner_area);
            } else if app.active_playlist_id.is_some() {
                // Render playlist tracks
                let title = app.active_playlist_title.as_deref().unwrap_or("Playlist");
                let rows: Vec<Row> = app.playlist_tracks
                    .iter()
                    .map(|track| Row::new(vec![
                        Cell::from(track.title.clone()),
                        Cell::from(track.artist.clone()),
                        Cell::from(track.duration.clone()),
                    ]))
                    .collect();
                
                let table = Table::new(
                    rows,
                    [Constraint::Percentage(50), Constraint::Percentage(40), Constraint::Percentage(10)]
                )
                .header(
                    Row::new(vec!["SONG TITLE", "ARTIST / SOURCE", "DURATION"])
                        .style(Style::default().fg(active_color).add_modifier(Modifier::BOLD))
                )
                .block(Block::default().title(format!(" Content: {} (Esc to go back) ", title)).title_style(Style::default().fg(Color::Yellow)))
                .highlight_style(Style::default().bg(Color::Rgb(20, 40, 40)).fg(active_color).add_modifier(Modifier::BOLD))
                .highlight_symbol("▶ ");

                let mut table_state = TableState::default();
                if app.focus == Focus::Main {
                    table_state.select(Some(app.selected_index));
                }
                f.render_stateful_widget(table, inner_area, &mut table_state);
            } else if app.library_playlists.is_empty() {
                let text = Paragraph::new("\n\n  No playlists found or loading...")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::Gray));
                f.render_widget(text, inner_area);
            } else {
                // Render library playlists
                let rows: Vec<Row> = app.library_playlists
                    .iter()
                    .map(|pl| Row::new(vec![
                        Cell::from(pl.title.clone()),
                        Cell::from(pl.author.clone()),
                        Cell::from(pl.song_count.clone()),
                    ]))
                    .collect();
                
                let table = Table::new(
                    rows,
                    [Constraint::Percentage(50), Constraint::Percentage(35), Constraint::Percentage(15)]
                )
                .header(
                    Row::new(vec!["PLAYLIST TITLE", "AUTHOR / CURATOR", "SONG COUNT"])
                        .style(Style::default().fg(active_color).add_modifier(Modifier::BOLD))
                )
                .highlight_style(Style::default().bg(Color::Rgb(20, 40, 40)).fg(active_color).add_modifier(Modifier::BOLD))
                .highlight_symbol("▶ ");

                let mut table_state = TableState::default();
                if app.focus == Focus::Main {
                    table_state.select(Some(app.selected_index));
                }
                f.render_stateful_widget(table, inner_area, &mut table_state);
            }
        }
        Tab::History => {
            if !app.client.is_authenticated() {
                let text = Paragraph::new("\n\n  Guest Mode: Listening history requires authentication.\n  Please log in using your Google account cookie in the Login tab.")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(alert_color));
                f.render_widget(text, inner_area);
            } else if app.history_tracks.is_empty() {
                let text = Paragraph::new("\n\n  History is empty or loading...")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::Gray));
                f.render_widget(text, inner_area);
            } else {
                let rows: Vec<Row> = app.history_tracks
                    .iter()
                    .map(|song| Row::new(vec![
                        Cell::from(song.title.clone()),
                        Cell::from(song.artist.clone()),
                        Cell::from(song.duration.clone()),
                    ]))
                    .collect();
                
                let table = Table::new(
                    rows,
                    [Constraint::Percentage(50), Constraint::Percentage(40), Constraint::Percentage(10)]
                )
                .header(
                    Row::new(vec!["RECENTLY PLAYED SONG TITLE", "ARTIST", "DURATION"])
                        .style(Style::default().fg(active_color).add_modifier(Modifier::BOLD))
                )
                .highlight_style(Style::default().bg(Color::Rgb(20, 40, 40)).fg(active_color).add_modifier(Modifier::BOLD))
                .highlight_symbol("▶ ");

                let mut table_state = TableState::default();
                if app.focus == Focus::Main {
                    table_state.select(Some(app.selected_index));
                }
                f.render_stateful_widget(table, inner_area, &mut table_state);
            }
        }
        Tab::Radio => {
            if app.recommendation_tracks.is_empty() {
                let text = Paragraph::new("\n\n  No radio yet.\n  Highlight or play a song anywhere, then press 'R' to start a radio of recommendations based on it.")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::Gray));
                f.render_widget(text, inner_area);
            } else {
                let rows: Vec<Row> = app.recommendation_tracks
                    .iter()
                    .map(|song| Row::new(vec![
                        Cell::from(song.title.clone()),
                        Cell::from(song.artist.clone()),
                        Cell::from(song.duration.clone()),
                    ]))
                    .collect();

                let seed_label = app.radio_seed_title
                    .as_deref()
                    .map(|s| format!(" Recommended from: {} ", s))
                    .unwrap_or_else(|| " Recommendations ".to_string());

                let table = Table::new(
                    rows,
                    [Constraint::Percentage(50), Constraint::Percentage(40), Constraint::Percentage(10)]
                )
                .header(
                    Row::new(vec!["RECOMMENDED SONG TITLE", "ARTIST", "DURATION"])
                        .style(Style::default().fg(active_color).add_modifier(Modifier::BOLD))
                )
                .block(Block::default().title(seed_label).title_style(Style::default().fg(Color::Yellow)))
                .highlight_style(Style::default().bg(Color::Rgb(20, 40, 40)).fg(active_color).add_modifier(Modifier::BOLD))
                .highlight_symbol("▶ ");

                let mut table_state = TableState::default();
                if app.focus == Focus::Main {
                    table_state.select(Some(app.selected_index));
                }
                f.render_stateful_widget(table, inner_area, &mut table_state);
            }
        }
        Tab::Login => {
            // Render instructions + raw input field
            let login_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(10),
                    Constraint::Length(3),
                    Constraint::Length(3),
                ])
                .split(inner_area);

            let instructions = "  LOG IN TO YOUR YOUTUBE MUSIC / GOOGLE ACCOUNT:\n\n\
                  Browser login (recommended):\n\
                  1. Press Ctrl+O to open music.youtube.com in your browser.\n\
                  2. Sign in there (skip if already signed in).\n\
                  3. Return here and press Enter to import your session automatically.\n\n\
                  Manual fallback: paste your 'Cookie' header below and press Enter.";
                
            let inst_widget = Paragraph::new(instructions).style(Style::default().fg(Color::Gray));
            f.render_widget(inst_widget, login_layout[0]);

            // Input field
            let input_border = if app.focus == Focus::LoginInput { Color::Yellow } else { muted_color };
            // Obfuscate pasted cookie value for privacy screen
            let len = app.login_input.len();
            let obfuscated_text = if len > 0 {
                format!(" {} (Press Enter to authenticate)...|", "*".repeat(len.min(40)))
            } else {
                " [Press Enter to import from browser, or paste a Cookie header] |".to_string()
            };

            let input_widget = Paragraph::new(obfuscated_text)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(input_border))
                        .title(" Ctrl+O: open browser   |   Enter: import / authenticate ")
                        .title_style(Style::default().fg(input_border))
                );
            f.render_widget(input_widget, login_layout[1]);

            // Status message
            let status_widget = Paragraph::new(format!("  STATUS: {}", app.login_status_msg))
                .style(Style::default().fg(alert_color).add_modifier(Modifier::BOLD));
            f.render_widget(status_widget, login_layout[2]);
        }
    }

    // -------------------------------------------------------------
    // BOTTOM SECTION: AUDIO PLAYER PANEL & CONTROLS
    // -------------------------------------------------------------
    let player_border_color = active_color;
    let player_block = Block::default()
        .title(" ACTIVE AUDIO PLAYER ")
        .title_style(Style::default().fg(player_border_color).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(player_border_color));

    let p = app.player.lock().unwrap();
    let current_track = p.current_track.clone();
    let is_paused = p.is_paused();
    let elapsed = p.elapsed_seconds();
    let is_loading = p.is_loading;
    let volume_level = p.volume();
    drop(p); // drop lock quickly

    let player_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Song details
            Constraint::Length(1), // Progress bar
            Constraint::Length(2), // Shortcut keys
        ])
        .split(player_block.inner(chunks[2]));

    // 1. Song Details line
    let (track_title, track_artist, duration_str) = match current_track {
        Some(t) => (t.title, t.artist, t.duration),
        None => ("No track playing".to_string(), "".to_string(), "0:00".to_string()),
    };

    let play_state_label = if is_loading {
        Span::styled("⚡ Loading... ", Style::default().fg(alert_color).add_modifier(Modifier::BOLD))
    } else if is_paused {
        Span::styled("⏸ Paused ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
    } else {
        Span::styled("▶ Playing ", Style::default().fg(success_color).add_modifier(Modifier::BOLD))
    };

    let details_text = Line::from(vec![
        play_state_label,
        Span::styled(format!(" \"{}\" ", track_title), Style::default().fg(active_color).add_modifier(Modifier::BOLD)),
        Span::raw("by "),
        Span::styled(format!(" {} ", track_artist), Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD)),
    ]);
    f.render_widget(Paragraph::new(details_text), player_layout[0]);

    // 2. Progress Bar
    // Convert duration e.g. "3:45" or "07:15" to seconds
    let total_seconds = parse_duration_to_seconds(&duration_str);
    
    // Elapsed time formatted
    let elapsed_str = format_seconds_to_duration(elapsed);
    
    // Calculate progress ratio
    let ratio = if total_seconds > 0 {
        (elapsed as f64 / total_seconds as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let progress_widget = Gauge::default()
        .gauge_style(Style::default().fg(active_color).bg(Color::Rgb(30, 30, 30)))
        .label(format!(" {} / {} ", elapsed_str, duration_str))
        .ratio(ratio);
    f.render_widget(progress_widget, player_layout[1]);

    // 3. Shortcuts & Volume Control
    // Highlight the repeat label when looping is active so it stands out.
    let repeat_style = if app.loop_mode == crate::app::LoopMode::Off {
        Style::default().fg(Color::Gray)
    } else {
        Style::default().fg(success_color).add_modifier(Modifier::BOLD)
    };
    let shortcuts_text = Line::from(vec![
        Span::styled("[Space] ", Style::default().fg(active_color).add_modifier(Modifier::BOLD)),
        Span::raw("Play/Pause  "),
        Span::styled("[n] ", Style::default().fg(active_color).add_modifier(Modifier::BOLD)),
        Span::raw("Next  "),
        Span::styled("[p] ", Style::default().fg(active_color).add_modifier(Modifier::BOLD)),
        Span::raw("Prev  "),
        Span::styled("[←/→] ", Style::default().fg(active_color).add_modifier(Modifier::BOLD)),
        Span::raw("Seek ±10s  "),
        Span::styled("[↑/↓] ", Style::default().fg(active_color).add_modifier(Modifier::BOLD)),
        Span::raw(format!("Volume ({:.0}%)  ", volume_level * 100.0)),
        Span::styled("[r] ", Style::default().fg(active_color).add_modifier(Modifier::BOLD)),
        Span::styled(format!("Repeat: {}  ", app.loop_mode.label()), repeat_style),
        Span::styled("[R] ", Style::default().fg(active_color).add_modifier(Modifier::BOLD)),
        Span::raw("Radio  "),
        Span::styled("[q] ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        Span::raw("Quit"),
    ]);
    f.render_widget(Paragraph::new(shortcuts_text).alignment(Alignment::Center), player_layout[2]);

    // Render the outer player block
    f.render_widget(player_block, chunks[2]);
}

// -----------------------------------------------------------------
// Helper Functions for Duration conversions
// -----------------------------------------------------------------
fn parse_duration_to_seconds(duration: &str) -> u64 {
    let parts: Vec<&str> = duration.trim().split(':').collect();
    if parts.len() == 2 {
        // MM:SS
        let mins = parts[0].parse::<u64>().unwrap_or(0);
        let secs = parts[1].parse::<u64>().unwrap_or(0);
        mins * 60 + secs
    } else if parts.len() == 3 {
        // HH:MM:SS
        let hrs = parts[0].parse::<u64>().unwrap_or(0);
        let mins = parts[1].parse::<u64>().unwrap_or(0);
        let secs = parts[2].parse::<u64>().unwrap_or(0);
        hrs * 3600 + mins * 60 + secs
    } else {
        0
    }
}

fn format_seconds_to_duration(seconds: u64) -> String {
    let hrs = seconds / 3600;
    let mins = (seconds % 3600) / 60;
    let secs = seconds % 60;
    
    if hrs > 0 {
        format!("{}:{:02}:{:02}", hrs, mins, secs)
    } else {
        format!("{}:{:02}", mins, secs)
    }
}
