//! Shared egui styling and chrome helpers for client-facing screens.
//!
//! Uses egui's built-in light theme to match the mix-browser look.

#[derive(Clone, Copy)]
pub struct ClientPalette {
    pub bg: egui::Color32,
    pub bg_alt: egui::Color32,
    pub panel: egui::Color32,
    pub panel_alt: egui::Color32,
    pub line: egui::Color32,
    pub text: egui::Color32,
    pub text_muted: egui::Color32,
    pub accent: egui::Color32,
    pub accent_soft: egui::Color32,
    pub success: egui::Color32,
    pub danger: egui::Color32,
}

pub fn apply_client_theme(ctx: &egui::Context) -> ClientPalette {
    let palette = palette();
    let mut style = (*ctx.style()).clone();

    // Use egui's built-in light theme — matches eframe/mix-browser defaults.
    style.visuals = egui::Visuals::light();

    ctx.set_style(style);
    palette
}

pub fn card_frame(fill: egui::Color32, stroke: egui::Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(18))
}

pub fn section_label(ui: &mut egui::Ui, text: &str, palette: ClientPalette) {
    ui.label(
        egui::RichText::new(text)
            .size(14.0)
            .strong()
            .color(palette.text_muted),
    );
}

pub fn paint_background(ui: &mut egui::Ui, palette: ClientPalette) {
    let rect = ui.max_rect();
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, palette.bg);
}

fn palette() -> ClientPalette {
    ClientPalette {
        bg: egui::Color32::from_rgb(248, 248, 248),
        bg_alt: egui::Color32::from_rgb(238, 238, 238),
        panel: egui::Color32::from_rgb(255, 255, 255),
        panel_alt: egui::Color32::from_rgb(245, 245, 245),
        line: egui::Color32::from_rgb(200, 200, 200),
        text: egui::Color32::from_rgb(30, 30, 30),
        text_muted: egui::Color32::from_rgb(110, 110, 110),
        accent: egui::Color32::from_rgb(0, 109, 204),
        accent_soft: egui::Color32::from_rgb(100, 160, 220),
        success: egui::Color32::from_rgb(40, 150, 60),
        danger: egui::Color32::from_rgb(200, 60, 50),
    }
}
