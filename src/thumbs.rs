//! Artwork: downloads YouTube thumbnails in the background, decodes them with
//! `image` and uploads them as egui textures. While a thumbnail is loading (or
//! when a track has none) a deterministic procedural cover is drawn instead, so
//! the UI never shows empty boxes.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};

use egui::{ColorImage, TextureHandle, TextureOptions};

use crate::model::fnv1a;
use crate::theme;

/// Maximum number of decoded artwork textures kept on the GPU.
const CACHE_LIMIT: usize = 96;
/// Longest edge we upload (thumbnails are shown at ~260 px at most).
const MAX_EDGE: u32 = 384;

struct Loaded {
    url: String,
    result: Result<(usize, usize, Vec<u8>), String>,
}

pub struct Thumbs {
    cache: HashMap<String, TextureHandle>,
    order: VecDeque<String>,
    pending: HashSet<String>,
    failed: HashSet<String>,
    placeholders: HashMap<String, TextureHandle>,
    sender: Sender<Loaded>,
    receiver: Receiver<Loaded>,
}

impl Thumbs {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            cache: HashMap::new(),
            order: VecDeque::new(),
            pending: HashSet::new(),
            failed: HashSet::new(),
            placeholders: HashMap::new(),
            sender,
            receiver,
        }
    }

    /// Texture for `url` if it is ready, otherwise starts a download.
    pub fn get(&mut self, ctx: &egui::Context, url: &str) -> Option<TextureHandle> {
        if url.is_empty() {
            return None;
        }
        if let Some(texture) = self.cache.get(url) {
            return Some(texture.clone());
        }
        if self.failed.contains(url) {
            return None;
        }
        self.fetch(url);
        let _ = ctx;
        None
    }

    /// Upload any thumbnails that finished decoding.
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(loaded) = self.receiver.try_recv() {
            self.pending.remove(&loaded.url);
            match loaded.result {
                Ok((width, height, pixels)) => {
                    let image = ColorImage::from_rgba_unmultiplied([width, height], &pixels);
                    let texture = ctx.load_texture(
                        format!("ghost-thumb-{}", fnv1a(loaded.url.as_bytes())),
                        image,
                        TextureOptions::LINEAR,
                    );
                    self.insert(loaded.url, texture);
                }
                Err(_) => {
                    self.failed.insert(loaded.url);
                }
            }
        }
    }

    fn insert(&mut self, url: String, texture: TextureHandle) {
        if self.cache.insert(url.clone(), texture).is_none() {
            self.order.push_back(url);
        }
        while self.order.len() > CACHE_LIMIT {
            if let Some(oldest) = self.order.pop_front() {
                self.cache.remove(&oldest);
            }
        }
    }

    /// Start downloading (and uploading) an image. Used for the now-playing art.
    pub fn prefetch(&mut self, url: &str) {
        if url.is_empty() || self.cache.contains_key(url) || self.failed.contains(url) {
            return;
        }
        self.fetch(url);
    }

    fn fetch(&mut self, url: &str) {
        if !self.pending.insert(url.to_string()) {
            return;
        }
        let url = url.to_string();
        let sender = self.sender.clone();
        ehttp::fetch(ehttp::Request::get(url.clone()), move |outcome| {
            let result = match outcome {
                Ok(response) if response.ok => {
                    decode(&response.bytes).ok_or_else(|| "decode failed".to_string())
                }
                Ok(response) => Err(format!("http {}", response.status)),
                Err(error) => Err(error),
            };
            let _ = sender.send(Loaded { url, result });
        });
    }

    /// Deterministic procedural cover used while artwork is missing.
    pub fn placeholder(&mut self, ctx: &egui::Context, key: &str) -> TextureHandle {
        let id = format!("ph-{}", fnv1a(key.as_bytes()));
        if let Some(texture) = self.placeholders.get(&id) {
            return texture.clone();
        }
        let image = procedural_cover(&id);
        let texture = ctx.load_texture(id.clone(), image, TextureOptions::LINEAR);
        self.placeholders.insert(id, texture.clone());
        if self.placeholders.len() > 64 {
            // Placeholders are cheap to regenerate; keep the map small.
            self.placeholders.clear();
            self.placeholders.insert(format!("ph-{}", fnv1a(key.as_bytes())), texture.clone());
        }
        texture
    }
}

impl Default for Thumbs {
    fn default() -> Self {
        Self::new()
    }
}

/// Decode jpeg/png/webp bytes into a (w, h, rgba) triple, downscaled.
fn decode(bytes: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let image = image::load_from_memory(bytes).ok()?;
    let image = if image.width().max(image.height()) > MAX_EDGE {
        image.resize(MAX_EDGE, MAX_EDGE, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    Some((width as usize, height as usize, rgba.into_raw()))
}

/// Two-colour gradient + soft blobs + a translucent ghost, derived from the
/// tile/track key so the same track always gets the same cover.
fn procedural_cover(seed_key: &str) -> ColorImage {
    const SIZE: usize = 192;
    let seed = fnv1a(seed_key.as_bytes());
    let hue = (seed % 360) as f32;
    let (top, bottom) = (
        hsv(hue, 0.45, 0.32),
        hsv((hue + 40.0) % 360.0, 0.62, 0.14),
    );
    let blob_count = 3 + (seed % 3) as usize;

    let mut pixels = vec![0u8; SIZE * SIZE * 4];
    for y in 0..SIZE {
        let t = y as f32 / SIZE as f32;
        for x in 0..SIZE {
            let base = lerp(top, bottom, t);
            pixels[(y * SIZE + x) * 4] = base[0];
            pixels[(y * SIZE + x) * 4 + 1] = base[1];
            pixels[(y * SIZE + x) * 4 + 2] = base[2];
            pixels[(y * SIZE + x) * 4 + 3] = 255;
        }
    }

    // Soft red blobs for texture.
    for index in 0..blob_count {
        let blob_seed = fnv1a(format!("{seed_key}-blob-{index}").as_bytes());
        let cx = (blob_seed % SIZE as u64) as f32;
        let cy = ((blob_seed >> 12) % SIZE as u64) as f32;
        let radius = 26.0 + ((blob_seed >> 24) % 40) as f32;
        for y in 0..SIZE {
            for x in 0..SIZE {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let distance = (dx * dx + dy * dy).sqrt();
                if distance > radius {
                    continue;
                }
                let strength = (1.0 - distance / radius).powf(2.0) * 0.5;
                let index = (y * SIZE + x) * 4;
                let (r, g, b) = (
                    pixels[index] as f32,
                    pixels[index + 1] as f32,
                    pixels[index + 2] as f32,
                );
                pixels[index] = (r * (1.0 - strength) + theme::RED.r() as f32 * strength) as u8;
                pixels[index + 1] = (g * (1.0 - strength) + theme::RED.g() as f32 * strength) as u8;
                pixels[index + 2] = (b * (1.0 - strength) + theme::RED.b() as f32 * strength) as u8;
            }
        }
    }

    // Translucent ghost silhouette in the middle.
    let scale = SIZE as f32 * 0.5;
    let center = (SIZE as f32 / 2.0, SIZE as f32 / 2.0 + 6.0);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let ux = (x as f32 - center.0) / scale;
            let uy = (y as f32 - center.1) / scale;
            if !inside_ghost(ux, uy) {
                continue;
            }
            let index = (y * SIZE + x) * 4;
            for channel in 0..3 {
                let value = pixels[index + channel] as f32;
                pixels[index + channel] = (value * 0.72 + 250.0 * 0.28) as u8;
            }
        }
    }

    ColorImage::from_rgba_unmultiplied([SIZE, SIZE], &pixels)
}

fn inside_ghost(x: f32, y: f32) -> bool {
    use crate::ghost::BODY;
    // Body: point in convex polygon via cross products.
    let mut sign = 0.0;
    for index in 0..BODY.len() {
        let (ax, ay) = BODY[index];
        let (bx, by) = BODY[(index + 1) % BODY.len()];
        let cross = (bx - ax) * (y - ay) - (by - ay) * (x - ax);
        if cross.abs() < f32::EPSILON {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return inside_feet(x, y);
        }
    }
    if y > 0.32 {
        return inside_feet(x, y);
    }
    true
}

fn inside_feet(x: f32, y: f32) -> bool {
    use crate::ghost::FEET;
    for (left, right, apex) in FEET {
        if y < 0.32 || y > 0.43 || x < left || x > right {
            continue;
        }
        // Straight-line interpolation between the sides and the apex.
        let t = (y - 0.32) / (0.43 - 0.32);
        let half = (right - left) / 2.0 * (1.0 - t);
        let middle = (left + right) / 2.0;
        if (x - middle).abs() <= half + 0.001 {
            return true;
        }
        let _ = apex;
    }
    false
}

fn hsv(hue: f32, saturation: f32, value: f32) -> [u8; 3] {
    let c = value * saturation;
    let h = (hue % 360.0) / 60.0;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = value - c;
    [
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    ]
}

fn lerp(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let mix = |x: u8, y: u8| (x as f32 * (1.0 - t) + y as f32 * t) as u8;
    [mix(a[0], b[0]), mix(a[1], b[1]), mix(a[2], b[2])]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_are_deterministic_and_sized() {
        let first = procedural_cover("seed:lofi-beats");
        let second = procedural_cover("seed:lofi-beats");
        assert_eq!(first.size, [192, 192]);
        assert_eq!(first.pixels, second.pixels);
        let other = procedural_cover("seed:focus-flow");
        assert_ne!(first.pixels, other.pixels);
    }

    #[test]
    fn ghost_hit_testing() {
        assert!(inside_ghost(0.0, -0.2)); // head
        assert!(inside_ghost(0.0, 0.0)); // body
        assert!(!inside_ghost(0.9, 0.9)); // outside
        assert!(!inside_ghost(0.0, 0.9)); // below the feet
    }
}
