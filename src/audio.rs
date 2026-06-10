use crate::yt::Track;
use rodio::{OutputStreamHandle, Sink, Source};
use std::io::Read;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, sync_channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const CHANNELS: u16 = 2;

/// Number of bytes read from ffmpeg per chunk on the decode thread (~0.35s of
/// stereo audio at 48kHz). Larger chunks mean fewer channel hand-offs and
/// allocations per second; the buffer reuse below keeps allocation near zero.
const CHUNK_BYTES: usize = 64 * 1024;
/// How many decoded chunks may sit in the buffer before the decode thread
/// blocks waiting for the audio thread to drain it. ~6s of read-ahead, which
/// is plenty to ride out network jitter without the audio thread ever blocking.
const BUFFER_CHUNKS: usize = 64;

/// Spawns a dedicated thread that reads raw PCM from ffmpeg's stdout and hands
/// it to the audio thread through a bounded channel. All blocking I/O (network
/// stalls, reconnects, range seeks) happens here, never on rodio's real-time
/// audio thread.
///
/// Returns the data receiver for [`PcmSource`] plus a recycle sender: the audio
/// thread returns drained sample buffers through it so the decode thread can
/// refill them instead of allocating a fresh `Vec` for every chunk.
fn spawn_decode_thread(mut stdout: ChildStdout) -> (Receiver<Vec<i16>>, Sender<Vec<i16>>) {
    let (tx, rx) = sync_channel::<Vec<i16>>(BUFFER_CHUNKS);
    let (recycle_tx, recycle_rx) = channel::<Vec<i16>>();
    thread::spawn(move || {
        let mut buf = [0u8; CHUNK_BYTES];
        // Carries a single byte across reads when a chunk boundary splits a
        // 16-bit sample (ffmpeg emits little-endian s16le).
        let mut pending: Option<u8> = None;
        loop {
            match stdout.read(&mut buf) {
                Ok(0) => break, // EOF: ffmpeg exited / stream finished.
                Ok(n) => {
                    // Reuse a buffer the audio thread handed back, else allocate.
                    let mut samples = recycle_rx.try_recv().unwrap_or_default();
                    samples.clear();
                    samples.reserve(n / 2 + 1);
                    let mut start = 0;
                    if let Some(lo) = pending.take() {
                        samples.push(i16::from_le_bytes([lo, buf[0]]));
                        start = 1;
                    }
                    let rest = &buf[start..n];
                    let pairs = rest.len() / 2;
                    for k in 0..pairs {
                        samples.push(i16::from_le_bytes([rest[k * 2], rest[k * 2 + 1]]));
                    }
                    if rest.len() % 2 == 1 {
                        pending = Some(rest[rest.len() - 1]);
                    }
                    if samples.is_empty() {
                        continue;
                    }
                    // Errors only if the receiver (PcmSource) was dropped, i.e.
                    // playback moved on. Stop reading and let ffmpeg be killed.
                    if tx.send(samples).is_err() {
                        break;
                    }
                }
                Err(_) => break, // Read error (e.g. process killed).
            }
        }
    });
    (rx, recycle_tx)
}

/// A custom rodio Source that pulls decoded PCM samples from the decode thread.
///
/// `next()` never blocks: when the buffer underruns (ffmpeg is connecting,
/// seeking, or stalled) it emits silence instead of freezing the audio thread,
/// and only reports end-of-stream once the decode thread has truly finished.
pub struct PcmSource {
    rx: Receiver<Vec<i16>>,
    recycle_tx: Sender<Vec<i16>>,
    current: Vec<i16>,
    pos: usize,
    channels: u16,
    sample_rate: u32,
    samples_read: Arc<AtomicU64>,
}

impl Iterator for PcmSource {
    type Item = i16;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.pos < self.current.len() {
                let s = self.current[self.pos];
                self.pos += 1;
                self.samples_read.fetch_add(1, Ordering::Relaxed);
                return Some(s);
            }
            // Hand the drained buffer back to the decode thread for reuse.
            if !self.current.is_empty() {
                let _ = self.recycle_tx.send(std::mem::take(&mut self.current));
                self.pos = 0;
            }
            match self.rx.try_recv() {
                Ok(chunk) => {
                    self.current = chunk;
                    self.pos = 0;
                }
                // Underrun: data hasn't arrived yet. Emit a silent sample so the
                // audio thread keeps spinning instead of blocking on ffmpeg.
                Err(TryRecvError::Empty) => return Some(0),
                // Decode thread finished (EOF or ffmpeg killed): end the source.
                Err(TryRecvError::Disconnected) => return None,
            }
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

    // Output sample rate. Matched to the audio device so rodio's mixer doesn't
    // have to resample a second time after ffmpeg already resampled the source.
    sample_rate: u32,

    // Loading state
    pub is_loading: bool,
}

impl AudioPlayer {
    pub fn new(
        stream_handle: OutputStreamHandle,
        sample_rate: u32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let sink = Sink::try_new(&stream_handle)?;

        Ok(Self {
            stream_handle,
            sink,
            ffmpeg_child: None,
            current_track: None,
            current_url: None,
            samples_read: Arc::new(AtomicU64::new(0)),
            volume: 1.0,
            sample_rate,
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
        total_samples / (CHANNELS as u64 * self.sample_rate as u64)
    }

    /// Starts playback from a specific URL at the given start offset in seconds
    pub fn start_playback(
        &mut self,
        track: Track,
        url: String,
        start_seconds: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.stop();

        let mut ffmpeg_cmd = Command::new("ffmpeg");

        // Pass -ss BEFORE -i for fast input seeking over HTTP.
        //
        // Reconnect on transient network errors, but deliberately NOT at EOF:
        // `-reconnect_at_eof` makes ffmpeg treat the natural end of a finite
        // track as a dropped connection and hang retrying, so songs never end.
        ffmpeg_cmd.args([
            "-reconnect",
            "1",
            "-reconnect_streamed",
            "1",
            "-reconnect_delay_max",
            "5",
            "-reconnect_on_network_error",
            "1",
            "-ss",
            &start_seconds.to_string(),
            "-i",
            &url,
            "-f",
            "s16le",
            "-ac",
            &CHANNELS.to_string(),
            "-ar",
            &self.sample_rate.to_string(),
            "-",
        ]);

        let mut child = ffmpeg_cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdout = child.stdout.take().ok_or("Failed to take ffmpeg stdout")?;

        // Set initial sample count
        let initial_samples = start_seconds * CHANNELS as u64 * self.sample_rate as u64;
        self.samples_read.store(initial_samples, Ordering::Relaxed);

        // Hand ffmpeg's stdout to a decode thread so the audio thread only ever
        // pulls from an in-memory buffer and never blocks on network I/O.
        let (rx, recycle_tx) = spawn_decode_thread(stdout);
        let source = PcmSource {
            rx,
            recycle_tx,
            current: Vec::new(),
            pos: 0,
            channels: CHANNELS,
            sample_rate: self.sample_rate,
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
pub async fn extract_stream_url(
    video_id: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
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
        return Err(format!(
            "yt-dlp failed: {}",
            String::from_utf8_lossy(&stdout_bytes.stderr)
        )
        .into());
    }

    let url_str = String::from_utf8(stdout_bytes.stdout)?.trim().to_string();
    Ok(url_str)
}
