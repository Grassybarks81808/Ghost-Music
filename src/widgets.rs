//! Reusable Ghost Music widgets: track rows, song/tile cards, chips and
//! transport buttons. Everything is painted with `epaint` so the look matches
//! the YouTube Music style dark/red theme.

use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, RichText, Sense, Shape,
    Stroke, Ui, Vec2,
};

use crate::ghost::{self, GhostStyle};
use crate::model::{self, Track};
use crate::theme::*;
use crate::thumbs::Thumbs;

/// What the user did to a row this frame.
#[derive(Debug, Default, Clone, Copy)]
pub struct RowResponse {
    /// Left click / Enter on the row body.
    pub activate: bool,
    pub add_to_queue: bool,
    pub play_next: bool,
    pub download: bool,
    pub like: bool,
    pub remove: bool,
    pub add_to_playlist: bool,
    pub secondary: bool,
}

impl RowResponse {
    pub fn any(&self) -> bool {
        self.activate
            || self.add_to_queue
            || self.play_next
            || self.download
            || self.like
            || self.remove
            || self.add_to_playlist
    }
}

pub struct RowTrack<'a> {
    pub track: &'a Track,
    /// Overrides the shown duration (used for local files).
    pub duration_text: Option<String>,
    /// Small muted text after the artist (views, album, "downloaded"…).
    pub extra: String,
    pub liked: bool,
    pub in_queue: bool,
    pub selected: bool,
    pub playing: bool,
    pub number: Option<usize>,
    /// Show the "remove from playlist" action.
    pub removable: bool,
}

impl<'a> RowTrack<'a> {
    pub fn new(track: &'a Track) -> Self {
        Self {
            track,
            duration_text: None,
            extra: String::new(),
            liked: false,
            in_queue: false,
            selected: false,
            playing: false,
            number: None,
            removable: false,
        }
    }
}

/// One row of a track list. The whole row is clickable, hover reveals actions.
pub fn track_row(
    ui: &mut Ui,
    thumbs: &mut Thumbs,
    ctx: &egui::Context,
    row: RowTrack<'_>,
) -> RowResponse {
    let height = 58.0;
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let hovered = response.hovered();
    let mut out = RowResponse::default();
    if response.clicked() {
        out.activate = true;
    }
    if response.secondary_clicked() {
        out.secondary = true;
    }

    let painter = ui.painter();
    let fill = if row.selected {
        RED_SOFT
    } else if hovered {
        PANEL_HOVER
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect.shrink2(Vec2::new(0.0, 1.0)), CornerRadius::same(8), fill);

    // Artwork (or number) on the left.
    let art_rect = Rect::from_min_size(
        Pos2::new(rect.left() + 8.0, rect.top() + (height - 42.0) / 2.0),
        Vec2::splat(42.0),
    );
    paint_artwork(ui, thumbs, ctx, art_rect, row.track, row.number, hovered, row.playing);

    // Right hand side: duration + actions.
    let mut meta_right = rect.right() - 10.0;
    let duration = row
        .duration_text
        .clone()
        .unwrap_or_else(|| model::format_duration(row.track.duration));
    let duration_width = 46.0;
    ui.painter().text(
        Pos2::new(meta_right - duration_width, rect.center().y),
        Align2::RIGHT_CENTER,
        duration,
        FontId::proportional(12.0),
        MUTED,
    );
    meta_right -= duration_width + 14.0;

    if !row.extra.is_empty() {
        let width = ui
            .painter()
            .layout_no_wrap(row.extra.clone(), FontId::proportional(11.0), FAINT)
            .rect
            .width();
        if meta_right - width > rect.left() + 320.0 {
            ui.painter().text(
                Pos2::new(meta_right, rect.center().y),
                Align2::RIGHT_CENTER,
                &row.extra,
                FontId::proportional(11.0),
                FAINT,
            );
            meta_right -= width + 14.0;
        }
    }

    // Hover-only action buttons, right to left.
    let mut action_left = meta_right;
    if hovered {
        let mut place = |ui: &mut Ui, glyph: &str, tip: &str, checked: bool| -> bool {
            let button_rect = Rect::from_center_size(
                Pos2::new(action_left - 15.0, rect.center().y),
                Vec2::splat(28.0),
            );
            action_left -= 32.0;
            let hovered_button = ui.rect_contains_pointer(button_rect);
            let color = if checked {
                RED
            } else if hovered_button {
                Color32::WHITE
            } else {
                MUTED
            };
            if hovered_button {
                ui.painter()
                    .rect_filled(button_rect, CornerRadius::same(6), Color32::from_rgb(44, 47, 58));
            }
            ui.painter().text(
                button_rect.center(),
                Align2::CENTER_CENTER,
                glyph,
                FontId::proportional(15.0),
                color,
            );
            let clicked = ui
                .interact(button_rect, ui.id().with(tip), Sense::click())
                .on_hover_text(tip)
                .clicked();
            clicked
        };

        if row.removable {
            out.remove = place(ui, "🗑", "Remove from this playlist", false);
            if out.remove {
                return out;
            }
        }
        out.download = place(ui, "⤓", "Download", false);
        if out.download {
            return out;
        }
        if row.in_queue {
            out.add_to_queue = place(ui, "✓", "Already in the queue", true);
            if out.add_to_queue {
                return out;
            }
        } else {
            out.add_to_queue = place(ui, "＋", "Add to queue", false);
            if out.add_to_queue {
                return out;
            }
        }
        out.like = place(ui, "♥", if row.liked { "Remove from liked" } else { "Like" }, row.liked);
        if out.like {
            return out;
        }
    }

    // Title + artist.
    let text_left = art_rect.right() + 12.0;
    let text_width = (action_left - text_left - 12.0).max(80.0);
    let title_color = if row.playing { RED } else { TEXT };
    let title = truncate(&row.track.title, text_width, 13.5, ui);
    ui.painter().text(
        Pos2::new(text_left, rect.center().y - 9.0),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(13.5),
        title_color,
    );
    let subtitle = if row.track.artist.is_empty() {
        "Ghost Music".to_string()
    } else {
        row.track.artist.clone()
    };
    ui.painter().text(
        Pos2::new(text_left, rect.center().y + 10.0),
        Align2::LEFT_CENTER,
        truncate(&subtitle, text_width, 11.5, ui),
        FontId::proportional(11.5),
        MUTED,
    );

    response.on_hover_cursor(egui::CursorIcon::PointingHand);
    out
}

/// Artwork square with a hover play badge and optional index number.
fn paint_artwork(
    ui: &mut Ui,
    thumbs: &mut Thumbs,
    ctx: &egui::Context,
    rect: Rect,
    track: &Track,
    number: Option<usize>,
    hovered: bool,
    playing: bool,
) {
    let radius = CornerRadius::same(7);
    let texture = thumbs
        .get(ctx, &track.thumbnail)
        .or_else(|| thumbs.get(ctx, &youtube_thumb_fallback(&track.id)));

    match texture {
        Some(texture) => {
            let painter = ui.painter().with_clip_rect(rect);
            painter.add(Shape::image(
                texture.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            ));
        }
        None => {
            let texture = thumbs.placeholder(ctx, &track.id);
            ui.painter().image(
                texture.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
            ghost::draw_badge(
                ui.painter(),
                rect.center(),
                rect.width() * 0.55,
                Color32::from_rgba_unmultiplied(255, 255, 255, 210),
            );
        }
    }
    ui.painter()
        .rect_stroke(rect, radius, Stroke::new(1.0, Color32::from_black_alpha(90)), egui::StrokeKind::Inside);

    if hovered && number.is_none() {
        ui.painter()
            .rect_filled(rect, radius, Color32::from_black_alpha(150));
        transport_glyph(ui.painter(), rect.center(), 14.0, if playing { Glyph::Pause } else { Glyph::Play }, Color32::WHITE);
    } else if let Some(number) = number {
        if hovered {
            ui.painter()
                .rect_filled(rect, radius, Color32::from_black_alpha(150));
            transport_glyph(ui.painter(), rect.center(), 14.0, Glyph::Play, Color32::WHITE);
        } else {
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                format!("{number}"),
                FontId::proportional(12.0),
                MUTED,
            );
        }
    }
}

pub fn youtube_thumb_fallback(id: &str) -> String {
    if model::is_youtube_id(id) {
        format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg")
    } else {
        String::new()
    }
}

fn truncate(text: &str, width: f32, size: f32, ui: &Ui) -> String {
    let font = FontId::proportional(size);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE);
    if galley.rect.width() <= width {
        return text.to_string();
    }
    let mut cut = text.to_string();
    while !cut.is_empty() {
        cut.pop();
        let candidate = format!("{}…", cut.trim_end());
        let galley = ui.painter().layout_no_wrap(candidate.clone(), font.clone(), Color32::WHITE);
        if galley.rect.width() <= width {
            return candidate;
        }
    }
    "…".to_string()
}

#[derive(Clone, Copy, PartialEq)]
pub enum Glyph {
    Play,
    Pause,
    Next,
    Previous,
    Shuffle,
    Repeat,
    RepeatOne,
    Volume,
    Muted,
    Heart,
    HeartFilled,
    Download,
    Queue,
    Search,
    Home,
    Library,
    Player,
    Settings,
    Refresh,
    Trash,
}

/// Vector transport glyphs (no font glyph roulette, scales cleanly).
pub fn transport_glyph(painter: &egui::Painter, center: Pos2, size: f32, glyph: Glyph, color: Color32) {
    let half = size / 2.0;
    let stroke = Stroke::new((size * 0.16).max(1.5), color);
    match glyph {
        Glyph::Play => {
            painter.add(Shape::convex_polygon(
                vec![
                    Pos2::new(center.x - half * 0.62, center.y - half),
                    Pos2::new(center.x - half * 0.62, center.y + half),
                    Pos2::new(center.x + half * 0.95, center.y),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Glyph::Pause => {
            let width = size * 0.26;
            for offset in [-1.0, 1.0] {
                let x = center.x + offset * size * 0.28;
                painter.rect_filled(
                    Rect::from_min_max(
                        Pos2::new(x - width / 2.0, center.y - half),
                        Pos2::new(x + width / 2.0, center.y + half),
                    ),
                    1.5,
                    color,
                );
            }
        }
        Glyph::Next | Glyph::Previous => {
            let flip = if glyph == Glyph::Previous { -1.0 } else { 1.0 };
            painter.add(Shape::convex_polygon(
                vec![
                    Pos2::new(center.x - flip * half * 0.75, center.y - half * 0.85),
                    Pos2::new(center.x - flip * half * 0.75, center.y + half * 0.85),
                    Pos2::new(center.x + flip * half * 0.45, center.y),
                ],
                color,
                Stroke::NONE,
            ));
            let bar_x = center.x + flip * half * 0.72;
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(bar_x - size * 0.07, center.y - half * 0.85),
                    Pos2::new(bar_x + size * 0.07, center.y + half * 0.85),
                ),
                1.0,
                color,
            );
        }
        Glyph::Shuffle => {
            let mut path = |points: [(f32, f32); 3]| {
                painter.add(Shape::line(
                    points
                        .iter()
                        .map(|(x, y)| Pos2::new(center.x + x * size, center.y + y * size))
                        .collect::<Vec<_>>(),
                    stroke,
                ));
            };
            path([(-0.5, -0.32), (-0.15, -0.32), (0.5, 0.32)]);
            path([(-0.5, 0.32), (-0.15, 0.32), (0.5, -0.32)]);
            for (x, y, flip) in [(0.5, 0.32, 1.0), (0.5, -0.32, 1.0)] {
                let tip = Pos2::new(center.x + x * size, center.y + y * size);
                painter.add(Shape::convex_polygon(
                    vec![
                        tip,
                        tip - Vec2::new(size * 0.24 * flip, 0.0),
                        tip - Vec2::new(size * 0.12 * flip, size * 0.22 * y.signum()),
                    ],
                    color,
                    Stroke::NONE,
                ));
            }
        }
        Glyph::Repeat | Glyph::RepeatOne => {
            let rect = Rect::from_center_size(center, Vec2::splat(size * 0.92));
            painter.add(Shape::line(
                vec![
                    Pos2::new(rect.left() + size * 0.16, rect.top() + size * 0.24),
                    Pos2::new(rect.right() - size * 0.16, rect.top() + size * 0.24),
                    Pos2::new(rect.right() - size * 0.16, rect.bottom() - size * 0.24),
                    Pos2::new(rect.left() + size * 0.16, rect.bottom() - size * 0.24),
                    Pos2::new(rect.left() + size * 0.16, rect.top() + size * 0.42),
                ],
                stroke,
            ));
            let tip = Pos2::new(rect.left() + size * 0.16, rect.top() + size * 0.24);
            painter.add(Shape::convex_polygon(
                vec![
                    tip - Vec2::new(0.0, size * 0.2),
                    tip + Vec2::new(size * 0.2, 0.0),
                    tip + Vec2::new(0.0, size * 0.2),
                ],
                color,
                Stroke::NONE,
            ));
            if glyph == Glyph::RepeatOne {
                painter.text(
                    center,
                    Align2::CENTER_CENTER,
                    "1",
                    FontId::proportional(size * 0.62),
                    color,
                );
            }
        }
        Glyph::Volume | Glyph::Muted => {
            let speaker = Rect::from_min_size(
                Pos2::new(center.x - half * 0.85, center.y - size * 0.16),
                Vec2::new(size * 0.22, size * 0.32),
            );
            painter.rect_filled(speaker, 1.0, color);
            painter.add(Shape::convex_polygon(
                vec![
                    Pos2::new(center.x - half * 0.4, center.y - size * 0.16),
                    Pos2::new(center.x - half * 0.02, center.y - size * 0.42),
                    Pos2::new(center.x - half * 0.02, center.y + size * 0.42),
                    Pos2::new(center.x - half * 0.4, center.y + size * 0.16),
                ],
                color,
                Stroke::NONE,
            ));
            if glyph == Glyph::Volume {
                for index in 0..2 {
                    let radius = size * (0.24 + index as f32 * 0.2);
                    painter.add(Shape::line(
                        (-40..=40)
                            .map(|step| {
                                let angle = step as f32 / 40.0 * 0.75;
                                Pos2::new(
                                    center.x + size * 0.05 + radius * angle.cos(),
                                    center.y + radius * angle.sin(),
                                )
                            })
                            .collect::<Vec<_>>(),
                        Stroke::new(1.6, color),
                    ));
                }
            } else {
                painter.add(Shape::line_segment(
                    [
                        Pos2::new(center.x + size * 0.16, center.y - size * 0.22),
                        Pos2::new(center.x + size * 0.5, center.y + size * 0.22),
                    ],
                    stroke,
                ));
                painter.add(Shape::line_segment(
                    [
                        Pos2::new(center.x + size * 0.5, center.y - size * 0.22),
                        Pos2::new(center.x + size * 0.16, center.y + size * 0.22),
                    ],
                    stroke,
                ));
            }
        }
        Glyph::Heart | Glyph::HeartFilled => {
            let filled = glyph == Glyph::HeartFilled;
            let r = size * 0.26;
            let left = Pos2::new(center.x - r * 0.85, center.y - r * 0.35);
            let right = Pos2::new(center.x + r * 0.85, center.y - r * 0.35);
            painter.circle_filled(left, r, color);
            painter.circle_filled(right, r, color);
            painter.add(Shape::convex_polygon(
                vec![
                    Pos2::new(left.x - r, left.y - r * 0.1),
                    Pos2::new(right.x + r, right.y - r * 0.1),
                    Pos2::new(center.x, center.y + size * 0.52),
                ],
                color,
                Stroke::NONE,
            ));
            if !filled {
                painter.circle_filled(center, r * 0.5, Color32::from_black_alpha(0));
            }
        }
        Glyph::Download => {
            painter.add(Shape::line_segment(
                [
                    Pos2::new(center.x, center.y - half * 0.9),
                    Pos2::new(center.x, center.y + half * 0.25),
                ],
                stroke,
            ));
            painter.add(Shape::convex_polygon(
                vec![
                    Pos2::new(center.x - half * 0.42, center.y + half * 0.05),
                    Pos2::new(center.x + half * 0.42, center.y + half * 0.05),
                    Pos2::new(center.x, center.y + half * 0.62),
                ],
                color,
                Stroke::NONE,
            ));
            painter.add(Shape::line_segment(
                [
                    Pos2::new(center.x - half * 0.75, center.y + half * 0.85),
                    Pos2::new(center.x + half * 0.75, center.y + half * 0.85),
                ],
                stroke,
            ));
        }
        Glyph::Queue => {
            for (index, width) in [1.0_f32, 0.7, 1.0].iter().enumerate() {
                let y = center.y + (index as f32 - 1.0) * size * 0.34;
                painter.add(Shape::line_segment(
                    [
                        Pos2::new(center.x - half * 0.9, y),
                        Pos2::new(center.x + half * 0.9 * width, y),
                    ],
                    stroke,
                ));
            }
        }
        Glyph::Search => {
            painter.circle_stroke(
                Pos2::new(center.x - size * 0.08, center.y - size * 0.08),
                size * 0.3,
                stroke,
            );
            painter.add(Shape::line_segment(
                [
                    Pos2::new(center.x + size * 0.12, center.y + size * 0.12),
                    Pos2::new(center.x + size * 0.42, center.y + size * 0.42),
                ],
                stroke,
            ));
        }
        Glyph::Home => {
            painter.add(Shape::line(
                vec![
                    Pos2::new(center.x - half, center.y),
                    Pos2::new(center.x, center.y - half * 0.9),
                    Pos2::new(center.x + half, center.y),
                ],
                stroke,
            ));
            painter.add(Shape::line(
                vec![
                    Pos2::new(center.x - half * 0.7, center.y - size * 0.05),
                    Pos2::new(center.x - half * 0.7, center.y + half * 0.85),
                    Pos2::new(center.x + half * 0.7, center.y + half * 0.85),
                    Pos2::new(center.x + half * 0.7, center.y - size * 0.05),
                ],
                stroke,
            ));
        }
        Glyph::Library | Glyph::Player => {
            if glyph == Glyph::Player {
                painter.circle_stroke(center, size * 0.44, stroke);
                transport_glyph(painter, center, size * 0.5, Glyph::Play, color);
            } else {
                for (index, height) in [0.95_f32, 0.6, 0.8].iter().enumerate() {
                    let x = center.x + (index as f32 - 1.0) * size * 0.36;
                    painter.rect_filled(
                        Rect::from_min_max(
                            Pos2::new(x - size * 0.08, center.y + half * height),
                            Pos2::new(x + size * 0.08, center.y - half * height),
                        ),
                        1.5,
                        color,
                    );
                }
            }
        }
        Glyph::Settings => {
            painter.circle_stroke(center, size * 0.22, stroke);
            for spoke in 0..6 {
                let angle = spoke as f32 / 6.0 * std::f32::consts::TAU;
                painter.add(Shape::line_segment(
                    [
                        center + Vec2::new(angle.cos(), angle.sin()) * size * 0.3,
                        center + Vec2::new(angle.cos(), angle.sin()) * size * 0.5,
                    ],
                    stroke,
                ));
            }
        }
        Glyph::Refresh => {
            painter.add(Shape::line(
                (0..=28)
                    .map(|step| {
                        let angle = step as f32 / 28.0 * std::f32::consts::TAU * 0.85 - 1.2;
                        center + Vec2::new(angle.cos(), angle.sin()) * size * 0.4
                    })
                    .collect::<Vec<_>>(),
                stroke,
            ));
        }
        Glyph::Trash => {
            painter.add(Shape::line_segment(
                [
                    Pos2::new(center.x - half * 0.8, center.y - half * 0.5),
                    Pos2::new(center.x + half * 0.8, center.y - half * 0.5),
                ],
                stroke,
            ));
            painter.add(Shape::line(
                vec![
                    Pos2::new(center.x - half * 0.55, center.y - half * 0.5),
                    Pos2::new(center.x - half * 0.45, center.y + half * 0.8),
                    Pos2::new(center.x + half * 0.45, center.y + half * 0.8),
                    Pos2::new(center.x + half * 0.55, center.y - half * 0.5),
                ],
                stroke,
            ));
        }
    }
}

/// Round filled icon button (transport bar, cards).
pub fn circle_button(
    ui: &mut Ui,
    size: f32,
    glyph: Glyph,
    fill: Color32,
    glyph_color: Color32,
    tooltip: &str,
    enabled: bool,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let hovered = response.hovered() && enabled;
    let fill = if !enabled {
        fill.gamma_multiply(0.4)
    } else if hovered {
        RED_HOVER
    } else {
        fill
    };
    let painter = ui.painter();
    painter.circle_filled(rect.center(), size / 2.0, fill);
    transport_glyph(painter, rect.center(), size * 0.42, glyph, glyph_color);
    if !tooltip.is_empty() {
        response.clone().on_hover_text(tooltip);
    }
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
    } else {
        false
    }
}

/// Flat icon button used in the header and rows.
pub fn icon_button(ui: &mut Ui, glyph: Glyph, size: f32, tooltip: &str, active: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let hovered = response.hovered();
    let color = if active {
        RED
    } else if hovered {
        Color32::WHITE
    } else {
        MUTED
    };
    if hovered {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), Color32::from_rgb(40, 43, 54));
    }
    transport_glyph(ui.painter(), rect.center(), size * 0.46, glyph, color);
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if !tooltip.is_empty() {
        response.clone().on_hover_text(tooltip);
    }
    response.clicked()
}

/// Rounded "chip" used for recent searches, filters and playlist labels.
pub fn chip(ui: &mut Ui, text: &str, active: bool) -> bool {
    let font = FontId::proportional(12.0);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font.clone(), TEXT);
    let size = Vec2::new(galley.rect.width() + 22.0, 27.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let hovered = response.hovered();
    let fill = if active {
        Color32::from_rgb(240, 240, 245)
    } else if hovered {
        Color32::from_rgb(52, 56, 68)
    } else {
        Color32::from_rgb(38, 41, 52)
    };
    let text_color = if active { Color32::from_rgb(16, 17, 22) } else { TEXT };
    ui.painter().rect_filled(rect, CornerRadius::same(14), fill);
    ui.painter()
        .text(rect.center(), Align2::CENTER_CENTER, text, font, text_color);
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

/// A tile in the Home grid: artwork, play overlay, title, subtitle.
pub fn media_card(
    ui: &mut Ui,
    thumbs: &mut Thumbs,
    ctx: &egui::Context,
    title: &str,
    subtitle: &str,
    thumbnail: &str,
    seed_key: &str,
    playable: bool,
    playing: bool,
    width: f32,
) -> bool {
    let art_size = width;
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width, art_size + 48.0),
        Sense::click(),
    );
    let hovered = response.hovered();
    let painter = ui.painter();

    let art_rect = Rect::from_min_size(rect.min, Vec2::splat(art_size));
    let texture = if thumbnail.is_empty() {
        None
    } else {
        thumbs.get(ctx, thumbnail)
    };
    match texture {
        Some(texture) => {
            let painter = ui.painter().with_clip_rect(art_rect);
            painter.add(Shape::image(
                texture.id(),
                art_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            ));
        }
        None => {
            let texture = thumbs.placeholder(ctx, seed_key);
            ui.painter().image(
                texture.id(),
                art_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
            ghost::draw_badge(
                ui.painter(),
                art_rect.center(),
                art_rect.width() * 0.36,
                Color32::from_rgba_unmultiplied(255, 255, 255, 200),
            );
        }
    }

    if hovered && playable {
        ui.painter()
            .rect_filled(art_rect, CornerRadius::ZERO, Color32::from_black_alpha(90));
        let button = Rect::from_min_size(
            Pos2::new(art_rect.right() - 54.0, art_rect.bottom() - 54.0),
            Vec2::splat(40.0),
        );
        ui.painter().circle_filled(button.center(), 20.0, RED);
        transport_glyph(
            ui.painter(),
            button.center(),
            17.0,
            if playing { Glyph::Pause } else { Glyph::Play },
            Color32::WHITE,
        );
    } else if playing {
        ui.painter()
            .rect_filled(art_rect, CornerRadius::ZERO, Color32::from_black_alpha(70));
        transport_glyph(ui.painter(), art_rect.center(), 26.0, Glyph::Pause, RED);
    }

    painter.rect_stroke(
        art_rect,
        CornerRadius::ZERO,
        Stroke::new(1.0, Color32::from_black_alpha(80)),
        egui::StrokeKind::Inside,
    );

    ui.painter().text(
        Pos2::new(rect.left() + 2.0, art_rect.bottom() + 9.0),
        Align2::LEFT_TOP,
        truncate(title, width - 4.0, 13.0, ui),
        FontId::proportional(13.0),
        TEXT,
    );
    ui.painter().text(
        Pos2::new(rect.left() + 2.0, art_rect.bottom() + 27.0),
        Align2::LEFT_TOP,
        truncate(subtitle, width - 4.0, 11.5, ui),
        FontId::proportional(11.5),
        MUTED,
    );

    response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Section title with an optional "see all" style hint on the right.
pub fn section_header(ui: &mut Ui, title: &str, hint: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(17.0).strong().color(TEXT));
        if !hint.is_empty() {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(hint).size(11.0).color(FAINT));
            });
        }
    });
}

/// Centred "playing now" indicator drawn over a card.
pub fn now_playing_badge(painter: &egui::Painter, rect: Rect) {
    let bar_width = 3.0;
    let base = rect.bottom() - 10.0;
    for index in 0..4 {
        let x = rect.left() + 10.0 + index as f32 * (bar_width + 4.0);
        let height = 6.0 + (index as f32 * 2.3).sin().abs() * 10.0;
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(x, base - height),
                Pos2::new(x + bar_width, base),
            ),
            1.5,
            RED,
        );
    }
}

/// A subtle "ghost" watermark used behind empty states.
pub fn empty_state(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(26.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(140.0, 120.0), Sense::hover());
        let painter = ui.painter();
        painter.circle_filled(rect.center(), 52.0, Color32::from_rgb(24, 18, 25));
        ghost::draw_fitted(painter, rect.shrink(18.0), RED, GhostStyle::with_waves(0.0));
        ui.add_space(10.0);
        ui.label(RichText::new(title).size(15.0).strong().color(TEXT));
        ui.label(RichText::new(subtitle).size(12.0).color(MUTED));
    });
}

/// Rounded panel used for the player bar and side panels.
pub fn panel_frame(fill: Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .corner_radius(CornerRadius::same(10))
}
