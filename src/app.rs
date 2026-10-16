//! The Ghost Music application: state, yt-dlp job plumbing, playback control
//! and the window chrome (top bar, sidebar, player bar).

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Align, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Vec2};

use crate::engine::{self, ApiCommand, Engine, JobUpdate, ProgressInfo};
use crate::ghost::{self, GhostStyle, Splash};
use crate::http_api::ApiServer;
use crate::model::{format_clock, format_duration, Track};
use crate::player::{self, PlaybackState, Player, PlayerUpdate};
use crate::store::Library;
use crate::theme::*;
use crate::thumbs::Thumbs;
use crate::tools;
use crate::views;
use crate::widgets::{self, Glyph};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Home,
    Explore,
    Library,
    Downloads,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    Off,
    All,
    One,
}

impl Repeat {
    fn next(self) -> Self {
        match self {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Repeat::Off => "off",
            Repeat::All => "all",
            Repeat::One => "one",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Good,
    Warn,
    Error,
}

pub struct DownloadItem {
    pub track: Track,
    pub mode: usize,
    pub progress: Option<ProgressInfo>,
    pub log: String,
    pub running: bool,
    pub success: bool,
    pub message: String,
    pub folder: String,
    pub started: Instant,
}

/// Everything the user can trigger; views collect these and `App` applies them.
pub enum Action {
    Search(String),
    SetView(View),
    PlayTrack(Track),
    /// Play a list starting at an index (used by "play all" buttons).
    PlayList(Vec<Track>, usize),
    QueueNext(Track),
    QueueLast(Track),
    Download(Track, usize),
    ToggleLike(Track),
    ToggleQueue(Track),
    RemoveFromQueue(usize),
    RemoveFromLiked(String),
    MoveQueue { from: usize, to: usize },
    PlayQueueIndex(usize),
    AddToPlaylist { name: String, track: Track },
    RemovePlaylist(String),
    CreatePlaylist(String),
    SetDownloadDir(String),
    SetDownloadMode(usize),
    ToggleShuffle,
    CycleRepeat,
    ClearRecents,
    PlaySeed(crate::model::Tile),
}

pub struct App {
    // plumbing
    pub engine: Arc<Engine>,
    pub api: Option<Arc<ApiServer>>,
    pub player: Player,
    pub thumbs: Thumbs,
    tx: Sender<JobUpdate>,
    rx: Receiver<JobUpdate>,
    ptx: Sender<PlayerUpdate>,
    prx: Receiver<PlayerUpdate>,
    splash: Splash,

    // library / data
    pub library: Library,
    pub results: Vec<crate::model::SearchEntry>,
    pub seeds: Vec<crate::model::Tile>,
    pub downloads: Vec<DownloadItem>,
    pub recent: Vec<Track>,

    // ui state
    pub view: View,
    pub query: String,
    pub searching: bool,
    pub searched_for: String,
    pub selected: Option<usize>,
    pub status: String,
    pub status_kind: StatusKind,
    pub queue_index: Option<usize>,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub playlist_name_input: String,
    pub download_dir_input: String,
    pub show_queue: bool,
    pub autoplay_similar: bool,

    /// Track waiting for its stream URL before it can start.
    pending_track: Option<Track>,
    /// Set when playback was requested by the HTTP API (autoplay similar).
    api_autoplay: bool,
    /// When the queue ends: keep playing related tracks.
    continue_after_queue: bool,
    last_publish: Instant,
    stream_error: Option<String>,
}

impl App {
    pub fn new(
        engine: Arc<Engine>,
        api: Option<Arc<ApiServer>>,
        library: Library,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let (ptx, prx) = mpsc::channel();
        engine.set_download_dir(library.directory());
        let player = Player::new(library.volume);
        let mut app = Self {
            engine,
            api,
            player,
            thumbs: Thumbs::new(),
            tx,
            rx,
            ptx,
            prx,
            splash: Splash::new(),
            seeds: crate::model::seed_tiles(),
            library,
            results: Vec::new(),
            downloads: Vec::new(),
            recent: Vec::new(),
            view: View::Home,
            query: String::new(),
            searching: false,
            searched_for: String::new(),
            selected: None,
            status: "Ready. Search for a song, an artist or paste a YouTube link.".to_string(),
            status_kind: StatusKind::Info,
            queue_index: None,
            shuffle: false,
            repeat: Repeat::Off,
            playlist_name_input: String::new(),
            download_dir_input: String::new(),
            show_queue: true,
            autoplay_similar: true,
            pending_track: None,
            api_autoplay: false,
            continue_after_queue: false,
            last_publish: Instant::now(),
            stream_error: None,
        };
        app.download_dir_input = app.library.directory().display().to_string();
        app
    }

    pub fn set_status(&mut self, kind: StatusKind, text: impl Into<String>) {
        self.status_kind = kind;
        self.status = text.into();
        if kind == StatusKind::Error {
            self.stream_error = Some(self.status.clone());
        }
    }

    pub fn api_base(&self) -> String {
        self.api.as_ref().map(|api| api.base_url()).unwrap_or_default()
    }

    // -- searching ---------------------------------------------------------

    pub fn start_search(&mut self, query: &str) {
        let query = query.trim().to_string();
        if query.is_empty() {
            return;
        }
        // A pasted YouTube link should behave like a direct request.
        if let Some(track) = track_from_link(&query) {
            self.searching = false;
            self.results.clear();
            self.enqueue_and_play(track);
            return;
        }
        if self.searching {
            return;
        }
        self.searching = true;
        self.query = query.clone();
        self.searched_for = query.clone();
        self.view = View::Explore;
        self.library.remember_search(&query);
        self.library.save();
        self.set_status(StatusKind::Info, format!("Searching for “{query}”…"));
        engine::spawn_search(self.tx.clone(), query, 25);
    }

    fn apply_search_results(&mut self, query: String, result: Result<Vec<crate::model::SearchEntry>, String>) {
        self.searching = false;
        match result {
            Ok(entries) => {
                let count = entries.len();
                self.searched_for = query;
                self.results = entries;
                self.selected = if count > 0 { Some(0) } else { None };
                if count == 0 {
                    self.set_status(StatusKind::Warn, "No results. Try different words or check yt-dlp.");
                } else {
                    self.set_status(StatusKind::Good, format!("Found {count} results."));
                }
                // Warm up the artwork of the first rows.
                for entry in self.results.iter().take(8) {
                    let url = entry.thumbnail_key();
                    self.thumbs.prefetch(&url);
                }
                if self.api_autoplay {
                    if let Some(first) = self.results.first().map(Track::from_entry) {
                        self.api_autoplay = false;
                        self.enqueue_and_play(first);
                    }
                }
            }
            Err(error) => {
                self.set_status(StatusKind::Error, error);
                self.results.clear();
            }
        }
    }

    // -- queue & playback --------------------------------------------------

    /// Add a track to the queue (deduplicated) and return its index.
    fn push_queue(&mut self, track: Track, next: bool) -> usize {
        if let Some(index) = self.library.playlist.iter().position(|t| t.id == track.id) {
            return index;
        }
        if next {
            let position = self.queue_index.map(|index| index + 1).unwrap_or(0);
            let position = position.min(self.library.playlist.len());
            self.library.playlist.insert(position, track);
            position
        } else {
            self.library.playlist.push(track);
            self.library.playlist.len() - 1
        }
    }

    /// Play a track immediately (queueing it if needed).
    pub fn enqueue_and_play(&mut self, track: Track) {
        let index = self.push_queue(track, false);
        self.play_queue_index(index);
        self.library.save();
    }

    pub fn play_list(&mut self, tracks: Vec<Track>, index: usize) {
        // Replace the queue: this mirrors "play all" in YouTube Music.
        self.library.playlist = tracks;
        self.library.save();
        self.play_queue_index(index.min(self.library.playlist.len().saturating_sub(1)));
    }

    pub fn play_queue_index(&mut self, index: usize) {
        let Some(track) = self.library.playlist.get(index).cloned() else {
            return;
        };
        self.queue_index = Some(index);
        self.request_playback(track, self.continue_after_queue);
    }

    /// Ask the engine for a stream URL, then hand the decoder to the player.
    pub fn request_playback(&mut self, track: Track, autoplay: bool) {
        if !self.player.is_available() {
            self.set_status(
                StatusKind::Warn,
                self.player
                    .error
                    .clone()
                    .unwrap_or_else(|| "No audio output device found.".to_string()),
            );
            return;
        }
        self.continue_after_queue = autoplay;
        let key = track.id.clone();
        self.pending_track = Some(track.clone());
        self.player.loading = Some(key.clone());
        self.player.state = PlaybackState::Loading;
        self.set_status(StatusKind::Info, format!("Tuning “{}”…", track.title));

        if self.engine.stream(&key).is_some() {
            let url = self.engine.local_stream_url(&track);
            player::spawn_open(self.ptx.clone(), key, url);
        } else if self.engine.begin_resolve(&key) {
            engine::spawn_stream_resolve(self.engine.clone(), self.tx.clone(), key, track.url.clone());
        }
    }

    pub fn next_track(&mut self, automatic: bool) {
        if self.library.playlist.is_empty() {
            self.player.stop();
            return;
        }
        let current = self.queue_index.unwrap_or(0);
        if automatic && self.repeat == Repeat::One {
            if let Some(track) = self.player.track.clone() {
                self.request_playback(track, self.continue_after_queue);
            }
            return;
        }
        let next = if self.shuffle {
            Some(unstable_pick(self.library.playlist.len(), current))
        } else if current + 1 < self.library.playlist.len() {
            Some(current + 1)
        } else if self.repeat == Repeat::All || (automatic && self.continue_after_queue) {
            Some(0)
        } else {
            None
        };
        match next {
            Some(index) => self.play_queue_index(index),
            None => {
                self.player.stop();
                self.set_status(StatusKind::Info, "End of the queue. Add more tracks or turn on repeat.");
            }
        }
    }

    pub fn previous_track(&mut self) {
        if self.library.playlist.is_empty() {
            return;
        }
        // Restart the track when we are past the first few seconds.
        if self.player.position() > 4.0 {
            let _ = self.player.seek(0.0);
            return;
        }
        let current = self.queue_index.unwrap_or(0);
        let index = if current == 0 {
            self.library.playlist.len() - 1
        } else {
            current - 1
        };
        self.play_queue_index(index);
    }

    pub fn toggle_play(&mut self) {
        match self.player.state {
            PlaybackState::Playing | PlaybackState::Paused => self.player.toggle(),
            _ => {
                if let Some(index) = self.queue_index {
                    self.play_queue_index(index);
                } else if let Some(first) = self.library.playlist.first().cloned() {
                    self.enqueue_and_play(first);
                } else if let Some(track) = self.results.first().map(Track::from_entry) {
                    self.enqueue_and_play(track);
                }
            }
        }
    }

    pub fn download(&mut self, track: Track, mode: usize) {
        let folder = self.library.directory();
        if !tools::available("yt-dlp") {
            self.set_status(StatusKind::Error, "yt-dlp is missing: put yt-dlp.exe next to Ghost Music.");
            return;
        }
        self.downloads.insert(
            0,
            DownloadItem {
                track: track.clone(),
                mode,
                progress: None,
                log: String::new(),
                running: true,
                success: false,
                message: String::new(),
                folder: folder.display().to_string(),
                started: Instant::now(),
            },
        );
        engine::spawn_download(self.tx.clone(), track.clone(), mode, folder);
        self.set_status(
            StatusKind::Info,
            format!(
                "Downloading “{}” as {}…",
                track.title,
                if mode == 0 { "MP3" } else { "MP4" }
            ),
        );
        if self.view != View::Downloads {
            self.view = View::Downloads;
        }
    }

    // -- job polling -------------------------------------------------------

    fn poll_jobs(&mut self) {
        while let Ok(job) = self.rx.try_recv() {
            match job {
                JobUpdate::SearchDone { query, result } => self.apply_search_results(query, result),
                JobUpdate::StreamReady { key, .. } => {
                    if let Some(track) = self.pending_track.clone() {
                        if track.id == key {
                            let url = self.engine.local_stream_url(&track);
                            player::spawn_open(self.ptx.clone(), key, url);
                        }
                    }
                }
                JobUpdate::StreamFailed { key, error } => {
                    self.pending_track = None;
                    self.player.state = PlaybackState::Idle;
                    if self.player.loading.as_deref() == Some(key.as_str()) {
                        self.set_status(StatusKind::Error, format!("Could not play this track: {error}"));
                    } else {
                        self.set_status(StatusKind::Warn, format!("Skipped a track: {error}"));
                    }
                }
                JobUpdate::DownloadStarted { title } => {
                    self.set_status(StatusKind::Info, format!("Downloading “{title}”…"));
                }
                JobUpdate::DownloadLog(piece) => {
                    if let Some(info) = ProgressInfo::parse(&piece) {
                        if let Some(item) = self.downloads.iter_mut().find(|item| item.running) {
                            item.progress = Some(info);
                        }
                    }
                    if let Some(item) = self.downloads.iter_mut().find(|item| item.running) {
                        item.log.push_str(&piece);
                        // Keep the tail short; this is a UI preview, not a file.
                        let length = item.log.len();
                        if length > 6000 {
                            item.log.drain(..length - 6000);
                        }
                    }
                }
                JobUpdate::DownloadDone { success, message, folder } => {
                    if let Some(item) = self.downloads.iter_mut().find(|item| item.running) {
                        item.running = false;
                        item.success = success;
                        item.message = message.clone();
                        item.folder = folder.clone();
                    }
                    if success {
                        self.set_status(StatusKind::Good, format!("Download finished. {message}"));
                    } else {
                        self.set_status(StatusKind::Error, message);
                    }
                }
            }
        }
    }

    fn poll_player(&mut self) {
        while let Ok(update) = self.prx.try_recv() {
            match update {
                PlayerUpdate::Opened { key, decoder } => {
                    let Some(track) = self.pending_track.clone() else {
                        continue;
                    };
                    if track.id != key {
                        continue;
                    }
                    match self.player.start(&key, track.clone(), *decoder) {
                        Ok(()) => {
                            self.set_status(StatusKind::Good, format!("Playing “{}”", track.title));
                            self.recent.retain(|entry| entry.id != track.id);
                            self.recent.insert(0, track.clone());
                            self.recent.truncate(30);
                            self.thumbs.prefetch(&track.thumbnail);
                            self.prefetch_next();
                        }
                        Err(error) => self.set_status(StatusKind::Error, error),
                    }
                }
                PlayerUpdate::Failed { key, error } => {
                    if self.player.loading.as_deref() == Some(key.as_str()) {
                        self.player.state = PlaybackState::Idle;
                        self.player.loading = None;
                        self.set_status(StatusKind::Error, error);
                    }
                }
            }
        }
    }

    /// Resolve the next track's URL in the background so "next" starts instantly.
    fn prefetch_next(&mut self) {
        let index = match self.queue_index {
            Some(index) => index,
            None => return,
        };
        let Some(next) = self.library.playlist.get(index + 1).cloned() else {
            return;
        };
        if self.engine.stream(&next.id).is_none() && self.engine.begin_resolve(&next.id) {
            engine::spawn_stream_resolve(self.engine.clone(), self.tx.clone(), next.id.clone(), next.url.clone());
        }
        if let Some(track) = self.library.playlist.get(index + 2).cloned() {
            if self.engine.stream(&track.id).is_none() && self.engine.begin_resolve(&track.id) {
                engine::spawn_stream_resolve(self.engine.clone(), self.tx.clone(), track.id.clone(), track.url.clone());
            }
        }
    }

    /// Commands issued through the HTTP API are executed here, on the UI thread.
    fn apply_api_commands(&mut self) {
        for command in self.engine.drain_commands() {
            match command {
                ApiCommand::Play { track, autoplay } => {
                    let index = self.push_queue(track, false);
                    self.queue_index = Some(index);
                    let track = self.library.playlist[index].clone();
                    self.request_playback(track, autoplay && self.autoplay_similar);
                }
                ApiCommand::PlayQueue { tracks, start_index, autoplay } => {
                    self.library.playlist = tracks;
                    self.continue_after_queue = autoplay && self.autoplay_similar;
                    self.play_queue_index(start_index);
                }
                ApiCommand::Enqueue { tracks, next } => {
                    for track in tracks {
                        self.push_queue(track, next);
                    }
                }
                ApiCommand::Pause => self.player.pause(),
                ApiCommand::Resume => self.player.resume(),
                ApiCommand::Toggle => self.player.toggle(),
                ApiCommand::Next => self.next_track(false),
                ApiCommand::Prev => self.previous_track(),
                ApiCommand::Stop => {
                    self.player.stop();
                    self.pending_track = None;
                }
                ApiCommand::Seek { seconds } => {
                    if let Err(error) = self.player.seek(seconds) {
                        self.set_status(StatusKind::Warn, error);
                    }
                }
                ApiCommand::SetVolume { volume } => {
                    self.player.set_volume(volume);
                    self.library.volume = volume;
                }
                ApiCommand::Shuffle => self.shuffle = !self.shuffle,
                ApiCommand::Repeat => self.repeat = self.repeat.next(),
            }
        }
    }

    /// Publish the player state for `GET /player/state` a few times per second.
    fn publish_state(&mut self) {
        if self.last_publish.elapsed() < Duration::from_millis(250) {
            return;
        }
        self.last_publish = Instant::now();
        let state = match self.player.state {
            PlaybackState::Playing => "playing",
            PlaybackState::Paused => "paused",
            PlaybackState::Loading => "loading",
            PlaybackState::Unavailable => "unavailable",
            _ => "idle",
        };
        let track = self.player.track.as_ref().map(|track| {
            serde_json::json!({
                "id": track.id,
                "title": track.title,
                "artist": track.artist,
                "duration": track.duration,
                "url": track.url,
                "thumbnail": track.thumbnail,
            })
        });
        let queue: Vec<serde_json::Value> = self
            .library
            .playlist
            .iter()
            .map(|track| {
                serde_json::json!({
                    "id": track.id,
                    "title": track.title,
                    "artist": track.artist,
                    "duration": track.duration,
                })
            })
            .collect();
        self.engine.publish_state(serde_json::json!({
            "app": "Ghost Music",
            "state": state,
            "track": track,
            "position": self.player.position(),
            "duration": self.player.track.as_ref().and_then(|track| track.duration),
            "volume": self.player.volume(),
            "muted": false,
            "shuffle": self.shuffle,
            "repeat": self.repeat.label(),
            "queue": queue,
            "queue_index": self.queue_index,
            "view": match self.view {
                View::Home => "home",
                View::Explore => "explore",
                View::Library => "library",
                View::Downloads => "downloads",
                View::Settings => "settings",
            },
            "last_search": self.searched_for,
            "liked": self.library.liked.len(),
        }));
    }

    // -- keyboard shortcuts ------------------------------------------------

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.memory(|memory| memory.focused().is_some());
        let (space, right, left, up, down, next, previous, slash, escape, search_focus) = ctx.input(|input| {
            (
                input.key_pressed(egui::Key::Space),
                input.key_pressed(egui::Key::ArrowRight),
                input.key_pressed(egui::Key::ArrowLeft),
                input.key_pressed(egui::Key::ArrowUp),
                input.key_pressed(egui::Key::ArrowDown),
                input.key_pressed(egui::Key::N),
                input.key_pressed(egui::Key::P),
                input.key_pressed(egui::Key::Slash),
                input.key_pressed(egui::Key::Escape),
                input.modifiers.command && input.key_pressed(egui::Key::K),
            )
        });
        if escape {
            ctx.memory_mut(|memory| memory.stop_text_input());
        }
        if typing {
            return;
        }
        if space {
            self.toggle_play();
        }
        if right {
            let target = self.player.position() + 5.0;
            let _ = self.player.seek(target);
        }
        if left {
            let target = (self.player.position() - 5.0).max(0.0);
            let _ = self.player.seek(target);
        }
        if up {
            let volume = (self.player.volume() + 0.05).min(1.0);
            self.player.set_volume(volume);
            self.library.volume = volume;
        }
        if down {
            let volume = (self.player.volume() - 0.05).max(0.0);
            self.player.set_volume(volume);
            self.library.volume = volume;
        }
        if next {
            self.next_track(false);
        }
        if previous {
            self.previous_track();
        }
        if slash || search_focus {
            ctx.memory_mut(|memory| memory.request_focus(search_field_id()));
        }
    }

    /// Playback bookkeeping that runs every frame.
    fn tick(&mut self) {
        if self.player.finished() {
            self.next_track(true);
        }
        if self.player.state == PlaybackState::Playing {
            self.stream_error = None;
        }
    }

    /// Fisher-Yates shuffle of the queue (keeps the current track first).
    pub fn shuffle_queue(&mut self) {
        let len = self.library.playlist.len();
        if len < 2 {
            return;
        }
        let mut state = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.subsec_nanos() as u64)
            .unwrap_or(0x9e37_79b9)
            | 1;
        for index in (1..len).rev() {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let target = (state >> 33) as usize % (index + 1);
            self.library.move_in_queue(index, target);
        }
        self.library.save();
    }

    // -- window chrome -----------------------------------------------------

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(34.0, 34.0), Sense::hover());
            ghost::draw_fitted(ui.painter(), rect, RED, GhostStyle::default());
            ui.add_space(2.0);
            ui.vertical(|ui| {
                ui.add_space(4.0);
                ui.label(RichText::new("Ghost Music").size(17.0).strong().color(TEXT));
            });

            ui.add_space(18.0);
            for (glyph, view, tip) in [
                (Glyph::Home, View::Home, "Home"),
                (Glyph::Search, View::Explore, "Explore"),
                (Glyph::Library, View::Library, "Library"),
            ] {
                if widgets::icon_button(ui, glyph, 32.0, tip, self.view == view) {
                    self.view = view;
                }
            }

            ui.add_space(10.0);
            let width = (ui.available_width() - 210.0).max(180.0);
            let response = ui.add_sized(
                [width, 34.0],
                egui::TextEdit::singleline(&mut self.query)
                    .id(search_field_id())
                    .hint_text("Search songs, artists, albums, or paste a link")
                    .margin(egui::Margin::symmetric(10, 6)),
            );
            let submit = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if submit {
                let query = self.query.clone();
                self.start_search(&query);
            }
            if widgets::icon_button(ui, Glyph::Search, 32.0, "Search (Enter)", false) {
                let query = self.query.clone();
                self.start_search(&query);
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::icon_button(ui, Glyph::Settings, 32.0, "Settings & API", self.view == View::Settings) {
                    self.view = View::Settings;
                }
                if widgets::icon_button(ui, Glyph::Download, 32.0, "Downloads", self.view == View::Downloads) {
                    self.view = View::Downloads;
                }
                // API indicator: green dot when the local API is up.
                let (rect, response) = ui.allocate_exact_size(Vec2::new(96.0, 26.0), Sense::click());
                let port = self.engine.api_port();
                let color = if port > 0 { GOOD } else { FAINT };
                ui.painter().circle_filled(Pos2::new(rect.left() + 8.0, rect.center().y), 4.0, color);
                ui.painter().text(
                    Pos2::new(rect.left() + 18.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    if port > 0 { format!("API :{port}") } else { "API off".to_string() },
                    FontId::proportional(11.0),
                    if port > 0 { GOOD } else { FAINT },
                );
                if response
                    .on_hover_text("Local HTTP API for other apps - click to open the settings")
                    .clicked()
                {
                    self.view = View::Settings;
                }
            });
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        let item = |ui: &mut egui::Ui, glyph: Glyph, label: &str, active: bool| -> bool {
            let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
            let hovered = response.hovered();
            if active || hovered {
                ui.painter().rect_filled(
                    rect,
                    egui::CornerRadius::same(8),
                    if active { RED_SOFT } else { PANEL_HOVER },
                );
            }
            let color = if active { RED } else if hovered { TEXT } else { MUTED };
            widgets::transport_glyph(
                ui.painter(),
                Pos2::new(rect.left() + 18.0, rect.center().y),
                16.0,
                glyph,
                color,
            );
            ui.painter().text(
                Pos2::new(rect.left() + 38.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                FontId::proportional(13.5),
                color,
            );
            response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
        };

        if item(ui, Glyph::Home, "Home", self.view == View::Home) {
            self.view = View::Home;
        }
        if item(ui, Glyph::Search, "Explore", self.view == View::Explore) {
            self.view = View::Explore;
        }
        if item(ui, Glyph::Library, "Library", self.view == View::Library) {
            self.view = View::Library;
        }
        if item(ui, Glyph::Download, "Downloads", self.view == View::Downloads) {
            self.view = View::Downloads;
        }
        if item(ui, Glyph::Settings, "Settings", self.view == View::Settings) {
            self.view = View::Settings;
        }
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label(RichText::new("QUEUE").size(11.0).strong().color(FAINT));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{}", self.library.playlist.len()))
                        .size(11.0)
                        .color(FAINT),
                );
            });
        });
        ui.add_space(2.0);
        let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
        scroll = scroll.id_salt("sidebar-queue");
        scroll.show(ui, |ui| {
            if self.library.playlist.is_empty() {
                ui.label(RichText::new("Nothing queued yet.").size(11.0).color(FAINT));
            }
            for index in 0..self.library.playlist.len() {
                let playing = self.queue_index == Some(index);
                let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::click());
                let hovered = response.hovered();
                if hovered {
                    ui.painter().rect_filled(rect, egui::CornerRadius::same(6), PANEL_HOVER);
                }
                let title = self.library.playlist[index].title.clone();
                let color = if playing { RED } else { MUTED };
                ui.painter().text(
                    Pos2::new(rect.left() + 6.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    if playing { "▶" } else { "•" },
                    FontId::proportional(10.0),
                    color,
                );
                ui.painter().text(
                    Pos2::new(rect.left() + 20.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    truncate_text(ui, &title, rect.width() - 28.0, 11.5),
                    FontId::proportional(11.5),
                    color,
                );
                if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.play_queue_index(index);
                }
            }
        });
    }

    fn player_bar(&mut self, ui: &mut egui::Ui) {
        let full = ui.available_rect_before_wrap();
        // Progress row with the ghost handle.
        let bar_rect = Rect::from_min_size(
            Pos2::new(full.left() + 8.0, full.top() + 8.0),
            Vec2::new((full.width() - 16.0).max(60.0), 14.0),
        );
        let duration = self.player.track.as_ref().and_then(|track| track.duration);
        let position = self.player.position();
        let progress = match duration {
            Some(duration) if duration > 0.0 => (position / duration).clamp(0.0, 1.0) as f32,
            _ => 0.0,
        };
        let painter = ui.painter();
        let rail = Rect::from_min_max(
            Pos2::new(bar_rect.left(), bar_rect.center().y - 2.0),
            Pos2::new(bar_rect.right(), bar_rect.center().y + 2.0),
        );
        painter.rect_filled(rail, 2.0, Color32::from_rgb(38, 41, 52));
        let mut filled = rail;
        filled.set_right(rail.left() + rail.width() * progress);
        painter.rect_filled(filled, 2.0, RED);
        ghost::draw_seek_ghost(painter, rail, progress, RED);
        painter.text(
            Pos2::new(rail.left(), rail.center().y - 12.0),
            egui::Align2::LEFT_BOTTOM,
            format_clock(position),
            FontId::proportional(10.0),
            MUTED,
        );
        painter.text(
            Pos2::new(rail.right(), rail.center().y - 12.0),
            egui::Align2::RIGHT_BOTTOM,
            format_duration(duration),
            FontId::proportional(10.0),
            MUTED,
        );
        let scrub = ui.interact(bar_rect, ui.id().with("scrub"), Sense::click_and_drag());
        if scrub.dragged() || scrub.clicked() {
            if let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) {
                if let Some(duration) = duration {
                    let ratio = ((pointer.x - rail.left()) / rail.width().max(1.0)).clamp(0.0, 1.0);
                    if let Err(error) = self.player.seek(ratio as f64 * duration) {
                        self.set_status(StatusKind::Warn, error);
                    }
                }
            }
        }

        // Transport row.
        let row = Rect::from_min_max(
            Pos2::new(full.left() + 8.0, bar_rect.bottom() + 6.0),
            Pos2::new(full.right() - 8.0, full.bottom() - 6.0),
        );

        // Right cluster: volume + queue toggle + like.
        let mut right_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(Pos2::new(row.right() - 250.0, row.top()), row.max))
                .layout(Layout::right_to_left(Align::Center)),
        );
        let track = self.player.track.clone();
        let liked = track
            .as_ref()
            .map(|track| self.library.is_liked(&track.id))
            .unwrap_or(false);
        if widgets::icon_button(&mut right_ui, Glyph::Queue, 30.0, "Queue panel", self.show_queue) {
            self.show_queue = !self.show_queue;
        }
        if let Some(track) = track.clone() {
            if widgets::icon_button(
                &mut right_ui,
                if liked { Glyph::HeartFilled } else { Glyph::Heart },
                30.0,
                if liked { "Remove from liked" } else { "Like this track" },
                liked,
            ) {
                self.library.toggle_like(&track);
                self.library.save();
            }
            if widgets::icon_button(&mut right_ui, Glyph::Download, 30.0, "Download this track", false) {
                self.download(track, self.library.download_mode);
            }
        }
        let mut volume = self.player.volume();
        let slider = right_ui.add_sized(
            [110.0, 20.0],
            egui::Slider::new(&mut volume, 0.0..=1.0).show_value(false),
        );
        if slider.changed() {
            self.player.set_volume(volume);
            self.library.volume = volume;
        }
        widgets::transport_glyph(
            &right_ui.painter(),
            Pos2::new(right_ui.min_rect().left() - 12.0, row.center().y),
            15.0,
            if volume <= 0.001 { Glyph::Muted } else { Glyph::Volume },
            MUTED,
        );

        // Left cluster: artwork + titles.
        let art = Rect::from_center_size(Pos2::new(row.left() + 26.0, row.center().y), Vec2::splat(48.0));
        let texture = track.as_ref().and_then(|track| self.thumbs.get(ui.ctx(), &track.thumbnail));
        match texture {
            Some(texture) => {
                ui.painter().add(egui::Shape::image(
                    texture.id(),
                    art,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                ));
            }
            None => {
                let seed = track
                    .as_ref()
                    .map(|track| track.id.clone())
                    .unwrap_or_else(|| "ghost".to_string());
                let texture = self.thumbs.placeholder(ui.ctx(), &seed);
                ui.painter().image(
                    texture.id(),
                    art,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        }
        ui.painter().rect_stroke(
            art,
            egui::CornerRadius::same(8),
            egui::Stroke::new(1.0, Color32::from_black_alpha(80)),
            egui::StrokeKind::Inside,
        );
        let text_left = art.right() + 12.0;
        let text_width = (row.center().x - 210.0 - text_left).max(120.0);
        let (title, subtitle) = match &track {
            Some(track) => (track.title.clone(), track.artist.clone()),
            None => ("Nothing playing".to_string(), "Pick a track to begin".to_string()),
        };
        ui.painter().text(
            Pos2::new(text_left, row.center().y - 9.0),
            egui::Align2::LEFT_CENTER,
            truncate_text(ui, &title, text_width, 13.5),
            FontId::proportional(13.5),
            TEXT,
        );
        ui.painter().text(
            Pos2::new(text_left, row.center().y + 10.0),
            egui::Align2::LEFT_CENTER,
            truncate_text(ui, &subtitle, text_width, 11.5),
            FontId::proportional(11.5),
            MUTED,
        );

        // Centre cluster.
        let buttons = 5.0;
        let width = buttons * 42.0;
        let center_rect = Rect::from_center_size(
            Pos2::new(row.center().x, row.center().y),
            Vec2::new(width, row.height().min(48.0)),
        );
        let mut center_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(center_rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        if widgets::icon_button(&mut center_ui, Glyph::Shuffle, 30.0, "Shuffle", self.shuffle) {
            self.shuffle = !self.shuffle;
        }
        if widgets::icon_button(&mut center_ui, Glyph::Previous, 34.0, "Previous", false) {
            self.previous_track();
        }
        let playing = self.player.state == PlaybackState::Playing;
        let loading = self.player.state == PlaybackState::Loading;
        let glyph = if loading {
            Glyph::Refresh
        } else if playing {
            Glyph::Pause
        } else {
            Glyph::Play
        };
        if widgets::circle_button(
            &mut center_ui,
            44.0,
            glyph,
            RED,
            Color32::WHITE,
            if playing { "Pause (Space)" } else { "Play (Space)" },
            true,
        ) {
            self.toggle_play();
        }
        if widgets::icon_button(&mut center_ui, Glyph::Next, 34.0, "Next", false) {
            self.next_track(false);
        }
        let repeat_glyph = if self.repeat == Repeat::One {
            Glyph::RepeatOne
        } else {
            Glyph::Repeat
        };
        if widgets::icon_button(
            &mut center_ui,
            repeat_glyph,
            30.0,
            "Repeat: off / all / one",
            self.repeat != Repeat::Off,
        ) {
            self.repeat = self.repeat.next();
        }

        // Status line under the bar.
        ui.painter().text(
            Pos2::new(full.left() + 12.0, full.bottom() - 9.0),
            egui::Align2::LEFT_BOTTOM,
            truncate_text(
                ui,
                &self.status,
                full.width() * 0.55,
                10.0,
            ),
            FontId::proportional(10.0),
            match self.status_kind {
                StatusKind::Good => GOOD,
                StatusKind::Warn => WARN,
                StatusKind::Error => RED,
                StatusKind::Info => FAINT,
            },
        );
    }

    /// Apply an action produced by a view.
    pub fn apply(&mut self, action: Action) {
        match action {
            Action::Search(query) => self.start_search(&query),
            Action::SetView(view) => self.view = view,
            Action::PlayTrack(track) => self.enqueue_and_play(track),
            Action::PlayList(tracks, index) => self.play_list(tracks, index),
            Action::QueueNext(track) => {
                let position = self.push_queue(track.clone(), true);
                self.set_status(
                    StatusKind::Good,
                    format!("“{}” will play next (#{}).", track.title, position + 1),
                );
            }
            Action::QueueLast(track) => {
                self.push_queue(track.clone(), false);
                self.set_status(StatusKind::Good, format!("Added “{}” to the queue.", track.title));
            }
            Action::Download(track, mode) => self.download(track, mode),
            Action::ToggleLike(track) => {
                let liked = self.library.toggle_like(&track);
                self.set_status(
                    StatusKind::Good,
                    if liked {
                        format!("Liked “{}”.", track.title)
                    } else {
                        format!("Removed “{}” from liked.", track.title)
                    },
                );
            }
            Action::ToggleQueue(track) => {
                let added = self.library.toggle_playlist(&track);
                self.set_status(
                    StatusKind::Good,
                    if added {
                        format!("Added “{}” to the queue.", track.title)
                    } else {
                        format!("Removed “{}” from the queue.", track.title)
                    },
                );
            }
            Action::RemoveFromQueue(index) => {
                if index < self.library.playlist.len() {
                    self.library.playlist.remove(index);
                    match self.queue_index {
                        Some(current) if current == index => {
                            if self.library.playlist.is_empty() {
                                self.queue_index = None;
                                self.player.stop();
                            } else {
                                self.play_queue_index(index.min(self.library.playlist.len() - 1));
                            }
                        }
                        Some(current) if current > index => self.queue_index = Some(current - 1),
                        _ => {}
                    }
                }
            }
            Action::RemoveFromLiked(id) => self.library.liked.retain(|track| track.id != id),
            Action::MoveQueue { from, to } => self.library.move_in_queue(from, to),
            Action::PlayQueueIndex(index) => self.play_queue_index(index),
            Action::AddToPlaylist { name, track } => {
                let added = self.library.add_to_playlist(&name, &track);
                if added {
                    self.set_status(StatusKind::Good, format!("Added to playlist “{name}”."));
                } else {
                    self.set_status(StatusKind::Warn, format!("“{}” is already in “{name}”.", track.title));
                }
            }
            Action::RemovePlaylist(id) => {
                self.library.remove_playlist(&id);
                self.set_status(StatusKind::Info, "Playlist removed.");
            }
            Action::CreatePlaylist(name) => {
                let name = name.trim().to_string();
                if name.is_empty() {
                    return;
                }
                let created = self.library.create_playlist(&name);
                self.set_status(
                    if created { StatusKind::Good } else { StatusKind::Warn },
                    if created {
                        format!("Playlist “{name}” created.")
                    } else {
                        format!("Playlist “{name}” already exists.")
                    },
                );
            }
            Action::SetDownloadDir(dir) => {
                self.library.download_dir = Some(dir.clone());
                let path = std::path::PathBuf::from(&dir);
                self.engine.set_download_dir(path.clone());
                if !path.exists() {
                    let _ = std::fs::create_dir_all(&path);
                }
                self.set_status(StatusKind::Good, format!("Downloads will be saved to {dir}."));
            }
            Action::SetDownloadMode(mode) => self.library.download_mode = mode,
            Action::ToggleShuffle => self.shuffle = !self.shuffle,
            Action::CycleRepeat => self.repeat = self.repeat.next(),
            Action::ClearRecents => {
                self.library.recent_searches.clear();
                self.recent.clear();
                self.set_status(StatusKind::Info, "History cleared.");
            }
            Action::PlaySeed(tile) => self.play_seed(tile),
        }
        self.library.save();
    }

    /// Seed tiles are both a visual identity and a one-click radio.
    fn play_seed(&mut self, tile: crate::model::Tile) {
        let query = crate::model::seed_query(&tile);
        self.query = query.clone();
        self.library.remember_search(&query);
        self.autoplay_similar = true;
        self.start_search(&query);
        self.continue_after_queue = true;
    }

    // -- egui entry point --------------------------------------------------

    pub fn ui(&mut self, ctx: &egui::Context) {
        self.poll_jobs();
        self.poll_player();
        self.apply_api_commands();
        self.handle_shortcuts(ctx);
        self.publish_state();

        if self.splash.draw(ctx) {
            let skip = ctx.input(|input| input.pointer.any_pressed() || !input.keys_down.is_empty());
            if skip {
                self.splash = Splash::finished();
            }
            return;
        }

        self.tick();

        egui::TopBottomPanel::top("ghost-top")
            .exact_height(56.0)
            .frame(
                egui::Frame::NONE
                    .fill(CHROME)
                    .inner_margin(egui::Margin::symmetric(14, 10))
                    .stroke(egui::Stroke::new(1.0, HAIRLINE)),
            )
            .show(ctx, |ui| self.top_bar(ui));

        egui::TopBottomPanel::bottom("ghost-player")
            .exact_height(100.0)
            .frame(
                egui::Frame::NONE
                    .fill(CHROME)
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .stroke(egui::Stroke::new(1.0, HAIRLINE)),
            )
            .show(ctx, |ui| self.player_bar(ui));

        egui::SidePanel::left("ghost-nav")
            .exact_width(206.0)
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(CHROME)
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .stroke(egui::Stroke::new(1.0, HAIRLINE)),
            )
            .show(ctx, |ui| self.sidebar(ui));

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(BG).inner_margin(egui::Margin::symmetric(18, 12)))
            .show(ctx, |ui| {
                self.thumbs.poll(ui.ctx());
                let mut actions: Vec<Action> = Vec::new();
                match self.view {
                    View::Home => views::home(self, ui, &mut actions),
                    View::Explore => views::explore(self, ui, &mut actions),
                    View::Library => views::library(self, ui, &mut actions),
                    View::Downloads => views::downloads(self, ui, &mut actions),
                    View::Settings => views::settings(self, ui, &mut actions),
                }
                for action in actions {
                    self.apply(action);
                }
            });

        self.queue_overlay(ctx);
    }

    /// Right-hand queue panel (toggled from the player bar).
    fn queue_overlay(&mut self, ctx: &egui::Context) {
        if !self.show_queue {
            return;
        }
        let mut open = true;
        egui::SidePanel::right("ghost-queue")
            .default_width(268.0)
            .frame(
                egui::Frame::NONE
                    .fill(PANEL_SUNKEN)
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .stroke(egui::Stroke::new(1.0, HAIRLINE)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Queue").size(16.0).strong().color(TEXT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Hide").clicked() {
                            open = false;
                        }
                        if ui.small_button("Clear").clicked() {
                            self.library.playlist.clear();
                            self.queue_index = None;
                            self.player.stop();
                        }
                        if ui.small_button("Shuffle").clicked() {
                            shuffle_requested = true;
                        }
                    });
                });
                ui.add_space(6.0);
                let mut shuffle_requested = false;
                let mut move_request: Option<(usize, usize)> = None;
                let mut play_request: Option<usize> = None;
                let mut remove_request: Option<usize> = None;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .id_salt("queue-panel")
                    .show(ui, |ui| {
                        if self.library.playlist.is_empty() {
                            ui.label(RichText::new("Your queue is empty.").size(12.0).color(FAINT));
                        }
                        for index in 0..self.library.playlist.len() {
                            let track = &self.library.playlist[index];
                            let (rect, response) =
                                ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::click());
                            let playing = self.queue_index == Some(index);
                            let hovered = response.hovered();
                            if playing || hovered {
                                ui.painter().rect_filled(
                                    rect,
                                    egui::CornerRadius::same(6),
                                    if playing { RED_SOFT } else { PANEL_HOVER },
                                );
                            }
                            ui.painter().text(
                                Pos2::new(rect.left() + 6.0, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                if playing { "▶" } else { "•" },
                                FontId::proportional(9.0),
                                if playing { RED } else { FAINT },
                            );
                            ui.painter().text(
                                Pos2::new(rect.left() + 20.0, rect.center().y - 7.0),
                                egui::Align2::LEFT_CENTER,
                                truncate_text(ui, &track.title, rect.width() - 96.0, 12.0),
                                FontId::proportional(12.0),
                                if playing { RED } else { TEXT },
                            );
                            ui.painter().text(
                                Pos2::new(rect.left() + 20.0, rect.center().y + 8.0),
                                egui::Align2::LEFT_CENTER,
                                truncate_text(ui, &track.artist, rect.width() - 96.0, 10.0),
                                FontId::proportional(10.0),
                                MUTED,
                            );
                            ui.painter().text(
                                Pos2::new(rect.right() - 46.0, rect.center().y),
                                egui::Align2::RIGHT_CENTER,
                                format_duration(track.duration),
                                FontId::proportional(10.0),
                                FAINT,
                            );
                            if hovered {
                                if widgets::icon_button(ui, Glyph::Trash, 24.0, "Remove", false) {
                                    remove_request = Some(index);
                                }
                                if index > 0 && ui.small_button("↑").on_hover_text("Move up").clicked() {
                                    move_request = Some((index, index - 1));
                                }
                                if index + 1 < self.library.playlist.len()
                                    && ui.small_button("↓").on_hover_text("Move down").clicked()
                                {
                                    move_request = Some((index, index + 1));
                                }
                            }
                            if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                play_request = Some(index);
                            }
                        }
                    });
                if shuffle_requested {
                    self.shuffle_queue();
                }
                if let Some((from, to)) = move_request {
                    self.library.move_in_queue(from, to);
                }
                if let Some(index) = remove_request {
                    self.apply(Action::RemoveFromQueue(index));
                }
                if let Some(index) = play_request {
                    self.play_queue_index(index);
                }
            });
        if !open {
            self.show_queue = false;
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Keep progress bars, spinners and the splash animating.
        ctx.request_repaint_after(Duration::from_millis(120));
        self.ui(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.library.volume = self.player.volume();
        self.library.save();
    }
}

/// Focus id shared between the top bar and the keyboard shortcut.
fn search_field_id() -> egui::Id {
    egui::Id::new("ghost-search-field")
}

fn truncate_text(ui: &egui::Ui, text: &str, width: f32, size: f32) -> String {
    let font = FontId::proportional(size);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font.clone(), TEXT);
    if galley.rect.width() <= width {
        return text.to_string();
    }
    let mut cut = text.to_string();
    while !cut.is_empty() {
        cut.pop();
        let candidate = format!("{}…", cut.trim_end());
        if ui
            .painter()
            .layout_no_wrap(candidate.clone(), font.clone(), TEXT)
            .rect
            .width()
            <= width
        {
            return candidate;
        }
    }
    "…".to_string()
}

/// Cheap "random" index that never repeats the current one.
fn unstable_pick(len: usize, avoid: usize) -> usize {
    if len <= 1 {
        return 0;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos() as usize)
        .unwrap_or(0);
    let mut index = nanos % len;
    if index == avoid {
        index = (index + 1) % len;
    }
    index
}

/// Recognise pasted YouTube links so they play without a search round-trip.
pub fn track_from_link(text: &str) -> Option<Track> {
    let text = text.trim();
    if !text.starts_with("http") {
        return None;
    }
    if !(text.contains("youtube.com/watch") || text.contains("youtu.be/") || text.contains("music.youtube.com")) {
        // Non-YouTube links still work through yt-dlp.
        return Some(Track {
            id: text.to_string(),
            title: text.to_string(),
            artist: String::new(),
            duration: None,
            url: text.to_string(),
            thumbnail: String::new(),
        });
    }
    let id = if let Some(rest) = text.split("v=").nth(1) {
        rest.split(['&', '?', '#']).next().unwrap_or(rest).to_string()
    } else if let Some(rest) = text.split("youtu.be/").nth(1) {
        rest.split(['&', '?', '#', '/']).next().unwrap_or(rest).to_string()
    } else {
        return None;
    };
    if id.is_empty() {
        return None;
    }
    Some(Track {
        id: id.clone(),
        title: format!("YouTube video {id}"),
        artist: "YouTube".to_string(),
        duration: None,
        url: format!("https://www.youtube.com/watch?v={id}"),
        thumbnail: format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg"),
    })
}
