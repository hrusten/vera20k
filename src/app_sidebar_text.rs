//! Sidebar text overlay using egui for credits display.
//!
//! The credits counter is rendered via egui (system font). "Ready" labels
//! are now rendered as bitmap font sprites via the GAME.FNT atlas in
//! app_sidebar_build.rs — see docs/SIDEBAR_READY_TEXT_RENDERING.md.
//!
//! All text is painted in absolute screen coordinates and clipped to the
//! sidebar panel so it cannot bleed onto the world view.

use egui::{Align2, Color32, FontFamily, FontId, Id, LayerId, Order, Pos2, Vec2};

use crate::sidebar::SidebarView;

const CREDITS_FONT_SIZE: f32 = 9.4;
/// Vertical offset from radar top to credits label center (at 1x scale).
const CREDITS_CENTER_Y_FROM_RADAR_1X: f32 = -36.0;
const CREDITS_TEXT_COLOR: Color32 = Color32::from_rgb(230, 240, 255);

/// Side-dependent text colors for "Ready" labels.
/// Stored as [f32; 3] tints for sprite rendering.
const READY_COLOR_ALLIED: [f32; 3] = [164.0 / 255.0, 210.0 / 255.0, 1.0];
const READY_COLOR_SOVIET: [f32; 3] = [1.0, 1.0, 0.0];
const READY_COLOR_YURI: [f32; 3] = [1.0, 1.0, 0.0];

/// Return the ready text tint for a given sidebar theme as `[f32; 3]`.
pub fn ready_color_for_theme(theme: crate::render::sidebar_chrome::SidebarTheme) -> [f32; 3] {
    use crate::render::sidebar_chrome::SidebarTheme;
    match theme {
        SidebarTheme::Allied => READY_COLOR_ALLIED,
        SidebarTheme::Soviet => READY_COLOR_SOVIET,
        SidebarTheme::Yuri => READY_COLOR_YURI,
    }
}

/// Draw sidebar text overlays via egui (currently credits label only).
/// "Ready" labels are rendered as sprite instances in app_sidebar_build.rs.
pub fn draw_sidebar_text_overlay(ctx: &egui::Context, view: &SidebarView, ui_scale: f32) {
    let pixels_per_point = ctx.pixels_per_point();
    let clip_rect = to_egui_rect(view.panel_rect, pixels_per_point);
    let layer = LayerId::new(Order::Foreground, Id::new("sidebar_text_overlay"));
    let painter = ctx.layer_painter(layer).with_clip_rect(clip_rect);

    draw_credits_label(&painter, view, pixels_per_point, ui_scale);
}

fn draw_credits_label(
    painter: &egui::Painter,
    view: &SidebarView,
    pixels_per_point: f32,
    ui_scale: f32,
) {
    let center = pos2_points(
        view.panel_rect.x + view.panel_rect.w * 0.5,
        view.layout.radar_y + CREDITS_CENTER_Y_FROM_RADAR_1X * ui_scale,
        pixels_per_point,
    );
    let credits_text = view.credits.to_string();
    let credits_font = FontId::new(CREDITS_FONT_SIZE * ui_scale, FontFamily::Proportional);

    painter.text(
        center + vec2_points(Vec2::new(1.0, 1.0), pixels_per_point),
        Align2::CENTER_CENTER,
        &credits_text,
        credits_font.clone(),
        Color32::from_rgb(0, 0, 0),
    );
    painter.text(
        center,
        Align2::CENTER_CENTER,
        credits_text,
        credits_font,
        CREDITS_TEXT_COLOR,
    );
}

fn to_egui_rect(rect: crate::sidebar::Rect, pixels_per_point: f32) -> egui::Rect {
    egui::Rect::from_min_max(
        pos2_points(rect.x, rect.y, pixels_per_point),
        pos2_points(rect.x + rect.w, rect.y + rect.h, pixels_per_point),
    )
}

fn pos2_points(x: f32, y: f32, pixels_per_point: f32) -> Pos2 {
    Pos2::new(x / pixels_per_point, y / pixels_per_point)
}

fn vec2_points(size: Vec2, pixels_per_point: f32) -> Vec2 {
    Vec2::new(size.x / pixels_per_point, size.y / pixels_per_point)
}
