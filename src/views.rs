//! Content views: Home, Explore (search), Library, Downloads and Settings.
//! Views never mutate the app directly - they push [`Action`]s that `App`
//! applies, which keeps playback and persistence in one place.

use egui::{Align, Color32, FontId, Layout, Pos2, RichText, Sense, Vec2};

use crate::app::{Action, App};
use crate::model::{self, Track};
use crate::theme::*;
use crate::widgets::{self, RowResponse, RowTrack};
use crate::tools;

/// Width of one card in the responsive grids.
fn card_width(available: f32) -> f32 {
    (available / 5.0 - 14.0).clamp(132.0, 196.0)
}

// ---------------------------------------------------------------------------
// Home
// ---------------------------------------------------------------------------

pub fn home(app: &mut App, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .id_salt("home")
        .show(ui, |ui| {
            // Greeting banner.
            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 108.0), Sense::hover());
            ui.painter().rect_filled(rect, egui::CornerRadius::same(14), Color32::from_rgb(30, 17, 25));
            ui.painter().circle_filled(
                Pos2::new(rect.right() - 60.0, rect.center().y),
                62.0,
                Color32::from_rgb(46, 20, 30),
            );
            crate::ghost::draw(
                ui.painter(),
                Pos2::new(rect.right() - 60.0, rect.center().y),
                104.0,
                RED,
                0.0,
                crate::ghost::GhostStyle::with_waves(ui.input(|i| i.time) as f32),
            );
            let hour = match std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() % 86400 / 3600)
                .unwrap_or(12)
            {
                0..=4 => "Still awake?",
                5..=11 => "Good morning",
                12..=16 => "Good afternoon",
                _ => "Good evening",
            };
            ui.painter().text(
                Pos2::new(rect.left() + 22.0, rect.center().y - 14.0),
                egui::Align2::LEFT_CENTER,
                format!("{hour} 👻"),
                FontId::proportional(22.0),
                TEXT,
            );
            let hint = if app.library.recent_searches.is_empty() {
                "Search for a song - or tap a tile below to start a mix."
            } else {
                "Pick up where you left off, or explore something new."
            };
            ui.painter().text(
                Pos2::new(rect.left() + 22.0, rect.center().y + 14.0),
                egui::Align2::LEFT_CENTER,
                hint,
                FontId::proportional(13.0),
                MUTED,
            );
            ui.add_space(16.0);

            // Recent searches.
            if !app.library.recent_searches.is_empty() {
                widgets::section_header(ui, "Recent searches", "");
                ui.add_space(4.0);
                let mut search: Option<String> = None;
                let recents = app.library.recent_searches.clone();
                ui.horizontal_wrapped(|ui| {
                    for entry in recents.iter().take(8) {
                        if widgets::chip(ui, entry, false) {
                            search = Some(entry.clone());
                        }
                    }
                    if widgets::chip(ui, "clear", false) {
                        search = Some(String::new());
                    }
                });
                if let Some(query) = search {
                    if query.is_empty() {
                        actions.push(Action::ClearRecents);
                    } else {
                        actions.push(Action::Search(query));
                    }
                }
                ui.add_space(14.0);
            }

            // Listen again (session history).
            if !app.recent.is_empty() {
                widgets::section_header(ui, "Listen again", "");
                ui.add_space(6.0);
                let width = card_width(ui.available_width());
                let recents = app.recent.clone();
                ui.horizontal_wrapped(|ui| {
                    for track in recents.iter().take(6) {
                        let playing = app
                            .player
                            .track
                            .as_ref()
                            .map(|current| current.id == track.id && app.player.is_playing())
                            .unwrap_or(false);
                        let clicked = widgets::media_card(
                            ui,
                            &mut app.thumbs,
                            &track.title,
                            &track.artist,
                            &track.thumbnail,
                            &track.id,
                            true,
                            playing,
                            width,
                        );
                        if clicked {
                            actions.push(Action::PlayTrack(track.clone()));
                        }
                    }
                });
                ui.add_space(16.0);
            }

            // Seed tiles.
            widgets::section_header(ui, "Explore mixes", "powered by yt-dlp");
            ui.add_space(6.0);
            let width = card_width(ui.available_width());
            let mut tiles = app.seeds.clone();
            let mut clicked_tile: Option<model::Tile> = None;
            ui.horizontal_wrapped(|ui| {
                for tile in tiles.drain(..) {
                    if widgets::media_card(
                        ui,
                        &mut app.thumbs,
                        &tile.title,
                        &tile.subtitle,
                        "",
                        &tile.id,
                        true,
                        false,
                        width,
                    ) {
                        clicked_tile = Some(tile);
                    }
                }
            });
            if let Some(tile) = clicked_tile {
                actions.push(Action::PlaySeed(tile));
            }

            ui.add_space(16.0);
            widgets::section_header(ui, "Your queue", "");
            ui.add_space(4.0);
            if app.library.playlist.is_empty() {
                ui.label(
                    RichText::new("Search results you play land here, like a YouTube Music queue.")
                        .size(12.0)
                        .color(FAINT),
                );
            } else {
                let count = app.library.playlist.len().min(6);
                show_rows(app, ui, actions, count, RowSource::Queue);
            }
            ui.add_space(20.0);
        });
}

// ---------------------------------------------------------------------------
// Explore / search results
// ---------------------------------------------------------------------------

enum RowSource {
    Queue,
    Results,
    Liked,
}

/// Shared renderer for the three list sources (index-based to keep borrows simple).
fn show_rows(
    app: &mut App,
    ui: &mut egui::Ui,
    actions: &mut Vec<Action>,
    limit: usize,
    source: RowSource,
) {
    let count = match source {
        RowSource::Queue => app.library.playlist.len(),
        RowSource::Results => app.results.len(),
        RowSource::Liked => app.library.liked.len(),
    }
    .min(limit.max(1));

    for index in 0..count {
        let track = match source {
            RowSource::Queue => app.library.playlist[index].clone(),
            RowSource::Results => Track::from_entry(&app.results[index]),
            RowSource::Liked => app.library.liked[index].clone(),
        };
        let playing = app
            .player
            .track
            .as_ref()
            .map(|current| current.id == track.id && app.player.is_playing())
            .unwrap_or(false);
        let selected = matches!(source, RowSource::Results) && app.selected == Some(index);
        let extra = match source {
            RowSource::Results => model::format_views(app.results[index].view_count),
            RowSource::Queue => "in queue".to_string(),
            RowSource::Liked => "liked".to_string(),
        };
        let mut row = RowTrack::new(&track);
        row.extra = extra;
        row.liked = app.library.is_liked(&track.id);
        row.in_queue = app.library.is_in_playlist(&track.id);
        row.selected = selected;
        row.playing = playing;
        row.number = match source {
            RowSource::Queue => Some(index + 1),
            RowSource::Results => Some(index + 1),
            RowSource::Liked => None,
        };
        row.removable = matches!(source, RowSource::Queue);

        let response: RowResponse = widgets::track_row(ui, &mut app.thumbs, row);
        apply_row_response(app, actions, &track, index, response, &source);
    }
}

fn apply_row_response(
    app: &mut App,
    actions: &mut Vec<Action>,
    track: &Track,
    index: usize,
    response: RowResponse,
    source: &RowSource,
) {
    if response.activate {
        match source {
            RowSource::Results => {
                app.selected = Some(index);
                actions.push(Action::PlayTrack(track.clone()));
            }
            RowSource::Queue => actions.push(Action::PlayQueueIndex(index)),
            RowSource::Liked => actions.push(Action::PlayTrack(track.clone())),
        }
    }
    if response.like {
        actions.push(Action::ToggleLike(track.clone()));
    }
    if response.add_to_queue {
        if matches!(source, RowSource::Queue) {
            // Already queued: this press removes it from the queue.
            actions.push(Action::RemoveFromQueue(index));
        } else if app.library.is_in_playlist(&track.id) {
            if let Some(position) = app.library.playlist.iter().position(|entry| entry.id == track.id)
            {
                actions.push(Action::RemoveFromQueue(position));
            }
        } else {
            actions.push(Action::ToggleQueue(track.clone()));
        }
    }
    if response.play_next {
        actions.push(Action::QueueNext(track.clone()));
    }
    if response.download {
        actions.push(Action::Download(track.clone(), app.library.download_mode));
    }
    if response.remove {
        match source {
            RowSource::Queue => actions.push(Action::RemoveFromQueue(index)),
            RowSource::Liked => actions.push(Action::RemoveFromLiked(track.id.clone())),
            RowSource::Results => {}
        }
    }
    if response.secondary {
        // Right click = quick "play next" (a real context menu is overkill here).
        actions.push(Action::QueueNext(track.clone()));
    }
}

pub fn explore(app: &mut App, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Explore").size(20.0).strong().color(TEXT));
        if app.searching {
            ui.add(egui::Spinner::new().size(14.0));
            ui.label(RichText::new("searching…").size(12.0).color(MUTED));
        } else if !app.searched_for.is_empty() {
            ui.label(
                RichText::new(format!("{} results for “{}”", app.results.len(), app.searched_for))
                    .size(12.0)
                    .color(MUTED),
            );
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if !app.results.is_empty() {
                if ui.button("⬇  Download all (MP3)").clicked() {
                    for entry in app.results.iter().take(10) {
                        actions.push(Action::Download(Track::from_entry(entry), 0));
                    }
                }
                if ui.button("▶  Play all").clicked() {
                    let tracks: Vec<Track> = app.results.iter().map(Track::from_entry).collect();
                    actions.push(Action::PlayList(tracks, 0));
                }
                if ui.button("＋  Queue all").clicked() {
                    for entry in app.results.iter().rev() {
                        actions.push(Action::QueueLast(Track::from_entry(entry)));
                    }
                }
            }
        });
    });
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(6.0);

    if app.results.is_empty() {
        widgets::empty_state(
            ui,
            if app.searching { "Searching the ghost…" } else { "Nothing here yet" },
            "Type in the search bar above, or paste a YouTube link to play it directly.",
        );
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .id_salt("explore")
        .show(ui, |ui| {
            let count = app.results.len();
            show_rows(app, ui, actions, count, RowSource::Results);
            ui.add_space(24.0);
        });
}

// ---------------------------------------------------------------------------
// Library
// ---------------------------------------------------------------------------

pub fn library(app: &mut App, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Library").size(20.0).strong().color(TEXT));
        ui.label(
            RichText::new(format!(
                "{} liked · {} queued · {} playlists",
                app.library.liked.len(),
                app.library.playlist.len(),
                app.library.playlists.len()
            ))
            .size(12.0)
            .color(MUTED),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if !app.library.playlist.is_empty() && ui.button("▶  Play queue").clicked() {
                actions.push(Action::PlayQueueIndex(0));
            }
        });
    });
    ui.add_space(8.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .id_salt("library")
        .show(ui, |ui| {
            // Playlist creation.
            widgets::section_header(ui, "Playlists", "");
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let response = ui.add_sized(
                    [220.0, 28.0],
                    egui::TextEdit::singleline(&mut app.playlist_name_input)
                        .hint_text("New playlist name"),
                );
                let submit = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if (ui.button("Create").clicked() || submit) && !app.playlist_name_input.trim().is_empty() {
                    let name = app.playlist_name_input.clone();
                    app.playlist_name_input.clear();
                    actions.push(Action::CreatePlaylist(name));
                }
            });
            ui.add_space(6.0);
            if app.library.playlists.is_empty() {
                ui.label(RichText::new("No playlists yet.").size(12.0).color(FAINT));
            }
            let mut add_request: Option<(String, Track)> = None;
            let mut remove_request: Option<String> = None;
            let mut play_request: Option<(Vec<Track>, usize)> = None;
            let current_track = app.player.track.clone();
            for index in 0..app.library.playlists.len() {
                let playlist = &app.library.playlists[index];
                let frame = egui::Frame::new()
                    .fill(PANEL)
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(12, 10));
                frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(38.0), Sense::hover());
                        ui.painter().rect_filled(rect, egui::CornerRadius::same(8), RED_SOFT);
                        crate::ghost::draw_fitted(
                            ui.painter(),
                            rect.shrink(6.0),
                            RED,
                            crate::ghost::GhostStyle::plain(),
                        );
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&playlist.name).strong().size(14.0).color(TEXT));
                            ui.label(
                                RichText::new(format!("{} tracks", playlist.tracks.len()))
                                    .size(11.0)
                                    .color(MUTED),
                            );
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("🗑").on_hover_text("Delete playlist").clicked() {
                                remove_request = Some(playlist.id.clone());
                            }
                            if ui.button("▶").on_hover_text("Play playlist").clicked() {
                                play_request = Some((playlist.tracks.clone(), 0));
                            }
                            if ui
                                .add_enabled(current_track.is_some(), egui::Button::new("＋ current"))
                                .on_hover_text("Add the playing track")
                                .clicked()
                            {
                                if let Some(track) = &current_track {
                                    add_request = Some((playlist.name.clone(), track.clone()));
                                }
                            }
                        });
                    });
                });
                ui.add_space(6.0);
            }
            if let Some((name, track)) = add_request {
                actions.push(Action::AddToPlaylist { name, track });
            }
            if let Some(id) = remove_request {
                actions.push(Action::RemovePlaylist(id));
            }
            if let Some((tracks, index)) = play_request {
                if !tracks.is_empty() {
                    actions.push(Action::PlayList(tracks, index));
                }
            }

            ui.add_space(16.0);
            widgets::section_header(ui, "Liked music", "");
            ui.add_space(4.0);
            if app.library.liked.is_empty() {
                ui.label(
                    RichText::new("Tap ♥ on any track to keep it here.")
                        .size(12.0)
                        .color(FAINT),
                );
            } else {
                let count = app.library.liked.len();
                show_rows(app, ui, actions, count, RowSource::Liked);
            }

            ui.add_space(16.0);
            widgets::section_header(ui, "Queue", "reorder it in the panel on the right");
            ui.add_space(4.0);
            if app.library.playlist.is_empty() {
                ui.label(RichText::new("Your queue is empty.").size(12.0).color(FAINT));
            } else {
                let count = app.library.playlist.len();
                show_rows(app, ui, actions, count, RowSource::Queue);
            }
            ui.add_space(24.0);
        });
}

// ---------------------------------------------------------------------------
// Downloads
// ---------------------------------------------------------------------------

pub fn downloads(app: &mut App, ui: &mut egui::Ui, _actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Downloads").size(20.0).strong().color(TEXT));
        ui.label(
            RichText::new(app.library.directory().display().to_string())
                .size(11.0)
                .color(MUTED),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Open folder").clicked() {
                let folder = app.library.directory();
                let _ = std::fs::create_dir_all(&folder);
                open_in_file_manager(&folder);
            }
        });
    });
    ui.add_space(8.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .id_salt("downloads")
        .show(ui, |ui| {
            if app.downloads.is_empty() {
                ui.label(
                    RichText::new(
                        "Nothing downloaded in this session. Use ⤓ on a row, or the Download button in the player bar.",
                    )
                    .size(12.0)
                    .color(FAINT),
                );
            }
            for index in 0..app.downloads.len() {
                let item = &app.downloads[index];
                let progress = item.progress.clone();
                let (running, success) = (item.running, item.success);
                egui::Frame::new()
                    .fill(PANEL)
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&item.track.title).strong().size(13.0).color(TEXT));
                            ui.label(
                                RichText::new(if item.mode == 0 { "MP3" } else { "MP4" })
                                    .size(10.0)
                                    .color(FAINT),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let (label, color) = if running {
                                    ("downloading", WARN)
                                } else if success {
                                    ("done", GOOD)
                                } else {
                                    ("failed", RED)
                                };
                                ui.label(RichText::new(label).size(11.0).color(color));
                                if running {
                                    ui.label(
                                        RichText::new(format!("{:.0}s", item.started.elapsed().as_secs_f32()))
                                            .size(11.0)
                                            .color(FAINT),
                                    );
                                }
                            });
                        });
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(&item.track.artist)
                                .size(11.0)
                                .color(MUTED),
                        );
                        ui.add_space(6.0);
                        let fraction = progress.as_ref().map(|p| p.percent / 100.0).unwrap_or(0.0);
                        let text = match &progress {
                            Some(info) => {
                                let mut text = format!("{:.1}%", info.percent);
                                if let Some(total) = info.total_bytes {
                                    text.push_str(&format!(" of {}", tools::human_bytes(total)));
                                }
                                if let Some(speed) = &info.speed {
                                    text.push_str(&format!(" · {speed}"));
                                }
                                if let Some(eta) = &info.eta {
                                    text.push_str(&format!(" · ETA {eta}"));
                                }
                                text
                            }
                            None => "starting…".to_string(),
                        };
                        ui.add(
                            egui::ProgressBar::new(fraction)
                                .desired_height(8.0)
                                .fill(RED)
                                .text(RichText::new(text).size(10.0)),
                        );
                        ui.add_space(4.0);
                        if !item.success && !item.running {
                            ui.label(RichText::new(&item.message).size(11.0).color(RED));
                        }
                        if !item.log.is_empty() {
                            let log = item.log.clone();
                            egui::CollapsingHeader::new(RichText::new("yt-dlp log").size(11.0).color(MUTED))
                                .id_salt(format!("log-{index}"))
                                .show(ui, |ui| {
                                    let mut log_text: &str = log.as_str();
                                    ui.add(
                                        egui::TextEdit::multiline(&mut log_text)
                                            .desired_width(f32::INFINITY)
                                            .desired_rows(6)
                                            .font(egui::TextStyle::Monospace),
                                    );
                                });
                        }
                    });
                ui.add_space(8.0);
            }

            // What is already on disk.
            ui.add_space(10.0);
            widgets::section_header(ui, "Files on disk", "");
            ui.add_space(4.0);
            let folder = app.library.directory();
            match std::fs::read_dir(&folder) {
                Ok(entries) => {
                    let mut files: Vec<(String, u64)> = entries
                        .flatten()
                        .filter(|entry| entry.path().is_file())
                        .map(|entry| {
                            (
                                entry.file_name().to_string_lossy().to_string(),
                                entry.metadata().map(|meta| meta.len()).unwrap_or(0),
                            )
                        })
                        .collect();
                    files.sort_by(|a, b| a.0.cmp(&b.0));
                    if files.is_empty() {
                        ui.label(RichText::new("No files yet.").size(12.0).color(FAINT));
                    }
                    for (name, size) in files.iter().take(60) {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("♫").size(12.0).color(RED));
                            ui.label(RichText::new(name).size(12.0).color(TEXT));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new(tools::human_bytes(*size))
                                        .size(11.0)
                                        .color(FAINT),
                                );
                            });
                        });
                    }
                }
                Err(_) => {
                    ui.label(
                        RichText::new("Download folder is not created yet.")
                            .size(12.0)
                            .color(FAINT),
                    );
                }
            }
            ui.add_space(24.0);
        });
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

pub fn settings(app: &mut App, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .id_salt("settings")
        .show(ui, |ui| {
            ui.label(RichText::new("Settings").size(20.0).strong().color(TEXT));
            ui.add_space(10.0);

            // Downloads.
            settings_section(ui, "Downloads", |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Folder").size(12.0).color(MUTED));
                    ui.add_sized(
                        [ui.available_width() - 120.0, 26.0],
                        egui::TextEdit::singleline(&mut app.download_dir_input),
                    );
                    if ui.button("Apply").clicked() {
                        actions.push(Action::SetDownloadDir(app.download_dir_input.clone()));
                    }
                    if ui.button("Default").clicked() {
                        let folder = tools::default_download_dir();
                        app.download_dir_input = folder.display().to_string();
                        actions.push(Action::SetDownloadDir(app.download_dir_input.clone()));
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Format").size(12.0).color(MUTED));
                    let mut mode = app.library.download_mode;
                    if ui.selectable_value(&mut mode, 0, "♪  MP3 audio").clicked() {
                        actions.push(Action::SetDownloadMode(0));
                    }
                    if ui.selectable_value(&mut mode, 1, "▣  MP4 video").clicked() {
                        actions.push(Action::SetDownloadMode(1));
                    }
                });
            });

            ui.add_space(10.0);

            // Playback.
            settings_section(ui, "Playback", |ui| {
                let mut volume = app.player.volume();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Volume").size(12.0).color(MUTED));
                    if ui
                        .add_sized([180.0, 20.0], egui::Slider::new(&mut volume, 0.0..=1.0).show_value(true))
                        .changed()
                    {
                        app.player.set_volume(volume);
                        app.library.volume = volume;
                        app.library.save();
                    }
                });
                ui.add_space(4.0);
                let mut autoplay = app.autoplay_similar;
                if ui
                    .checkbox(&mut autoplay, "Keep playing related tracks when the queue ends")
                    .changed()
                {
                    app.autoplay_similar = autoplay;
                }
                ui.add_space(4.0);
                if ui.button("Clear search history").clicked() {
                    actions.push(Action::ClearRecents);
                }
            });

            ui.add_space(10.0);

            // Tools.
            settings_section(ui, "Media tools", |ui| {
                let yt_dlp = tools::tool_version("yt-dlp");
                let ffmpeg = tools::tool_version("ffmpeg");
                tool_line(ui, "yt-dlp", yt_dlp.as_deref(), "required for search & streaming");
                tool_line(ui, "ffmpeg", ffmpeg.as_deref(), "MP3 conversion & HLS streams");
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Tip: drop yt-dlp.exe and ffmpeg.exe next to Ghost Music.exe - the app also checks PATH.",
                    )
                    .size(11.0)
                    .color(FAINT),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("yt-dlp releases ↗").clicked() {
                        open_url("https://github.com/yt-dlp/yt-dlp/releases/latest");
                    }
                    if ui.button("ffmpeg downloads ↗").clicked() {
                        open_url("https://www.gyan.dev/ffmpeg/builds/");
                    }
                });
            });

            ui.add_space(10.0);

            // Local API.
            settings_section(ui, "Local API (Ghost Music Connect)", |ui| {
                let base = app.api_base();
                if base.is_empty() {
                    ui.label(RichText::new("The local API could not start.").size(12.0).color(RED));
                } else {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Base URL").size(12.0).color(MUTED));
                        let mut text = base.clone();
                        ui.add_sized(
                            [ui.available_width() - 190.0, 26.0],
                            egui::TextEdit::singleline(&mut text),
                        );
                        if ui.button("Open in browser").clicked() {
                            open_url(&base);
                        }
                        if ui.button("Copy").clicked() {
                            ui.ctx().copy_text(base.clone());
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "Endpoints: GET /health, /search?q=, /player/state, /downloads · POST /player/play, /player/pause, /player/next, /player/seek, /player/volume, /download",
                        )
                        .size(11.0)
                        .color(FAINT),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "Example: curl -X POST http://127.0.0.1:PORT/player/play -d \"{\\\"q\\\":\\\"lofi beats\\\"}\"",
                        )
                        .size(11.0)
                        .color(FAINT),
                    );
                }
            });

            ui.add_space(10.0);

            // Shortcuts + about.
            settings_section(ui, "Keyboard shortcuts", |ui| {
                for (keys, what) in [
                    ("Space", "play / pause"),
                    ("← / →", "seek 5 seconds"),
                    ("↑ / ↓", "volume"),
                    ("N / P", "next / previous"),
                    ("Ctrl + K", "focus the search box"),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(keys).size(12.0).strong().color(TEXT));
                        ui.label(RichText::new(what).size(12.0).color(MUTED));
                    });
                }
            });

            ui.add_space(10.0);
            settings_section(ui, "About", |ui| {
                ui.label(
                    RichText::new(format!("Ghost Music v{} · Rust + egui", env!("CARGO_PKG_VERSION")))
                        .size(12.0)
                        .color(TEXT),
                );
                ui.label(
                    RichText::new(
                        "Streams and downloads are handled by yt-dlp. Only download or stream content you have permission to use.",
                    )
                    .size(11.0)
                    .color(FAINT),
                );
                if !app.library.playlist.is_empty() {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "Queue: {} tracks · position {}",
                            app.library.playlist.len(),
                            app.queue_index.map(|index| index + 1).unwrap_or(0)
                        ))
                        .size(11.0)
                        .color(FAINT),
                    );
                }
            });
            ui.add_space(24.0);
        });
}

fn settings_section(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(PANEL)
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.label(RichText::new(title).size(14.0).strong().color(TEXT));
            ui.add_space(6.0);
            content(ui);
        });
}

fn tool_line(ui: &mut egui::Ui, name: &str, version: Option<&str>, hint: &str) {
    ui.horizontal(|ui| {
        let (dot, color) = if version.is_some() {
            ("●", GOOD)
        } else {
            ("●", RED)
        };
        ui.label(RichText::new(dot).size(12.0).color(color));
        ui.label(RichText::new(name).size(12.5).strong().color(TEXT));
        match version {
            Some(version) => {
                ui.label(RichText::new(version).size(11.5).color(MUTED));
            }
            None => {
                ui.label(RichText::new("not found").size(11.5).color(RED));
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(hint).size(11.0).color(FAINT));
        });
    });
}

fn open_url(url: &str) {
    let _ = open_external(url);
}

fn open_in_file_manager(path: &std::path::Path) {
    let _ = open_external(&path.display().to_string());
}

/// Open a URL or folder with the OS default handler (no extra crates needed).
fn open_external(target: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", target])
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(target).spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(target).spawn()?;
    }
    Ok(())
}
