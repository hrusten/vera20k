//! Small egui helpers for mission announcements and mission result screens.

use crate::ui::client_theme;

/// Draw a transient mission announcement banner near the top-center.
pub fn draw_mission_banner(ctx: &egui::Context, text: &str) {
    let palette = client_theme::apply_client_theme(ctx);
    egui::Area::new("mission_banner".into())
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 24.0))
        .interactable(false)
        .show(ctx, |ui| {
            client_theme::card_frame(
                egui::Color32::from_rgba_unmultiplied(
                    palette.panel.r(),
                    palette.panel.g(),
                    palette.panel.b(),
                    230,
                ),
                palette.accent,
            )
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(text)
                        .size(22.0)
                        .strong()
                        .color(palette.accent),
                );
            });
        });
}

/// Draw the mission result screen. Returns `true` when the user wants to
/// leave the result screen and return to the main menu.
pub fn draw_mission_result_screen(ctx: &egui::Context, title: &str, detail: &str) -> bool {
    let palette = client_theme::apply_client_theme(ctx);
    let mut back_to_menu = false;
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(palette.bg))
        .show(ctx, |ui| {
            client_theme::paint_background(ui, palette);
            ui.vertical_centered(|ui| {
                ui.add_space(96.0);
                client_theme::card_frame(palette.panel, palette.line).show(ui, |ui| {
                    ui.set_max_width(560.0);
                    client_theme::section_label(ui, "MISSION REPORT", palette);
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(title)
                            .size(46.0)
                            .strong()
                            .color(palette.text),
                    );
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(detail)
                            .size(18.0)
                            .color(palette.text_muted),
                    );
                    ui.add_space(28.0);
                    if ui
                        .add_sized(
                            egui::vec2(220.0, 44.0),
                            egui::Button::new(
                                egui::RichText::new("Back To Menu").size(18.0).strong(),
                            ),
                        )
                        .clicked()
                    {
                        back_to_menu = true;
                    }
                });
            });
        });
    back_to_menu
}
