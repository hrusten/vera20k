//! Overlay sprite atlas — loads overlay/terrain SHP sprites into a packed GPU texture.
//!
//! At map load time, all overlay entries (from [OverlayPack]) and terrain objects
//! (from [Terrain]) are collected. Each unique (name, frame) combination has its
//! SHP frame rendered to RGBA and shelf-packed into a single GPU texture atlas.
//!
//! Follows the same pattern as sprite_atlas.rs and unit_atlas.rs.
//!
//! ## Dependency rules
//! - Part of render/ — depends on assets/ (SHP/Palette), render/batch (GPU upload).
//! - Reads overlay data from map/ (OverlayEntry, TerrainObject).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

use crate::assets::asset_manager::AssetManager;
use crate::assets::pal_file::Palette;
use crate::assets::shp_file::ShpFile;
use crate::map::overlay::{OverlayEntry, TerrainObject};
use crate::map::overlay_types::{
    OverlayTypeFlags, OverlayTypeRegistry, resolve_overlay_name_for_render,
};
use crate::render::batch::{BatchRenderer, BatchTexture};
use crate::render::gpu::GpuContext;
use crate::rules::art_data::{self, ArtRegistry};
use crate::rules::ini_parser::IniFile;

/// Maximum atlas texture width for overlay sprites (pixels).

/// Padding between sprites in the atlas to prevent texture bleeding.
const SPRITE_PADDING: u32 = 1;

/// Cache key: unique combination of overlay name and frame index.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OverlaySpriteKey {
    /// Overlay or terrain object name (e.g., "GEM01", "INTREE01").
    pub name: String,
    /// Frame/variant index.
    pub frame: u8,
}

/// UV and offset data for one overlay sprite within the atlas.
#[derive(Debug, Clone, Copy)]
pub struct OverlaySpriteEntry {
    /// Top-left UV coordinate in the atlas (0.0..1.0).
    pub uv_origin: [f32; 2],
    /// UV width and height (0.0..1.0).
    pub uv_size: [f32; 2],
    /// Sprite dimensions in pixels.
    pub pixel_size: [f32; 2],
    /// X offset from the cell center to the sprite's top-left corner.
    pub offset_x: f32,
    /// Y offset from the cell center to the sprite's top-left corner.
    pub offset_y: f32,
}

/// A GPU texture atlas containing pre-rendered overlay sprites.
pub struct OverlayAtlas {
    /// The packed GPU texture.
    pub texture: BatchTexture,
    /// Lookup: (name, frame) → UV rectangle + offset data.
    entries: HashMap<OverlaySpriteKey, OverlaySpriteEntry>,
    /// Terrain objects with animation: name → total frame count.
    /// Only populated for terrain objects whose SHP has more than 1 frame.
    terrain_anim_frames: HashMap<String, u8>,
}

impl OverlayAtlas {
    /// Look up the atlas entry for a given (name, frame) pair.
    pub fn get(&self, key: &OverlaySpriteKey) -> Option<&OverlaySpriteEntry> {
        self.entries.get(key)
    }

    /// Number of unique sprites in the atlas.
    pub fn sprite_count(&self) -> usize {
        self.entries.len()
    }

    /// Get the animation frame count for an animated terrain object.
    /// Returns None for non-animated terrain objects (single frame).
    pub fn terrain_anim_frame_count(&self, name: &str) -> Option<u8> {
        self.terrain_anim_frames.get(name).copied()
    }
}

/// Intermediate rendered sprite before atlas packing.
struct RenderedOverlay {
    key: OverlaySpriteKey,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    offset_x: f32,
    offset_y: f32,
}

/// Build an overlay sprite atlas from map overlay/terrain data.
///
/// Loads SHP sprites for each unique overlay type and packs into a GPU texture.
/// Returns None if no overlays can be rendered.
pub fn build_overlay_atlas(
    gpu: &GpuContext,
    batch: &BatchRenderer,
    overlays: &[OverlayEntry],
    terrain_objects: &[TerrainObject],
    asset_manager: &AssetManager,
    theater_palette: &Palette,
    unit_palette: &Palette,
    tiberium_palette: &Palette,
    theater_ext: &str,
    theater_name: &str,
    overlay_registry: &OverlayTypeRegistry,
    rules_ini: &IniFile,
    art_registry: &ArtRegistry,
) -> Option<OverlayAtlas> {
    // Collect unique (name, frame) pairs from overlays.
    let mut needed: HashSet<OverlaySpriteKey> = HashSet::new();

    for entry in overlays {
        if let Some(mapped_name) =
            resolve_overlay_name_for_render(overlay_registry, entry.overlay_id)
        {
            needed.insert(OverlaySpriteKey {
                name: mapped_name.clone(),
                frame: entry.frame,
            });
            // Also include frame 0 as fallback.
            needed.insert(OverlaySpriteKey {
                name: mapped_name,
                frame: 0,
            });
        }
    }

    // For ALL wall types in the registry, pre-load all 16 connectivity frames
    // so player-built walls can use any bitmask frame even if they weren't
    // present in the original map's OverlayPack.
    let mut wall_names_loaded: HashSet<String> = HashSet::new();
    for overlay_id in 0u8..=u8::MAX {
        let is_wall: bool = overlay_registry
            .flags(overlay_id)
            .map(|f| f.wall)
            .unwrap_or(false);
        if !is_wall {
            continue;
        }
        if let Some(mapped_name) = resolve_overlay_name_for_render(overlay_registry, overlay_id) {
            if wall_names_loaded.insert(mapped_name.clone()) {
                for frame in 0u8..16u8 {
                    needed.insert(OverlaySpriteKey {
                        name: mapped_name.clone(),
                        frame,
                    });
                }
            }
        }
    }
    if !wall_names_loaded.is_empty() {
        log::info!(
            "Pre-loaded 16 connectivity frames for {} wall type(s): {:?}",
            wall_names_loaded.len(),
            wall_names_loaded,
        );
    }

    // For terrain objects, probe SHP frame counts. Animated objects (flags, etc.)
    // need all frames loaded; static objects just need frame 0.
    let mut terrain_anim_frames: HashMap<String, u8> = HashMap::new();
    for obj in terrain_objects {
        if terrain_anim_frames.contains_key(&obj.name)
            || needed.contains(&OverlaySpriteKey {
                name: obj.name.clone(),
                frame: 0,
            })
        {
            // Already processed this terrain type.
            continue;
        }
        let frame_count = probe_terrain_shp_frame_count(
            asset_manager,
            &obj.name,
            theater_ext,
            theater_name,
            rules_ini,
            art_registry,
        );
        if frame_count > 1 {
            terrain_anim_frames.insert(obj.name.clone(), frame_count);
            for frame in 0..frame_count {
                needed.insert(OverlaySpriteKey {
                    name: obj.name.clone(),
                    frame,
                });
            }
        } else {
            needed.insert(OverlaySpriteKey {
                name: obj.name.clone(),
                frame: 0,
            });
        }
    }

    if needed.is_empty() {
        log::info!("No overlay/terrain sprites needed — skipping overlay atlas");
        return None;
    }

    log::info!(
        "Building overlay atlas for {} unique (name, frame) pairs",
        needed.len()
    );

    // Render each unique sprite.
    let mut rendered: Vec<RenderedOverlay> = Vec::with_capacity(needed.len());
    let mut load_fail_count: u32 = 0;

    for key in &needed {
        let flags: OverlayTypeFlags = overlay_registry
            .flags_by_name(&key.name)
            .cloned()
            .unwrap_or_default();
        // Terrain objects (e.g. TIBTRE01) aren't in OverlayTypeRegistry, so flags
        // will be default. Check rules.ini for SpawnsTiberium=yes to detect
        // tiberium trees — the original engine uses unit palette + -12px Y offset for these.
        let spawns_tiberium: bool = !flags.tiberium
            && rules_ini
                .section(&key.name)
                .and_then(|s| s.get_bool("SpawnsTiberium"))
                .unwrap_or(false);
        // Palette selection:
        // - Tiberium overlays → tiberium palette (e.g., temperat.pal), NO remap
        // - SpawnsTiberium terrain objects → unit palette
        // - Walls/veins/veinhole → unit palette
        // - Everything else → theater palette
        let palette: &Palette = if flags.tiberium {
            tiberium_palette
        } else if spawns_tiberium || flags.wall || flags.is_veins || flags.is_veinhole_monster {
            unit_palette
        } else {
            theater_palette
        };
        match render_overlay_sprite(
            asset_manager,
            palette,
            key,
            theater_ext,
            theater_name,
            rules_ini,
            art_registry,
            &flags,
            spawns_tiberium,
        ) {
            Some(sprite) => {
                rendered.push(sprite);
            }
            None => {
                load_fail_count += 1;
                // Only log at debug level — some overlay types (e.g., CYCL)
                // are unused RA1 remnants with no backing SHP file.
                let image_id: String = art_registry.resolve_overlay_image_id(&key.name, rules_ini);
                let candidates: Vec<String> = art_data::overlay_shp_candidates(
                    Some(art_registry),
                    &key.name,
                    &image_id,
                    theater_ext,
                    theater_name,
                );
                log::debug!(
                    "Overlay sprite not found: name={} frame={} (tried: {:?})",
                    key.name,
                    key.frame,
                    candidates,
                );
            }
        }
    }

    log::info!(
        "Overlay sprites: {} rendered, {} failed to load (of {} needed)",
        rendered.len(),
        load_fail_count,
        needed.len()
    );

    if rendered.is_empty() {
        return None;
    }

    if !terrain_anim_frames.is_empty() {
        log::info!(
            "Animated terrain objects: {} types ({:?})",
            terrain_anim_frames.len(),
            terrain_anim_frames,
        );
    }

    Some(pack_overlay_sprites(
        gpu,
        batch,
        &rendered,
        terrain_anim_frames,
    ))
}

/// Load and render a single overlay SHP sprite to RGBA pixels.
///
/// Uses explicit overlay image resolution first, then original-style filename
/// conventions. Repo-only numeric-suffix fallback remains local to this module.
fn render_overlay_sprite(
    asset_manager: &AssetManager,
    palette: &Palette,
    key: &OverlaySpriteKey,
    theater_ext: &str,
    theater_name: &str,
    rules_ini: &IniFile,
    art_registry: &ArtRegistry,
    flags: &OverlayTypeFlags,
    spawns_tiberium: bool,
) -> Option<RenderedOverlay> {
    let image_id: String = art_registry.resolve_overlay_image_id(&key.name, rules_ini);
    let mut candidates: Vec<String> = art_data::overlay_shp_candidates(
        Some(art_registry),
        &key.name,
        &image_id,
        theater_ext,
        theater_name,
    );
    // Debug override: force all tiberium overlays to render using one chosen image
    // (e.g. TIB01/TIB02/TIB03) to quickly validate sprite selection issues.
    if flags.tiberium {
        if let Some(forced_name) = forced_tiberium_image_name() {
            candidates.insert(
                0,
                format!("{}.{}", forced_name.to_ascii_lowercase(), theater_ext),
            );
            candidates.insert(1, format!("{}.shp", forced_name.to_ascii_lowercase()));
            candidates.insert(2, format!("{}.{}", forced_name, theater_ext));
            candidates.insert(3, format!("{}.shp", forced_name));
        }
    }

    if let Some(alias) = decrement_numeric_suffix(&key.name) {
        candidates.push(format!("{}.{}", alias, theater_ext));
        candidates.push(format!("{}.shp", alias));
        candidates.push(format!("{}.{}", alias.to_ascii_lowercase(), theater_ext));
        candidates.push(format!("{}.shp", alias.to_ascii_lowercase()));
    }

    // Tiberium overlays now use the dedicated tiberium palette (e.g., temperat.pal)
    // which already has correct ore colors at all indices — no remap needed.
    // Walls use the unit palette with default remap range colors.

    let mut found_name: String = String::new();
    let mut shp_opt: Option<ShpFile> = None;
    for name in &candidates {
        let Some(data) = asset_manager.get_ref(name) else {
            continue;
        };
        let Ok(shp) = ShpFile::from_bytes(data) else {
            continue;
        };
        // Skip template files with no drawable frames (e.g. some bridge stubs).
        let has_drawable = shp
            .frames
            .iter()
            .any(|fr| fr.frame_width > 0 && fr.frame_height > 0);
        if !has_drawable {
            continue;
        }
        found_name = name.clone();
        shp_opt = Some(shp);
        break;
    }
    let shp: ShpFile = shp_opt?;
    log::trace!("Overlay sprite {} uses {}", key.name, found_name);

    if shp.frames.is_empty() {
        return None;
    }

    // Bridge SHPs contain shadow frames in the second half (e.g., frames
    // 18-35 of a 36-frame file). Cap to the normal (non-shadow) range so
    // we never accidentally render a shadow blob as the bridge surface.
    let max_normal_frame: usize = if flags.bridge_deck {
        shp.frames.len() / 2
    } else {
        shp.frames.len()
    };

    // Select frame:
    // High bridge overlays (BRIDGE1/2, BRIDGEB1/2) share one SHP file
    // (bridge.tem / bridgb.tem). The OverlayDataPack frame value already
    // encodes the direction: frames 0-8 = EW, frames 9-17 = NS.
    // No additional offset is needed — the map data handles it.
    // 1) requested frame if in range and non-empty
    // 2) frame 0 if non-empty
    // 3) first non-empty frame
    let requested_idx: usize = key.frame as usize;
    let mut frame_idx: usize = if requested_idx < max_normal_frame {
        requested_idx
    } else {
        0
    };
    let is_non_empty = |idx: usize| -> bool {
        shp.frames
            .get(idx)
            .map(|fr| fr.frame_width > 0 && fr.frame_height > 0)
            .unwrap_or(false)
    };
    if !is_non_empty(frame_idx) {
        if is_non_empty(0) {
            frame_idx = 0;
        } else if let Some((idx, _)) = shp
            .frames
            .iter()
            .enumerate()
            .find(|(_, fr)| fr.frame_width > 0 && fr.frame_height > 0)
        {
            frame_idx = idx;
        } else {
            return None;
        }
    }

    let frame = &shp.frames[frame_idx];

    let frame_rgba: Vec<u8> = match shp.frame_to_rgba(frame_idx, palette) {
        Ok(rgba) => rgba,
        Err(_) => return None,
    };

    // Blit sub-frame into full SHP bounds for consistent dimensions.
    let full_w: u32 = shp.width as u32;
    let full_h: u32 = shp.height as u32;
    let mut full_rgba: Vec<u8> = vec![0u8; (full_w * full_h * 4) as usize];

    let fw: u32 = frame.frame_width as u32;
    let fh: u32 = frame.frame_height as u32;
    let fx: u32 = frame.frame_x as u32;
    let fy: u32 = frame.frame_y as u32;

    for y in 0..fh {
        let dst_y: u32 = fy + y;
        if dst_y >= full_h {
            break;
        }
        let src_off: usize = (y * fw * 4) as usize;
        let copy_w: u32 = fw.min(full_w.saturating_sub(fx));
        let dst_off: usize = ((dst_y * full_w + fx) * 4) as usize;
        let bytes: usize = (copy_w * 4) as usize;
        if src_off + bytes <= frame_rgba.len() && dst_off + bytes <= full_rgba.len() {
            full_rgba[dst_off..dst_off + bytes]
                .copy_from_slice(&frame_rgba[src_off..src_off + bytes]);
        }
    }

    // Center the overlay sprite on the cell center.
    // The original engine applies a -CellHeight Y offset for Tiberium, Walls, Veins, Crates, and
    // SpawnsTiberium terrain objects (e.g. TIBTRE01). RA2 CellHeight = 15px.
    let y_offset: f32 = if spawns_tiberium {
        -15.0
    } else {
        flags.y_draw_offset()
    };
    let offset_x: f32 = -(full_w as f32) / 2.0;
    let offset_y: f32 = -(full_h as f32) / 2.0 + y_offset;

    Some(RenderedOverlay {
        key: key.clone(),
        rgba: full_rgba,
        width: full_w,
        height: full_h,
        offset_x,
        offset_y,
    })
}

/// If a name ends in digits, return a variant with that numeric suffix decremented.
/// Example: "LOBRDG27" -> "LOBRDG26", "FENCE21" -> "FENCE20".
fn decrement_numeric_suffix(name: &str) -> Option<String> {
    let split: usize = name.rfind(|c: char| !c.is_ascii_digit())?;
    if split + 1 >= name.len() {
        return None;
    }
    let (prefix, digits) = name.split_at(split + 1);
    let width: usize = digits.len();
    let n: u32 = digits.parse().ok()?;
    if n == 0 {
        return None;
    }
    Some(format!("{}{:0width$}", prefix, n - 1, width = width))
}

fn forced_tiberium_image_name() -> Option<&'static str> {
    static FORCED: OnceLock<Option<String>> = OnceLock::new();
    FORCED
        .get_or_init(|| {
            std::env::var("RA2_FORCE_TIB_IMAGE")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        })
        .as_deref()
}

/// Compute average color from an RGBA pixel buffer, ignoring fully transparent pixels.
fn average_frame_color(rgba: &[u8]) -> [u8; 3] {
    let (mut r_sum, mut g_sum, mut b_sum, mut count) = (0u32, 0u32, 0u32, 0u32);
    for pixel in rgba.chunks_exact(4) {
        if pixel[3] > 0 {
            r_sum += pixel[0] as u32;
            g_sum += pixel[1] as u32;
            b_sum += pixel[2] as u32;
            count += 1;
        }
    }
    if count == 0 {
        return [0, 0, 0];
    }
    [
        (r_sum / count) as u8,
        (g_sum / count) as u8,
        (b_sum / count) as u8,
    ]
}

/// Compute average radar color for each tiberium overlay (id, frame) pair.
/// Renders each tiberium SHP frame with the tiberium palette and averages
/// the non-transparent pixels to get the representative radar color.
pub fn compute_tiberium_radar_colors(
    asset_manager: &AssetManager,
    tib_palette: &Palette,
    overlay_registry: &OverlayTypeRegistry,
    overlay_entries: &[OverlayEntry],
    overlay_names: &BTreeMap<u8, String>,
    theater_ext: &str,
    rules_ini: &IniFile,
    art_registry: &ArtRegistry,
) -> HashMap<(u8, u8), [u8; 3]> {
    // Collect unique (overlay_id, frame) pairs for tiberium overlays.
    let mut needed: HashSet<(u8, u8)> = HashSet::new();
    for entry in overlay_entries {
        let is_tiberium: bool = overlay_registry
            .flags(entry.overlay_id)
            .map(|f| f.tiberium)
            .unwrap_or(false);
        if is_tiberium {
            needed.insert((entry.overlay_id, entry.frame));
        }
    }

    if needed.is_empty() {
        return HashMap::new();
    }

    // Group by overlay_id so we load each SHP only once.
    let mut ids: HashSet<u8> = HashSet::new();
    for &(id, _) in &needed {
        ids.insert(id);
    }

    // Cache: overlay_id -> loaded ShpFile
    let mut shp_cache: HashMap<u8, ShpFile> = HashMap::new();
    for &overlay_id in &ids {
        let name: &str = match overlay_names.get(&overlay_id) {
            Some(n) => n.as_str(),
            None => continue,
        };
        let image_id: String = art_registry.resolve_overlay_image_id(name, rules_ini);
        let candidates: Vec<String> = art_data::overlay_shp_candidates(
            Some(art_registry),
            name,
            &image_id,
            theater_ext,
            "", // theater_name not needed for basic candidate generation
        );

        for candidate in &candidates {
            let Some(data) = asset_manager.get_ref(candidate) else {
                continue;
            };
            let Ok(shp) = ShpFile::from_bytes(data) else {
                continue;
            };
            shp_cache.insert(overlay_id, shp);
            break;
        }
    }

    let mut result: HashMap<(u8, u8), [u8; 3]> = HashMap::with_capacity(needed.len());
    for &(overlay_id, frame) in &needed {
        let Some(shp) = shp_cache.get(&overlay_id) else {
            continue;
        };
        let frame_idx: usize = frame as usize;
        if frame_idx >= shp.frames.len() {
            continue;
        }
        let rgba: Vec<u8> = match shp.frame_to_rgba(frame_idx, tib_palette) {
            Ok(data) => data,
            Err(_) => continue,
        };
        let color: [u8; 3] = average_frame_color(&rgba);
        // Skip pure black — likely empty/failed frames.
        if color != [0, 0, 0] {
            result.insert((overlay_id, frame), color);
        }
    }

    log::info!(
        "Computed tiberium radar colors: {} entries from {} overlay IDs",
        result.len(),
        ids.len(),
    );

    result
}

/// Probe the SHP frame count for a terrain object.
///
/// Loads the SHP file and returns the number of non-empty frames.
/// Returns 1 if the SHP cannot be loaded or has only one frame.
fn probe_terrain_shp_frame_count(
    asset_manager: &AssetManager,
    name: &str,
    theater_ext: &str,
    theater_name: &str,
    rules_ini: &IniFile,
    art_registry: &ArtRegistry,
) -> u8 {
    let image_id: String = art_registry.resolve_overlay_image_id(name, rules_ini);
    let candidates: Vec<String> = art_data::overlay_shp_candidates(
        Some(art_registry),
        name,
        &image_id,
        theater_ext,
        theater_name,
    );
    for candidate in &candidates {
        let Some(data) = asset_manager.get_ref(candidate) else {
            continue;
        };
        let Ok(shp) = ShpFile::from_bytes(data) else {
            continue;
        };
        // Terrain SHPs store shadow frames in the second half (same layout
        // as buildings/bridges). Only the first half are normal image frames.
        let normal = (shp.frames.len() / 2).max(1).min(255) as u8;
        return normal;
    }
    1
}

#[cfg(test)]
mod tests {
    use super::decrement_numeric_suffix;

    #[test]
    fn test_decrement_numeric_suffix_is_local_fallback() {
        assert_eq!(
            decrement_numeric_suffix("LOBRDG27"),
            Some("LOBRDG26".to_string())
        );
        assert_eq!(decrement_numeric_suffix("FENCE00"), None);
        assert_eq!(decrement_numeric_suffix("BRIDGE"), None);
    }
}

/// Shelf-pack rendered overlay sprites into a GPU texture atlas.
fn pack_overlay_sprites(
    gpu: &GpuContext,
    batch: &BatchRenderer,
    sprites: &[RenderedOverlay],
    terrain_anim_frames: HashMap<String, u8>,
) -> OverlayAtlas {
    // Sort by height descending for shelf packing efficiency.
    let mut indices: Vec<usize> = (0..sprites.len()).collect();
    indices.sort_by(|&a, &b| sprites[b].height.cmp(&sprites[a].height));

    let total_area: u64 = sprites
        .iter()
        .map(|s| {
            (s.width as u64 + SPRITE_PADDING as u64) * (s.height as u64 + SPRITE_PADDING as u64)
        })
        .sum();
    let estimated_side: u32 = (total_area as f64).sqrt().ceil() as u32;
    let max_texture_dim: u32 = gpu.device.limits().max_texture_dimension_2d;
    let mut atlas_width: u32 = estimated_side.clamp(64, max_texture_dim);

    // Shelf-pack with retry: widen atlas if height exceeds GPU texture limit.
    let placements: Vec<(usize, u32, u32)>;
    let atlas_height: u32;
    loop {
        let mut trial: Vec<(usize, u32, u32)> = Vec::with_capacity(sprites.len());
        let mut cx: u32 = 0;
        let mut cy: u32 = 0;
        let mut shelf_h: u32 = 0;
        for &idx in &indices {
            let w: u32 = sprites[idx].width;
            let h: u32 = sprites[idx].height;
            if cx + w > atlas_width {
                cy += shelf_h + SPRITE_PADDING;
                cx = 0;
                shelf_h = 0;
            }
            trial.push((idx, cx, cy));
            cx += w + SPRITE_PADDING;
            shelf_h = shelf_h.max(h);
        }
        let trial_height: u32 = trial
            .iter()
            .map(|&(idx, _, py)| py + sprites[idx].height)
            .max()
            .unwrap_or(1);
        if trial_height <= max_texture_dim {
            placements = trial;
            atlas_height = trial_height;
            break;
        }
        if atlas_width >= max_texture_dim {
            log::warn!(
                "Overlay atlas height {} exceeds GPU limit {} at max width",
                trial_height,
                max_texture_dim
            );
            placements = trial;
            atlas_height = trial_height.min(max_texture_dim);
            break;
        }
        atlas_width = (atlas_width.saturating_mul(2)).min(max_texture_dim);
    }

    let mut rgba: Vec<u8> = vec![0u8; (atlas_width * atlas_height * 4) as usize];
    let mut entries: HashMap<OverlaySpriteKey, OverlaySpriteEntry> =
        HashMap::with_capacity(placements.len());
    let aw: f32 = atlas_width as f32;
    let ah: f32 = atlas_height as f32;

    for &(idx, px, py) in &placements {
        let spr: &RenderedOverlay = &sprites[idx];
        let w: u32 = spr.width;
        let h: u32 = spr.height;

        for y in 0..h {
            let src_start: usize = (y * w * 4) as usize;
            let src_end: usize = src_start + (w * 4) as usize;
            let dst_start: usize = (((py + y) * atlas_width + px) * 4) as usize;
            let dst_end: usize = dst_start + (w * 4) as usize;
            if src_end <= spr.rgba.len() && dst_end <= rgba.len() {
                rgba[dst_start..dst_end].copy_from_slice(&spr.rgba[src_start..src_end]);
            }
        }

        entries.insert(
            spr.key.clone(),
            OverlaySpriteEntry {
                uv_origin: [px as f32 / aw, py as f32 / ah],
                uv_size: [w as f32 / aw, h as f32 / ah],
                pixel_size: [w as f32, h as f32],
                offset_x: spr.offset_x,
                offset_y: spr.offset_y,
            },
        );
    }

    log::info!(
        "Overlay atlas: {}x{} px ({:.1} MB), {} sprites",
        atlas_width,
        atlas_height,
        (atlas_width as u64 * atlas_height as u64 * 4) as f64 / (1024.0 * 1024.0),
        entries.len()
    );

    let texture: BatchTexture = batch.create_texture(gpu, &rgba, atlas_width, atlas_height);
    OverlayAtlas {
        texture,
        entries,
        terrain_anim_frames,
    }
}
