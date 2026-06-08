mod auth;
mod yt;
mod audio;
mod app;
mod ui;
mod media;

use std::io;
use std::time::Duration;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use app::{App, Focus};
use auth::get_cookie_path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting YTM-TUI Player...");

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Initialize Rodio OutputStream at the main thread level to keep it alive
    let (_stream, stream_handle) = rodio::OutputStream::try_default()?;

    // Load credentials and initialize application state
    let cookie_path = get_cookie_path();
    let mut app = App::new(cookie_path, stream_handle).await;

    // macOS Now Playing / remote media controls. `None` on other platforms or
    // if registration fails; the app keeps working without it either way.
    let mut media_session = media::MediaSession::new();

    // Main event loop
    let mut last_tick = std::time::Instant::now();
    let tick_rate = Duration::from_millis(200);

    loop {
        // Draw TUI
        terminal.draw(|f| ui::render(f, &mut app))?;

        // Check for auto-advancing queue when current song finishes
        let is_song_finished = {
            let p = app.player.lock().unwrap();
            // Call the public helper is_empty() method
            p.is_empty() && p.current_track.is_some() && !p.is_loading && !p.is_paused()
        };

        if is_song_finished {
            app.on_song_finished();
        }

        // Service the OS media controls: pump the run loop so remote-command
        // callbacks fire, apply any queued commands, then publish the current
        // playback state to Now Playing / Control Center.
        if let Some(session) = media_session.as_mut() {
            session.pump();
            for cmd in session.poll() {
                app.handle_media_command(cmd);
            }
            let snapshot = {
                let p = app.player.lock().unwrap();
                media::NowPlaying {
                    track: p.current_track.clone(),
                    is_loading: p.is_loading,
                    is_paused: p.is_paused(),
                    elapsed: p.elapsed_seconds(),
                }
            };
            session.update(&snapshot);
        }

        // Poll for crossterm input events
        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                // Global quit command: Char('q') when not in text input fields
                if key.code == event::KeyCode::Char('q') 
                    && app.focus != Focus::SearchInput 
                    && app.focus != Focus::LoginInput 
                {
                    break;
                }
                
                app.handle_key_event(key).await;
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = std::time::Instant::now();
        }
    }

    // Clean up playback and kill any active ffmpeg children
    {
        let mut p = app.player.lock().unwrap();
        p.stop();
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    println!("YTM-TUI Player shut down cleanly. Goodbye!");
    Ok(())
}
