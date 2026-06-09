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
    println!("Starting Rusty Tube...");

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Initialize Rodio OutputStream at the main thread level to keep it alive
    let (_stream, stream_handle) = rodio::OutputStream::try_default()?;

    // Query the output device's native sample rate so ffmpeg can decode straight
    // to it and rodio's mixer doesn't resample a second time. Falls back to a
    // safe default if the device can't be probed.
    let device_sample_rate = {
        use rodio::cpal::traits::{DeviceTrait, HostTrait};
        rodio::cpal::default_host()
            .default_output_device()
            .and_then(|d| d.default_output_config().ok())
            .map(|c| c.sample_rate().0)
            .unwrap_or(44100)
    };

    // Load credentials and initialize application state
    let cookie_path = get_cookie_path();
    let mut app = App::new(cookie_path, stream_handle, device_sample_rate).await;

    // macOS Now Playing / remote media controls. `None` on other platforms or
    // if registration fails; the app keeps working without it either way.
    let mut media_session = media::MediaSession::new();

    // Main event loop. The loop spins at `tick_rate` to service media controls
    // and song-finished detection, but the TUI is only redrawn when something
    // actually changed (a key/media event) or once a second to advance the
    // elapsed-time display — rather than every iteration.
    let mut last_tick = std::time::Instant::now();
    let tick_rate = Duration::from_millis(200);
    let redraw_interval = Duration::from_secs(1);
    let mut last_draw = std::time::Instant::now();
    let mut needs_redraw = true;

    loop {
        // Draw TUI only when dirty or the 1Hz progress tick is due.
        if needs_redraw || last_draw.elapsed() >= redraw_interval {
            terminal.draw(|f| ui::render(f, &mut app))?;
            last_draw = std::time::Instant::now();
            needs_redraw = false;
        }

        // Check for auto-advancing queue when current song finishes
        let is_song_finished = {
            let p = app.player.lock().unwrap();
            // Call the public helper is_empty() method
            p.is_empty() && p.current_track.is_some() && !p.is_loading && !p.is_paused()
        };

        if is_song_finished {
            app.on_song_finished();
            needs_redraw = true;
        }

        // Service the OS media controls: pump the run loop so remote-command
        // callbacks fire, apply any queued commands, then publish the current
        // playback state to Now Playing / Control Center.
        if let Some(session) = media_session.as_mut() {
            session.pump();
            for cmd in session.poll() {
                app.handle_media_command(cmd);
                needs_redraw = true;
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
                needs_redraw = true;
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

    println!("Rusty Tube shut down cleanly. Goodbye!");
    Ok(())
}
