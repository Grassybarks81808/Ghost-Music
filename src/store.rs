//! Tiny JSON persistence for settings, playlists, recent searches and likes.
//! Stored in `%APPDATA%/Ghost Music/library.json` on Windows (XDG data dir on
//! Linux/macOS), so an update of the .exe never loses the user's library.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::model::Track;

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub download_dir: Option<String>,
    /// 0 = MP3 audio, 1 = MP4 video.
    #[serde(default)]
    pub download_mode: usize,
    #[serde(default = "default_volume")]
    pub volume: f32,
    #[serde(default)]
    pub recent_searches: Vec<String>,
    #[serde(default)]
    pub liked: Vec<Track>,
    #[serde(default)]
    pub playlist: Vec<Track>,
    #[serde(default)]
    pub playlists: Vec<Playlist>,
}

fn default_version() -> u32 {
    FORMAT_VERSION
}

fn default_volume() -> f32 {
    0.8
}

impl Default for Library {
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            download_dir: None,
            download_mode: 0,
            volume: 0.8,
            recent_searches: Vec::new(),
            liked: Vec::new(),
            playlist: Vec::new(),
            playlists: Vec::new(),
        }
    }
}

impl Library {
    /// Load from disk, falling back to defaults on any problem (a corrupt file
    /// is renamed so the user can inspect it).
    pub fn load() -> Self {
        let Some(path) = store_path() else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        match serde_json::from_str::<Self>(&text) {
            Ok(library) => library,
            Err(error) => {
                eprintln!("Ghost Music: library.json is unreadable ({error}); starting fresh");
                let _ = std::fs::rename(&path, path.with_extension("json.broken"));
                Self::default()
            }
        }
    }

    pub fn save(&self) {
        let Some(path) = store_path() else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                // Write to a temp file first so an interrupted save cannot
                // destroy an existing library.
                let temp = path.with_extension("json.tmp");
                if std::fs::write(&temp, json).is_ok() {
                    let _ = std::fs::rename(&temp, &path);
                }
            }
            Err(error) => eprintln!("Ghost Music: could not serialise library ({error})"),
        }
    }

    pub fn directory(&self) -> PathBuf {
        self.download_dir
            .as_ref()
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(crate::tools::default_download_dir)
    }

    pub fn is_liked(&self, id: &str) -> bool {
        self.liked.iter().any(|track| track.id == id)
    }

    /// Returns true when the track is liked after the call.
    pub fn toggle_like(&mut self, track: &Track) -> bool {
        if let Some(index) = self.liked.iter().position(|t| t.id == track.id) {
            self.liked.remove(index);
            false
        } else {
            self.liked.insert(0, track.clone());
            true
        }
    }

    pub fn is_in_playlist(&self, id: &str) -> bool {
        self.playlist.iter().any(|track| track.id == id)
    }

    /// Returns true when the track is in the playlist after the call.
    pub fn toggle_playlist(&mut self, track: &Track) -> bool {
        if let Some(index) = self.playlist.iter().position(|t| t.id == track.id) {
            self.playlist.remove(index);
            false
        } else {
            self.playlist.push(track.clone());
            true
        }
    }

    pub fn remember_search(&mut self, query: &str) {
        let query = query.trim();
        if query.is_empty() {
            return;
        }
        self.recent_searches.retain(|entry| !entry.eq_ignore_ascii_case(query));
        self.recent_searches.insert(0, query.to_string());
        self.recent_searches.truncate(10);
    }

    /// Create an empty user playlist. Returns false when it already exists.
    pub fn create_playlist(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let id = format!("pl:{}", crate::model::fnv1a(name.as_bytes()));
        if self.playlists.iter().any(|playlist| playlist.id == id) {
            return false;
        }
        self.playlists.push(Playlist { id, name: name.to_string(), tracks: Vec::new() });
        true
    }

    /// Add a track to a named user playlist, creating it on demand.
    pub fn add_to_playlist(&mut self, name: &str, track: &Track) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let id = format!("pl:{}", crate::model::fnv1a(name.as_bytes()));
        let playlist = match self.playlists.iter_mut().find(|p| p.id == id) {
            Some(playlist) => playlist,
            None => {
                self.playlists.push(Playlist {
                    id: id.clone(),
                    name: name.to_string(),
                    tracks: Vec::new(),
                });
                self.playlists.last_mut().expect("just pushed")
            }
        };
        if playlist.tracks.iter().any(|t| t.id == track.id) {
            return false;
        }
        playlist.tracks.push(track.clone());
        true
    }

    pub fn remove_playlist(&mut self, id: &str) {
        self.playlists.retain(|playlist| playlist.id != id);
    }

    /// Move a track inside the queue (the shared "play next" playlist).
    pub fn move_in_queue(&mut self, from: usize, to: usize) {
        if from >= self.playlist.len() || to >= self.playlist.len() || from == to {
            return;
        }
        let track = self.playlist.remove(from);
        self.playlist.insert(to, track);
    }
}

/// `%APPDATA%/Ghost Music/library.json` on Windows.
pub fn store_path() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
    }?;
    Some(base.join("Ghost Music").join("library.json"))
}

/// Directory used by `--export`/`--list`.
pub fn data_dir() -> Option<PathBuf> {
    store_path().and_then(|path| path.parent().map(|p| p.to_path_buf()))
}
