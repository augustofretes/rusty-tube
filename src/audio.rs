use std::io::Read;
use std::process::{Command, Stdio, ChildStdout, Child};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use rodio::{OutputStreamHandle, Sink, Source};
use crate::yt::Track;

const SAMPLE_RATE: u32 = 44100;
const CHANNELS: u16 = 2;

/// A custom rodio Source that pulls raw 16-bit PCM samples from ffmpeg's stdout.
pub struct PcmSource {
    stdout: ChildStdout,
    channels: u16,
    sample_rate: u32,
    samples_read: Arc<AtomicU64>,
}

impl Iterator for PcmSource {
    type Item = i16;

    fn next(&mut self) -> Option<Self::Item> {
        let mut buf = [0u8; 2];
        match self.stdout.read_exact(&mut buf) {
            Ok(_) => {
                self.samples_read.fetch_add(1, Ordering::Relaxed);
                Some(i16::from_le_bytes(buf))
            }
            Err(_) => None, // EOF or read error (e.g. process killed)
        }
    }
}

impl Source for PcmSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

pub struct AudioPlayer {
    stream_handle: OutputStreamHandle,
    sink: Sink,
    
    // Subprocess state
    ffmpeg_child: Option<Child>,
    
    // Playback state
    pub current_track: Option<Track>,
    current_url: Option<String>,
    pub samples_read: Arc<AtomicU64>,
    volume: f32,
    
    // Loading state
    pub is_loading: bool,
}

impl AudioPlayer {
    pub fn new(stream_handle: OutputStreamHandle) -> Result<Self, Box<dyn std::error::Error>> {
        let sink = Sink::try_new(&stream_handle)?;
        
        Ok(Self {
            stream_handle,
            sink,
            ffmpeg_child: None,
            current_track: None,
            current_url: None,
            samples_read: Arc::new(AtomicU64::new(0)),
            volume: 1.0,
            is_loading: false,
        })
    }

    /// Returns true if the sink has no more audio sources queued
    pub fn is_empty(&self) -> bool {
        self.sink.empty()
    }

    /// Stops current playback and kills ffmpeg process
    pub fn stop(&mut self) {
        self.sink.stop();
        self.stop_ffmpeg();
        self.current_track = None;
        self.current_url = None;
        self.samples_read.store(0, Ordering::Relaxed);
        self.is_loading = false;
        
        // Recreate the sink to ensure it's in a clean state
        if let Ok(new_sink) = Sink::try_new(&self.stream_handle) {
            new_sink.set_volume(self.volume);
            self.sink = new_sink;
        }
    }

    /// Internal function to kill the active ffmpeg process
    fn stop_ffmpeg(&mut self) {
        if let Some(mut child) = self.ffmpeg_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Pauses playback
    pub fn pause(&mut self) {
        self.sink.pause();
    }

    /// Resumes playback
    pub fn resume(&mut self) {
        self.sink.play();
    }

    /// Returns true if the audio is currently paused
    pub fn is_paused(&self) -> bool {
        self.sink.is_paused()
    }

    /// Sets the volume (0.0 to 1.0)
    pub fn set_volume(&mut self, vol: f32) {
        let clamped = vol.clamp(0.0, 1.0);
        self.volume = clamped;
        self.sink.set_volume(clamped);
    }

    /// Gets the current volume
    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Gets elapsed playback time in seconds
    pub fn elapsed_seconds(&self) -> u64 {
        let total_samples = self.samples_read.load(Ordering::Relaxed);
        total_samples / (CHANNELS as u64 * SAMPLE_RATE as u64)
    }

    /// Starts playback from a specific URL at the given start offset in seconds
    pub fn start_playback(&mut self, track: Track, url: String, start_seconds: u64) -> Result<(), Box<dyn std::error::Error>> {
        self.stop();

        let mut ffmpeg_cmd = Command::new("ffmpeg");
        
        // Pass -ss BEFORE -i for fast input seeking over HTTP
        ffmpeg_cmd.args([
            "-reconnect", "1",
            "-reconnect_at_eof", "1",
            "-reconnect_streamed", "1",
            "-reconnect_delay_max", "5",
            "-reconnect_on_network_error", "1",
            "-ss", &start_seconds.to_string(),
            "-i", &url,
            "-f", "s16le",
            "-ac", &CHANNELS.to_string(),
            "-ar", &SAMPLE_RATE.to_string(),
            "-"
        ]);
        
        let mut child = ffmpeg_cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
            
        let stdout = child.stdout.take().ok_or("Failed to take ffmpeg stdout")?;
        
        // Set initial sample count
        let initial_samples = start_seconds * CHANNELS as u64 * SAMPLE_RATE as u64;
        self.samples_read.store(initial_samples, Ordering::Relaxed);
        
        // Create custom source
        let source = PcmSource {
            stdout,
            channels: CHANNELS,
            sample_rate: SAMPLE_RATE,
            samples_read: self.samples_read.clone(),
        };
        
        self.sink.append(source);
        self.sink.play();
        
        self.ffmpeg_child = Some(child);
        self.current_track = Some(track);
        self.current_url = Some(url);
        self.is_loading = false;
        
        Ok(())
    }

    /// Seeks to a specific position in seconds
    pub fn seek(&mut self, seconds: u64) {
        let track = match &self.current_track {
            Some(t) => t.clone(),
            None => return,
        };
        let url = match &self.current_url {
            Some(u) => u.clone(),
            None => return,
        };
        
        // Re-start playback from the new position
        if let Err(e) = self.start_playback(track, url, seconds) {
            eprintln!("Seek error: {}", e);
        }
    }
}

/// Helper function to asynchronously extract stream URL using yt-dlp
pub async fn extract_stream_url(video_id: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let video_url = format!("https://www.youtube.com/watch?v={}", video_id);
    
    // Spawn task to run blocking command
    let video_url_clone = video_url.clone();
    let stdout_bytes = tokio::task::spawn_blocking(move || {
        Command::new("yt-dlp")
            .args(["-f", "140", "-g", &video_url_clone]) // format 140 is M4A (Symphonia/ffmpeg compatible)
            .output()
    })
    .await??;

    if !stdout_bytes.status.success() {
        return Err(format!("yt-dlp failed: {}", String::from_utf8_lossy(&stdout_bytes.stderr)).into());
    }

    let url_str = String::from_utf8(stdout_bytes.stdout)?.trim().to_string();
    Ok(url_str)
}
