use crate::audio::{extract_stream_url, AudioPlayer};
use crate::auth::{delete_cookie, save_cookie};
use crate::yt::{Playlist, Track, YtClient};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Search = 0,
    Library = 1,
    Playlists = 2,
    History = 3,
    Radio = 4,
    Login = 5,
}

impl Tab {
    pub fn all() -> &'static [Tab] {
        &[
            Tab::Search,
            Tab::Library,
            Tab::Playlists,
            Tab::History,
            Tab::Radio,
            Tab::Login,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Tab::Search => "Search",
            Tab::Library => "Library",
            Tab::Playlists => "Playlists",
            Tab::History => "History",
            Tab::Radio => "Radio",
            Tab::Login => "Account",
        }
    }

    /// A single-width glyph shown next to the tab label in the sidebar.
    pub fn icon(&self) -> &'static str {
        match self {
            Tab::Search => "/",
            Tab::Library => "♥",
            Tab::Playlists => "≡",
            Tab::History => "↺",
            Tab::Radio => "∿",
            Tab::Login => "⚿",
        }
    }
}

/// Repeat/loop behaviour applied when a track finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopMode {
    Off,
    Queue,
    One,
}

impl LoopMode {
    fn next(self) -> Self {
        match self {
            LoopMode::Off => LoopMode::Queue,
            LoopMode::Queue => LoopMode::One,
            LoopMode::One => LoopMode::Off,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            LoopMode::Off => "Off",
            LoopMode::Queue => "All",
            LoopMode::One => "One",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Main,
    SearchInput,
    LoginInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchType {
    Songs,
    Playlists,
}

pub struct App {
    pub client: Option<YtClient>,
    pub player: Arc<Mutex<AudioPlayer>>,
    client_init_rx: Option<UnboundedReceiver<YtClient>>,
    auth_refresh_rx: Option<UnboundedReceiver<AuthDataRefresh>>,

    // UI Layout state
    pub active_tab: Tab,
    pub focus: Focus,
    pub selected_index: usize,

    // Search tab state
    pub search_input: String,
    pub search_type: SearchType,
    pub searched_songs: Vec<Track>,
    pub searched_playlists: Vec<Playlist>,

    // Library tab state
    pub library_songs: Vec<Track>,

    // Playlists tab state
    pub library_playlists: Vec<Playlist>,
    pub playlist_tracks: Vec<Track>,
    pub active_playlist_id: Option<String>,
    pub active_playlist_title: Option<String>,

    // History tab state
    pub history_tracks: Vec<Track>,

    // Radio / recommendations tab state
    pub recommendation_tracks: Vec<Track>,
    pub radio_seed_title: Option<String>,

    // Login state
    pub login_input: String,
    pub login_status_msg: String,
    pub cookie_path_str: String,

    // Queue state
    pub queue: Vec<Track>,
    pub queue_index: usize,
    pub loop_mode: LoopMode,

    // Status logs shown in UI
    pub status_message: String,
    pub auth_data_loading: bool,
}

struct AuthDataRefresh {
    playlists: Result<Vec<Playlist>, String>,
    songs: Result<Vec<Track>, String>,
    history: Result<Vec<Track>, String>,
}

impl App {
    pub async fn new(
        cookie_path: Option<PathBuf>,
        stream_handle: rodio::OutputStreamHandle,
        sample_rate: u32,
    ) -> Self {
        let player = Arc::new(Mutex::new(
            AudioPlayer::new(stream_handle, sample_rate).unwrap(),
        ));

        let cookie_path_str = cookie_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let status_message = "Connecting to YouTube Music...".to_string();

        let mut app = Self {
            client: None,
            player,
            client_init_rx: None,
            auth_refresh_rx: None,
            active_tab: Tab::Search,
            focus: Focus::Sidebar,
            selected_index: 0,
            search_input: String::new(),
            search_type: SearchType::Songs,
            searched_songs: Vec::new(),
            searched_playlists: Vec::new(),
            library_songs: Vec::new(),
            library_playlists: Vec::new(),
            playlist_tracks: Vec::new(),
            active_playlist_id: None,
            active_playlist_title: None,
            history_tracks: Vec::new(),
            recommendation_tracks: Vec::new(),
            radio_seed_title: None,
            login_input: String::new(),
            login_status_msg: String::new(),
            cookie_path_str,
            queue: Vec::new(),
            queue_index: 0,
            loop_mode: LoopMode::Off,
            status_message,
            auth_data_loading: false,
        };

        app.start_youtube_client_init(cookie_path);

        app
    }

    pub fn is_authenticated(&self) -> bool {
        self.client
            .as_ref()
            .is_some_and(|client| client.is_authenticated())
    }

    pub fn is_client_ready(&self) -> bool {
        self.client.is_some()
    }

    fn start_youtube_client_init(&mut self, cookie_path: Option<PathBuf>) {
        let (tx, rx) = unbounded_channel();
        self.client_init_rx = Some(rx);

        tokio::spawn(async move {
            let client = YtClient::init(cookie_path.as_deref()).await;
            let _ = tx.send(client);
        });
    }

    /// Starts fetching personal library data in the background.
    pub fn start_authenticated_data_refresh(&mut self) {
        let Some(client) = self.client.as_ref() else {
            return;
        };

        if !client.is_authenticated() {
            return;
        }

        let client = client.clone();
        let (tx, rx) = unbounded_channel();
        self.auth_refresh_rx = Some(rx);
        self.auth_data_loading = true;
        self.status_message = "Refreshing library data...".to_string();

        tokio::spawn(async move {
            let (playlists, songs, history) = tokio::join!(
                client.get_library_playlists(),
                client.get_library_songs(),
                client.get_history(),
            );

            let _ = tx.send(AuthDataRefresh {
                playlists: playlists.map_err(|e| e.to_string()),
                songs: songs.map_err(|e| e.to_string()),
                history: history.map_err(|e| e.to_string()),
            });
        });
    }

    /// Applies completed background work. Returns true when the UI should redraw.
    pub fn poll_background_tasks(&mut self) -> bool {
        let mut changed = false;

        if let Some(rx) = self.client_init_rx.as_mut() {
            match rx.try_recv() {
                Ok(client) => {
                    let is_authenticated = client.is_authenticated();
                    self.client = Some(client);
                    self.client_init_rx = None;
                    self.status_message = if is_authenticated {
                        "Welcome! Logged in with Google account.".to_string()
                    } else {
                        "Running in Guest Mode. Press 'Tab' to go to Login tab to log in."
                            .to_string()
                    };

                    if is_authenticated {
                        self.start_authenticated_data_refresh();
                    }

                    changed = true;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    self.client_init_rx = None;
                    self.status_message = "Failed to initialize YouTube Music client.".to_string();
                    changed = true;
                }
            }
        }

        let Some(rx) = self.auth_refresh_rx.as_mut() else {
            return changed;
        };

        match rx.try_recv() {
            Ok(refresh) => {
                self.auth_refresh_rx = None;
                self.auth_data_loading = false;
                let mut failed = 0;

                match refresh.playlists {
                    Ok(playlists) => self.library_playlists = playlists,
                    Err(_) => failed += 1,
                }
                match refresh.songs {
                    Ok(songs) => self.library_songs = songs,
                    Err(_) => failed += 1,
                }
                match refresh.history {
                    Ok(history) => self.history_tracks = history,
                    Err(_) => failed += 1,
                }

                self.status_message = if failed == 0 {
                    "Library details updated.".to_string()
                } else if failed == 3 {
                    "Failed to refresh library details.".to_string()
                } else {
                    "Library details partially updated.".to_string()
                };
                true
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => changed,
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                self.auth_refresh_rx = None;
                self.auth_data_loading = false;
                self.status_message = "Library refresh stopped.".to_string();
                true
            }
        }
    }

    /// Selects the next item in the current focused panel/list
    pub fn select_next(&mut self) {
        let max_len = self.current_list_len();
        if max_len > 0 {
            if self.selected_index >= max_len - 1 {
                self.selected_index = 0;
            } else {
                self.selected_index += 1;
            }
        }
    }

    /// Selects the previous item in the current focused panel/list
    pub fn select_prev(&mut self) {
        let max_len = self.current_list_len();
        if max_len > 0 {
            if self.selected_index == 0 {
                self.selected_index = max_len - 1;
            } else {
                self.selected_index -= 1;
            }
        }
    }

    /// Gets the length of the currently active list based on focus and tab
    fn current_list_len(&self) -> usize {
        match self.focus {
            Focus::Sidebar => Tab::all().len(),
            Focus::Main => match self.active_tab {
                Tab::Search => match self.search_type {
                    SearchType::Songs => self.searched_songs.len(),
                    SearchType::Playlists => self.searched_playlists.len(),
                },
                Tab::Library => self.library_songs.len(),
                Tab::Playlists => {
                    if self.active_playlist_id.is_some() {
                        self.playlist_tracks.len()
                    } else {
                        self.library_playlists.len()
                    }
                }
                Tab::History => self.history_tracks.len(),
                Tab::Radio => self.recommendation_tracks.len(),
                Tab::Login => 0,
            },
            Focus::SearchInput | Focus::LoginInput => 0,
        }
    }

    /// Plays a track, sets the queue and spawns the load in the background
    pub fn play_track(&mut self, track: Track, queue: Vec<Track>, queue_index: usize) {
        self.queue = queue;
        self.queue_index = queue_index;

        let player_lock = self.player.clone();
        let track_id = track.id.clone();
        let track_clone = track.clone();

        {
            let mut p = player_lock.lock().unwrap();
            p.stop();
            p.current_track = Some(track_clone.clone());
            p.is_loading = true;
        }

        self.status_message = format!("Loading URL for: {}...", track.title);

        // Spawn background task to load the audio URL and start playback
        tokio::spawn(async move {
            match extract_stream_url(&track_id).await {
                Ok(url) => {
                    let mut p = player_lock.lock().unwrap();
                    if p.current_track.as_ref().map(|t| &t.id) == Some(&track_id) {
                        if let Err(e) = p.start_playback(track_clone, url, 0) {
                            eprintln!("Failed to start playback: {}", e);
                            p.is_loading = false;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to load audio stream URL: {}", e);
                    let mut p = player_lock.lock().unwrap();
                    p.is_loading = false;
                }
            }
        });
    }

    /// Advances to the next track in the queue (if any)
    pub fn play_next(&mut self) {
        if self.queue.is_empty() {
            return;
        }
        if self.queue_index + 1 < self.queue.len() {
            let next_index = self.queue_index + 1;
            let next_track = self.queue[next_index].clone();
            self.play_track(next_track, self.queue.clone(), next_index);
        } else {
            // End of queue
            self.status_message = "Reached end of playback queue.".to_string();
        }
    }

    /// Called when the current track finishes on its own. Applies the active
    /// loop mode to decide what (if anything) plays next.
    pub fn on_song_finished(&mut self) {
        match self.loop_mode {
            LoopMode::One => {
                // Replay the current track from the start.
                if let Some(track) = self.queue.get(self.queue_index).cloned() {
                    let index = self.queue_index;
                    self.play_track(track, self.queue.clone(), index);
                }
            }
            LoopMode::Queue => {
                if self.queue.is_empty() {
                    return;
                }
                let next_index = if self.queue_index + 1 < self.queue.len() {
                    self.queue_index + 1
                } else {
                    0 // Wrap around to the start of the queue.
                };
                let next_track = self.queue[next_index].clone();
                self.play_track(next_track, self.queue.clone(), next_index);
            }
            LoopMode::Off => self.play_next(),
        }
    }

    /// Cycles the loop mode: Off -> All -> One -> Off.
    pub fn cycle_loop_mode(&mut self) {
        self.loop_mode = self.loop_mode.next();
        self.status_message = format!("Repeat: {}", self.loop_mode.label());
    }

    /// Backtracks to the previous track in the queue (if any)
    pub fn play_prev(&mut self) {
        if self.queue.is_empty() {
            return;
        }
        if self.queue_index > 0 {
            let prev_index = self.queue_index - 1;
            let prev_track = self.queue[prev_index].clone();
            self.play_track(prev_track, self.queue.clone(), prev_index);
        }
    }

    /// Handles keyboard events
    pub async fn handle_key_event(&mut self, key: KeyEvent) {
        if self.focus != Focus::SearchInput
            && self.focus != Focus::LoginInput
            && Self::is_wasd_audio_key(key)
        {
            self.handle_global_audio_keys(key);
            return;
        }

        match self.focus {
            Focus::Sidebar => self.handle_sidebar_key(key).await,
            Focus::Main => self.handle_main_key(key).await,
            Focus::SearchInput => self.handle_search_input_key(key).await,
            Focus::LoginInput => self.handle_login_input_key(key).await,
        }
    }

    async fn handle_sidebar_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next();
                self.active_tab = Tab::all()[self.selected_index];
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_prev();
                self.active_tab = Tab::all()[self.selected_index];
            }
            KeyCode::Right | KeyCode::Enter | KeyCode::Char('l') => {
                // Shift focus to main panel
                self.focus = Focus::Main;
                self.selected_index = 0;

                // If switching to login tab, focus input automatically
                if self.active_tab == Tab::Login {
                    self.focus = Focus::LoginInput;
                }
            }
            KeyCode::Char('/') | KeyCode::Char('s') => {
                // Shortcut to search input
                self.active_tab = Tab::Search;
                self.selected_index = Tab::Search as usize;
                self.focus = Focus::SearchInput;
            }
            KeyCode::Char('R') => self.start_radio().await,
            _ => self.handle_global_audio_keys(key),
        }
    }

    async fn handle_main_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Left | KeyCode::Esc | KeyCode::Char('h') => {
                // Go back to sidebar
                if self.active_tab == Tab::Playlists && self.active_playlist_id.is_some() {
                    // If in playlist tracks, go back to playlist list
                    self.active_playlist_id = None;
                    self.active_playlist_title = None;
                    self.playlist_tracks.clear();
                    self.selected_index = 0;
                } else {
                    self.focus = Focus::Sidebar;
                    self.selected_index = self.active_tab as usize;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_prev();
            }
            KeyCode::Char('/') | KeyCode::Char('s') if self.active_tab == Tab::Search => {
                // Focus search input
                self.focus = Focus::SearchInput;
            }
            KeyCode::Enter => {
                self.execute_main_selection().await;
            }
            KeyCode::Char('R') => self.start_radio().await,
            KeyCode::Tab => {
                // Toggle search type if on search tab
                if self.active_tab == Tab::Search {
                    self.search_type = match self.search_type {
                        SearchType::Songs => SearchType::Playlists,
                        SearchType::Playlists => SearchType::Songs,
                    };
                    self.selected_index = 0;
                    self.status_message =
                        format!("Switched search filter to: {:?}", self.search_type);
                }
            }
            _ => self.handle_global_audio_keys(key),
        }
    }

    async fn execute_main_selection(&mut self) {
        if self.current_list_len() == 0 {
            return;
        }

        match self.active_tab {
            Tab::Search => match self.search_type {
                SearchType::Songs => {
                    let track = self.searched_songs[self.selected_index].clone();
                    self.play_track(track, self.searched_songs.clone(), self.selected_index);
                }
                SearchType::Playlists => {
                    let playlist = self.searched_playlists[self.selected_index].clone();
                    self.load_playlist_tracks(&playlist.id, &playlist.title)
                        .await;
                }
            },
            Tab::Library => {
                let track = self.library_songs[self.selected_index].clone();
                self.play_track(track, self.library_songs.clone(), self.selected_index);
            }
            Tab::Playlists => {
                if self.active_playlist_id.is_some() {
                    // Play track
                    let track = self.playlist_tracks[self.selected_index].clone();
                    self.play_track(track, self.playlist_tracks.clone(), self.selected_index);
                } else {
                    // Open playlist
                    let playlist = self.library_playlists[self.selected_index].clone();
                    self.load_playlist_tracks(&playlist.id, &playlist.title)
                        .await;
                }
            }
            Tab::History => {
                let track = self.history_tracks[self.selected_index].clone();
                self.play_track(track, self.history_tracks.clone(), self.selected_index);
            }
            Tab::Radio => {
                let track = self.recommendation_tracks[self.selected_index].clone();
                self.play_track(
                    track,
                    self.recommendation_tracks.clone(),
                    self.selected_index,
                );
            }
            Tab::Login => {}
        }
    }

    /// Returns the track currently highlighted in the focused main list, if any.
    fn current_selected_track(&self) -> Option<Track> {
        if self.focus != Focus::Main {
            return None;
        }
        match self.active_tab {
            Tab::Search => match self.search_type {
                SearchType::Songs => self.searched_songs.get(self.selected_index).cloned(),
                SearchType::Playlists => None,
            },
            Tab::Library => self.library_songs.get(self.selected_index).cloned(),
            Tab::Playlists => {
                if self.active_playlist_id.is_some() {
                    self.playlist_tracks.get(self.selected_index).cloned()
                } else {
                    None
                }
            }
            Tab::History => self.history_tracks.get(self.selected_index).cloned(),
            Tab::Radio => self.recommendation_tracks.get(self.selected_index).cloned(),
            Tab::Login => None,
        }
    }

    /// Starts a "radio" of recommended tracks. The seed is the highlighted song
    /// (if a list is focused), otherwise the track currently playing. The result
    /// populates the Radio tab and begins playing through the recommendations.
    async fn start_radio(&mut self) {
        let seed = self
            .current_selected_track()
            .or_else(|| self.player.lock().unwrap().current_track.clone());

        let seed = match seed {
            Some(t) => t,
            None => {
                self.status_message = "Select or play a song first to start a radio.".to_string();
                return;
            }
        };

        self.status_message = format!("Building radio from: {}...", seed.title);
        let Some(client) = self.client.as_ref() else {
            self.status_message = "Still connecting to YouTube Music...".to_string();
            return;
        };

        match client.get_watch_playlist(&seed.id).await {
            Ok(tracks) if !tracks.is_empty() => {
                self.recommendation_tracks = tracks;
                self.radio_seed_title = Some(seed.title.clone());
                self.active_tab = Tab::Radio;
                self.focus = Focus::Main;
                self.selected_index = 0;
                self.status_message = format!(
                    "Radio: {} recommendations.",
                    self.recommendation_tracks.len()
                );

                let first = self.recommendation_tracks[0].clone();
                self.play_track(first, self.recommendation_tracks.clone(), 0);
            }
            Ok(_) => {
                self.status_message = "No recommendations found for this track.".to_string();
            }
            Err(e) => {
                self.status_message = format!("Failed to load radio: {}", e);
            }
        }
    }

    async fn load_playlist_tracks(&mut self, playlist_id: &str, playlist_title: &str) {
        self.status_message = format!("Loading tracks for: {}...", playlist_title);
        let Some(client) = self.client.as_ref() else {
            self.status_message = "Still connecting to YouTube Music...".to_string();
            return;
        };

        match client.get_playlist_tracks(playlist_id).await {
            Ok(tracks) => {
                self.playlist_tracks = tracks;
                self.active_playlist_id = Some(playlist_id.to_string());
                self.active_playlist_title = Some(playlist_title.to_string());
                self.selected_index = 0;
                self.focus = Focus::Main;
                self.status_message = format!("Loaded {} tracks.", self.playlist_tracks.len());
            }
            Err(e) => {
                self.status_message = format!("Failed to load playlist: {}", e);
            }
        }
    }

    async fn handle_search_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.focus = Focus::Main;
            }
            KeyCode::Enter => {
                if !self.search_input.trim().is_empty() {
                    let query = self.search_input.clone();
                    self.status_message = format!("Searching for '{}'...", query);

                    let Some(client) = self.client.as_ref() else {
                        self.status_message = "Still connecting to YouTube Music...".to_string();
                        return;
                    };

                    // Search in songs
                    if let Ok(songs) = client.search_songs(&query).await {
                        self.searched_songs = songs;
                    }

                    // Search in playlists
                    if let Ok(playlists) = client.search_playlists(&query).await {
                        self.searched_playlists = playlists;
                    }

                    self.status_message = "Search results loaded.".to_string();
                    self.focus = Focus::Main;
                    self.selected_index = 0;
                }
            }
            KeyCode::Char(c) => {
                self.search_input.push(c);
            }
            KeyCode::Backspace => {
                self.search_input.pop();
            }
            _ => {}
        }
    }

    /// Imports the YouTube Music session straight from the user's browser,
    /// then validates and activates it. This is the primary login path.
    async fn login_with_browser(&mut self) {
        self.login_status_msg = "Reading session from your browser...".to_string();
        match crate::auth::extract_browser_cookies() {
            Ok(cookie) => self.apply_cookie(cookie).await,
            Err(e) => self.login_status_msg = e,
        }
    }

    /// Saves the given cookie header, verifies it against YouTube Music, and on
    /// success swaps in the authenticated client and loads the user's library.
    async fn apply_cookie(&mut self, cookie: String) {
        self.login_status_msg = "Verifying session...".to_string();

        match save_cookie(&cookie) {
            Ok(path) => {
                // Attempt to load and verify directly
                match ytmapi_rs::YtMusic::from_cookie_file(&path).await {
                    Ok(validated_client) => {
                        self.client = Some(YtClient::Authenticated(validated_client));
                        self.cookie_path_str = path.to_string_lossy().to_string();
                        self.login_status_msg =
                            "Login Successful! Connected to account.".to_string();
                        self.login_input.clear();
                        self.status_message = "Logged in successfully.".to_string();

                        self.start_authenticated_data_refresh();

                        self.focus = Focus::Sidebar;
                        self.selected_index = Tab::Library as usize;
                        self.active_tab = Tab::Library;
                    }
                    Err(e) => {
                        let _ = delete_cookie();
                        // Display the exact verification error
                        self.login_status_msg = format!("Verification Failed: {}", e);
                    }
                }
            }
            Err(e) => {
                self.login_status_msg = format!("Failed to save cookie: {}", e);
            }
        }
    }

    async fn handle_login_input_key(&mut self, key: KeyEvent) {
        // Ctrl+O: open the browser at music.youtube.com so the user can sign in.
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('o') | KeyCode::Char('O'))
        {
            match crate::auth::open_browser_login() {
                Ok(_) => {
                    self.login_status_msg =
                        "Opened music.youtube.com. Sign in there, then press Enter to import."
                            .to_string();
                }
                Err(e) => self.login_status_msg = format!("Could not open browser: {}", e),
            }
            return;
        }

        match key.code {
            KeyCode::Esc => {
                self.focus = Focus::Sidebar;
                self.selected_index = Tab::Login as usize;
            }
            KeyCode::Enter => {
                let cookie = self.login_input.trim().to_string();
                if cookie.is_empty() {
                    // Primary path: import the session from the browser automatically.
                    self.login_with_browser().await;
                } else {
                    // Fallback: a manually pasted Cookie header.
                    self.apply_cookie(cookie).await;
                }
            }
            KeyCode::Char(c) => {
                self.login_input.push(c);
            }
            KeyCode::Backspace => {
                self.login_input.pop();
            }
            _ => {}
        }
    }

    /// Applies a playback command from the OS media controls (media keys,
    /// AirPods, Control Center). Mirrors the in-app keyboard shortcuts.
    pub fn handle_media_command(&mut self, cmd: crate::media::MediaCommand) {
        use crate::media::MediaCommand;
        match cmd {
            MediaCommand::Play => self.player.lock().unwrap().resume(),
            MediaCommand::Pause => self.player.lock().unwrap().pause(),
            MediaCommand::Toggle => {
                let mut p = self.player.lock().unwrap();
                if p.is_paused() {
                    p.resume();
                } else {
                    p.pause();
                }
            }
            MediaCommand::Next => self.play_next(),
            MediaCommand::Previous => self.play_prev(),
            MediaCommand::Stop => self.player.lock().unwrap().stop(),
        }
    }

    /// Handles global audio playback shortcut keys across any focus mode (except inputs)
    fn handle_global_audio_keys(&mut self, key: KeyEvent) {
        let mut p = self.player.lock().unwrap();
        match key.code {
            KeyCode::Char(' ') => {
                if p.is_paused() {
                    p.resume();
                    self.status_message = "Playback resumed.".to_string();
                } else {
                    p.pause();
                    self.status_message = "Playback paused.".to_string();
                }
            }
            KeyCode::Char('n') | KeyCode::Char('>') => {
                // Drop lock before calling play_next, which locks again
                drop(p);
                self.play_next();
            }
            KeyCode::Char('p') | KeyCode::Char('<') => {
                drop(p);
                self.play_prev();
            }
            KeyCode::Char('r') => {
                drop(p);
                self.cycle_loop_mode();
            }
            KeyCode::Right => {
                // Seek forward 10 seconds
                let elapsed = p.elapsed_seconds();
                p.seek(elapsed + 10);
                self.status_message = format!("Seek forward: {}s", elapsed + 10);
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                // Seek forward 10 seconds
                let elapsed = p.elapsed_seconds();
                p.seek(elapsed + 10);
                self.status_message = format!("Seek forward: {}s", elapsed + 10);
            }
            KeyCode::Left => {
                // Seek backward 10 seconds
                let elapsed = p.elapsed_seconds();
                let new_pos = elapsed.saturating_sub(10);
                p.seek(new_pos);
                self.status_message = format!("Seek backward: {}s", new_pos);
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                // Seek backward 10 seconds
                let elapsed = p.elapsed_seconds();
                let new_pos = elapsed.saturating_sub(10);
                p.seek(new_pos);
                self.status_message = format!("Seek backward: {}s", new_pos);
            }
            KeyCode::Up => {
                // Volume up
                let vol = p.volume();
                let new_vol = (vol + 0.05).min(1.0);
                p.set_volume(new_vol);
                self.status_message = format!("Volume: {:.0}%", new_vol * 100.0);
            }
            KeyCode::Char('w') | KeyCode::Char('W') => {
                // Volume up
                let vol = p.volume();
                let new_vol = (vol + 0.05).min(1.0);
                p.set_volume(new_vol);
                self.status_message = format!("Volume: {:.0}%", new_vol * 100.0);
            }
            KeyCode::Down => {
                // Volume down
                let vol = p.volume();
                let new_vol = (vol - 0.05).max(0.0);
                p.set_volume(new_vol);
                self.status_message = format!("Volume: {:.0}%", new_vol * 100.0);
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                // Volume down
                let vol = p.volume();
                let new_vol = (vol - 0.05).max(0.0);
                p.set_volume(new_vol);
                self.status_message = format!("Volume: {:.0}%", new_vol * 100.0);
            }
            KeyCode::Char('q') => {
                // Stop audio before exit
                p.stop();
            }
            _ => {}
        }
    }

    fn is_wasd_audio_key(key: KeyEvent) -> bool {
        let plain_key = key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT;
        plain_key
            && matches!(
                key.code,
                KeyCode::Char('w')
                    | KeyCode::Char('W')
                    | KeyCode::Char('a')
                    | KeyCode::Char('A')
                    | KeyCode::Char('s')
                    | KeyCode::Char('S')
                    | KeyCode::Char('d')
                    | KeyCode::Char('D')
            )
    }
}
