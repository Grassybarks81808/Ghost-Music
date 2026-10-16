//! Local HTTP API server ("Ghost Music Connect").
//!
//! Bound to `127.0.0.1` on an ephemeral port, this gives other programs (and
//! the bundled remote-control page) access to the player:
//!
//! ```text
//! GET  /                     HTML remote control page
//! GET  /health               app + tool versions, API version
//! GET  /search?q=...&limit=  yt-dlp search as JSON
//! GET  /stream/{id}          proxies the resolved audio for the app player
//! GET  /thumb/{id}?u=...     proxies artwork (avoids CORS when embedded)
//! GET  /player/state         published player state
//! POST /player/play          {id | url | track | q, queue?}
//! POST /player/queue         {tracks | track | id | url | q, next?}
//! POST /player/pause|resume|toggle|next|prev|stop|seek|volume|shuffle|repeat
//! GET  /downloads            files in the download folder
//! POST /download             {id | url | q, mode: "audio" | "video"}
//! ```
//!
//! Implemented with `std::net` only: audio is piped straight through, so
//! playback never writes to disk unless a source is HLS-only - then ffmpeg
//! remuxes it once into the cache folder.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use serde_json::{json, Value};

use crate::engine::{self, ApiCommand, Engine};
use crate::model::Track;
use crate::tools;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const API_VERSION: u32 = 1;
/// Exposed so `--version` / `--help` and the README can quote the API revision.
pub const API_VERSION_INFO: u32 = API_VERSION;
const READ_CHUNK: usize = 32 * 1024;
const LIMIT_ARTWORK: u64 = 6 * 1024 * 1024;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/124.0 Safari/537.36 GhostMusic";

pub struct ApiInfo {
    pub yt_dlp: Option<String>,
    pub ffmpeg: Option<String>,
    pub started: Instant,
    pub requests: AtomicU64,
}

pub struct ApiServer {
    pub port: u16,
    pub engine: Arc<Engine>,
    pub info: Arc<ApiInfo>,
}

impl ApiServer {
    /// Start listening on an ephemeral loopback port.
    pub fn start(
        engine: Arc<Engine>,
        yt_dlp: Option<String>,
        ffmpeg: Option<String>,
    ) -> Option<Arc<Self>> {
        let listener = TcpListener::bind("127.0.0.1:0").ok()?;
        let port = listener.local_addr().ok()?.port();
        let server = Arc::new(Self {
            port,
            engine: engine.clone(),
            info: Arc::new(ApiInfo {
                yt_dlp,
                ffmpeg,
                started: Instant::now(),
                requests: AtomicU64::new(0),
            }),
        });
        engine.set_api_port(port);

        let worker = server.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else {
                    continue;
                };
                let worker = worker.clone();
                thread::spawn(move || {
                    worker.info.requests.fetch_add(1, Ordering::Relaxed);
                    let _ = stream.set_nodelay(true);
                    if let Err(error) = worker.handle(stream) {
                        use std::io::ErrorKind;
                        if !matches!(
                            error.kind(),
                            ErrorKind::BrokenPipe | ErrorKind::ConnectionReset | ErrorKind::UnexpectedEof
                        ) {
                            eprintln!("Ghost Music API: {error}");
                        }
                    }
                });
            }
        });

        prune_cache();
        Some(server)
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn handle(&self, mut stream: TcpStream) -> std::io::Result<()> {
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        stream.set_write_timeout(Some(Duration::from_secs(60)))?;
        let mut reader = BufReader::new(stream.try_clone()?);

        let Some(request) = read_request(&mut reader)? else {
            return Ok(());
        };
        let (route, query) = split_path(&request.path);
        let route = route.to_string();
        let method = request.method.as_str();

        match (method, route.as_str()) {
            ("GET", "/") | ("GET", "/index.html") => {
                write_response(&mut stream, 200, "text/html; charset=utf-8", INDEX_HTML.as_bytes())
            }
            ("GET", "/health") => self.json(&mut stream, 200, self.health()),
            ("GET", "/search") => {
                let text = query.get("q").cloned().unwrap_or_default();
                if text.trim().is_empty() {
                    return self.json(&mut stream, 400, json!({ "error": "missing ?q=<search>" }));
                }
                let limit = query
                    .get("limit")
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(20)
                    .clamp(1, 50);
                match engine::search_sync(&text, limit) {
                    Ok(entries) => {
                        let results: Vec<Value> = entries
                            .iter()
                            .map(|entry| {
                                let track = Track::from_entry(entry);
                                json!({
                                    "id": track.id,
                                    "title": track.title,
                                    "artist": track.artist,
                                    "duration": track.duration,
                                    "duration_text": crate::model::format_duration(track.duration),
                                    "url": track.url,
                                    "thumbnail": track.thumbnail,
                                    "views": crate::model::format_views(entry.view_count),
                                })
                            })
                            .collect();
                        self.json(
                            &mut stream,
                            200,
                            json!({ "query": text, "count": results.len(), "results": results }),
                        )
                    }
                    Err(error) => self.json(&mut stream, 502, json!({ "error": error })),
                }
            }
            ("GET", route) if route.starts_with("/stream/") => {
                let key = url_decode(route.trim_start_matches("/stream/"));
                self.proxy_stream(&mut stream, &mut reader, &key)
            }
            ("GET", route) if route.starts_with("/thumb/") => {
                let upstream = query.get("u").cloned().unwrap_or_default();
                self.proxy_thumb(&mut stream, &upstream)
            }
            ("GET", "/player/state") => match self.engine.state() {
                Some(state) => self.json(&mut stream, 200, state),
                None => self.json(&mut stream, 200, json!({ "status": "starting" })),
            },
            ("POST", "/player/play") => {
                let value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
                self.play(&mut stream, value)
            }
            ("POST", "/player/queue") => {
                let value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
                self.queue(&mut stream, value)
            }
            ("POST", "/player/pause") => self.command(&mut stream, ApiCommand::Pause),
            ("POST", "/player/resume") => self.command(&mut stream, ApiCommand::Resume),
            ("POST", "/player/toggle") => self.command(&mut stream, ApiCommand::Toggle),
            ("POST", "/player/next") => self.command(&mut stream, ApiCommand::Next),
            ("POST", "/player/prev") | ("POST", "/player/previous") => {
                self.command(&mut stream, ApiCommand::Prev)
            }
            ("POST", "/player/stop") => self.command(&mut stream, ApiCommand::Stop),
            ("POST", "/player/shuffle") => self.command(&mut stream, ApiCommand::Shuffle),
            ("POST", "/player/repeat") => self.command(&mut stream, ApiCommand::Repeat),
            ("POST", "/player/seek") => {
                let value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
                let seconds = value
                    .get("seconds")
                    .or_else(|| value.get("position"))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                self.command(&mut stream, ApiCommand::Seek { seconds })
            }
            ("POST", "/player/volume") => {
                let value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
                let volume = value
                    .get("volume")
                    .or_else(|| value.get("level"))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.8)
                    .clamp(0.0, 1.0) as f32;
                self.command(&mut stream, ApiCommand::SetVolume { volume })
            }
            ("GET", "/downloads") => {
                let folder = self.engine.download_dir();
                let mut files = Vec::new();
                if let Ok(entries) = std::fs::read_dir(&folder) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            let bytes = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
                            files.push(json!({
                                "file": path.file_name().map(|name| name.to_string_lossy().to_string()),
                                "path": path.display().to_string(),
                                "bytes": bytes,
                                "size": tools::human_bytes(bytes),
                            }));
                        }
                    }
                }
                self.json(
                    &mut stream,
                    200,
                    json!({
                        "folder": folder.display().to_string(),
                        "count": files.len(),
                        "files": files,
                    }),
                )
            }
            ("POST", "/download") => {
                let value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
                self.download(&mut stream, value)
            }
            ("GET", "/events") => self.json(
                &mut stream,
                200,
                json!({
                    "served": self.engine.served(),
                    "uptime_secs": self.info.started.elapsed().as_secs(),
                }),
            ),
            _ => self.json(
                &mut stream,
                404,
                json!({
                    "error": "unknown endpoint",
                    "documentation": format!("http://127.0.0.1:{}/", self.port),
                }),
            ),
        }
    }

    fn health(&self) -> Value {
        json!({
            "app": "Ghost Music",
            "version": VERSION,
            "api_version": API_VERSION,
            "port": self.port,
            "uptime_secs": self.info.started.elapsed().as_secs(),
            "requests": self.info.requests.load(Ordering::Relaxed),
            "yt_dlp": self.info.yt_dlp,
            "ffmpeg": self.info.ffmpeg,
            "download_dir": self.engine.download_dir().display().to_string(),
            "endpoints": [
                "GET /health",
                "GET /search?q=&limit=",
                "GET /stream/{id}",
                "GET /thumb/{id}?u=",
                "GET /player/state",
                "POST /player/play",
                "POST /player/queue",
                "POST /player/pause",
                "POST /player/resume",
                "POST /player/toggle",
                "POST /player/next",
                "POST /player/prev",
                "POST /player/stop",
                "POST /player/seek",
                "POST /player/volume",
                "POST /player/shuffle",
                "POST /player/repeat",
                "GET /downloads",
                "POST /download",
                "GET /events"
            ]
        })
    }

    /// `POST /player/play`
    fn play(&self, stream: &mut TcpStream, value: Value) -> std::io::Result<()> {
        if let Some(queue) = value.get("queue").and_then(Value::as_array) {
            let tracks: Vec<Track> = queue.iter().filter_map(track_from_value).collect();
            if tracks.is_empty() {
                return self.json(stream, 400, json!({ "error": "queue is empty" }));
            }
            let start_index = value.get("start_index").and_then(Value::as_u64).unwrap_or(0) as usize;
            self.engine.push_command(ApiCommand::PlayQueue { tracks, start_index, autoplay: true });
            return self.json(stream, 202, json!({ "queued": "play_queue" }));
        }

        if let Some(track) = value.get("track").and_then(track_from_value) {
            self.engine.push_command(ApiCommand::Play { track, autoplay: true });
            return self.json(stream, 202, json!({ "queued": "play" }));
        }

        if let Some(id) = value.get("id").and_then(Value::as_str) {
            let track = track_from_value(&json!({
                "id": id,
                "title": value.get("title").and_then(Value::as_str).unwrap_or(id),
                "artist": value.get("artist").and_then(Value::as_str).unwrap_or(""),
                "url": format!("https://www.youtube.com/watch?v={id}"),
            }))
            .expect("id is present");
            self.engine.push_command(ApiCommand::Play { track, autoplay: true });
            return self.json(stream, 202, json!({ "queued": "play" }));
        }

        if let Some(url) = value.get("url").and_then(Value::as_str) {
            let track = Track {
                id: url.to_string(),
                title: value.get("title").and_then(Value::as_str).unwrap_or(url).to_string(),
                artist: value.get("artist").and_then(Value::as_str).unwrap_or("").to_string(),
                duration: None,
                url: url.to_string(),
                thumbnail: String::new(),
            };
            self.engine.push_command(ApiCommand::Play { track, autoplay: true });
            return self.json(stream, 202, json!({ "queued": "play" }));
        }

        // Fall back to a search: play the best match (and optionally the rest).
        if let Some(text) = value.get("q").and_then(Value::as_str) {
            let queue_all = value.get("queue").and_then(Value::as_bool).unwrap_or(false);
            match engine::search_sync(text, if queue_all { 20 } else { 5 }) {
                Ok(entries) if !entries.is_empty() => {
                    let tracks: Vec<Track> = entries.iter().map(Track::from_entry).collect();
                    let command = if queue_all {
                        ApiCommand::PlayQueue { tracks, start_index: 0, autoplay: true }
                    } else {
                        ApiCommand::Play { track: tracks[0].clone(), autoplay: true }
                    };
                    self.engine.push_command(command);
                    self.json(stream, 202, json!({ "queued": "play", "query": text }))
                }
                Ok(_) => self.json(stream, 404, json!({ "error": "no results" })),
                Err(error) => self.json(stream, 502, json!({ "error": error })),
            }
        } else {
            self.json(stream, 400, json!({ "error": "provide one of: track, id, url, q" }))
        }
    }

    /// `POST /player/queue` - add tracks to the queue without playing them.
    fn queue(&self, stream: &mut TcpStream, value: Value) -> std::io::Result<()> {
        let mut tracks: Vec<Track> = Vec::new();
        if let Some(list) = value.get("tracks").and_then(Value::as_array) {
            tracks.extend(list.iter().filter_map(track_from_value));
        } else if let Some(track) = value.get("track").and_then(track_from_value) {
            tracks.push(track);
        } else if let Some(id) = value.get("id").and_then(Value::as_str) {
            tracks.push(Track {
                id: id.to_string(),
                title: value.get("title").and_then(Value::as_str).unwrap_or(id).to_string(),
                artist: String::new(),
                duration: None,
                url: format!("https://www.youtube.com/watch?v={id}"),
                thumbnail: String::new(),
            });
        } else if let Some(url) = value.get("url").and_then(Value::as_str) {
            tracks.push(Track {
                id: url.to_string(),
                title: url.to_string(),
                artist: String::new(),
                duration: None,
                url: url.to_string(),
                thumbnail: String::new(),
            });
        } else if let Some(text) = value.get("q").and_then(Value::as_str) {
            match engine::search_sync(text, value.get("limit").and_then(Value::as_u64).unwrap_or(10) as usize) {
                Ok(entries) => tracks.extend(entries.iter().map(Track::from_entry)),
                Err(error) => return self.json(stream, 502, json!({ "error": error })),
            }
        }

        if tracks.is_empty() {
            return self.json(stream, 400, json!({ "error": "provide one of: tracks, track, id, url, q" }));
        }
        let count = tracks.len();
        let next = value.get("next").and_then(Value::as_bool).unwrap_or(false);
        self.engine.push_command(ApiCommand::Enqueue { tracks, next });
        self.json(stream, 202, json!({ "queued": "enqueue", "count": count, "next": next }))
    }

    fn command(&self, stream: &mut TcpStream, command: ApiCommand) -> std::io::Result<()> {
        self.engine.push_command(command.clone());
        self.json(stream, 202, json!({ "queued": command }))
    }

    /// `POST /download`
    fn download(&self, stream: &mut TcpStream, value: Value) -> std::io::Result<()> {
        let mode = match value.get("mode").and_then(Value::as_str).unwrap_or("audio") {
            "video" | "mp4" => 1,
            _ => 0,
        };

        let track = if let Some(track) = value.get("track").and_then(track_from_value) {
            Some(track)
        } else if let Some(id) = value.get("id").and_then(Value::as_str) {
            Some(Track {
                id: id.to_string(),
                title: value.get("title").and_then(Value::as_str).unwrap_or(id).to_string(),
                artist: String::new(),
                duration: None,
                url: format!("https://www.youtube.com/watch?v={id}"),
                thumbnail: String::new(),
            })
        } else if let Some(url) = value.get("url").and_then(Value::as_str) {
            Some(Track {
                id: url.to_string(),
                title: url.to_string(),
                artist: String::new(),
                duration: None,
                url: url.to_string(),
                thumbnail: String::new(),
            })
        } else if let Some(text) = value.get("q").and_then(Value::as_str) {
            match engine::search_sync(text, 1) {
                Ok(entries) if !entries.is_empty() => Some(Track::from_entry(&entries[0])),
                Ok(_) => return self.json(stream, 404, json!({ "error": "no results" })),
                Err(error) => return self.json(stream, 502, json!({ "error": error })),
            }
        } else {
            None
        };

        let Some(track) = track else {
            return self.json(stream, 400, json!({ "error": "provide one of: track, id, url, q" }));
        };

        let folder = self.engine.download_dir();
        let response = json!({
            "queued": "download",
            "title": track.title,
            "mode": if mode == 0 { "audio" } else { "video" },
            "folder": folder.display().to_string(),
        });

        // API downloads write their yt-dlp log to a file instead of the UI.
        let (tx, rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let mut log = String::new();
            for update in rx {
                if let engine::JobUpdate::DownloadLog(piece) = update {
                    log.push_str(&piece);
                }
            }
            if let Some(dir) = crate::store::data_dir() {
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::fs::write(dir.join("api-downloads.log"), log);
            }
        });
        engine::spawn_download(tx, track, mode, folder);

        self.json(stream, 202, response)
    }

    // -- media proxying ----------------------------------------------------

    fn proxy_stream(
        &self,
        stream: &mut TcpStream,
        reader: &mut BufReader<TcpStream>,
        key: &str,
    ) -> std::io::Result<()> {
        let range = read_headers(reader)?;

        // The UI resolves streams in the background; if this id has not been
        // resolved yet, do it here so the API also works on its own
        // (e.g. inside an <audio> tag or from --api-only).
        let (upstream, kind, headers) = match self.engine.stream(key) {
            Some(found) => found,
            None => {
                let watch = if key.starts_with("http") {
                    key.to_string()
                } else {
                    format!("https://www.youtube.com/watch?v={key}")
                };
                match engine::resolve_stream_url(&watch) {
                    Ok((url, kind, headers)) => {
                        self.engine
                            .remember_stream(key.to_string(), url.clone(), kind, headers.clone());
                        (url, kind, headers)
                    }
                    Err(error) => {
                        return self.json(
                            stream,
                            502,
                            json!({ "error": error, "id": key, "hint": "check that yt-dlp is installed and up to date" }),
                        )
                    }
                }
            }
        };

        self.engine.note_served(key);

        // HLS only: remux once with ffmpeg, then serve the prepared file.
        if kind.needs_remux() {
            let path = match prepare_hls(key, &upstream) {
                Ok(path) => path,
                Err(error) => {
                    let body = json!({ "error": error }).to_string();
                    return write_response(stream, 502, "application/json", body.as_bytes());
                }
            };
            return serve_file(stream, &path, range.as_deref());
        }

        let mut request = ureq::get(&upstream).set("User-Agent", USER_AGENT);
        for (name, value) in &headers {
            request = request.set(name, value);
        }
        if let Some(range) = &range {
            request = request.set("Range", range);
        }

        let response = match request.call() {
            Ok(response) => response,
            Err(error) => {
                let body = json!({ "error": format!("upstream request failed: {error}") }).to_string();
                return write_response(stream, 502, "application/json", body.as_bytes());
            }
        };

        let status = response.status();
        let content_type = response.header("Content-Type").unwrap_or("audio/webm").to_string();
        let content_length = response.header("Content-Length").map(str::to_string);
        let content_range = response.header("Content-Range").map(str::to_string);
        let chunked = content_length.is_none();

        let mut head = String::new();
        head.push_str(&format!("HTTP/1.1 {} {}\r\n", status, status_text(status)));
        head.push_str("Access-Control-Allow-Origin: *\r\n");
        head.push_str("Accept-Ranges: bytes\r\n");
        head.push_str(&format!("Content-Type: {content_type}\r\n"));
        if let Some(length) = &content_length {
            head.push_str(&format!("Content-Length: {length}\r\n"));
        }
        if let Some(range) = &content_range {
            head.push_str(&format!("Content-Range: {range}\r\n"));
        }
        if chunked {
            head.push_str("Transfer-Encoding: chunked\r\n");
        }
        head.push_str("Connection: close\r\n\r\n");
        stream.write_all(head.as_bytes())?;

        let mut upstream_reader = response.into_reader();
        let mut buffer = vec![0u8; READ_CHUNK];
        loop {
            match upstream_reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    if chunked && stream.write_all(format!("{read:x}\r\n").as_bytes()).is_err() {
                        break;
                    }
                    if stream.write_all(&buffer[..read]).is_err() {
                        break;
                    }
                    if chunked && stream.write_all(b"\r\n").is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        if chunked {
            let _ = stream.write_all(b"0\r\n\r\n");
        }
        let _ = stream.flush();
        Ok(())
    }

    fn proxy_thumb(&self, stream: &mut TcpStream, upstream: &str) -> std::io::Result<()> {
        if upstream.is_empty() || !upstream.starts_with("http") {
            return self.json(stream, 400, json!({ "error": "missing ?u=<image url>" }));
        }
        match ureq::get(upstream).set("User-Agent", USER_AGENT).call() {
            Ok(response) => {
                let content_type = response.header("Content-Type").unwrap_or("image/jpeg").to_string();
                let mut bytes = Vec::new();
                let _ = response.into_reader().take(LIMIT_ARTWORK).read_to_end(&mut bytes);
                let mut head = String::new();
                head.push_str("HTTP/1.1 200 OK\r\n");
                head.push_str("Access-Control-Allow-Origin: *\r\n");
                head.push_str("Cache-Control: public, max-age=86400\r\n");
                head.push_str(&format!("Content-Type: {content_type}\r\n"));
                head.push_str(&format!("Content-Length: {}\r\n", bytes.len()));
                head.push_str("Connection: close\r\n\r\n");
                stream.write_all(head.as_bytes())?;
                stream.write_all(&bytes)?;
                stream.flush()
            }
            Err(error) => self.json(stream, 502, json!({ "error": error.to_string() })),
        }
    }

    fn json(&self, stream: &mut TcpStream, status: u16, value: Value) -> std::io::Result<()> {
        let body = serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string());
        write_response(stream, status, "application/json; charset=utf-8", body.as_bytes())
    }
}

/// Remux an HLS stream into a cached m4a file so the player can seek in it.
fn prepare_hls(key: &str, upstream: &str) -> Result<PathBuf, String> {
    let dir = tools::cache_dir().join("hls");
    std::fs::create_dir_all(&dir).map_err(|error| format!("cache dir: {error}"))?;
    let safe: String = key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let target = dir.join(format!("{safe}.m4a"));
    if std::fs::metadata(&target).map(|meta| meta.len() > 64 * 1024).unwrap_or(false) {
        return Ok(target);
    }
    if !tools::available("ffmpeg") {
        return Err("This track is only available as an HLS stream and ffmpeg is missing. \
             Put ffmpeg.exe next to Ghost Music or on PATH."
            .to_string());
    }
    let output = Command::new(tools::resolve("ffmpeg"))
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(["-i", upstream])
        // Preparing up to 20 minutes keeps start-up quick; long tracks continue
        // seamlessly when downloaded instead.
        .args(["-t", "1200", "-vn", "-c:a", "copy", "-movflags", "+faststart"])
        .arg(&target)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("could not run ffmpeg: {error}"))?;
    if !output.status.success() {
        let _ = std::fs::remove_file(&target);
        return Err(tools::stderr_hint(&output.stderr));
    }
    Ok(target)
}

/// Serve a file with byte-range support.
fn serve_file(stream: &mut TcpStream, path: &PathBuf, range: Option<&str>) -> std::io::Result<()> {
    use std::io::{Seek, SeekFrom};

    let mut file = std::fs::File::open(path)?;
    let length = file.metadata()?.len();
    let (start, end) = match range.and_then(parse_range) {
        Some((start, end)) => (start, end.min(length.saturating_sub(1))),
        None => (0, length.saturating_sub(1)),
    };
    let partial = range.is_some();

    let mut head = String::new();
    head.push_str(if partial { "HTTP/1.1 206 Partial Content\r\n" } else { "HTTP/1.1 200 OK\r\n" });
    head.push_str("Access-Control-Allow-Origin: *\r\n");
    head.push_str("Accept-Ranges: bytes\r\n");
    head.push_str("Content-Type: audio/mp4\r\n");
    head.push_str(&format!("Content-Length: {}\r\n", end.saturating_sub(start) + 1));
    if partial {
        head.push_str(&format!("Content-Range: bytes {start}-{end}/{length}\r\n"));
    }
    head.push_str("Connection: close\r\n\r\n");
    stream.write_all(head.as_bytes())?;

    file.seek(SeekFrom::Start(start))?;
    let mut remaining = end.saturating_sub(start) + 1;
    let mut buffer = vec![0u8; READ_CHUNK];
    while remaining > 0 {
        let want = (buffer.len() as u64).min(remaining) as usize;
        let read = match file.read(&mut buffer[..want]) {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        if stream.write_all(&buffer[..read]).is_err() {
            break;
        }
        remaining -= read as u64;
    }
    stream.flush()
}

fn parse_range(value: &str) -> Option<(u64, u64)> {
    let value = value.trim().strip_prefix("bytes=")?;
    let (start, end) = value.split_once('-')?;
    let start: u64 = start.trim().parse().ok()?;
    let end: u64 = if end.trim().is_empty() {
        u64::MAX
    } else {
        end.trim().parse().ok()?
    };
    Some((start, end))
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {} {}\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status,
        status_text(status),
        content_type,
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        204 => "No Content",
        206 => "Partial Content",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        502 => "Bad Gateway",
        _ => "OK",
    }
}

struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
}

/// Read the request line, headers and (if present) the body.
fn read_request(reader: &mut BufReader<TcpStream>) -> std::io::Result<Option<Request>> {
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(None);
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or("/").to_string();

    let mut content_length = 0usize;
    read_head(reader, &mut |line| {
        if let Some(value) = line.strip_prefix("content-length:") {
            content_length = value.trim().parse().unwrap_or(0);
        }
    })?;

    let mut body = vec![0u8; content_length.min(1024 * 1024)];
    if !body.is_empty() {
        reader.read_exact(&mut body)?;
    }
    Ok(Some(Request { method, path, body }))
}

/// Read the request head of an already-consumed request (media requests).
fn read_headers(reader: &mut BufReader<TcpStream>) -> std::io::Result<Option<String>> {
    let mut range = None;
    read_head(reader, &mut |line| {
        if let Some(value) = line.strip_prefix("range:") {
            range = Some(value.trim().to_string());
        }
    })?;
    Ok(range)
}

/// Read header lines until the blank line, lower-cased, calling `visit` for each.
fn read_head(
    reader: &mut BufReader<TcpStream>,
    visit: &mut impl FnMut(&str),
) -> std::io::Result<()> {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        let lower = trimmed.to_ascii_lowercase();
        visit(&lower);
    }
    Ok(())
}

fn split_path(path: &str) -> (&str, HashMap<String, String>) {
    let mut query = HashMap::new();
    let (route, raw_query) = match path.split_once('?') {
        Some((route, query)) => (route, query),
        None => (path, ""),
    };
    for pair in raw_query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = match pair.split_once('=') {
            Some((key, value)) => (key, value),
            None => (pair, ""),
        };
        query.insert(url_decode(key), url_decode(value));
    }
    (route, query)
}

pub fn url_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 3 <= bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                    .ok()
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok());
                match hex {
                    Some(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    None => {
                        out.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Build a `Track` from an API JSON object (or a bare URL string).
fn track_from_value(value: &Value) -> Option<Track> {
    if let Some(url) = value.as_str() {
        return Some(Track {
            id: url.to_string(),
            title: url.to_string(),
            artist: String::new(),
            duration: None,
            url: url.to_string(),
            thumbnail: String::new(),
        });
    }
    let id = value.get("id").and_then(Value::as_str)?;
    let url = value
        .get("url")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            if id.starts_with("http") {
                id.to_string()
            } else {
                format!("https://www.youtube.com/watch?v={id}")
            }
        });
    Some(Track {
        id: id.to_string(),
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Unknown title")
            .to_string(),
        artist: value.get("artist").and_then(Value::as_str).unwrap_or("").to_string(),
        duration: value.get("duration").and_then(Value::as_f64),
        thumbnail: value.get("thumbnail").and_then(Value::as_str).unwrap_or("").to_string(),
        url,
    })
}

/// Drop cache entries older than a week (called once at start-up).
fn prune_cache() {
    let dirs = [tools::cache_dir().join("hls"), tools::cache_dir().join("thumbs")];
    let week = Duration::from_secs(7 * 24 * 60 * 60);
    let now = SystemTime::now();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let stale = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .map(|modified| now.duration_since(modified).map(|age| age > week).unwrap_or(false))
                .unwrap_or(false);
            if stale {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

/// `GET` helper used by `--selftest` and the API smoke test.
pub fn http_get(port: u16, path: &str) -> Option<(u16, String)> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(30))).ok()?;
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").as_bytes(),
        )
        .ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    let status = response
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    Some((status, response))
}

/// Used by `--selftest`: verifies the stream registry helpers without network.
pub fn registry_smoke_test(engine: &Engine) -> bool {
    let track = Track {
        id: "ghost-music-selftest".to_string(),
        title: "Self test".to_string(),
        artist: "Ghost Music".to_string(),
        duration: Some(1.0),
        url: "https://example.invalid/watch".to_string(),
        thumbnail: String::new(),
    };
    engine.local_stream_url(&track).ends_with("/stream/ghost-music-selftest")
        && engine.stream(&track.id).is_none()
        && crate::engine::StreamKind::Direct.as_str() == "direct"
        && crate::engine::StreamKind::Hls.needs_remux()
}

/// Bundled remote-control page (served from `/`).
const INDEX_HTML: &str = include_str!("../assets/remote.html");
