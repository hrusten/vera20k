//! UI panels for the mix-browser tool.
//!
//! Contains the toolbar, status bar, archive list, all-SHPs list, and
//! preview panel rendering. Split from mix-browser.rs for file size.

use eframe::egui;

use crate::mix_browser_preview::PreviewState;
use crate::{BrowserViewMode, MixBrowserApp};

/// Render the top toolbar: view mode, archive picker, filter, zoom, asset search.
///
/// Returns a pending search (asset_name, palette_name) if the user triggered one.
pub fn draw_toolbar(app: &mut MixBrowserApp, ctx: &egui::Context) -> Option<(String, String)> {
    let mut pending_search: Option<(String, String)> = None;

    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("View:");
            ui.selectable_value(&mut app.view_mode, BrowserViewMode::Archive, "Archive");
            ui.selectable_value(&mut app.view_mode, BrowserViewMode::AllShps, "All SHPs");

            if ui
                .button(if app.shp_index.is_some() {
                    "Rescan SHPs"
                } else {
                    "Scan SHPs"
                })
                .clicked()
            {
                app.rebuild_shp_index();
                app.view_mode = BrowserViewMode::AllShps;
            }

            if app.view_mode == BrowserViewMode::Archive {
                ui.separator();
                ui.label("Archive:");
                let current_name = app
                    .loaded
                    .get(app.selected)
                    .map(|c| c.mix_name.clone())
                    .unwrap_or_default();
                egui::ComboBox::from_id_salt("archive_picker")
                    .selected_text(&current_name)
                    .width(260.0)
                    .show_ui(ui, |ui| {
                        draw_archive_combo(app, ui);
                    });

                if !app.sidebar_archive_names.is_empty() {
                    ui.separator();
                    ui.label("Sidebar:");
                    if ui.button("Load All").clicked() {
                        app.load_sidebar_archives();
                    }
                    let sidebar_names = app.sidebar_archive_names.clone();
                    for name in sidebar_names {
                        let is_selected = app
                            .loaded
                            .get(app.selected)
                            .map(|contents| contents.mix_name.eq_ignore_ascii_case(&name))
                            .unwrap_or(false);
                        if ui.selectable_label(is_selected, &name).clicked() {
                            app.select_archive(&name);
                        }
                    }
                }
            }

            ui.separator();
            ui.label("Filter:");
            ui.text_edit_singleline(&mut app.filter);

            ui.separator();
            ui.label("Zoom:");
            ui.selectable_value(&mut app.preview_zoom, 1.0, "1x");
            ui.selectable_value(&mut app.preview_zoom, 2.0, "2x");
            ui.selectable_value(&mut app.preview_zoom, 4.0, "4x");
        });

        ui.horizontal(|ui| {
            ui.label("Asset search:");
            let search_response = ui.add(
                egui::TextEdit::singleline(&mut app.asset_search)
                    .hint_text("e.g. pips.shp, mouse.shp")
                    .desired_width(220.0),
            );
            ui.label("Palette:");
            ui.add(egui::TextEdit::singleline(&mut app.search_palette_name).desired_width(140.0));
            let go_clicked = ui.button("Go").clicked();
            let enter_pressed = search_response.lost_focus()
                && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if go_clicked || enter_pressed {
                pending_search = Some((app.asset_search.clone(), app.search_palette_name.clone()));
            }
        });
    });

    pending_search
}

/// Render the archive combo box entries (loaded + available).
fn draw_archive_combo(app: &mut MixBrowserApp, ui: &mut egui::Ui) {
    for (i, contents) in app.loaded.iter().enumerate() {
        let label = format!("{} ({} entries)", contents.mix_name, contents.entries.len());
        if ui.selectable_label(app.selected == i, label).clicked() {
            app.selected = i;
        }
    }
    ui.separator();
    let names = app.archive_names.clone();
    for name in names {
        let already = app
            .loaded
            .iter()
            .any(|c| c.mix_name.eq_ignore_ascii_case(&name));
        if !already && ui.selectable_label(false, &name).clicked() {
            let idx = app.ensure_loaded(&name);
            app.selected = idx;
        }
    }
}

/// Render the bottom status bar.
pub fn draw_status_bar(app: &MixBrowserApp, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("status").show(ctx, |ui| match app.view_mode {
        BrowserViewMode::Archive => {
            if let Some(contents) = app.loaded.get(app.selected) {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "{}: {} entries, {} identified, {:.1} KB",
                        contents.mix_name,
                        contents.entries.len(),
                        contents.identified_count,
                        contents.archive_size as f64 / 1024.0,
                    ));
                    if let Some(error) = &contents.error {
                        ui.colored_label(egui::Color32::RED, error);
                    }
                });
            }
        }
        BrowserViewMode::AllShps => {
            if let Some(index) = &app.shp_index {
                ui.label(format!(
                    "All SHPs: {} parsed entries across {} archives ({} total scanned)",
                    index.rows.len(),
                    index.scanned_archives,
                    index.scanned_entries
                ));
            } else {
                ui.label("All SHPs: not scanned yet");
            }
        }
    });
}

/// House color choices for the remap toggle.
pub const HOUSE_COLORS: &[&str] = &[
    "None",
    "Gold",
    "DarkBlue",
    "DarkRed",
    "Green",
    "Orange",
    "Purple",
    "LightBlue",
    "Brown",
];

/// Interactions returned from the preview panel that need mutable access.
pub struct PreviewPanelResult {
    pub pending_frame: Option<usize>,
    pub toggle_play: bool,
    pub new_fps: Option<f32>,
    pub export_png: bool,
    pub house_color_changed: Option<usize>,
    pub palette_changed: Option<String>,
}

/// Render the right-side preview panel. Returns pending interactions.
pub fn draw_preview_panel(
    preview: &Option<PreviewState>,
    preview_zoom: f32,
    available_palettes: &[String],
    ctx: &egui::Context,
) -> PreviewPanelResult {
    let mut result = PreviewPanelResult {
        pending_frame: None,
        toggle_play: false,
        new_fps: None,
        export_png: false,
        house_color_changed: None,
        palette_changed: None,
    };

    egui::SidePanel::right("preview_panel")
        .min_width(220.0)
        .default_width(320.0)
        .show(ctx, |ui| {
            ui.heading("Preview");
            ui.separator();

            if let Some(preview) = preview {
                draw_preview_info(ui, preview);
                if let Some(frame) = draw_frame_navigation(ui, preview) {
                    result.pending_frame = Some(frame);
                }
                let (toggle, fps) = draw_animation_controls(ui, preview);
                result.toggle_play = toggle;
                result.new_fps = fps;

                // Palette selector (for SHP and TMP previews).
                if preview.shp.is_some() || preview.file_type.starts_with("TMP") {
                    result.palette_changed = draw_palette_combo(ui, preview, available_palettes);
                }

                // House color remap + Export controls.
                if preview.shp.is_some() {
                    ui.horizontal(|ui| {
                        result.house_color_changed = draw_house_color_combo(ui, preview);
                        if ui.button("Export PNG").clicked() {
                            result.export_png = true;
                        }
                    });
                } else if preview.texture.is_some() {
                    // PAL/other previews can still export.
                    if ui.button("Export PNG").clicked() {
                        result.export_png = true;
                    }
                }

                ui.separator();
                if let Some(texture) = &preview.texture {
                    let size = texture.size_vec2() * preview_zoom;
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add(egui::Image::new(texture).fit_to_exact_size(size));
                        });
                }
            } else {
                ui.label("Click an entry to preview");
            }
        });

    result
}

/// Draw preview metadata (source, name, type, palette, errors).
fn draw_preview_info(ui: &mut egui::Ui, preview: &PreviewState) {
    ui.label(format!("Source: {}", preview.source_name));
    ui.label(format!("Name: {}", preview.resolved_name));
    ui.label(format!("Type: {}", preview.file_type));
    ui.label(format!("Size: {}", preview.dimensions));
    if let Some(palette_name) = &preview.palette_name {
        ui.label(format!("Palette: {}", palette_name));
    }
    if let Some(error) = &preview.error {
        ui.colored_label(egui::Color32::RED, error);
    }
}

/// Draw frame navigation controls (buttons + slider). Returns new frame if changed.
fn draw_frame_navigation(ui: &mut egui::Ui, preview: &PreviewState) -> Option<usize> {
    if preview.frame_count <= 1 {
        return None;
    }

    ui.label(format!(
        "Frame: {} / {}",
        preview.current_frame + 1,
        preview.frame_count
    ));
    let mut new_frame = preview.current_frame;
    ui.horizontal(|ui| {
        if ui.button("|<").clicked() {
            new_frame = 0;
        }
        if ui.button("<").clicked() && preview.current_frame > 0 {
            new_frame = preview.current_frame - 1;
        }
        if ui.button(">").clicked() && preview.current_frame + 1 < preview.frame_count {
            new_frame = preview.current_frame + 1;
        }
        if ui.button(">|").clicked() {
            new_frame = preview.frame_count - 1;
        }
    });
    let mut slider_val = preview.current_frame;
    if ui
        .add(egui::Slider::new(
            &mut slider_val,
            0..=(preview.frame_count - 1),
        ))
        .changed()
    {
        new_frame = slider_val;
    }
    if new_frame != preview.current_frame {
        Some(new_frame)
    } else {
        None
    }
}

/// Draw animation play/pause and FPS controls.
/// Returns (toggle_play_clicked, new_fps_if_changed).
fn draw_animation_controls(ui: &mut egui::Ui, preview: &PreviewState) -> (bool, Option<f32>) {
    if preview.frame_count <= 1 {
        return (false, None);
    }
    let mut toggle = false;
    let mut new_fps: Option<f32> = None;
    ui.horizontal(|ui| {
        let play_label = if preview.is_playing { "Pause" } else { "Play" };
        if ui.button(play_label).clicked() {
            toggle = true;
        }
        let mut fps = preview.play_speed_fps;
        if ui
            .add(egui::Slider::new(&mut fps, 1.0..=60.0).text("fps"))
            .changed()
        {
            new_fps = Some(fps);
        }
    });
    (toggle, new_fps)
}

/// Render the central panel: archive entry grid or all-SHPs grid.
///
/// Returns (source_archive, hash, hinted_name) if an entry was clicked.
pub fn draw_central_panel(
    app: &MixBrowserApp,
    ctx: &egui::Context,
) -> Option<(String, i32, String)> {
    let mut clicked: Option<(String, i32, String)> = None;
    let preview_hash = app.preview.as_ref().map(|p| p.entry_hash);
    let preview_source = app.preview.as_ref().map(|p| p.source_name.clone());
    let filter_lower = app.filter.to_ascii_lowercase();

    egui::CentralPanel::default().show(ctx, |ui| match app.view_mode {
        BrowserViewMode::Archive => {
            clicked = draw_archive_grid(app, &filter_lower, preview_hash, &preview_source, ui);
        }
        BrowserViewMode::AllShps => {
            clicked = draw_all_shps_grid(app, &filter_lower, preview_hash, &preview_source, ui);
        }
    });

    clicked
}

/// Render the archive entry grid. Returns clicked entry if any.
fn draw_archive_grid(
    app: &MixBrowserApp,
    filter_lower: &str,
    preview_hash: Option<i32>,
    preview_source: &Option<String>,
    ui: &mut egui::Ui,
) -> Option<(String, i32, String)> {
    let Some(contents) = app.loaded.get(app.selected) else {
        ui.label("No archive loaded.");
        return None;
    };

    let mut clicked: Option<(String, i32, String)> = None;
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("mix_entries")
                .striped(true)
                .num_columns(5)
                .min_col_width(40.0)
                .show(ui, |ui| {
                    for header in ["#", "Hash", "Size", "Name", "Type / Details"] {
                        ui.strong(header);
                    }
                    ui.end_row();

                    for row in &contents.entries {
                        if !filter_lower.is_empty()
                            && !matches_filter(&row.name, &row.file_type, row.hash, filter_lower)
                        {
                            continue;
                        }
                        let is_sel = preview_hash == Some(row.hash)
                            && preview_source.as_deref() == Some(contents.mix_name.as_str());
                        let name_color = entry_name_color(is_sel, row.identified);
                        let cells = [
                            (row.index.to_string(), egui::Color32::WHITE),
                            (format!("{:#010X}", row.hash), egui::Color32::WHITE),
                            (row.size.to_string(), egui::Color32::WHITE),
                            (row.name.clone(), name_color),
                            (row.file_type.clone(), egui::Color32::WHITE),
                        ];
                        if draw_clickable_row(ui, &cells) {
                            clicked = Some((contents.mix_name.clone(), row.hash, row.name.clone()));
                        }
                    }
                });
        });
    clicked
}

/// Render the all-SHPs index grid. Returns clicked entry if any.
fn draw_all_shps_grid(
    app: &MixBrowserApp,
    filter_lower: &str,
    preview_hash: Option<i32>,
    preview_source: &Option<String>,
    ui: &mut egui::Ui,
) -> Option<(String, i32, String)> {
    let Some(index) = &app.shp_index else {
        ui.label("Building SHP index...");
        return None;
    };

    let mut clicked: Option<(String, i32, String)> = None;
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("all_shp_entries")
                .striped(true)
                .num_columns(7)
                .min_col_width(40.0)
                .show(ui, |ui| {
                    for header in ["Source", "#", "Hash", "Name", "Size", "Dims", "Frames"] {
                        ui.strong(header);
                    }
                    ui.end_row();

                    for row in &index.rows {
                        if !filter_lower.is_empty()
                            && !matches_filter_shp(&row.source, &row.name, row.hash, filter_lower)
                        {
                            continue;
                        }
                        let is_sel = preview_hash == Some(row.hash)
                            && preview_source.as_deref() == Some(row.source.as_str());
                        let source_color = if is_sel {
                            egui::Color32::YELLOW
                        } else {
                            egui::Color32::WHITE
                        };
                        let name_color = entry_name_color(is_sel, row.identified);
                        let cells = [
                            (row.source.clone(), source_color),
                            (row.index.to_string(), egui::Color32::WHITE),
                            (format!("{:#010X}", row.hash), egui::Color32::WHITE),
                            (row.name.clone(), name_color),
                            (row.size.to_string(), egui::Color32::WHITE),
                            (
                                format!("{}x{}", row.width, row.height),
                                egui::Color32::WHITE,
                            ),
                            (row.frames.to_string(), egui::Color32::WHITE),
                        ];
                        if draw_clickable_row(ui, &cells) {
                            clicked = Some((row.source.clone(), row.hash, row.name.clone()));
                        }
                    }
                });
        });
    clicked
}

/// Draw a row of clickable colored cells. Returns true if any cell was clicked.
fn draw_clickable_row(ui: &mut egui::Ui, cells: &[(String, egui::Color32)]) -> bool {
    let mut any_clicked = false;
    for (text, color) in cells {
        if ui.colored_label(*color, text).clicked() {
            any_clicked = true;
        }
    }
    ui.end_row();
    any_clicked
}

/// Choose name color: yellow if selected, green if identified, gray otherwise.
fn entry_name_color(is_selected: bool, identified: bool) -> egui::Color32 {
    if is_selected {
        egui::Color32::YELLOW
    } else if identified {
        egui::Color32::LIGHT_GREEN
    } else {
        egui::Color32::GRAY
    }
}

/// Check if an archive entry matches the filter string.
fn matches_filter(name: &str, file_type: &str, hash: i32, filter: &str) -> bool {
    let name_lower = name.to_ascii_lowercase();
    let type_lower = file_type.to_ascii_lowercase();
    let hash_text = format!("{:#010X}", hash).to_ascii_lowercase();
    name_lower.contains(filter) || type_lower.contains(filter) || hash_text.contains(filter)
}

/// Draw a house color remap combo box. Returns Some(index) if changed.
fn draw_house_color_combo(ui: &mut egui::Ui, preview: &PreviewState) -> Option<usize> {
    let current = preview.house_color_index;
    let current_label = HOUSE_COLORS.get(current).copied().unwrap_or("None");
    let mut changed: Option<usize> = None;
    egui::ComboBox::from_id_salt("house_color")
        .selected_text(current_label)
        .width(100.0)
        .show_ui(ui, |ui| {
            for (i, label) in HOUSE_COLORS.iter().enumerate() {
                if ui.selectable_label(current == i, *label).clicked() && i != current {
                    changed = Some(i);
                }
            }
        });
    changed
}

/// Draw a palette selector combo box. Returns Some(palette_name) if changed.
fn draw_palette_combo(
    ui: &mut egui::Ui,
    preview: &PreviewState,
    available_palettes: &[String],
) -> Option<String> {
    let current_label = preview.palette_name.as_deref().unwrap_or("(none)");
    let mut changed: Option<String> = None;

    ui.horizontal(|ui| {
        ui.label("Palette:");
        egui::ComboBox::from_id_salt("palette_select")
            .selected_text(current_label)
            .width(160.0)
            .show_ui(ui, |ui| {
                for pal_name in available_palettes {
                    let is_current = preview
                        .palette_name
                        .as_deref()
                        .map(|n| n == pal_name)
                        .unwrap_or(false);
                    if ui.selectable_label(is_current, pal_name).clicked() && !is_current {
                        changed = Some(pal_name.clone());
                    }
                }
            });
    });

    changed
}

/// Check if an all-SHPs entry matches the filter string.
fn matches_filter_shp(source: &str, name: &str, hash: i32, filter: &str) -> bool {
    let source_lower = source.to_ascii_lowercase();
    let name_lower = name.to_ascii_lowercase();
    let hash_text = format!("{:#010X}", hash).to_ascii_lowercase();
    source_lower.contains(filter) || name_lower.contains(filter) || hash_text.contains(filter)
}
