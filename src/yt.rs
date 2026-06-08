use std::path::Path;
use ytmapi_rs::YtMusic;
use ytmapi_rs::auth::browser::BrowserToken;
use ytmapi_rs::auth::noauth::NoAuthToken;
use ytmapi_rs::common::{PlaylistID, VideoID, YoutubeID};
use ytmapi_rs::parse::{PlaylistItem, HistoryItem, SearchResultPlaylist};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub duration: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Playlist {
    pub id: String,
    pub title: String,
    pub author: String,
    pub song_count: String,
}

pub enum YtClient {
    Authenticated(YtMusic<BrowserToken>),
    Unauthenticated(YtMusic<NoAuthToken>),
}

impl YtClient {
    /// Initializes a client. First attempts to load from the given cookie path.
    /// If cookie doesn't exist or load fails, falls back to unauthenticated guest mode.
    pub async fn init(cookie_path_opt: Option<&Path>) -> Self {
        if let Some(cookie_path) = cookie_path_opt {
            if cookie_path.exists() {
                match YtMusic::from_cookie_file(cookie_path).await {
                    Ok(client) => {
                        println!("Initialized authenticated YouTube Music client.");
                        return YtClient::Authenticated(client);
                    }
                    Err(e) => {
                        eprintln!("Failed to load cookie: {}. Falling back to Guest mode.", e);
                    }
                }
            }
        }
        
        match YtMusic::new_unauthenticated().await {
            Ok(client) => {
                println!("Initialized guest (unauthenticated) YouTube Music client.");
                YtClient::Unauthenticated(client)
            }
            Err(e) => {
                panic!("Fatal: Failed to initialize even guest YouTube Music client: {}", e);
            }
        }
    }

    /// Returns true if client is authenticated
    pub fn is_authenticated(&self) -> bool {
        matches!(self, YtClient::Authenticated(_))
    }

    /// Search for songs and return a list of mapped Tracks
    pub async fn search_songs(&self, query_str: &str) -> Result<Vec<Track>, ytmapi_rs::Error> {
        let raw_songs = match self {
            YtClient::Authenticated(yt) => yt.search_songs(query_str).await?,
            YtClient::Unauthenticated(yt) => yt.search_songs(query_str).await?,
        };

        let tracks = raw_songs
            .into_iter()
            .map(|song| Track {
                id: song.video_id.get_raw().to_string(),
                title: song.title,
                artist: song.artist,
                duration: song.duration,
            })
            .collect();

        Ok(tracks)
    }

    /// Search for playlists and return a list of mapped Playlists
    pub async fn search_playlists(&self, query_str: &str) -> Result<Vec<Playlist>, ytmapi_rs::Error> {
        let raw_playlists = match self {
            YtClient::Authenticated(yt) => yt.search_playlists(query_str).await?,
            YtClient::Unauthenticated(yt) => yt.search_playlists(query_str).await?,
        };

        let playlists = raw_playlists
            .into_iter()
            .map(|pl| match pl {
                SearchResultPlaylist::Featured(f) => Playlist {
                    id: f.playlist_id.get_raw().to_string(),
                    title: f.title,
                    author: "YouTube Music".to_string(),
                    song_count: f.songs.clone(),
                },
                SearchResultPlaylist::Community(c) => Playlist {
                    id: c.playlist_id.get_raw().to_string(),
                    title: c.title,
                    author: c.author,
                    song_count: c.views, // Views/tracks count text
                },
                SearchResultPlaylist::Podcast(p) => Playlist {
                    id: p.podcast_id.get_raw().to_string(),
                    title: p.title,
                    author: p.publisher,
                    song_count: "Podcast".to_string(),
                },
                _ => Playlist {
                    id: "".to_string(),
                    title: "Unknown Playlist".to_string(),
                    author: "Unknown".to_string(),
                    song_count: "".to_string(),
                }
            })
            .collect();

        Ok(playlists)
    }

    /// Fetch playlist tracks
    pub async fn get_playlist_tracks(&self, playlist_id: &str) -> Result<Vec<Track>, ytmapi_rs::Error> {
        let pl_id = PlaylistID::from_raw(playlist_id.to_string());
        let raw_tracks = match self {
            YtClient::Authenticated(yt) => yt.get_playlist_tracks(pl_id).await?,
            YtClient::Unauthenticated(yt) => yt.get_playlist_tracks(pl_id).await?,
        };

        let tracks = raw_tracks
            .into_iter()
            .filter_map(|item| match item {
                PlaylistItem::Song(s) => Some(Track {
                    id: s.video_id.get_raw().to_string(),
                    title: s.title,
                    artist: s.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "),
                    duration: s.duration,
                }),
                PlaylistItem::Video(v) => Some(Track {
                    id: v.video_id.get_raw().to_string(),
                    title: v.title,
                    artist: v.channel_name,
                    duration: v.duration,
                }),
                PlaylistItem::UploadSong(u) => Some(Track {
                    id: u.video_id.get_raw().to_string(),
                    title: u.title,
                    artist: u.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "),
                    duration: u.duration,
                }),
                PlaylistItem::Episode(e) => Some(Track {
                    id: e.episode_id.get_raw().to_string(),
                    title: e.title,
                    artist: e.podcast_name,
                    duration: "".to_string(),
                }),
            })
            .collect();

        Ok(tracks)
    }

    /// Fetch a "radio"/recommendations queue of tracks related to a given video.
    /// Works in both authenticated and guest mode.
    pub async fn get_watch_playlist(&self, video_id: &str) -> Result<Vec<Track>, ytmapi_rs::Error> {
        let vid = VideoID::from_raw(video_id.to_string());
        let raw_tracks = match self {
            YtClient::Authenticated(yt) => yt.get_watch_playlist_from_video_id(vid).await?,
            YtClient::Unauthenticated(yt) => yt.get_watch_playlist_from_video_id(vid).await?,
        };

        let tracks = raw_tracks
            .into_iter()
            .map(|t| Track {
                id: t.video_id.get_raw().to_string(),
                title: t.title,
                artist: t.author,
                duration: t.duration,
            })
            .collect();

        Ok(tracks)
    }

    /// Fetch user history (Requires Authentication)
    pub async fn get_history(&self) -> Result<Vec<Track>, ytmapi_rs::Error> {
        let yt = match self {
            YtClient::Authenticated(yt) => yt,
            YtClient::Unauthenticated(_) => return Ok(vec![]), // Guest has no history
        };

        let raw_periods = yt.get_history().await?;
        let mut tracks = Vec::new();

        for period in raw_periods {
            for item in period.items {
                match item {
                    HistoryItem::Song(s) => tracks.push(Track {
                        id: s.video_id.get_raw().to_string(),
                        title: s.title,
                        artist: s.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "),
                        duration: s.duration,
                    }),
                    HistoryItem::Video(v) => tracks.push(Track {
                        id: v.video_id.get_raw().to_string(),
                        title: v.title,
                        artist: v.channel_name,
                        duration: v.duration,
                    }),
                    HistoryItem::UploadSong(u) => tracks.push(Track {
                        id: u.video_id.get_raw().to_string(),
                        title: u.title,
                        artist: u.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "),
                        duration: u.duration,
                    }),
                    HistoryItem::Episode(e) => tracks.push(Track {
                        id: e.episode_id.get_raw().to_string(),
                        title: e.title,
                        artist: e.podcast_name,
                        duration: "".to_string(),
                    }),
                }
            }
        }

        Ok(tracks)
    }

    /// Fetch user library playlists (Requires Authentication)
    pub async fn get_library_playlists(&self) -> Result<Vec<Playlist>, ytmapi_rs::Error> {
        let yt = match self {
            YtClient::Authenticated(yt) => yt,
            YtClient::Unauthenticated(_) => return Ok(vec![]),
        };

        let raw_playlists = yt.get_library_playlists().await?;
        let playlists = raw_playlists
            .into_iter()
            .map(|pl| Playlist {
                id: pl.playlist_id.get_raw().to_string(),
                title: pl.title,
                author: pl.author,
                song_count: pl.tracks,
            })
            .collect();

        Ok(playlists)
    }

    /// Fetch user library songs (Liked Songs) (Requires Authentication)
    pub async fn get_library_songs(&self) -> Result<Vec<Track>, ytmapi_rs::Error> {
        let yt = match self {
            YtClient::Authenticated(yt) => yt,
            YtClient::Unauthenticated(_) => return Ok(vec![]),
        };

        let raw_songs = yt.get_library_songs().await?;
        let tracks = raw_songs
            .into_iter()
            .map(|song| Track {
                id: song.video_id.get_raw().to_string(),
                title: song.title,
                artist: song.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "),
                duration: song.duration,
            })
            .collect();

        Ok(tracks)
    }
}
