//! macOS "Now Playing" integration.
//!
//! Publishes the current track and playback state to Control Center, and
//! receives remote commands (media keys, AirPods, the Control Center widget) —
//! translating them into player actions handled in `app.rs`.
//!
//! The whole thing is event-driven: the only recurring work is draining a small
//! queue on the existing TUI tick, so it costs effectively nothing in steady
//! state. On non-macOS targets every method is a no-op stub.

use crate::yt::Track;

/// A snapshot of what the player is doing, used to refresh Now Playing.
pub struct NowPlaying {
    pub track: Option<Track>,
    pub is_loading: bool,
    pub is_paused: bool,
    pub elapsed: u64,
}

/// A high-level playback command originating from the OS.
pub enum MediaCommand {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Stop,
}

#[cfg(target_os = "macos")]
pub use macos::MediaSession;
#[cfg(not(target_os = "macos"))]
pub use stub::MediaSession;

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::mpsc::{self, Receiver};
    use std::time::Duration;

    use souvlaki::{
        MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition,
        PlatformConfig,
    };

    use super::{MediaCommand, NowPlaying};

    pub struct MediaSession {
        controls: MediaControls,
        events: Receiver<MediaControlEvent>,
        // Change-detection so we only talk to the OS when something changes.
        last_track_id: Option<String>,
        last_paused: Option<bool>,
        last_elapsed: u64,
    }

    impl MediaSession {
        /// Registers with the system. Returns `None` (and the app keeps working
        /// without media controls) if registration fails for any reason.
        ///
        /// Must be called on the main thread — `MPRemoteCommandCenter` requires
        /// it — which is where the TUI loop runs.
        pub fn new() -> Option<Self> {
            let config = PlatformConfig {
                dbus_name: "rusty-tube",
                display_name: "Rusty Tube",
                hwnd: None,
            };
            let mut controls = MediaControls::new(config).ok()?;

            // The handler must be `Send + 'static`, so bridge into the main
            // thread through a channel that we drain each tick.
            let (tx, rx) = mpsc::channel();
            controls
                .attach(move |event| {
                    let _ = tx.send(event);
                })
                .ok()?;

            Some(Self {
                controls,
                events: rx,
                last_track_id: None,
                last_paused: None,
                last_elapsed: 0,
            })
        }

        /// Processes any pending main-thread run-loop sources without blocking,
        /// so MediaPlayer remote-command callbacks fire. Returns immediately
        /// when the queue is empty.
        pub fn pump(&self) {
            runloop::pump();
        }

        /// Drains queued remote commands, mapping them to player actions.
        pub fn poll(&self) -> Vec<MediaCommand> {
            self.events
                .try_iter()
                .filter_map(|event| match event {
                    MediaControlEvent::Play => Some(MediaCommand::Play),
                    MediaControlEvent::Pause => Some(MediaCommand::Pause),
                    MediaControlEvent::Toggle => Some(MediaCommand::Toggle),
                    MediaControlEvent::Next => Some(MediaCommand::Next),
                    MediaControlEvent::Previous => Some(MediaCommand::Previous),
                    MediaControlEvent::Stop => Some(MediaCommand::Stop),
                    _ => None,
                })
                .collect()
        }

        /// Refreshes Now Playing from the latest player snapshot. Metadata is
        /// pushed only when the track changes; playback state only when the
        /// pause state flips or the position jumps (a seek), so the Control
        /// Center scrubber stays accurate without per-second updates.
        pub fn update(&mut self, state: &NowPlaying) {
            let current_id = state.track.as_ref().map(|t| t.id.clone());

            if current_id != self.last_track_id {
                if let Some(track) = &state.track {
                    let _ = self.controls.set_metadata(MediaMetadata {
                        title: Some(&track.title),
                        artist: Some(&track.artist),
                        album: None,
                        cover_url: None,
                        duration: parse_duration(&track.duration),
                    });
                }
                self.last_track_id = current_id;
                // Force a playback re-sync below on the new track.
                self.last_paused = None;
            }

            // While loading there's no audio yet, so treat it as paused.
            let paused = state.track.is_none() || state.is_loading || state.is_paused;

            // A position jump means a seek (or a restart): normal playback only
            // advances `elapsed` by at most 1 second between ticks.
            let jumped = !paused
                && (state.elapsed < self.last_elapsed || state.elapsed > self.last_elapsed + 1);

            if self.last_paused != Some(paused) || jumped {
                let progress = Some(MediaPosition(Duration::from_secs(state.elapsed)));
                let playback = if state.track.is_none() {
                    MediaPlayback::Stopped
                } else if paused {
                    MediaPlayback::Paused { progress }
                } else {
                    MediaPlayback::Playing { progress }
                };
                let _ = self.controls.set_playback(playback);
                self.last_paused = Some(paused);
            }

            self.last_elapsed = state.elapsed;
        }
    }

    /// Parses a `"M:SS"` / `"H:MM:SS"` duration string into a `Duration`.
    fn parse_duration(s: &str) -> Option<Duration> {
        let mut secs: u64 = 0;
        for part in s.split(':') {
            secs = secs * 60 + part.trim().parse::<u64>().ok()?;
        }
        (secs > 0).then(|| Duration::from_secs(secs))
    }

    /// Minimal FFI to pump the Core Foundation run loop. MediaPlayer delivers
    /// remote-command callbacks via the main-thread run loop, which our TUI
    /// loop doesn't otherwise run, so we drain it briefly each tick.
    mod runloop {
        use std::os::raw::c_void;

        type CFStringRef = *const c_void;

        #[link(name = "CoreFoundation", kind = "framework")]
        extern "C" {
            static kCFRunLoopDefaultMode: CFStringRef;
            fn CFRunLoopRunInMode(
                mode: CFStringRef,
                seconds: f64,
                return_after_source_handled: u8,
            ) -> i32;
        }

        const KCF_RUN_LOOP_RUN_HANDLED_SOURCE: i32 = 4;

        pub fn pump() {
            // Non-blocking (0s timeout); loop until no more sources are pending.
            unsafe {
                while CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.0, 1)
                    == KCF_RUN_LOOP_RUN_HANDLED_SOURCE
                {}
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod stub {
    use super::{MediaCommand, NowPlaying};

    pub struct MediaSession;

    impl MediaSession {
        pub fn new() -> Option<Self> {
            None
        }
        pub fn pump(&self) {}
        pub fn poll(&self) -> Vec<MediaCommand> {
            Vec::new()
        }
        pub fn update(&mut self, _state: &NowPlaying) {}
    }
}
