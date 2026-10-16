use eframe::egui::{self, Align, Color32, FontId, Layout, Pos2, Rect, RichText, Shape, Stroke, Vec2};
use serde::Deserialize;
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Instant,
};

const RED: Color32 = Color32::from_rgb(244, 43, 65);
const BG: Color32 = Color32::from_rgb(14, 15, 20);
const PANEL: Color32 = Color32::from_rgb(24, 25, 33);
const MUTED: Color32 = Color32::from_rgb(151, 155, 170);

#[derive(Debug, Clone, Deserialize, Default)]
struct Video {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    channel: String,
    #[serde(default)]
    uploader: String,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    thumbnail: String,
    #[serde(default)]
    webpage_url: String,
}

#[derive(Debug, Deserialize, Default)]
struct SearchResult {
    #[serde(default)]
    entries: Vec<Video>,
}

enum JobResult {
    Search(Result<Vec<Video>, String>),
    Download(String),
}

struct GhostMusic {
    started: Instant,
    query: String,
    videos: Vec<Video>,
    selected: Option<usize>,
    status: String,
    busy: bool,
    tx: Sender<JobResult>,
    rx: Receiver<JobResult>,
    progress: f32,
    download_mode: usize,
}

impl Default for GhostMusic {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            started: Instant::now(),
            query: String::new(),
            videos: Vec::new(),
            selected: None,
            status: "Ready when you are. Search for a track or video.".into(),
            busy: false,
            tx,
            rx,
            progress: 0.35,
            download_mode: 0,
        }
    }
}

impl GhostMusic {
    fn search(&mut self) {
        let query = self.query.trim().to_owned();
        if query.is_empty() || self.busy {
            return;
        }
        self.busy = true;
        self.status = format!("Searching YouTube for “{query}”…");
        let tx = self.tx.clone();
        thread::spawn(move || {
            let query_arg = format!("ytsearch10:{query}");
            let output = Command::new("yt-dlp")
                .args(["--dump-single-json", "--flat-playlist", "--no-warnings", &query_arg])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();
            let result = match output {
                Ok(out) if out.status.success() => serde_json::from_slice::<SearchResult>(&out.stdout)
                    .map(|data| data.entries)
                    .map_err(|e| format!("Could not read yt-dlp results: {e}")),
                Ok(out) => Err(format_tool_error(&out.stderr)),
                Err(e) => Err(format!("Could not start yt-dlp ({e}). Install yt-dlp.exe and put it beside Ghost Music or on PATH.")),
            };
            let _ = tx.send(JobResult::Search(result));
        });
    }

    fn download_selected(&mut self) {
        if self.busy {
            return;
        }
        let Some(video) = self.selected.and_then(|i| self.videos.get(i)).cloned() else {
            self.status = "Choose a result first, then download it.".into();
            return;
        };
        let url = if video.webpage_url.is_empty() {
            format!("https://www.youtube.com/watch?v={}", video.id)
        } else {
            video.webpage_url.clone()
        };
        self.busy = true;
        let mode = self.download_mode;
        self.status = format!("Preparing download: {}…", video.title);
        let tx = self.tx.clone();
        thread::spawn(move || {
            let folder = download_folder();
            let _ = std::fs::create_dir_all(&folder);
            let template = folder.join("%(title).180B [%(id)s].%(ext)s");
            let template = template.to_string_lossy().to_string();
            let mut cmd = Command::new("yt-dlp");
            if mode == 0 {
                cmd.args(["-x", "--audio-format", "mp3", "--audio-quality", "0"]);
            } else {
                cmd.args(["-f", "bestvideo+bestaudio/best", "--merge-output-format", "mp4"]);
            }
            let output = cmd.args(["-o", &template, &url]).output();
            let message = match output {
                Ok(out) if out.status.success() => format!("Download complete! Saved in: {}", folder.display()),
                Ok(out) => format!("Download failed: {}", format_tool_error(&out.stderr)),
                Err(e) => format!("Could not start yt-dlp ({e}). Install yt-dlp.exe and put it beside Ghost Music or on PATH."),
            };
            let _ = tx.send(JobResult::Download(message));
        });
    }

    fn poll_jobs(&mut self) {
        while let Ok(job) = self.rx.try_recv() {
            self.busy = false;
            match job {
                JobResult::Search(Ok(videos)) => {
                    let count = videos.len();
                    self.videos = videos;
                    self.selected = if count > 0 { Some(0) } else { None };
                    self.status = if count == 0 { "No results found. Try another search.".into() } else { format!("Found {count} results.") };
                }
                JobResult::Search(Err(error)) => self.status = error,
                JobResult::Download(message) => self.status = message,
            }
        }
    }

    fn draw_ghost(painter: &egui::Painter, center: Pos2, size: f32, color: Color32, rotation: f32, headphones: bool, waves: bool) {
        let local = [
            (-0.48, 0.18), (-0.48, -0.18), (-0.42, -0.38), (-0.27, -0.49),
            (0.0, -0.53), (0.27, -0.49), (0.42, -0.38), (0.48, -0.18),
            (0.48, 0.40), (0.29, 0.30), (0.12, 0.42), (0.0, 0.32),
            (-0.14, 0.43), (-0.30, 0.31), (-0.48, 0.40),
        ];
        let rotate = |x: f32, y: f32| {
            let x = x * size;
            let y = y * size;
            Pos2::new(center.x + x * rotation.cos() - y * rotation.sin(), center.y + x * rotation.sin() + y * rotation.cos())
        };
        let points: Vec<Pos2> = local.iter().map(|(x, y)| rotate(*x, *y)).collect();
        painter.add(Shape::convex_polygon(points, color, Stroke::new(1.5, color)));
        for (x, y) in [(-0.17, -0.08), (0.17, -0.08)] {
            let eye = rotate(x, y);
            painter.circle_filled(eye, (size * 0.045).max(2.0), BG);
        }
        let mouth = rotate(0.0, 0.06);
        painter.circle_stroke(mouth, (size * 0.045).max(2.0), Stroke::new(2.0, BG));
        if headphones {
            let r = size * 0.51;
            painter.circle_stroke(center, r, Stroke::new((size * 0.07).max(3.0), Color32::from_rgb(245, 245, 250)));
            let left = rotate(-0.49, -0.03);
            let right = rotate(0.49, -0.03);
            painter.line_segment([left, Pos2::new(left.x - size * 0.02, left.y + size * 0.16)], Stroke::new(size * 0.12, Color32::from_rgb(245, 245, 250)));
            painter.line_segment([right, Pos2::new(right.x + size * 0.02, right.y + size * 0.16)], Stroke::new(size * 0.12, Color32::from_rgb(245, 245, 250)));
        }
        if waves {
            for i in 0..3 {
                let h = size * (0.2 + i as f32 * 0.13);
                let alpha = 220_u8.saturating_sub(i as u8 * 55);
                let wave_color = Color32::from_rgba_unmultiplied(RED.r(), RED.g(), RED.b(), alpha);
                let xoff = size * (0.68 + i as f32 * 0.16);
                painter.line_segment([Pos2::new(center.x + xoff, center.y - h), Pos2::new(center.x + xoff, center.y + h)], Stroke::new(3.0, wave_color));
                painter.line_segment([Pos2::new(center.x - xoff, center.y - h), Pos2::new(center.x - xoff, center.y + h)], Stroke::new(3.0, wave_color));
            }
        }
    }

    fn startup(&self, ctx: &egui::Context) -> bool {
        let elapsed = self.started.elapsed().as_secs_f32();
        if elapsed >= 15.0 {
            return false;
        }
        ctx.request_repaint();
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(BG)).show(ctx, |ui| {
            let rect = ui.max_rect();
            let center = rect.center();
            let painter = ui.painter();
            let phase = elapsed / 15.0;
            let rotation = if elapsed < 4.0 { elapsed / 4.0 * std::f32::consts::TAU } else { std::f32::consts::TAU };
            let headphones = elapsed >= 4.0;
            let waves = elapsed >= 8.0;
            let drop_y = if elapsed < 4.0 { -30.0 } else if elapsed < 6.0 { -30.0 + (elapsed - 4.0) / 2.0 * 35.0 } else { 5.0 };
            painter.circle_filled(center, 170.0 + phase * 15.0, Color32::from_rgb(23, 17, 25));
            Self::draw_ghost(painter, Pos2::new(center.x, center.y + drop_y), 115.0, RED, rotation, headphones, waves);
            ui.allocate_ui_at_rect(Rect::from_center_size(Pos2::new(center.x, center.y + 205.0), Vec2::new(rect.width(), 70.0)), |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("GHOST MUSIC").font(FontId::proportional(31.0)).strong().color(Color32::WHITE));
                    let caption = if elapsed < 4.0 { "A new sound is awakening…" } else if elapsed < 8.0 { "Putting on the headphones…" } else { "Let the tones flow." };
                    ui.label(RichText::new(caption).size(15.0).color(MUTED));
                });
            });
            ui.painter().text(Pos2::new(rect.right() - 28.0, rect.bottom() - 22.0), egui::Align2::RIGHT_BOTTOM, "GHOST MUSIC  •  v0.1.0", FontId::proportional(10.0), MUTED);
        });
        true
    }
}

fn format_tool_error(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let trimmed = text.trim();
    if trimmed.is_empty() { "yt-dlp returned an error.".into() } else { trimmed.chars().take(600).collect() }
}

fn download_folder() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("Downloads")
        .join("Ghost Music")
}

fn format_duration(seconds: Option<f64>) -> String {
    let Some(seconds) = seconds else { return "—:—".into() };
    let s = seconds.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

impl eframe::App for GhostMusic {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_jobs();
        if self.startup(ctx) {
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(150));
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(BG)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                Self::draw_ghost(ui.painter(), Pos2::new(ui.min_rect().left() + 23.0, ui.min_rect().top() + 22.0), 25.0, RED, 0.0, true, false);
                ui.add_space(48.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("GHOST MUSIC").size(20.0).strong().color(Color32::WHITE));
                    ui.label(RichText::new("Your sound. Your world.").size(11.0).color(MUTED));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("WINDOWS • RUST").size(10.0).color(MUTED));
                });
            });
            ui.add_space(18.0);
            ui.separator();
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("DISCOVER").size(13.0).strong().color(RED));
                ui.add_space(12.0);
                let search = ui.add_sized([ui.available_width() - 94.0, 38.0], egui::TextEdit::singleline(&mut self.query).hint_text("Search songs, artists, mixes, videos…"));
                let clicked = ui.add_enabled(!self.busy, egui::Button::new(RichText::new("Search").strong()).fill(RED).min_size(Vec2::new(78.0, 38.0))).clicked();
                if clicked || (search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    self.search();
                }
            });
            ui.add_space(14.0);
            ui.columns(2, |columns| {
                columns[0].vertical(|ui| {
                    ui.label(RichText::new("SEARCH RESULTS").size(12.0).strong().color(MUTED));
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().id_salt("results").max_height(ui.available_height() - 5.0).show(ui, |ui| {
                        if self.videos.is_empty() {
                            ui.add_space(18.0);
                            ui.label(RichText::new("Your next favourite track is out there.").color(Color32::WHITE));
                            ui.label(RichText::new("Search above to find it with yt-dlp.").color(MUTED));
                        }
                        for (index, video) in self.videos.iter().enumerate() {
                            let selected = self.selected == Some(index);
                            let frame = egui::Frame::new().fill(if selected { Color32::from_rgb(57, 25, 34) } else { PANEL }).corner_radius(10.0).inner_margin(10.0);
                            let response = frame.show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(48.0), egui::Sense::hover());
                                    ui.painter().rect_filled(rect, 7.0, Color32::from_rgb(43, 28, 37));
                                    Self::draw_ghost(ui.painter(), rect.center(), 29.0, RED, 0.0, false, false);
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new(&video.title).strong().color(Color32::WHITE).size(13.0));
                                        ui.label(RichText::new(if !video.channel.is_empty() { &video.channel } else { &video.uploader }).color(MUTED).size(11.0));
                                        ui.label(RichText::new(format_duration(video.duration)).color(RED).size(10.0));
                                    });
                                });
                            }).response;
                            if response.interact(egui::Sense::click()).clicked() {
                                self.selected = Some(index);
                            }
                            ui.add_space(6.0);
                        }
                    });
                });
                columns[1].vertical(|ui| {
                    ui.label(RichText::new("NOW PLAYING").size(12.0).strong().color(MUTED));
                    ui.add_space(6.0);
                    egui::Frame::new().fill(PANEL).corner_radius(14.0).inner_margin(18.0).show(ui, |ui| {
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 170.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 12.0, Color32::from_rgb(33, 20, 31));
                        let center = rect.center();
                        Self::draw_ghost(ui.painter(), center, 86.0, RED, 0.0, true, true);
                        ui.add_space(10.0);
                        if let Some(video) = self.selected.and_then(|i| self.videos.get(i)) {
                            ui.label(RichText::new(&video.title).strong().size(16.0).color(Color32::WHITE));
                            ui.label(RichText::new(if !video.channel.is_empty() { &video.channel } else { &video.uploader }).color(MUTED));
                        } else {
                            ui.label(RichText::new("Pick a sound to get started").strong().size(16.0).color(Color32::WHITE));
                            ui.label(RichText::new("Your selected track will appear here.").color(MUTED));
                        }
                        ui.add_space(16.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("0:00").size(10.0).color(MUTED));
                            let slider_width = ui.available_width() - 44.0;
                            ui.add_sized([slider_width.max(50.0), 18.0], egui::Slider::new(&mut self.progress, 0.0..=1.0).show_value(false).trailing_fill(true));
                            ui.label(RichText::new("—:—").size(10.0).color(MUTED));
                        });
                        // The timeline thumb is styled as a ghost in the visual player; the slider remains keyboard accessible.
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("👻").size(21.0).color(RED));
                            ui.label(RichText::new("Ghost timeline").size(11.0).color(MUTED));
                        });
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut self.download_mode, 0, "♫  Audio / MP3");
                            ui.selectable_value(&mut self.download_mode, 1, "▣  Video / MP4");
                        });
                        if ui.add_enabled(!self.busy && self.selected.is_some(), egui::Button::new(RichText::new("↓  Download selected").strong()).fill(RED).min_size(Vec2::new(ui.available_width(), 38.0))).clicked() {
                            self.download_selected();
                        }
                    });
                });
            });
            ui.add_space(8.0);
            egui::Frame::new().fill(Color32::from_rgb(20, 21, 27)).corner_radius(8.0).inner_margin(9.0).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(if self.busy { "● WORKING" } else { "● STATUS" }).color(if self.busy { RED } else { Color32::from_rgb(83, 205, 139) }).strong().size(11.0));
                    ui.label(RichText::new(&self.status).color(MUTED).size(11.0));
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Tip: install yt-dlp.exe; FFmpeg is recommended for MP3 conversion.").size(10.0).color(MUTED));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.hyperlink_to(RichText::new("yt-dlp ↗").size(10.0).color(RED), "https://github.com/yt-dlp/yt-dlp/releases/latest");
                });
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Ghost Music")
            .with_inner_size([1050.0, 760.0])
            .with_min_inner_size([760.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native("Ghost Music", options, Box::new(|_cc| Ok(Box::<GhostMusic>::default())))
}
