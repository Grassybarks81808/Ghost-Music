//! The yt-dlp engine.
//!
//! * [`Engine`] is the shared state between the UI and the local HTTP API:
//!   the stream-URL cache, the API command queue, the download folder and the
//!   published player state.
//! * The `spawn_*` functions run yt-dlp on worker threads and report progress
//!   back to the UI over an `mpsc` channel.
//!
//! Playback resolves a direct media URL with `yt-dlp -g` and caches it. All
//! audio then flows through the local API server (`/stream/{id}`) so the UI and
//! the HTTP API behave identically.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::model::{SearchEntry, Track};
use crate::tools;

/// How long a resolved media URL stays valid before we re-resolve it.
const STREAM_TTL: Duration = Duration::from_secs(5 * 60 * 60);

/// Commands the HTTP API can ask the UI to perform.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum ApiCommand {
    Play { track: Track, #[serde(default = "yes")] autoplay: bool },
    PlayQueue { tracks: Vec<Track>, #[serde(default)] start_index: usize, #[serde(default = "yes")] autoplay: bool },
    Enqueue { tracks: Vec<Track>, #[serde(default)] next: bool },
    Pause,
    Resume,
    Toggle,
    Next,
    Prev,
    Stop,
    Seek { seconds: f64 },
    SetVolume { volume: f32 },
    Shuffle,
    Repeat,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    Direct,
    Hls,
}

impl StreamKind {
    pub fn as_str(self) -> &'static str {
        match self {
            StreamKind::Direct => "direct",
            StreamKind::Hls => "hls",
        }
    }

    /// HLS is remuxed to a temporary m4a file before playback; direct
    /// progressive downloads are streamed by rodio as they arrive.
    pub fn needs_remux(self) -> bool {
        matches!(self, StreamKind::Hls)
    }
}

pub struct StreamEntry {
    pub url: String,
    pub kind: StreamKind,
    pub expires: Instant,
    /// ISO-8601 expiry reported back to API clients when known.
    pub http_headers: Vec<(String, String)>,
}

/// Messages sent from worker threads to the UI.
#[derive(Debug)]
pub enum JobUpdate {
    SearchDone { query: String, result: Result<Vec<SearchEntry>, String> },
    StreamReady { key: String, url: String, kind: StreamKind },
    StreamFailed { key: String, error: String },
    DownloadStarted { title: String },
    DownloadLog(String),
    DownloadDone { success: bool, message: String, folder: String },
}

#[derive(Default)]
pub struct Engine {
    api_port: Mutex<u16>,
    streams: Mutex<HashMap<String, StreamEntry>>,
    resolving: Mutex<HashSet<String>>,
    commands: Mutex<VecDeque<ApiCommand>>,
    /// Records that /stream/{id} served audio, for the API events endpoint.
    served: Mutex<VecDeque<String>>,
    /// Where downloads are written; shared with the HTTP API.
    download_dir: Mutex<Option<std::path::PathBuf>>,
    /// Snapshot of the player published by the UI for `GET /player/state`.
    state: Mutex<Option<serde_json::Value>>,
}

impl Engine {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn set_api_port(&self, port: u16) {
        if let Ok(mut current) = self.api_port.lock() {
            *current = port;
        }
    }

    pub fn api_port(&self) -> u16 {
        self.api_port.lock().map(|port| *port).unwrap_or(0)
    }

    /// URL the player feeds to rodio: always the local API so that UI and HTTP
    /// API share one code path.
    pub fn local_stream_url(&self, track: &Track) -> String {
        format!(
            "http://127.0.0.1:{}/stream/{}",
            self.api_port(),
            urlencoding(&track.id)
        )
    }

    pub fn stream(&self, key: &str) -> Option<(String, StreamKind, Vec<(String, String)>)> {
        let map = self.streams.lock().ok()?;
        let entry = map.get(key)?;
        if entry.expires <= Instant::now() {
            return None;
        }
        Some((entry.url.clone(), entry.kind, entry.http_headers.clone()))
    }

    /// Remember a resolved media URL (used by the UI worker and the HTTP API).
    pub fn remember_stream(&self, key: String, url: String, kind: StreamKind, headers: Vec<(String, String)>) {
        if let Ok(mut map) = self.streams.lock() {
            map.insert(
                key,
                StreamEntry { url, kind, expires: Instant::now() + STREAM_TTL, http_headers: headers },
            );
        }
    }

    /// Mark `key` as being resolved so we do not spawn duplicate jobs.
    pub fn begin_resolve(&self, key: &str) -> bool {
        match self.resolving.lock() {
            Ok(mut set) => set.insert(key.to_string()),
            Err(_) => false,
        }
    }

    pub fn end_resolve(&self, key: &str) {
        if let Ok(mut set) = self.resolving.lock() {
            set.remove(key);
        }
    }

    pub fn is_resolving(&self, key: &str) -> bool {
        self.resolving
            .lock()
            .map(|set| set.contains(key))
            .unwrap_or(false)
    }

    pub fn push_command(&self, command: ApiCommand) {
        if let Ok(mut queue) = self.commands.lock() {
            queue.push_back(command);
        }
    }

    pub fn drain_commands(&self) -> Vec<ApiCommand> {
        match self.commands.lock() {
            Ok(mut queue) => queue.drain(..).collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn note_served(&self, key: &str) {
        if let Ok(mut log) = self.served.lock() {
            log.push_back(key.to_string());
            while log.len() > 50 {
                log.pop_front();
            }
        }
    }

    pub fn served(&self) -> Vec<String> {
        self.served.lock().map(|log| log.iter().cloned().collect()).unwrap_or_default()
    }

    /// Download folder: shared between the settings window and the HTTP API.
    pub fn download_dir(&self) -> std::path::PathBuf {
        self.download_dir
            .lock()
            .ok()
            .and_then(|dir| dir.clone())
            .unwrap_or_else(tools::default_download_dir)
    }

    pub fn set_download_dir(&self, dir: std::path::PathBuf) {
        if let Ok(mut current) = self.download_dir.lock() {
            *current = Some(dir);
        }
    }

    /// The UI publishes a JSON snapshot of the player a few times per second so
    /// `GET /player/state` always reflects what is on screen.
    pub fn publish_state(&self, state: serde_json::Value) {
        if let Ok(mut current) = self.state.lock() {
            *current = Some(state);
        }
    }

    pub fn state(&self) -> Option<serde_json::Value> {
        self.state.lock().ok().and_then(|state| state.clone())
    }
}

/// Minimal percent-encoding for path segments (video ids are already safe, but
/// hand-written ids from the API may not be).
pub fn urlencoding(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Common yt-dlp arguments: quiet-ish, no colour codes, no disk cache surprises.
fn base_args(command: &mut Command) {
    command
        .arg("--no-colors")
        .arg("--newline")
        .arg("--no-playlist")
        .arg("--socket-timeout")
        .arg("15")
        .arg("--retries")
        .arg("5")
        .arg("--fragment-retries")
        .arg("5");
}

fn tool_missing() -> String {
    "yt-dlp was not found. Put yt-dlp.exe next to Ghost Music (or on PATH) and restart the app."
        .to_string()
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

pub fn spawn_search(tx: Sender<JobUpdate>, query: String, limit: usize) {
    thread::spawn(move || {
        let result = search_sync(&query, limit);
        let _ = tx.send(JobUpdate::SearchDone { query, result });
    });
}

/// Blocking search used by both the UI worker and the HTTP API.
pub fn search_sync(query: &str, limit: usize) -> Result<Vec<SearchEntry>, String> {
    if !tools::available("yt-dlp") {
        return Err(tool_missing());
    }
    let target = format!("ytsearch{limit}:{query}");
    let mut command = Command::new(tools::resolve("yt-dlp"));
    base_args(&mut command);
    let output = command
        .arg("--dump-single-json")
        .arg("--flat-playlist")
        .arg("--no-warnings")
        .arg(&target)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("Could not start yt-dlp: {error}"))?;

    if !output.status.success() {
        return Err(tools::stderr_hint(&output.stderr));
    }
    #[derive(Deserialize)]
    struct Wrapper {
        #[serde(default)]
        entries: Vec<SearchEntry>,
    }
    let wrapper: Wrapper = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Could not read the yt-dlp result: {error}"))?;
    Ok(wrapper.entries)
}

// ---------------------------------------------------------------------------
// Stream resolution (for playback)
// ---------------------------------------------------------------------------

pub fn spawn_stream_resolve(
    engine: Arc<Engine>,
    tx: Sender<JobUpdate>,
    key: String,
    url: String,
) {
    thread::spawn(move || match resolve_stream_url(&url) {
        Ok((stream_url, kind, headers)) => {
            engine.remember_stream(key.clone(), stream_url.clone(), kind, headers);
            engine.end_resolve(&key);
            let _ = tx.send(JobUpdate::StreamReady { key, url: stream_url, kind });
        }
        Err(error) => {
            engine.end_resolve(&key);
            let _ = tx.send(JobUpdate::StreamFailed { key, error });
        }
    });
}

/// Ask yt-dlp for a direct, seekable media URL.
pub fn resolve_stream_url(page_url: &str) -> Result<(String, StreamKind, Vec<(String, String)>), String> {
    if !tools::available("yt-dlp") {
        return Err(tool_missing());
    }
    // Prefer progressive HTTP audio; HLS-only video is remuxed later.
    let preferred = "bestaudio[protocol*=http]/bestaudio/best[protocol*=http]/bestaudio/best";
    let mut headers = Vec::new();
    if let Ok((url, kind)) = run_get_url(page_url, preferred) {
        if !url.starts_with("m3u8") {
            return Ok((url, kind, headers));
        }
    }
    // Retry once, forcing progressive formats, and pick up HTTP headers that
    // some sources (e.g. googlevideo with cookies) require.
    let (url, kind, extra) = run_get_url_with_headers(page_url, "bestaudio[protocol*=http]/best[protocol*=http]")?;
    headers.extend(extra);
    Ok((url, kind, headers))
}

fn run_get_url(page_url: &str, format: &str) -> Result<(String, StreamKind), String> {
    let (url, kind, _) = run_get_url_with_headers(page_url, format)?;
    Ok((url, kind))
}

fn run_get_url_with_headers(
    page_url: &str,
    format: &str,
) -> Result<(String, StreamKind, Vec<(String, String)>), String> {
    let mut command = Command::new(tools::resolve("yt-dlp"));
    base_args(&mut command);
    let output = command
        .args(["-f", format, "-g", "--no-warnings"])
        .arg(page_url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("Could not start yt-dlp: {error}"))?;

    if !output.status.success() {
        return Err(tools::stderr_hint(&output.stderr));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut urls = text
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("http") || line.starts_with("m3u8"))
        .map(str::to_string);
    let first = urls.next().ok_or_else(|| "yt-dlp returned no playable URL.".to_string())?;
    let mut kind = if first.contains(".m3u8") || first.starts_with("m3u8") {
        StreamKind::Hls
    } else {
        StreamKind::Direct
    };
    // `-g` prints every stream URL of a merged format; when a second URL shows
    // up we asked for a video+audio pair, which the audio-only player cannot
    // use, so treat it as HLS-style (needs ffmpeg/remux) and keep the first.
    if urls.next().is_some() {
        kind = StreamKind::Hls;
    }
    Ok((first, kind, Vec::new()))
}

// ---------------------------------------------------------------------------
// Downloads (with live progress)
// ---------------------------------------------------------------------------

/// 0 = MP3 audio, 1 = MP4 video.
pub fn spawn_download(
    tx: Sender<JobUpdate>,
    track: Track,
    mode: usize,
    folder: std::path::PathBuf,
) {
    thread::spawn(move || {
        let title = track.title.clone();
        let _ = tx.send(JobUpdate::DownloadStarted { title });

        if !tools::available("yt-dlp") {
            let _ = tx.send(JobUpdate::DownloadDone {
                success: false,
                message: tool_missing(),
                folder: folder.display().to_string(),
            });
            return;
        }
        let _ = std::fs::create_dir_all(&folder);

        let mut command = Command::new(tools::resolve("yt-dlp"));
        base_args(&mut command);
        command
            .arg("--windows-filenames")
            .arg("--progress")
            .arg("--concurrent-fragments")
            .arg("4")
            .arg("-o")
            .arg(tools::join_template(&folder, "%(title).180B [%(id)s].%(ext)s"));

        if mode == 0 {
            command
                .args(["-x", "--audio-format", "mp3", "--audio-quality", "0"])
                .args(["--embed-metadata", "--embed-thumbnail", "--convert-thumbnails", "jpg"]);
            if !tools::available("ffmpeg") {
                // Without ffmpeg yt-dlp keeps the original container; tell the
                // user instead of failing silently.
                let _ = tx.send(JobUpdate::DownloadLog(
                    "ffmpeg was not found - the audio keeps its original format (webm/m4a) instead of MP3.\n"
                        .to_string(),
                ));
                command = Command::new(tools::resolve("yt-dlp"));
                base_args(&mut command);
                command
                    .arg("--windows-filenames")
                    .arg("--progress")
                    .arg("-o")
                    .arg(tools::join_template(&folder, "%(title).180B [%(id)s].%(ext)s"))
                    .args(["-f", "bestaudio/best"]);
            }
        } else {
            command
                .args(["-f", "bestvideo+bestaudio/best"])
                .args(["--merge-output-format", "mp4"])
                .arg("--embed-metadata");
        }

        let mut child = match command
            .arg(&track.url)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                let _ = tx.send(JobUpdate::DownloadDone {
                    success: false,
                    message: format!("Could not start yt-dlp: {error}"),
                    folder: folder.display().to_string(),
                });
                return;
            }
        };

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let mut handles = Vec::new();
        if let Some(mut stdout) = stdout {
            let tx = tx.clone();
            handles.push(thread::spawn(move || pump(&mut stdout, &tx)));
        }
        if let Some(mut stderr) = stderr {
            let tx = tx.clone();
            handles.push(thread::spawn(move || pump(&mut stderr, &tx)));
        }
        let status = child.wait();
        for handle in handles {
            let _ = handle.join();
        }

        let (success, message) = match status {
            Ok(status) if status.success() => (
                true,
                format!("Saved to {}", folder.display()),
            ),
            _ => (false, "Download failed - see the log below.".to_string()),
        };
        let _ = tx.send(JobUpdate::DownloadDone {
            success,
            message,
            folder: folder.display().to_string(),
        });
    });
}

/// Forward yt-dlp output to the UI in small pieces so progress feels live.
fn pump(reader: &mut impl Read, tx: &Sender<JobUpdate>) {
    let mut buffer = [0u8; 512];
    let mut pending = String::new();
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        pending.push_str(&strip_ansi(&String::from_utf8_lossy(&buffer[..read])));
        while let Some(cut) = pending.rfind(&['[', '\n'][..]) {
            let piece: String = pending.drain(..=cut).collect();
            if !piece.trim().is_empty() {
                let _ = tx.send(JobUpdate::Log(piece));
            }
        }
        if pending.len() > 400 {
            let piece = std::mem::take(&mut pending);
            let _ = tx.send(JobUpdate::Log(piece));
        }
    }
    if !pending.trim().is_empty() {
        let _ = tx.send(JobUpdate::Log(pending));
    }
}

/// Remove ANSI colour/escape sequences from a chunk of yt-dlp output.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Skip "[...m" style sequences (and any other ESC command).
            if chars.peek() == Some(&'[') {
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else if c == '\r' {
            // Progress updates are re-drawn on the same line.
        } else {
            out.push(c);
        }
    }
    out
}

/// Parsed yt-dlp progress line, shown in the download panel.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProgressInfo {
    pub percent: f32,
    pub total_bytes: Option<u64>,
    pub speed: Option<String>,
    pub eta: Option<String>,
}

impl ProgressInfo {
    /// True when the line looks like a `[download] 12.3% of ...` update.
    pub fn parse(text: &str) -> Option<Self> {
        let percent_end = text.rfind('%')?;
        let before = &text[..percent_end];
        let start = before
            .rfind(|c: char| !(c.is_ascii_digit() || c == '.'))
            .map(|index| index + 1)
            .unwrap_or(0);
        let percent: f32 = before[start..].trim().parse().ok()?;
        if !(0.0..=100.0).contains(&percent) {
            return None;
        }

        let mut info = ProgressInfo { percent, ..Default::default() };
        let rest = &text[percent_end + 1..];
        let tokens: Vec<&str> = rest.split_whitespace().collect();
        let mut index = 0;
        while index < tokens.len() {
            match tokens[index] {
                "of" => {
                    let mut value = tokens.get(index + 1).copied().unwrap_or_default();
                    let mut step = 2;
                    if value == "~" {
                        value = tokens.get(index + 2).copied().unwrap_or_default();
                        step = 3;
                    }
                    info.total_bytes = parse_size(value);
                    index += step;
                }
                "at" => {
                    info.speed = tokens.get(index + 1).map(|token| token.to_string());
                    index += 2;
                }
                "ETA" => {
                    info.eta = tokens.get(index + 1).map(|token| token.to_string());
                    index += 2;
                }
                _ => index += 1,
            }
        }
        Some(info)
    }
}

/// "3.94MiB" -> Some(4131389)
pub fn parse_size(token: &str) -> Option<u64> {
    let token = token.trim();
    if token.is_empty() || token.starts_with("Unknown") {
        return None;
    }
    let split = token
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(token.len());
    let (number, unit) = token.split_at(split);
    let value: f64 = number.parse().ok()?;
    let factor = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1.0,
        "kib" | "kb" => 1024.0,
        "mib" | "mb" => 1024.0 * 1024.0,
        "gib" | "gb" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((value * factor) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_progress_lines() {
        let info = ProgressInfo::parse("[download]  12.3% of   3.94MiB at  1.98MiB/s ETA 00:02").unwrap();
        assert!((info.percent - 12.3).abs() < 0.001);
        assert_eq!(info.total_bytes, Some(4131389));
        assert_eq!(info.speed.as_deref(), Some("1.98MiB/s"));
        assert_eq!(info.eta.as_deref(), Some("00:02"));

        let info = ProgressInfo::parse("[download]  45.0% of ~ 12.00MiB at Unknown B/s ETA Unknown").unwrap();
        assert_eq!(info.total_bytes, Some(12582912));
        assert_eq!(info.eta.as_deref(), Some("Unknown"));

        assert!(ProgressInfo::parse("[download] Destination: out.webm").is_none());
    }

    #[test]
    fn strips_ansi_colours() {
        assert_eq!(strip_ansi("\u{1b}[0;33m[download]\u{1b}[0m hi\r\n"), "[download] hi\n");
    }

    #[test]
    fn encodes_ids() {
        assert_eq!(urlencoding("dQw4w9WgXcQ"), "dQw4w9WgXcQ");
        assert_eq!(urlencoding("a b/c"), "a%20b%2Fc");
    }
}
