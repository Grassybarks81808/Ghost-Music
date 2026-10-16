//! The Ghost Music mascot: a pure-`epaint` vector ghost with headphones, used
//! for the splash screen, the header, list rows, cards and the seek handle.

use egui::{Align2, Color32, FontId, Pos2, Rect, Shape, Stroke, Vec2};

use crate::theme::{self, MUTED, RED, TEXT};

/// Local (unit) outline of the ghost body: rounded dome, straight sides, flat
/// bottom. The wavy skirt is drawn as separate feet so every polygon stays
/// convex (what `epaint` fills reliably).
pub const BODY: [(f32, f32); 9] = [
    (-0.48, 0.32),
    (-0.48, -0.18),
    (-0.42, -0.38),
    (-0.27, -0.49),
    (0.0, -0.53),
    (0.27, -0.49),
    (0.42, -0.38),
    (0.48, -0.18),
    (0.48, 0.32),
];

/// Triangle feet under the body: (left, right, apex) in unit coordinates.
pub const FEET: [(f32, f32, f32); 3] = [(-0.48, -0.14, -0.31), (-0.14, 0.12, -0.01), (0.12, 0.48, 0.28)];
const FOOT_BASE: f32 = 0.32;
const FOOT_APEX: f32 = 0.43;

#[derive(Debug, Clone, Copy)]
pub struct GhostStyle {
    pub headphones: bool,
    pub eyes: bool,
    pub smile: bool,
    pub waves: bool,
    /// Animation phase for the sound waves (seconds).
    pub wave_phase: f32,
}

impl Default for GhostStyle {
    fn default() -> Self {
        Self { headphones: true, eyes: true, smile: true, waves: false, wave_phase: 0.0 }
    }
}

impl GhostStyle {
    pub fn plain() -> Self {
        Self { headphones: false, eyes: false, smile: false, waves: false, wave_phase: 0.0 }
    }

    pub fn with_waves(phase: f32) -> Self {
        Self { waves: true, wave_phase: phase, ..Self::default() }
    }
}

/// Draw the mascot. `size` is roughly the body width in points.
pub fn draw(
    painter: &egui::Painter,
    center: Pos2,
    size: f32,
    color: Color32,
    rotation: f32,
    style: GhostStyle,
) {
    let rotate = |x: f32, y: f32| -> Pos2 {
        let (x, y) = (x * size, y * size);
        Pos2::new(
            center.x + x * rotation.cos() - y * rotation.sin(),
            center.y + x * rotation.sin() + y * rotation.cos(),
        )
    };

    // Soft glow behind the body.
    painter.circle_filled(center, size * 0.72, Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 20));

    let body: Vec<Pos2> = BODY.iter().map(|(x, y)| rotate(*x, *y)).collect();
    painter.add(Shape::convex_polygon(body, color, Stroke::NONE));

    for (left, right, apex) in FEET {
        let triangle = vec![
            rotate(left, FOOT_BASE),
            rotate(right, FOOT_BASE),
            rotate(apex, FOOT_APEX),
        ];
        painter.add(Shape::convex_polygon(triangle, color, Stroke::NONE));
    }

    let eye_color = Color32::from_rgb(12, 13, 17);
    if style.eyes {
        for x in [-0.17, 0.17] {
            painter.circle_filled(rotate(x, -0.1), (size * 0.05).max(1.4), eye_color);
        }
    }
    if style.smile {
        let mouth_center = rotate(0.0, 0.06);
        painter.add(Shape::line(
            (0..=10)
                .map(|step| {
                    let t = step as f32 / 10.0;
                    let x = -0.1 + 0.2 * t;
                    let y = 0.09 + 0.09 * (t * std::f32::consts::PI).sin();
                    rotate(x, y)
                })
                .collect::<Vec<_>>(),
            Stroke::new((size * 0.035).max(1.0), eye_color),
        ));
        let _ = mouth_center;
    }

    if style.headphones {
        let band = Color32::from_rgb(246, 246, 252);
        painter.circle_stroke(center, size * 0.52, Stroke::new((size * 0.065).max(1.6), band));
        for x in [-0.5, 0.5] {
            let top = rotate(x, -0.1);
            let bottom = rotate(x, 0.14);
            painter.add(Shape::line_segment(
                [top, bottom],
                Stroke::new((size * 0.11).max(2.4), band),
            ));
        }
    }

    if style.waves {
        let count = 3;
        for index in 0..count {
            let i = index as f32;
            let wobble = 0.55 + 0.45 * (style.wave_phase * 3.4 + i * 1.1).sin().abs();
            let height = size * (0.18 + i * 0.11) * wobble;
            let alpha = 230_u8.saturating_sub(index as u8 * 60);
            let wave = Color32::from_rgba_unmultiplied(RED.r(), RED.g(), RED.b(), alpha);
            let offset = size * (0.72 + i * 0.15);
            let width = (size * 0.045).max(1.5);
            painter.add(Shape::line_segment(
                [
                    Pos2::new(center.x + offset, center.y - height),
                    Pos2::new(center.x + offset, center.y + height),
                ],
                Stroke::new(width, wave),
            ));
            painter.add(Shape::line_segment(
                [
                    Pos2::new(center.x - offset, center.y - height),
                    Pos2::new(center.x - offset, center.y + height),
                ],
                Stroke::new(width, wave),
            ));
        }
    }
}

/// Small ghost used as a bullet/list marker.
pub fn draw_badge(painter: &egui::Painter, center: Pos2, size: f32, color: Color32) {
    draw(painter, center, size, color, 0.0, GhostStyle::plain());
}

/// The signature seek handle: a tiny ghost sitting on the progress bar.
pub fn draw_seek_ghost(painter: &egui::Painter, rect: Rect, progress: f32, color: Color32) {
    let x = rect.left() + rect.width() * progress.clamp(0.0, 1.0);
    let center = Pos2::new(x, rect.center().y - 2.0);
    painter.circle_filled(center, 9.0, Color32::from_rgb(20, 21, 27));
    draw(painter, center, 8.0, color, 0.0, GhostStyle::plain());
}

/// Animated launch screen (about four seconds, click/press any key to skip).
pub struct Splash {
    started: std::time::Instant,
    done: bool,
}

impl Splash {
    pub fn new() -> Self {
        Self { started: std::time::Instant::now(), done: false }
    }

    /// A splash that is already over (used when the user clicks to skip).
    pub fn finished() -> Self {
        Self { started: std::time::Instant::now(), done: true }
    }

    pub fn skipped(&self) -> bool {
        self.done
            || self.started.elapsed().as_secs_f32() >= if self.fast() { 1.6 } else { 4.0 }
    }

    /// Short splash for `--selftest` / CI screenshots.
    fn fast(&self) -> bool {
        std::env::args().any(|arg| arg == "--quick")
    }

    /// Returns true while the splash is still on screen.
    pub fn draw(&self, ctx: &egui::Context) -> bool {
        if self.skipped() {
            return false;
        }
        ctx.request_repaint();
        let time = self.started.elapsed().as_secs_f32();
        let duration = if self.fast() { 1.6 } else { 4.0 };
        let progress = (time / duration).clamp(0.0, 1.0);

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::BG))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let center = rect.center();
                let painter = ui.painter();

                // Rotating halo.
                let halo_radius = 150.0 + 26.0 * (time * 2.0).sin();
                painter.circle_filled(center, halo_radius, Color32::from_rgb(24, 15, 22));
                painter.circle_stroke(
                    center,
                    halo_radius + 6.0,
                    Stroke::new(1.5, Color32::from_rgba_unmultiplied(RED.r(), RED.g(), RED.b(), 90)),
                );
                for spoke in 0..36 {
                    let angle = spoke as f32 / 36.0 * std::f32::consts::TAU + time * 1.6;
                    let inner = halo_radius + 12.0;
                    let outer = inner + 8.0 + 14.0 * ((spoke as f32 * 0.6 + time * 4.0).sin().abs());
                    let alpha = (140.0 * (1.0 - progress)) as u8 + 20;
                    let color = Color32::from_rgba_unmultiplied(RED.r(), RED.g(), RED.b(), alpha);
                    painter.add(Shape::line_segment(
                        [
                            center + Vec2::new(angle.cos(), angle.sin()) * inner,
                            center + Vec2::new(angle.cos(), angle.sin()) * outer,
                        ],
                        Stroke::new(2.0, color),
                    ));
                }

                // The mascot: rotates in, then headphones drop, then waves flow.
                let rotation = if progress < 0.4 {
                    (progress / 0.4) * std::f32::consts::TAU
                } else {
                    std::f32::consts::TAU
                };
                let scale = 0.65 + 0.35 * (progress / 0.4).min(1.0);
                let headphones = progress > 0.45;
                let waves = progress > 0.7;
                let drop = if progress > 0.45 {
                    (1.0 - ((progress - 0.45) / 0.25).min(1.0)) * -22.0
                } else {
                    -22.0
                };
                let style = GhostStyle {
                    headphones,
                    eyes: progress > 0.25,
                    smile: progress > 0.3,
                    waves,
                    wave_phase: time,
                };
                draw(
                    painter,
                    Pos2::new(center.x, center.y + drop),
                    190.0 * scale,
                    RED,
                    rotation,
                    style,
                );

                let caption = if progress < 0.25 {
                    "A new sound is awakening…"
                } else if progress < 0.45 {
                    "Tuning the ghost…"
                } else if progress < 0.7 {
                    "Putting on the headphones…"
                } else {
                    "Let the tones flow."
                };

                painter.text(
                    Pos2::new(center.x, center.y + 210.0),
                    Align2::CENTER_CENTER,
                    "GHOST MUSIC",
                    FontId::proportional(34.0),
                    TEXT,
                );
                painter.text(
                    Pos2::new(center.x, center.y + 244.0),
                    Align2::CENTER_CENTER,
                    caption,
                    FontId::proportional(15.0),
                    MUTED,
                );

                // Progress rail.
                let rail = Rect::from_center_size(Pos2::new(center.x, center.y + 288.0), Vec2::new(260.0, 4.0));
                painter.rect_filled(rail, 2.0, Color32::from_rgb(36, 24, 32));
                let mut filled = rail;
                filled.set_right(rail.left() + rail.width() * progress);
                painter.rect_filled(filled, 2.0, RED);
                draw_seek_ghost(painter, filled, 1.0, RED);

                painter.text(
                    Pos2::new(rect.right() - 22.0, rect.bottom() - 18.0),
                    Align2::RIGHT_BOTTOM,
                    "click or press any key to skip",
                    FontId::proportional(10.0),
                    theme::FAINT,
                );
            });
        true
    }
}

impl Default for Splash {
    fn default() -> Self {
        Self::new()
    }
}

/// Draw a ghost into a rectangle, scaled to fit (used for cards/rows).
pub fn draw_fitted(painter: &egui::Painter, rect: Rect, color: Color32, style: GhostStyle) {
    let size = rect.height().min(rect.width()) * 1.05;
    draw(painter, rect.center(), size, color, 0.0, style);
}
