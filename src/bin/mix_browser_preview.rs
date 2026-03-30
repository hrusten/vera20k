//! Preview state and SHP rendering for the mix-browser tool.
//!
//! Manages the right-side preview panel: SHP frame rendering with palette
//! inference, frame navigation, and asset search.

use crate::mix_browser_renderers;
use eframe::egui;
use vera20k::assets::asset_manager::AssetManager;
use vera20k::assets::pal_file::Palette;
use vera20k::assets::shp_file::ShpFile;
use vera20k::assets::tmp_file::TmpFile;
use vera20k::rules::art_data::ArtRegistry;

/// State for the currently previewed asset in the right panel.
pub struct PreviewState {
    pub entry_hash: i32,
    pub source_name: String,
    pub resolved_name: String,
    pub current_frame: usize,
    pub frame_count: usize,
    pub dimensions: String,
    pub texture: Option<egui::TextureHandle>,
    pub error: Option<String>,
    pub file_type: String,
    pub shp: Option<ShpFile>,
    pub palette: Option<Palette>,
    pub palette_name: Option<String>,
    /// Raw asset bytes, kept for re-rendering (e.g. palette change, export).
    pub raw_bytes: Option<Vec<u8>>,
    /// Animation playback state.
    pub is_playing: bool,
    pub play_speed_fps: f32,
    pub last_frame_time: f64,
    /// House color index for remap preview (0 = none, 1-8 = color).
    pub house_color_index: usize,
}

impl PreviewState {
    /// Create an error-only preview (no renderable data).
    pub fn error(entry_hash: i32, source_name: &str, resolved_name: &str, error: String) -> Self {
        Self {
            entry_hash,
            source_name: source_name.to_string(),
            resolved_name: resolved_name.to_string(),
            current_frame: 0,
            frame_count: 0,
            dimensions: String::new(),
            texture: None,
            error: Some(error),
            file_type: String::new(),
            shp: None,
            palette: None,
            palette_name: None,
            raw_bytes: None,
            is_playing: false,
            play_speed_fps: 15.0,
            last_frame_time: 0.0,
            house_color_index: 0,
        }
    }
}

/// Render a single SHP frame into an egui texture.
///
/// Index 0 pixels are drawn as a checkerboard pattern (transparent).
/// All other pixels use the palette color lookup.
pub fn render_shp_frame_texture(
    ctx: &egui::Context,
    shp: &ShpFile,
    frame_idx: usize,
    palette: &Palette,
) -> Option<egui::TextureHandle> {
    if frame_idx >= shp.frames.len() {
        return None;
    }

    let frame = &shp.frames[frame_idx];
    let width = shp.width as usize;
    let height = shp.height as usize;
    if width == 0 || height == 0 {
        return None;
    }

    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let fx = x as i32 - frame.frame_x as i32;
            let fy = y as i32 - frame.frame_y as i32;
            let in_frame = fx >= 0
                && fy >= 0
                && fx < frame.frame_width as i32
                && fy < frame.frame_height as i32;

            if in_frame {
                let idx = fy as usize * frame.frame_width as usize + fx as usize;
                let palette_index = frame.pixels[idx];
                if palette_index == 0 {
                    let checker = ((x / 4) + (y / 4)) % 2 == 0;
                    let value = if checker { 40u8 } else { 60u8 };
                    rgba.extend_from_slice(&[value, value, value, 255]);
                } else {
                    let color = palette.colors[palette_index as usize];
                    rgba.extend_from_slice(&[color.r, color.g, color.b, 255]);
                }
            } else {
                let checker = ((x / 4) + (y / 4)) % 2 == 0;
                let value = if checker { 40u8 } else { 60u8 };
                rgba.extend_from_slice(&[value, value, value, 255]);
            }
        }
    }

    let image = egui::ColorImage::from_rgba_unmultiplied([width, height], &rgba);
    Some(ctx.load_texture(
        format!("preview_{}_{}_{}", shp.width, shp.height, frame_idx),
        image,
        egui::TextureOptions::NEAREST,
    ))
}

/// Source archive name → most likely palette mapping.
///
/// RA2/YR assets are organized by category in MIX archives. The archive name
/// is the strongest signal for which palette to use, since assets within an
/// archive are designed for a specific palette family.
const ARCHIVE_PALETTE_MAP: &[(&str, &[&str])] = &[
    // Isometric terrain palettes (per theater).
    ("isotem", &["isotem.pal"]),
    ("isotemp", &["isotem.pal"]),
    ("isosno", &["isosno.pal"]),
    ("isosnow", &["isosno.pal"]),
    ("isourb", &["isourb.pal"]),
    ("isodes", &["isodes.pal"]),
    ("isolun", &["isolun.pal"]),
    ("isonurb", &["isonurb.pal"]),
    // Theater terrain extras.
    ("tem", &["temperat.pal", "unittem.pal"]),
    ("temperat", &["temperat.pal", "unittem.pal"]),
    ("sno", &["snow.pal", "unitsno.pal"]),
    ("snow", &["snow.pal", "unitsno.pal"]),
    ("urb", &["urban.pal", "uniturb.pal"]),
    ("urban", &["urban.pal", "uniturb.pal"]),
    ("des", &["desert.pal", "unitdes.pal"]),
    ("desert", &["desert.pal", "unitdes.pal"]),
    ("lun", &["lunar.pal", "unitlun.pal"]),
    ("lunar", &["lunar.pal", "unitlun.pal"]),
    // Sidebar chrome archives.
    ("sidec01", &["sidebar.pal"]),
    ("sidec02", &["sidebar.pal"]),
    ("sidec02md", &["sidebar.pal"]),
    // Cameo archives.
    ("cameo", &["cameo.pal"]),
    ("cameomd", &["cameomd.pal", "cameo.pal"]),
    // Unit/building archives (use temperate unit palette as default).
    ("conquer", &["unittem.pal"]),
    ("conqmd", &["unittem.pal"]),
    ("local", &["unittem.pal"]),
    ("localmd", &["unittem.pal"]),
    ("cache", &["unittem.pal"]),
    ("cachemd", &["unittem.pal"]),
    ("mapsmd03", &["unittem.pal"]),
    ("maps01", &["unittem.pal"]),
    ("maps02", &["unittem.pal"]),
    // Expansion.
    ("expandmd01", &["unittem.pal"]),
    ("ra2", &["unittem.pal"]),
    ("ra2md", &["unittem.pal"]),
];

/// Smart palette inference: tries to find the best palette for a given asset
/// using art.ini declarations, source archive mapping, and filename heuristics.
pub fn find_palette_for_asset(
    asset_manager: &AssetManager,
    dict: &[(String, i32)],
    art_registry: &ArtRegistry,
    asset_name: Option<&str>,
    source_archive: &str,
    override_name: Option<&str>,
) -> (Option<Palette>, Option<String>) {
    let mut palette_names: Vec<String> = Vec::new();

    // 1. User-specified override takes highest priority.
    if let Some(override_name) = override_name {
        let trimmed = override_name.trim();
        if !trimmed.is_empty() {
            palette_names.push(trimmed.to_ascii_lowercase());
        }
    }

    let lower = asset_name.unwrap_or_default().to_ascii_lowercase();
    let source_lower = source_archive.to_ascii_lowercase();

    // 2. art.ini Palette= lookup (e.g., Palette=anim → anim.pal).
    //    Strip extension to get the base name for art.ini lookup.
    let base_name = lower
        .strip_suffix(".shp")
        .or_else(|| lower.strip_suffix(".vxl"))
        .or_else(|| lower.strip_suffix(".hva"))
        .unwrap_or(&lower);
    if let Some(pal_base) = art_registry.resolve_declared_palette_id(base_name, "") {
        palette_names.push(format!("{}.pal", pal_base));
    }

    // 3. Source archive → palette mapping (strongest automatic signal).
    let archive_base = source_lower.strip_suffix(".mix").unwrap_or(&source_lower);
    for &(pattern, palettes) in ARCHIVE_PALETTE_MAP {
        if archive_base == pattern {
            for pal in palettes {
                palette_names.push(pal.to_string());
            }
            break;
        }
    }

    // 4. Filename-based heuristics (catch remaining cases).
    if lower == "radary.shp" {
        palette_names.push("radaryuri.pal".to_string());
    }
    if lower.contains("mouse") || lower.contains("cursor") || lower.contains("pointer") {
        palette_names.push("mousepal.pal".to_string());
    }

    let is_cameo = lower.contains("icon") || source_lower.contains("cameo");
    if is_cameo {
        palette_names.push("cameomd.pal".to_string());
        palette_names.push("cameo.pal".to_string());
    }

    let is_sidebar_ui = lower.starts_with("side")
        || lower.starts_with("tab")
        || lower.contains("radar")
        || lower.contains("power")
        || lower.contains("repair")
        || lower.contains("sell")
        || lower.contains("clock")
        || lower.contains("credits")
        || lower.contains("dialog")
        || lower.contains("title")
        || lower.contains("button")
        || lower.contains("btn")
        || lower.contains("scroll")
        || lower.contains("tooltip")
        || lower.contains("menu")
        || lower.contains("sidebar")
        || source_lower.contains("sidec");
    if is_sidebar_ui {
        palette_names.push("sidebar.pal".to_string());
        palette_names.push("uibkgd.pal".to_string());
        palette_names.push("uibkgdy.pal".to_string());
    }

    if lower.contains("anim") || source_lower.contains("anim") {
        palette_names.push("anim.pal".to_string());
    }

    // Loading screen assets (ls800bkgr.shp, loadbtn.shp, etc.).
    if lower.starts_with("ls") || lower.starts_with("load") || lower.contains("loading") {
        palette_names.push("ls800bkg.pal".to_string());
        palette_names.push("ls640bkg.pal".to_string());
        palette_names.push("load.pal".to_string());
    }

    // 5. Fallback chain: try common palettes in order.
    for candidate in [
        "unittem.pal",
        "temperat.pal",
        "isotem.pal",
        "uniturb.pal",
        "unitsno.pal",
        "unitdes.pal",
        "unitlun.pal",
        "anim.pal",
        "sidebar.pal",
        "cameo.pal",
    ] {
        palette_names.push(candidate.to_string());
    }

    palette_names.dedup();

    // Try each candidate palette name.
    for palette_name in palette_names {
        let Some(bytes) = asset_manager.get(&palette_name) else {
            continue;
        };
        let Ok(palette) = Palette::from_bytes(&bytes) else {
            continue;
        };
        return (Some(palette), Some(palette_name));
    }

    // Last resort: scan the source archive for any 768-byte entry.
    if let Some(archive) = asset_manager.archive(source_archive) {
        for entry in archive.entries() {
            if entry.size != 768 {
                continue;
            }
            let Some(bytes) = archive.get_by_id(entry.id) else {
                continue;
            };
            let Ok(palette) = Palette::from_bytes(bytes) else {
                continue;
            };
            let pal_name = resolve_name_from_dict(dict, entry.id)
                .unwrap_or_else(|| format!("archive-pal {:#010X}", entry.id as u32));
            return (Some(palette), Some(pal_name));
        }
    }

    (None, None)
}

/// Look up a hash in the dictionary, returning the resolved name if found.
fn resolve_name_from_dict(dict: &[(String, i32)], hash: i32) -> Option<String> {
    dict.iter()
        .find(|(_, candidate_hash)| *candidate_hash == hash)
        .map(|(name, _)| name.clone())
}

/// Build a PreviewState from raw bytes. Tries SHP parsing first, then
/// falls back to a generic info display.
pub fn preview_from_bytes(
    asset_manager: &AssetManager,
    dict: &[(String, i32)],
    art_registry: &ArtRegistry,
    source_label: &str,
    palette_source_archive: &str,
    entry_hash: i32,
    data: Vec<u8>,
    hinted_name: Option<&str>,
    palette_override: Option<&str>,
    ctx: &egui::Context,
) -> PreviewState {
    let file_type = crate::mix_browser_data::detect_file_type(&data);
    let resolved_name = hinted_name
        .map(ToOwned::to_owned)
        .or_else(|| resolve_name_from_dict(dict, entry_hash))
        .unwrap_or_else(|| format!("??? ({:#010X})", entry_hash));

    // PAL: 768 bytes → render as 16x16 color grid.
    if data.len() == 768 {
        if let Ok(palette) = Palette::from_bytes(&data) {
            let image = mix_browser_renderers::render_palette_grid(&palette);
            let texture = ctx.load_texture(
                format!("pal_preview_{}", entry_hash),
                image,
                egui::TextureOptions::NEAREST,
            );
            let pal_name = resolved_name.clone();
            return PreviewState {
                entry_hash,
                source_name: source_label.to_string(),
                resolved_name,
                current_frame: 0,
                frame_count: 0,
                dimensions: "256 colors (16x16 grid)".to_string(),
                texture: Some(texture),
                error: None,
                file_type,
                shp: None,
                palette: Some(palette),
                palette_name: Some(pal_name),
                raw_bytes: Some(data),
                is_playing: false,
                play_speed_fps: 15.0,
                last_frame_time: 0.0,
                house_color_index: 0,
            };
        }
    }

    // TMP: detect by file_type string (set by detect_file_type).
    if file_type.starts_with("TMP") {
        if let Ok(tmp) = TmpFile::from_bytes(&data) {
            // Find a theater palette for rendering.
            let (palette, palette_name) = find_palette_for_asset(
                asset_manager,
                dict,
                art_registry,
                Some(&resolved_name),
                palette_source_archive,
                palette_override,
            );
            if let Some(pal) = &palette {
                if let Some((image, tile_count)) =
                    mix_browser_renderers::render_tmp_preview(&tmp, pal)
                {
                    let total_cells = (tmp.template_width * tmp.template_height) as usize;
                    let texture = ctx.load_texture(
                        format!("tmp_preview_{}", entry_hash),
                        image,
                        egui::TextureOptions::NEAREST,
                    );
                    return PreviewState {
                        entry_hash,
                        source_name: source_label.to_string(),
                        resolved_name,
                        current_frame: 0,
                        frame_count: 0,
                        dimensions: format!(
                            "{}x{} template, {}/{} tiles",
                            tmp.template_width, tmp.template_height, tile_count, total_cells
                        ),
                        texture: Some(texture),
                        error: None,
                        file_type,
                        shp: None,
                        palette,
                        palette_name,
                        raw_bytes: Some(data),
                        is_playing: false,
                        play_speed_fps: 15.0,
                        last_frame_time: 0.0,
                        house_color_index: 0,
                    };
                }
            }
        }
    }

    match ShpFile::from_bytes(&data) {
        Ok(shp) => {
            let (palette, palette_name) = find_palette_for_asset(
                asset_manager,
                dict,
                art_registry,
                Some(&resolved_name),
                palette_source_archive,
                palette_override,
            );
            let dimensions = format!("{}x{}, {} frames", shp.width, shp.height, shp.frames.len());
            let frame_count = shp.frames.len();
            let texture = palette
                .as_ref()
                .and_then(|pal| render_shp_frame_texture(ctx, &shp, 0, pal));

            let error = if palette.is_none() {
                Some("No palette found".to_string())
            } else if texture.is_none() {
                Some("Frame 0 render failed".to_string())
            } else {
                None
            };

            PreviewState {
                entry_hash,
                source_name: source_label.to_string(),
                resolved_name,
                current_frame: 0,
                frame_count,
                dimensions,
                texture,
                error,
                file_type,
                shp: Some(shp),
                palette,
                palette_name,
                raw_bytes: Some(data),
                is_playing: false,
                play_speed_fps: 15.0,
                last_frame_time: 0.0,
                house_color_index: 0,
            }
        }
        Err(_) => PreviewState {
            entry_hash,
            source_name: source_label.to_string(),
            resolved_name,
            current_frame: 0,
            frame_count: 0,
            dimensions: format!("{} bytes", data.len()),
            texture: None,
            error: None,
            file_type,
            shp: None,
            palette: None,
            palette_name: None,
            raw_bytes: Some(data),
            is_playing: false,
            play_speed_fps: 15.0,
            last_frame_time: 0.0,
            house_color_index: 0,
        },
    }
}

/// Update the preview to show a different frame of the current SHP.
pub fn set_preview_frame(preview: &mut PreviewState, frame: usize, ctx: &egui::Context) {
    let Some(shp) = &preview.shp else {
        return;
    };
    let Some(palette) = &preview.palette else {
        return;
    };
    if frame >= shp.frames.len() {
        return;
    }

    preview.current_frame = frame;
    preview.texture = render_shp_frame_texture(ctx, shp, frame, palette);
}

/// Search for an asset by filename across all archives, preview it if found.
pub fn search_asset(
    asset_manager: &AssetManager,
    dict: &[(String, i32)],
    art_registry: &ArtRegistry,
    name: &str,
    palette_name: &str,
    ctx: &egui::Context,
) -> (PreviewState, Option<String>) {
    let name_lower = name.trim().to_ascii_lowercase();
    if name_lower.is_empty() {
        return (
            PreviewState::error(0, "search", &name_lower, "Empty search".to_string()),
            None,
        );
    }

    let (data, source) = match asset_manager.get_with_source(&name_lower) {
        Some(pair) => pair,
        None => {
            return (
                PreviewState::error(
                    0,
                    &format!("search: {}", name_lower),
                    &name_lower,
                    format!("'{}' not found in any archive", name_lower),
                ),
                None,
            );
        }
    };

    let preview = preview_from_bytes(
        asset_manager,
        dict,
        art_registry,
        &format!("found in: {source}"),
        &source,
        0,
        data,
        Some(&name_lower),
        Some(palette_name),
        ctx,
    );

    (preview, Some(source))
}
