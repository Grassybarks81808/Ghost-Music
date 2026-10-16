//! Data model: search results, playable tracks and library persistence types.

use serde::{Deserialize, Serialize};

/// One entry of a yt-dlp search (raw JSON shape).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SearchEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    /// Flat playlist results use `channel`, full extracts use `uploader`.
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub uploader: String,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub thumbnails: Vec<Thumbnail>,
    #[serde(default)]
    pub webpage_url: String,
    #[serde(default)]
    pub view_count: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Thumbnail {
    #[serde(default)]
    pub url: String,
}

impl SearchEntry {
    /// Stable identity: prefer the YouTube id, fall back to the URL.
    pub fn key(&self) -> String {
        if !self.id.is_empty() {
            self.id.clone()
        } else {
            self.webpage_url.clone()
        }
    }

    pub fn artist(&self) -> &str {
        if !self.channel.is_empty() {
            &self.channel
        } else {
            &self.uploader
        }
    }

    pub fn watch_url(&self) -> String {
        if self.webpage_url.is_empty() {
            format!("https://www.youtube.com/watch?v={}", self.id)
        } else {
            self.webpage_url.clone()
        }
    }

    /// Best available thumbnail. Prefers an explicitly provided URL, otherwise
    /// derives one from the video id (the flat extractor omits thumbnails).
    /// `maxresdefault` does not exist for every video; the caller retries with
    /// `hqdefault` when the download fails.
    pub fn thumbnail_candidates(&self) -> Vec<String> {
        let mut urls = Vec::new();
        if let Some(url) = &self.thumbnail {
            if !url.is_empty() {
                urls.push(url.clone());
            }
        }
        // Largest first so the UI gets a crisp image.
        for thumb in self.thumbnails.iter().rev() {
            if !thumb.url.is_empty() && !urls.contains(&thumb.url) {
                urls.push(thumb.url.clone());
            }
        }
        if is_youtube_id(&self.id) {
            for size in ["maxresdefault", "hqdefault"] {
                let url = format!("https://i.ytimg.com/vi/{}/{}.jpg", self.id, size);
                if !urls.contains(&url) {
                    urls.push(url);
                }
            }
        }
        urls
    }

    pub fn thumbnail_key(&self) -> String {
        self.thumbnail_candidates()
            .into_iter()
            .next()
            .unwrap_or_default()
    }
}

pub fn is_youtube_id(id: &str) -> bool {
    id.len() == 11
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A track that can be streamed or downloaded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub duration: Option<f64>,
    pub url: String,
    #[serde(default)]
    pub thumbnail: String,
}

impl Track {
    pub fn from_entry(entry: &SearchEntry) -> Self {
        Self {
            id: entry.key(),
            title: if entry.title.is_empty() {
                "Unknown title".to_string()
            } else {
                entry.title.clone()
            },
            artist: entry.artist().to_string(),
            duration: entry.duration,
            url: entry.watch_url(),
            thumbnail: entry.thumbnail_key(),
        }
    }
}

/// `m:ss` or `h:mm:ss`; em dash when unknown.
pub fn format_duration(seconds: Option<f64>) -> String {
    let Some(seconds) = seconds else {
        return "—:—".to_string();
    };
    if !seconds.is_finite() || seconds < 0.0 {
        return "—:—".to_string();
    }
    let total = seconds.round() as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// `format_duration` for values that are always known (playback position).
pub fn format_clock(seconds: f64) -> String {
    format_duration(Some(seconds.max(0.0)))
}

/// 1234567 -> "1.2M views"
pub fn format_views(count: Option<u64>) -> String {
    match count {
        None => String::new(),
        Some(n) if n >= 1_000_000_000 => format!("{:.1}B views", n as f64 / 1e9),
        Some(n) if n >= 1_000_000 => format!("{:.1}M views", n as f64 / 1e6),
        Some(n) if n >= 1_000 => format!("{:.1}K views", n as f64 / 1e3),
        Some(n) => format!("{n} views"),
    }
}

/// A row of the home grid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tile {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    #[serde(default)]
    pub thumbnail: String,
    #[serde(default)]
    pub seed: u64,
}

/// Small deterministic hash so a seed tile always gets the same colours.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Curated starting points shown on the Home grid before the user searches.
/// The label is what the tile shows, the query is what yt-dlp searches for.
pub const SEEDS: [(&str, &str); 12] = [
    ("Lofi beats to study to", "lofi hip hop radio beats to relax study to"),
    ("Today's top hits", "top hits playlist 2025"),
    ("Rock anthems", "best rock anthems of all time"),
    ("Piano & focus", "calm piano music for focus"),
    ("Afrobeats party", "best afrobeats mix 2025"),
    ("Hip hop classics", "90s hip hop classics playlist"),
    ("Late night R&B", "late night rnb slow jams"),
    ("EDM & dance", "best edm dance mix"),
    ("Movie soundtracks", "best movie soundtrack themes"),
    ("Jazz & soul", "smooth jazz soul classics"),
    ("Bongo Flava", "bongo flava mix tanzania"),
    ("Classical essentials", "classical music masterpieces"),
];

const SEED_EMOJI: [&str; 12] = ["🎧", "🔥", "🎸", "🎹", "🥁", "🎤", "🌙", "⚡", "🎬", "🎷", "🪘", "🎻"];

pub fn seed_tiles() -> Vec<Tile> {
    SEEDS
        .iter()
        .zip(SEED_EMOJI)
        .map(|((label, _query), emoji)| Tile {
            id: format!("seed:{}", fnv1a(label.as_bytes())),
            title: format!("{emoji}  {label}"),
            subtitle: "Tap to start a mix".to_string(),
            thumbnail: String::new(),
            seed: fnv1a(label.as_bytes()),
        })
        .collect()
}

/// The search that powers a seed tile.
pub fn seed_query(tile: &Tile) -> String {
    SEEDS
        .iter()
        .find(|(label, _)| tile.title.ends_with(label))
        .map(|(_, query)| (*query).to_string())
        .unwrap_or_else(|| tile.title.clone())
}
