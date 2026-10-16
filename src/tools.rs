//! Locating and invoking the external tools Ghost Music shells out to:
//! `yt-dlp` (search + download) and `ffmpeg` (mp3 conversion / muxing).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

/// Executable name, with `.exe` on Windows.
pub fn exe(name: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Directories searched in order: next to the .exe, the working directory,
/// the project root (during `cargo run`), a `bin/` folder, then PATH.
fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(current) = std::env::current_exe() {
        if let Some(parent) = current.parent() {
            dirs.push(parent.to_path_buf());
            dirs.push(parent.join("bin"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        if !dirs.contains(&cwd) {
            dirs.push(cwd.clone());
        }
        dirs.push(cwd.join("bin"));
    }
    if let Some(manifest) = option_env!("CARGO_MANIFEST_DIR") {
        let manifest = PathBuf::from(manifest);
        if !dirs.contains(&manifest) {
            dirs.push(manifest);
        }
    }
    dirs
}

fn on_path(name: &str) -> bool {
    let path = match std::env::var_os("PATH") {
        Some(path) => path,
        None => return false,
    };
    std::env::split_paths(&path).any(|dir| dir.join(name).is_file())
}

fn bundled(name: &str) -> Option<PathBuf> {
    let file = exe(name);
    search_dirs()
        .into_iter()
        .map(|dir| dir.join(&file))
        .find(|candidate| candidate.is_file())
}

/// Resolve `name` to something spawnable: an absolute path to a bundled copy,
/// otherwise the bare name (which lets the OS search PATH).
pub fn resolve(name: &str) -> String {
    bundled(name)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| exe(name))
}

/// Is the tool available (bundled or on PATH)? Cached per process.
pub fn available(name: &'static str) -> bool {
    static CACHE: OnceLock<std::sync::Mutex<std::collections::HashMap<&'static str, bool>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    if let Ok(map) = cache.lock() {
        if let Some(found) = map.get(name) {
            return *found;
        }
    }
    let found = bundled(name).is_some() || on_path(&exe(name));
    if let Ok(mut map) = cache.lock() {
        map.insert(name, found);
    }
    found
}

/// `yt-dlp --version`, used to show the user which engine they are running.
pub fn tool_version(name: &str) -> Option<String> {
    if !available_static(name) {
        return None;
    }
    let output = Command::new(resolve(name))
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text.lines().next().unwrap_or_default().to_string())
    }
}

/// `available` for non-'static names (used where the caller owns the string).
fn available_static(name: &str) -> bool {
    bundled(name).is_some() || on_path(&exe(name))
}

/// Where downloads go: the user's Downloads folder, then `Ghost Music`.
pub fn default_download_dir() -> PathBuf {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
    } else {
        std::env::var_os("HOME").map(PathBuf::from)
    }
    .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    base.join("Downloads").join("Ghost Music")
}

/// Cache directory for streams and artwork.
pub fn cache_dir() -> PathBuf {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
    } else {
        std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from)
    }
    .unwrap_or_else(std::env::temp_dir);
    base.join("Ghost Music").join("cache")
}

/// First ~600 characters of a stderr blob, for the status bar.
pub fn stderr_hint(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let mut lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return "yt-dlp returned an error.".to_string();
    }
    // yt-dlp prints progress first and the actual error last.
    let message = if lines.len() > 1 && lines[0].to_uppercase().starts_with("ERROR") {
        lines.remove(0)
    } else {
        lines[lines.len() - 1]
    };
    let message = message.strip_prefix("ERROR: ").unwrap_or(message);
    message.chars().take(600).collect()
}

/// Human readable "12.3 MB" / "845 KB".
pub fn human_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let bytes = bytes as f64;
    if bytes >= KB * KB * KB {
        format!("{:.2} GB", bytes / (KB * KB * KB))
    } else if bytes >= KB * KB {
        format!("{:.1} MB", bytes / (KB * KB))
    } else if bytes >= KB {
        format!("{:.0} KB", bytes / KB)
    } else {
        format!("{bytes:.0} B")
    }
}

/// True when `client` can be reached on the given host:port (TCP handshake
/// only). Used by `--selftest` so CI can prove the local API works.
pub fn port_open(host: &str, port: u16) -> bool {
    use std::net::{TcpStream, ToSocketAddrs};
    let Ok(mut addrs) = (host, port).to_socket_addrs() else {
        return false;
    };
    addrs.any(|addr| TcpStream::connect_timeout(&addr, Duration::from_millis(400)).is_ok())
}

/// Split a filename-ish template path into (dir, template).
pub fn join_template(dir: &Path, template: &str) -> String {
    dir.join(template).to_string_lossy().to_string()
}

