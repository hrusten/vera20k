//! Pause menu overlay — shown when the player presses ESC during gameplay.
//!
//! Renders an egui panel with resume, game speed, music controls, and return-to-menu.
//!
//! ## Dependency rules
//! - Part of ui/ — no dependencies on render/, assets/, audio/.
//! - Takes pure data in, returns actions out.

use crate::ui::client_theme;

/// Speed presets available in the pause menu (ticks per second).
const SPEED_PRESETS: &[(u32, &str)] = &[
    (15, "15 tps (RA2 native)"),
    (30, "30 tps"),
    (45, "45 tps (default)"),
    (60, "60 tps"),
    (100, "100 tps"),
    (200, "200 tps"),
    (500, "500 tps"),
];

/// Actions the pause menu can produce.
pub enum PauseMenuAction {
    /// Player wants to resume gameplay.
    Resume,
    /// Player wants to go back to the main menu.
    ReturnToMenu,
    /// Player clicked "Next Track".
    NextTrack,
    /// Player changed the music volume.
    SetMusicVolume(f64),
    /// Player changed the game speed.
    SetGameSpeed(u32),
    /// No action this frame.
    None,
}

/// State passed into the pause menu for display.
pub struct PauseMenuInfo<'a> {
    pub current_track: Option<&'a str>,
    pub volume: f64,
    pub speed_tps: u32,
}

/// Draw the pause menu overlay. Returns the action chosen by the player.
pub fn draw_pause_menu(ctx: &egui::Context, info: &PauseMenuInfo<'_>) -> PauseMenuAction {
    let palette = client_theme::apply_client_theme(ctx);
    let mut action: PauseMenuAction = PauseMenuAction::None;
    let mut volume: f64 = info.volume;
    let mut speed_tps: u32 = info.speed_tps;

    // Semi-transparent backdrop to dim the game.
    egui::Area::new("pause_backdrop".into())
        .fixed_pos(egui::pos2(0.0, 0.0))
        .interactable(false)
        .show(ctx, |ui| {
            let screen: egui::Rect = ctx.content_rect();
            ui.painter().rect_filled(
                screen,
                0.0,
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 120),
            );
        });

    egui::Window::new("")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .frame(client_theme::card_frame(palette.panel, palette.line))
        .min_width(360.0)
        .show(ctx, |ui| {
            ui.set_max_width(360.0);
            ui.vertical(|ui| {
                client_theme::section_label(ui, "SESSION", palette);
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Paused")
                        .size(32.0)
                        .strong()
                        .color(palette.text),
                );
                ui.label(
                    egui::RichText::new("Adjust speed, audio, or return to command.")
                        .size(15.0)
                        .color(palette.text_muted),
                );

                ui.add_space(18.0);
                if ui
                    .add_sized(
                        egui::vec2(220.0, 42.0),
                        egui::Button::new(egui::RichText::new("Resume").size(18.0).strong()),
                    )
                    .clicked()
                {
                    action = PauseMenuAction::Resume;
                }

                // ── Game Speed ──────────────────────────────────────
                ui.add_space(18.0);
                client_theme::card_frame(palette.bg_alt, palette.line.gamma_multiply(0.65)).show(
                    ui,
                    |ui| {
                        client_theme::section_label(ui, "GAME SPEED", palette);
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new("Simulation Rate")
                                .size(22.0)
                                .strong()
                                .color(palette.text),
                        );
                        ui.add_space(4.0);

                        let selected_label = SPEED_PRESETS
                            .iter()
                            .find(|(tps, _)| *tps == speed_tps)
                            .map(|(_, label)| *label)
                            .unwrap_or("Custom");

                        egui::ComboBox::from_id_salt("speed_select")
                            .width(220.0)
                            .selected_text(egui::RichText::new(selected_label).size(14.0))
                            .show_ui(ui, |ui| {
                                for &(tps, label) in SPEED_PRESETS {
                                    ui.selectable_value(&mut speed_tps, tps, label);
                                }
                            });

                        if speed_tps != info.speed_tps {
                            action = PauseMenuAction::SetGameSpeed(speed_tps);
                        }
                    },
                );

                // ── Music ───────────────────────────────────────────
                ui.add_space(18.0);
                client_theme::card_frame(palette.bg_alt, palette.line.gamma_multiply(0.65)).show(
                    ui,
                    |ui| {
                        client_theme::section_label(ui, "MUSIC", palette);
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new("Playback")
                                .size(22.0)
                                .strong()
                                .color(palette.text),
                        );

                        let track_label: &str = info.current_track.unwrap_or("(none)");
                        ui.label(
                            egui::RichText::new(format!("Now playing: {}", track_label))
                                .size(13.0)
                                .color(palette.text_muted),
                        );
                        ui.add_space(8.0);

                        if ui
                            .add_sized(
                                egui::vec2(160.0, 34.0),
                                egui::Button::new(egui::RichText::new("Next Track").size(14.0)),
                            )
                            .clicked()
                        {
                            action = PauseMenuAction::NextTrack;
                        }

                        ui.add_space(10.0);

                        let mut vol_f32: f32 = volume as f32;
                        ui.label(
                            egui::RichText::new("Volume")
                                .size(12.0)
                                .strong()
                                .color(palette.text_muted),
                        );
                        ui.add(
                            egui::Slider::new(&mut vol_f32, 0.0..=1.0)
                                .show_value(true)
                                .text("music"),
                        );
                        if (vol_f32 as f64 - volume).abs() > 0.001 {
                            volume = vol_f32 as f64;
                            action = PauseMenuAction::SetMusicVolume(volume);
                        }
                    },
                );

                ui.add_space(18.0);
                if ui
                    .add_sized(
                        egui::vec2(220.0, 42.0),
                        egui::Button::new(
                            egui::RichText::new("Return to Menu")
                                .size(18.0)
                                .strong()
                                .color(palette.danger),
                        ),
                    )
                    .clicked()
                {
                    action = PauseMenuAction::ReturnToMenu;
                }

                ui.add_space(4.0);
            });
        });

    action
}
