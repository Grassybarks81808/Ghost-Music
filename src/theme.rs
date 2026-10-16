//! Ghost Music colour palette and egui theme.

use egui::{Color32, CornerRadius, Stroke, Visuals};

/// Brand red used for accents, active tabs and the ghost mark.
pub const RED: Color32 = Color32::from_rgb(244, 43, 65);
pub const RED_HOVER: Color32 = Color32::from_rgb(255, 74, 94);
pub const RED_DIM: Color32 = Color32::from_rgb(120, 26, 40);
/// Deep red used for "selected" surfaces.
pub const RED_SOFT: Color32 = Color32::from_rgb(58, 24, 33);

/// Window / content background.
pub const BG: Color32 = Color32::from_rgb(11, 12, 16);
/// Cards and raised surfaces.
pub const PANEL: Color32 = Color32::from_rgb(21, 23, 30);
pub const PANEL_HOVER: Color32 = Color32::from_rgb(30, 33, 42);
pub const PANEL_SUNKEN: Color32 = Color32::from_rgb(16, 18, 24);
/// Top bar / player bar chrome.
pub const CHROME: Color32 = Color32::from_rgb(15, 16, 21);
pub const HAIRLINE: Color32 = Color32::from_rgb(38, 41, 52);

pub const TEXT: Color32 = Color32::from_rgb(242, 243, 248);
pub const MUTED: Color32 = Color32::from_rgb(150, 155, 172);
pub const FAINT: Color32 = Color32::from_rgb(102, 107, 124);

pub const GOOD: Color32 = Color32::from_rgb(88, 214, 148);
pub const WARN: Color32 = Color32::from_rgb(240, 186, 84);

/// Widgets bubble red + the app theme.
pub fn apply(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();

    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.faint_bg_color = PANEL_SUNKEN;
    visuals.extreme_bg_color = PANEL_SUNKEN;
    visuals.code_bg_color = PANEL_SUNKEN;
    visuals.hyperlink_color = RED;
    visuals.selection.bg_fill = RED_DIM;
    visuals.selection.stroke = Stroke::new(1.0, TEXT);
    visuals.window_stroke = Stroke::new(1.0, HAIRLINE);
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.slider_trailing_fill = true;

    let radius = CornerRadius::same(8);
    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.weak_bg_fill = PANEL;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, HAIRLINE);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, MUTED);
    visuals.widgets.noninteractive.corner_radius = radius;

    visuals.widgets.inactive.bg_fill = PANEL_HOVER;
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(27, 29, 37);
    visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.inactive.corner_radius = radius;

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(40, 43, 54);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(40, 43, 54);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, RED_DIM);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.hovered.corner_radius = radius;

    visuals.widgets.active.bg_fill = RED;
    visuals.widgets.active.weak_bg_fill = RED;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, RED_HOVER);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.corner_radius = radius;

    visuals.widgets.open.bg_fill = PANEL_HOVER;
    visuals.widgets.open.weak_bg_fill = PANEL_HOVER;
    visuals.widgets.open.corner_radius = radius;

    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.slider_width = 140.0;
    style.spacing.scroll.bar_width = 9.0;
    style.spacing.scroll.bar_inner_margin = 3.0;
    style.spacing.scroll.bar_outer_margin = 0.0;
    style.spacing.scroll.floating = true;
    style.spacing.scroll.dormant_background_opacity = 0.0;
    style.spacing.interact_size.y = 26.0;
    ctx.set_style(style);
}
