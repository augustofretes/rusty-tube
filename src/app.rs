use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::audio::{AudioPlayer, extract_stream_url};
use crate::yt::{Track, Playlist, YtClient};
use crate::auth::{save_cookie, delete_cookie};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Search = 0,
    Library = 1,
    Playlists = 2,
    History = 3,
    Login = 4,
}

impl Tab {
    pub fn all() -> &'static [Tab] {
        &[Tab::Search, Tab::Library, Tab::Playlists, Tab::History, Tab::Login]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Tab::Search => "🔍 Search",
            Tab::Library => "❤️ Library (Liked)",
            Tab::Playlists => "📁 Playlists",
            Tab::History => "🕒 History",
            Tab::Login => "🔑 Account Login",
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
    pub client: YtClient,
    pub player: Arc<Mutex<AudioPlayer>>,
    
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
    
    // Login state
    pub login_input: String,
    pub login_status_msg: String,
    pub cookie_path_str: String,
    
    // Queue state
    pub queue: Vec<Track>,
    pub queue_index: usize,
    
    // Status logs shown in UI
    pub status_message: String,
}

impl App {
    pub async fn new(cookie_path: Option<PathBuf>, stream_handle: rodio::OutputStreamHandle) -> Self {
        let client = YtClient::init(cookie_path.as_deref()).await;
        let player = Arc::new(Mutex::new(AudioPlayer::new(stream_handle).unwrap()));
        
        let cookie_path_str = cookie_path
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown".to_string());
            
        let status_message = if client.is_authenticated() {
            "Welcome! Logged in with Google account.".to_string()
        } else {
            "Running in Guest Mode. Press 'Tab' to go to Login tab to log in.".to_string()
        };

        let mut app = Self {
            client,
            player,
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
            login_input: String::new(),
            login_status_msg: String::new(),
            cookie_path_str,
            queue: Vec::new(),
            queue_index: 0,
            status_message,
        };

        // Pre-fetch some library details if authenticated
        if app.client.is_authenticated() {
            app.refresh_authenticated_data().await;
        }

        app
    }

    /// Fetches all personal library playlists/songs in the background
    pub async fn refresh_authenticated_data(&mut self) {
        self.status_message = "Refreshing library data...".to_string();
        if let Ok(playlists) = self.client.get_library_playlists().await {
            self.library_playlists = playlists;
        }
        if let Ok(songs) = self.client.get_library_songs().await {
            self.library_songs = songs;
        }
        if let Ok(history) = self.client.get_history().await {
            self.history_tracks = history;
        }
        self.status_message = "Library details updated.".to_string();
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
            KeyCode::Tab => {
                // Toggle search type if on search tab
                if self.active_tab == Tab::Search {
                    self.search_type = match self.search_type {
                        SearchType::Songs => SearchType::Playlists,
                        SearchType::Playlists => SearchType::Songs,
                    };
                    self.selected_index = 0;
                    self.status_message = format!("Switched search filter to: {:?}", self.search_type);
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
                    self.load_playlist_tracks(&playlist.id, &playlist.title).await;
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
                    self.load_playlist_tracks(&playlist.id, &playlist.title).await;
                }
            }
            Tab::History => {
                let track = self.history_tracks[self.selected_index].clone();
                self.play_track(track, self.history_tracks.clone(), self.selected_index);
            }
            Tab::Login => {}
        }
    }

    async fn load_playlist_tracks(&mut self, playlist_id: &str, playlist_title: &str) {
        self.status_message = format!("Loading tracks for: {}...", playlist_title);
        match self.client.get_playlist_tracks(playlist_id).await {
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
                    
                    // Search in songs
                    if let Ok(songs) = self.client.search_songs(&query).await {
                        self.searched_songs = songs;
                    }
                    
                    // Search in playlists
                    if let Ok(playlists) = self.client.search_playlists(&query).await {
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
                        self.client = YtClient::Authenticated(validated_client);
                        self.cookie_path_str = path.to_string_lossy().to_string();
                        self.login_status_msg = "Login Successful! Connected to account.".to_string();
                        self.login_input.clear();
                        self.status_message = "Logged in successfully.".to_string();

                        // Load library data
                        self.refresh_authenticated_data().await;

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
            KeyCode::Right => {
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
            KeyCode::Up => {
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
            KeyCode::Char('q') => {
                // Stop audio before exit
                p.stop();
            }
            _ => {}
        }
    }
}
