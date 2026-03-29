//! Unit sprite atlas — pre-renders voxel models into a packed GPU texture.
//!
//! At map load time, all VXL-rendered entities are identified by (type_id, facing).
//! Each unique combination is rendered once via the software rasterizer, then all
//! resulting sprites are shelf-packed into a single GPU texture atlas. During the
//! render loop, unit SpriteInstances reference UV regions within this atlas.
//!
//! This matches the proven TileAtlas approach: one texture, one draw call.
//!
//! ## Dependency rules
//! - Part of render/ — depends on assets/ (VXL/HVA/Palette), render/batch (GPU upload),
//!   render/vxl_raster (software rendering).
//! - Reads from sim/ via EntityStore iteration (GameEntity fields).

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::assets::asset_manager::AssetManager;
use crate::assets::hva_file::HvaFile;
use crate::assets::pal_file::Palette;
use crate::assets::vpl_file::VplFile;
use crate::assets::vxl_file::VxlFile;
use crate::map::houses::HouseColorMap;
use crate::render::batch::{BatchRenderer, BatchTexture};
use crate::render::gpu::GpuContext;
use crate::render::vxl_compute::VxlComputeRenderer;
use crate::render::vxl_raster::{self, VxlRenderParams, VxlSprite};
use crate::rules::art_data::{self, ArtRegistry};
use crate::rules::house_colors::{self, HouseColorIndex};
use crate::rules::ruleset::RuleSet;

/// Maximum atlas texture width for unit sprites (pixels).

/// Padding between sprites in the atlas to prevent texture bleeding.
const SPRITE_PADDING: u32 = 1;
/// Body/composite facing quantization step: 4 = 64 buckets (5.6° per bucket).
/// Doubled from 32 to 64 to reduce visible rotation snapping. The original engine
/// uses 32 facing directions with sub-tick interpolation; 64 atlas buckets
/// approximates that smoothness without runtime re-rendering.
const UNIT_FACING_STEP: u8 = 4;
/// Number of pre-rendered facing directions for body/composite sprites.
const UNIT_FACING_BUCKETS: u8 = 64;
/// Turret/barrel facing quantization step: 2 = 128 buckets (2.8° per bucket).
/// Doubled from 64→128 to match body smoothness improvement. Turrets rotate
/// frequently during combat, so finer resolution prevents visible stepping.
const TURRET_FACING_STEP: u8 = 2;
/// Number of pre-rendered facing directions for turret/barrel sprites.
const TURRET_FACING_BUCKETS: u8 = 128;

// VxlLayer lives in sim::components — re-exported here for convenience.
pub use crate::sim::components::VxlLayer;

/// Cache key: unique combination of object type, facing, house color, layer, frame, and slope.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnitSpriteKey {
    /// Object type ID from rules.ini (e.g., "HTNK").
    pub type_id: String,
    /// Facing direction (0–255).
    pub facing: u8,
    /// House color index for palette remapping (different per player).
    pub house_color: HouseColorIndex,
    /// Which VXL layer this entry represents.
    pub layer: VxlLayer,
    /// HVA animation frame index. 0 for most units; >0 for multi-frame animations.
    pub frame: u32,
    /// Terrain slope type (0–8). 0 = flat, 1-4 = edge ramps, 5-8 = corner ramps.
    /// Different slopes produce distinct pre-rendered sprites with tilted models.
    pub slope_type: u8,
}

/// UV and offset data for one sprite within the unit atlas.
#[derive(Debug, Clone, Copy)]
pub struct UnitSpriteEntry {
    /// Top-left UV coordinate in the atlas (0.0..1.0).
    pub uv_origin: [f32; 2],
    /// UV width and height (0.0..1.0).
    pub uv_size: [f32; 2],
    /// Sprite dimensions in pixels.
    pub pixel_size: [f32; 2],
    /// X offset from the model's center to the sprite's top-left corner.
    /// Used to position the sprite so the unit appears centered on its cell.
    pub offset_x: f32,
    /// Y offset from the model's center to the sprite's top-left corner.
    pub offset_y: f32,
}

/// A GPU texture atlas containing pre-rendered unit voxel sprites.
///
/// Created once at map load. Queried per-frame to build unit SpriteInstances.
pub struct UnitAtlas {
    /// The packed GPU texture containing all unit sprites.
    pub texture: BatchTexture,
    /// Lookup: (type_id, facing, frame) → UV rectangle + offset data.
    entries: HashMap<UnitSpriteKey, UnitSpriteEntry>,
    /// HVA frame counts per (type_id, layer). Missing entries have 1 frame.
    /// Used at spawn time to initialize VoxelAnimation components.
    pub frame_counts: BTreeMap<(String, VxlLayer), u32>,
    /// Cached rendered sprites for incremental rebuild. On subsequent rebuilds,
    /// only genuinely new sprite keys are rendered; cached sprites are reused
    /// and everything is repacked.
    rendered_cache: Vec<CachedUnitSprite>,
    /// How many sprites were rendered via GPU compute in the last build.
    pub gpu_rendered: u32,
    /// How many sprites were rendered via CPU rasterizer in the last build.
    pub cpu_rendered: u32,
}

impl UnitAtlas {
    /// Look up the atlas entry for a given key.
    pub fn get(&self, key: &UnitSpriteKey) -> Option<&UnitSpriteEntry> {
        self.entries.get(key)
    }

    /// Number of unique sprites in the atlas.
    pub fn sprite_count(&self) -> usize {
        self.entries.len()
    }

    /// Get the HVA frame count for a (type_id, layer) pair. Returns 1 if unknown.
    pub fn frame_count_for(&self, type_id: &str, layer: VxlLayer) -> u32 {
        self.frame_counts
            .get(&(type_id.to_string(), layer))
            .copied()
            .unwrap_or(1)
    }

    /// Check whether the atlas already contains all sprite keys needed by the
    /// current ECS world. Returns true if no rebuild is necessary.
    pub fn has_all_keys(&self, needed: &HashSet<UnitSpriteKey>) -> bool {
        needed.iter().all(|k| self.entries.contains_key(k))
    }
}

/// Intermediate rendered sprite before atlas packing (temporary, during build).
struct RenderedSprite {
    key: UnitSpriteKey,
    sprite: VxlSprite,
}

/// Cached rendered unit sprite — RGBA only, depth buffer stripped.
/// Depth is only used during VXL compositing (body+turret+barrel merge),
/// not after packing. Stripping it saves ~40% cache memory per sprite.
struct CachedUnitSprite {
    key: UnitSpriteKey,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    offset_x: f32,
    offset_y: f32,
}

impl CachedUnitSprite {
    fn from_rendered(rs: RenderedSprite) -> Self {
        Self {
            key: rs.key,
            rgba: rs.sprite.rgba,
            width: rs.sprite.width,
            height: rs.sprite.height,
            offset_x: rs.sprite.offset_x,
            offset_y: rs.sprite.offset_y,
        }
    }
}

/// Collect the set of unit sprite keys needed by the current ECS world.
///
/// Used by the incremental rebuild path to diff against the existing atlas.
/// Ground vehicles get all 9 slope variants (0-8) pre-rendered so that no
/// atlas rebuild is needed when they drive onto ramps.
pub fn collect_needed_unit_keys(
    entities: &crate::sim::entity_store::EntityStore,
    asset_manager: &AssetManager,
    rules: Option<&RuleSet>,
    art: Option<&ArtRegistry>,
    house_colors: &HouseColorMap,
    interner: Option<&crate::sim::intern::StringInterner>,
) -> HashSet<UnitSpriteKey> {
    use crate::map::entities::EntityCategory;
    let mut needed: HashSet<UnitSpriteKey> = HashSet::new();
    let mut frame_counts: BTreeMap<(String, VxlLayer), u32> = BTreeMap::new();
    for entity in entities.values() {
        if !entity.is_voxel {
            continue;
        }
        let owner_str = interner.map_or("", |i| i.resolve(entity.owner));
        let type_str = interner.map_or("", |i| i.resolve(entity.type_ref));
        let color_idx: HouseColorIndex =
            house_colors.get(owner_str).copied().unwrap_or_default();
        // Ground vehicles can drive onto any ramp, so pre-render all 9 slope
        // variants (0=flat, 1-8=ramps) upfront. Aircraft never tilt on slopes.
        let is_ground_vehicle: bool = entity.category != EntityCategory::Aircraft;
        let has_turret: bool = rules
            .and_then(|r| r.object(type_str))
            .map(|o| o.has_turret)
            .unwrap_or(false);
        let layers: &[VxlLayer] = if has_turret {
            &[VxlLayer::Body, VxlLayer::Turret, VxlLayer::Barrel]
        } else {
            &[VxlLayer::Composite]
        };
        for &layer in layers {
            let fc_key: (String, VxlLayer) = (type_str.to_string(), layer);
            if !frame_counts.contains_key(&fc_key) {
                let fc: u32 =
                    detect_hva_frame_count(asset_manager, type_str, layer, rules, art);
                frame_counts.insert(fc_key.clone(), fc);
            }
            let num_frames: u32 = frame_counts[&fc_key];
            let (step, buckets) = facing_config_for_layer(layer);
            // Ground vehicles: generate all 9 slope variants (0-8) so no
            // atlas rebuild is needed when driving onto ramps.
            // Aircraft: only slope_type=0 (flat).
            let slope_range: std::ops::RangeInclusive<u8> = if is_ground_vehicle {
                0..=8
            } else {
                0..=0
            };
            for bucket in 0..buckets {
                let facing: u8 = bucket.saturating_mul(step);
                for frame in 0..num_frames {
                    for slope in slope_range.clone() {
                        needed.insert(UnitSpriteKey {
                            type_id: type_str.to_string(),
                            facing,
                            house_color: color_idx,
                            layer,
                            frame,
                            slope_type: slope,
                        });
                    }
                }
            }
        }
    }

    // Step 1b: Building turret VXLs — non-voxel buildings with TurretAnimIsVoxel=true.
    // Buildings don't tilt on slopes, so slope_type is always 0.
    {
        for entity in entities.values() {
            if entity.is_voxel || entity.category != EntityCategory::Structure {
                continue;
            }
            let btype_str = interner.map_or("", |i| i.resolve(entity.type_ref));
            let bowner_str = interner.map_or("", |i| i.resolve(entity.owner));
            let obj = match rules.and_then(|r| r.object(btype_str)) {
                Some(o) => o,
                None => continue,
            };
            if !obj.turret_anim_is_voxel {
                continue;
            }
            let turret_id = match &obj.turret_anim {
                Some(id) => id,
                None => continue,
            };
            let hc: HouseColorIndex =
                house_colors.get(bowner_str).copied().unwrap_or_default();
            for bucket in 0..TURRET_FACING_BUCKETS {
                let facing: u8 = bucket.saturating_mul(TURRET_FACING_STEP);
                needed.insert(UnitSpriteKey {
                    type_id: turret_id.clone(),
                    facing,
                    house_color: hc,
                    layer: VxlLayer::Composite,
                    frame: 0,
                    slope_type: 0,
                });
            }
        }
    }

    needed
}

/// Build a unit sprite atlas from all VoxelModel entities in the ECS world.
///
/// Uses incremental rendering: if `existing` is provided, its cached rendered
/// sprites are reused and only genuinely new keys are rendered. This avoids
/// the expensive VXL software rasterization for sprites already in the atlas.
///
/// 1. Queries the world for all (TypeRef, Facing, VoxelModel) entities.
/// 2. Collects unique (type_id, facing) pairs.
/// 3. Diffs against cached sprites — renders only new keys.
/// 4. Shelf-packs all sprites (cached + new) into a single atlas texture.
///
/// Returns None if no voxel entities exist or all fail to load.
pub fn build_unit_atlas(
    gpu: &GpuContext,
    batch: &BatchRenderer,
    entities: &crate::sim::entity_store::EntityStore,
    asset_manager: &AssetManager,
    palette: &Palette,
    rules: Option<&RuleSet>,
    art: Option<&ArtRegistry>,
    house_colors: &HouseColorMap,
    existing: Option<UnitAtlas>,
    mut compute: Option<&mut VxlComputeRenderer>,
    interner: Option<&crate::sim::intern::StringInterner>,
) -> Option<UnitAtlas> {
    use crate::map::entities::EntityCategory;
    // Step 1: Collect unique (type_id, facing, house_color, layer, frame, slope_type) keys.
    // For turret units, insert separate Body/Turret/Barrel entries per facing.
    // For non-turret units, insert a single Composite entry per facing.
    // Multi-frame HVA units get entries for each frame.
    let mut needed: HashSet<UnitSpriteKey> = HashSet::new();
    let mut frame_counts: BTreeMap<(String, VxlLayer), u32> = BTreeMap::new();
    for entity in entities.values() {
        if !entity.is_voxel {
            continue;
        }
        let owner_str = interner.map_or("", |i| i.resolve(entity.owner));
        let type_str = interner.map_or("", |i| i.resolve(entity.type_ref));
        let color_idx: HouseColorIndex =
            house_colors.get(owner_str).copied().unwrap_or_default();
        let is_ground_vehicle: bool = entity.category != EntityCategory::Aircraft;
        let has_turret: bool = rules
            .and_then(|r| r.object(type_str))
            .map(|o| o.has_turret)
            .unwrap_or(false);
        let layers: &[VxlLayer] = if has_turret {
            &[VxlLayer::Body, VxlLayer::Turret, VxlLayer::Barrel]
        } else {
            &[VxlLayer::Composite]
        };
        for &layer in layers {
            let fc_key: (String, VxlLayer) = (type_str.to_string(), layer);
            if !frame_counts.contains_key(&fc_key) {
                let fc: u32 =
                    detect_hva_frame_count(asset_manager, type_str, layer, rules, art);
                if fc > 1 {
                    log::info!(
                        "VXL {}/{:?} has {} HVA frames — rendering all",
                        type_str,
                        layer,
                        fc,
                    );
                }
                frame_counts.insert(fc_key.clone(), fc);
            }
            let num_frames: u32 = frame_counts[&fc_key];
            let (step, buckets) = facing_config_for_layer(layer);
            let slope_range: std::ops::RangeInclusive<u8> = if is_ground_vehicle {
                0..=8
            } else {
                0..=0
            };
            for bucket in 0..buckets {
                let facing: u8 = bucket.saturating_mul(step);
                for frame in 0..num_frames {
                    for slope in slope_range.clone() {
                        needed.insert(UnitSpriteKey {
                            type_id: type_str.to_string(),
                            facing,
                            house_color: color_idx,
                            layer,
                            frame,
                            slope_type: slope,
                        });
                    }
                }
            }
        }
    }

    // Step 1b: Building turret VXLs — non-voxel buildings with TurretAnimIsVoxel=true.
    // These are separate VXL models (e.g., SAM.VXL for NASAM) drawn on top of SHP buildings.
    {
        for entity in entities.values() {
            if entity.is_voxel || entity.category != EntityCategory::Structure {
                continue;
            }
            let btype_str = interner.map_or("", |i| i.resolve(entity.type_ref));
            let bowner_str = interner.map_or("", |i| i.resolve(entity.owner));
            let obj = match rules.and_then(|r| r.object(btype_str)) {
                Some(o) => o,
                None => continue,
            };
            if !obj.turret_anim_is_voxel {
                continue;
            }
            let turret_id = match &obj.turret_anim {
                Some(id) => id,
                None => continue,
            };
            let hc: HouseColorIndex =
                house_colors.get(bowner_str).copied().unwrap_or_default();
            for bucket in 0..TURRET_FACING_BUCKETS {
                let facing: u8 = bucket.saturating_mul(TURRET_FACING_STEP);
                needed.insert(UnitSpriteKey {
                    type_id: turret_id.clone(),
                    facing,
                    house_color: hc,
                    layer: VxlLayer::Composite,
                    frame: 0,
                    slope_type: 0,
                });
            }
        }
    }

    if needed.is_empty() {
        log::info!("No voxel entities found — skipping unit atlas");
        return None;
    }

    // Step 1.5: Extract cached sprites from existing atlas, diff against needed keys.
    let mut cached: Vec<CachedUnitSprite> = existing
        .map(|atlas| atlas.rendered_cache)
        .unwrap_or_default();
    let cached_keys: HashSet<UnitSpriteKey> = cached.iter().map(|s| s.key.clone()).collect();
    let new_keys: Vec<UnitSpriteKey> = needed
        .iter()
        .filter(|k| !cached_keys.contains(k))
        .cloned()
        .collect();

    log::info!(
        "Unit atlas: {} cached, {} new to render, {} total needed",
        cached.len(),
        new_keys.len(),
        needed.len(),
    );

    // Step 2: Render only new sprites (skip cached ones).
    let mut gpu_rendered: u32 = 0;
    let mut cpu_rendered: u32 = 0;
    if !new_keys.is_empty() {
        // Load VPL file for Blinn-Phong lighting lookup (optional).
        let vpl: Option<VplFile> =
            asset_manager
                .get("VOXELS.VPL")
                .and_then(|data| match VplFile::from_bytes(&data) {
                    Ok(v) => {
                        log::info!("Loaded VOXELS.VPL ({} lighting sections)", v.num_sections);
                        Some(v)
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to parse VOXELS.VPL: {} — using fallback N·L shading",
                            e
                        );
                        None
                    }
                });

        // Upload VPL and palette to GPU compute renderer if available.
        if let Some(ref mut comp) = compute {
            if let Some(ref vpl_file) = vpl {
                comp.upload_vpl(&gpu.device, &gpu.queue, vpl_file);
            }
            comp.upload_palette(&gpu.device, &gpu.queue, palette);
        }

        for key in &new_keys {
            match render_unit_sprite(
                asset_manager,
                palette,
                key,
                rules,
                art,
                vpl.as_ref(),
                compute.as_deref_mut(),
                gpu,
            ) {
                Some((sprite, used_gpu)) => {
                    if used_gpu {
                        gpu_rendered += 1;
                    } else {
                        cpu_rendered += 1;
                    }
                    cached.push(CachedUnitSprite::from_rendered(RenderedSprite {
                        key: key.clone(),
                        sprite,
                    }));
                }
                None => {
                    log::warn!("Failed to render VXL for {}", key.type_id);
                }
            }
        }
        if gpu_rendered > 0 || cpu_rendered > 0 {
            log::info!(
                "VXL render: {} GPU compute, {} CPU rasterizer",
                gpu_rendered,
                cpu_rendered,
            );
        }
    }

    if cached.is_empty() {
        log::warn!("No unit sprites rendered — unit atlas will be empty");
        return None;
    }

    // Step 3: Shelf-pack all sprites (cached + newly rendered) into atlas.
    let mut atlas: UnitAtlas = pack_sprites(gpu, batch, &cached, frame_counts);
    atlas.rendered_cache = cached;
    atlas.gpu_rendered = gpu_rendered;
    atlas.cpu_rendered = cpu_rendered;
    log::info!(
        "Unit atlas built: {} sprites, {}x{} px",
        atlas.sprite_count(),
        atlas.texture.width,
        atlas.texture.height,
    );
    Some(atlas)
}

/// Load and render a single VXL model to a 2D sprite.
///
/// Uses ArtRegistry to resolve the correct VXL/HVA filenames.
/// Falls back to direct {TYPE_ID}.VXL if art data is unavailable.
fn render_unit_sprite(
    asset_manager: &AssetManager,
    palette: &Palette,
    key: &UnitSpriteKey,
    rules: Option<&RuleSet>,
    art: Option<&ArtRegistry>,
    vpl: Option<&VplFile>,
    mut compute: Option<&mut VxlComputeRenderer>,
    gpu: &GpuContext,
) -> Option<(VxlSprite, bool)> {
    // Resolve image name: type_id → rules.ini Image= → art.ini Image= override.
    let rules_image: String = rules
        .and_then(|r| r.object(&key.type_id))
        .map(|o| o.image.clone())
        .unwrap_or_else(|| key.type_id.clone());
    let image: String = art
        .map(|a| a.resolve_effective_image_id(&key.type_id, &rules_image))
        .unwrap_or_else(|| rules_image.to_uppercase());

    let (vxl_name, hva_name): (String, String) = art_data::voxel_asset_names(&image);

    let vxl_data: Vec<u8> = asset_manager.get(&vxl_name)?;
    let vxl: VxlFile = match VxlFile::from_bytes(&vxl_data) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("Failed to parse {}: {}", vxl_name, e);
            return None;
        }
    };

    // HVA is optional — some models don't have animation files.
    let hva: Option<HvaFile> =
        asset_manager
            .get(&hva_name)
            .and_then(|data| match HvaFile::from_bytes(&data) {
                Ok(h) => Some(h),
                Err(e) => {
                    log::trace!("No HVA for {} ({}), using default pose", key.type_id, e);
                    None
                }
            });

    let params: VxlRenderParams = VxlRenderParams {
        frame: key.frame,
        facing: key.facing, // already quantized by atlas key generation
        slope_type: key.slope_type,
        ..VxlRenderParams::default()
    };

    // Apply house color remapping: replace palette indices 16–31 with house ramp.
    let ramp = house_colors::house_color_ramp(key.house_color);
    let remapped_pal: Palette = palette.with_house_colors(ramp);

    // Branch based on layer: Composite renders all parts together,
    // Body/Turret/Barrel render only the requested part.
    //
    // GPU compute path: available when `compute` is Some and VPL is loaded.
    // For Composite: all limbs from body+turret+barrel are splatted into one
    // atomic framebuffer — atomicMin handles depth compositing automatically.
    // For separated layers: falls back to CPU (needs per-layer depth buffer).
    let use_gpu: bool = compute.is_some() && vpl.is_some() && key.layer == VxlLayer::Composite;

    let sprite: VxlSprite = if use_gpu {
        // GPU compute path for Composite layer.
        let comp = compute.as_deref_mut().unwrap();

        // Upload house-color-remapped palette for this sprite.
        comp.upload_palette(&gpu.device, &gpu.queue, &remapped_pal);

        // Prepare limb data for all VXLs (body + turret + barrel).
        let mut all_limb_data = Vec::new();

        let (body_limbs, _body_fp) = vxl_raster::prepare_limb_data(&vxl, hva.as_ref(), &params);
        all_limb_data.extend(body_limbs);

        // Turret VXL.
        let tur_vxl_name = format!("{}TUR.VXL", image);
        if let Some(tur_data) = asset_manager.get(&tur_vxl_name) {
            if let Ok(tur_vxl) = VxlFile::from_bytes(&tur_data) {
                let tur_hva_name = format!("{}TUR.HVA", image);
                let tur_hva = asset_manager
                    .get(&tur_hva_name)
                    .and_then(|d| HvaFile::from_bytes(&d).ok());
                let (tur_limbs, _) =
                    vxl_raster::prepare_limb_data(&tur_vxl, tur_hva.as_ref(), &params);
                all_limb_data.extend(tur_limbs);
            }
        }

        // Barrel VXL (try BARL then BARREL).
        let barl_vxl_name = format!("{}BARL.VXL", image);
        let barrel_vxl_name = format!("{}BARREL.VXL", image);
        let barl_data = asset_manager
            .get(&barl_vxl_name)
            .or_else(|| asset_manager.get(&barrel_vxl_name));
        if let Some(bd) = barl_data {
            if let Ok(barl_vxl) = VxlFile::from_bytes(&bd) {
                let barl_hva_name = format!("{}BARL.HVA", image);
                let barrel_hva_name = format!("{}BARREL.HVA", image);
                let barl_hva = asset_manager
                    .get(&barl_hva_name)
                    .or_else(|| asset_manager.get(&barrel_hva_name))
                    .and_then(|d| HvaFile::from_bytes(&d).ok());
                let (barl_limbs, _) =
                    vxl_raster::prepare_limb_data(&barl_vxl, barl_hva.as_ref(), &params);
                all_limb_data.extend(barl_limbs);
            }
        }

        if all_limb_data.is_empty() {
            return None;
        }

        // Compute max footprint across all limbs.
        let max_fp: f32 = all_limb_data
            .iter()
            .map(|ld| vxl_raster::compute_voxel_footprint(&ld.combined, params.scale))
            .fold(1.0f32, f32::max);

        let bounds = vxl_raster::compute_sprite_bounds(&all_limb_data, params.scale, max_fp);

        // Build GpuLimb list from LimbRenderData + VXL sparse voxels.
        // We need to map each LimbRenderData back to its VXL's sparse voxel list.
        // Since prepare_limb_data skips empty limbs, we rebuild from the grids.
        use crate::render::vxl_compute::GpuLimb;
        let gpu_limbs: Vec<GpuLimb> = all_limb_data
            .iter()
            .map(|ld| {
                // Extract non-empty voxels from the dense grid.
                let sy = ld.size_y as usize;
                let sz = ld.size_z as usize;
                let mut positions = Vec::new();
                let mut data = Vec::new();
                for x in 0..ld.size_x as usize {
                    for y in 0..sy {
                        for z in 0..sz {
                            let idx = x * sy * sz + y * sz + z;
                            let packed = ld.grid[idx];
                            if packed == 0 {
                                continue;
                            }
                            let color = (packed >> 8) as u8;
                            let normal = (packed & 0xFF) as u8;
                            positions.push(x as u32 | ((y as u32) << 8) | ((z as u32) << 16));
                            data.push(color as u32 | ((normal as u32) << 8));
                        }
                    }
                }
                GpuLimb {
                    positions,
                    data,
                    vpl_pages: ld.vpl_pages,
                    combined: ld.combined,
                }
            })
            .collect();

        let rgba = comp.render_sprite(&gpu.device, &gpu.queue, &gpu_limbs, &bounds, params.scale);

        VxlSprite {
            rgba,
            depth: vec![],
            width: bounds.width,
            height: bounds.height,
            offset_x: bounds.offset_x,
            offset_y: bounds.offset_y,
        }
    } else {
        // CPU fallback path.
        match key.layer {
            VxlLayer::Composite => {
                let body_sprite: VxlSprite =
                    vxl_raster::render_vxl(&vxl, hva.as_ref(), &remapped_pal, &params, vpl);
                let mut layers: Vec<VxlSprite> = vec![body_sprite];
                if let Some(turret) = render_optional_layer(
                    asset_manager,
                    &format!("{}TUR", image),
                    &remapped_pal,
                    &params,
                    vpl,
                ) {
                    layers.push(turret);
                }
                if let Some(barrel) = render_optional_layer(
                    asset_manager,
                    &format!("{}BARL", image),
                    &remapped_pal,
                    &params,
                    vpl,
                )
                .or_else(|| {
                    render_optional_layer(
                        asset_manager,
                        &format!("{}BARREL", image),
                        &remapped_pal,
                        &params,
                        vpl,
                    )
                }) {
                    layers.push(barrel);
                }
                composite_vxl_layers(&layers)
            }
            VxlLayer::Body | VxlLayer::Turret | VxlLayer::Barrel => {
                let body_sprite: VxlSprite =
                    vxl_raster::render_vxl(&vxl, hva.as_ref(), &remapped_pal, &params, vpl);
                let turret_sprite: Option<VxlSprite> = render_optional_layer(
                    asset_manager,
                    &format!("{}TUR", image),
                    &remapped_pal,
                    &params,
                    vpl,
                );
                let barrel_sprite: Option<VxlSprite> = render_optional_layer(
                    asset_manager,
                    &format!("{}BARL", image),
                    &remapped_pal,
                    &params,
                    vpl,
                )
                .or_else(|| {
                    render_optional_layer(
                        asset_manager,
                        &format!("{}BARREL", image),
                        &remapped_pal,
                        &params,
                        vpl,
                    )
                });

                let all_layers: Vec<&VxlSprite> = [Some(&body_sprite)]
                    .into_iter()
                    .chain([turret_sprite.as_ref(), barrel_sprite.as_ref()])
                    .flatten()
                    .collect();

                let requested: Option<&VxlSprite> = match key.layer {
                    VxlLayer::Body => Some(&body_sprite),
                    VxlLayer::Turret => turret_sprite.as_ref(),
                    VxlLayer::Barrel => barrel_sprite.as_ref(),
                    _ => unreachable!(),
                };
                let requested: &VxlSprite = match requested {
                    Some(s) => s,
                    None => return None,
                };

                pad_layer_to_union_bounds(requested, &all_layers)
            }
        }
    };

    // Skip tiny/empty sprites (degenerate models).
    if sprite.width <= 1 && sprite.height <= 1 {
        log::trace!(
            "VXL {} produced empty sprite at facing {}",
            key.type_id,
            key.facing
        );
        return None;
    }

    Some((sprite, use_gpu))
}

/// Detect the HVA animation frame count for a given (type_id, layer) combo.
///
/// Loads the HVA file from the asset manager and returns `frame_count`.
/// Returns 1 if no HVA is found or if parsing fails (single-frame default).
fn detect_hva_frame_count(
    asset_manager: &AssetManager,
    type_id: &str,
    layer: VxlLayer,
    rules: Option<&RuleSet>,
    art: Option<&ArtRegistry>,
) -> u32 {
    let rules_image: String = rules
        .and_then(|r| r.object(type_id))
        .map(|o| o.image.clone())
        .unwrap_or_else(|| type_id.to_string());
    let image: String = art
        .map(|a| a.resolve_effective_image_id(type_id, &rules_image))
        .unwrap_or_else(|| rules_image.to_uppercase());

    let hva_name: String = match layer {
        VxlLayer::Composite | VxlLayer::Body => art_data::voxel_asset_names(&image).1,
        VxlLayer::Turret => format!("{}TUR.HVA", image),
        VxlLayer::Barrel => format!("{}BARL.HVA", image),
    };

    let frame_count: u32 = asset_manager
        .get(&hva_name)
        .and_then(|data| HvaFile::from_bytes(&data).ok())
        .map(|h| h.frame_count)
        .unwrap_or(1);

    // Also try BARREL suffix if BARL had no HVA.
    if layer == VxlLayer::Barrel && frame_count <= 1 {
        let alt_name: String = format!("{}BARREL.HVA", image);
        let alt_count: u32 = asset_manager
            .get(&alt_name)
            .and_then(|data| HvaFile::from_bytes(&data).ok())
            .map(|h| h.frame_count)
            .unwrap_or(1);
        if alt_count > 1 {
            return alt_count;
        }
    }

    frame_count.max(1)
}

fn render_optional_layer(
    asset_manager: &AssetManager,
    layer_base: &str,
    palette: &Palette,
    params: &VxlRenderParams,
    vpl: Option<&VplFile>,
) -> Option<VxlSprite> {
    let vxl_name = format!("{}.VXL", layer_base);
    let vxl_data = asset_manager.get(&vxl_name)?;
    let vxl = VxlFile::from_bytes(&vxl_data).ok()?;
    let hva_name = format!("{}.HVA", layer_base);
    let hva = asset_manager
        .get(&hva_name)
        .and_then(|data| HvaFile::from_bytes(&data).ok());
    Some(vxl_raster::render_vxl(
        &vxl,
        hva.as_ref(),
        palette,
        params,
        vpl,
    ))
}

/// Composite body/turret/barrel layers using depth-correct Z-buffer merging.
/// Each layer's per-pixel depth is compared against the shared depth buffer,
/// so turret voxels behind the body are correctly occluded (and vice versa).
fn composite_vxl_layers(layers: &[VxlSprite]) -> VxlSprite {
    if layers.is_empty() {
        return VxlSprite {
            rgba: vec![0, 0, 0, 0],
            depth: vec![f32::NEG_INFINITY],
            width: 1,
            height: 1,
            offset_x: 0.0,
            offset_y: 0.0,
        };
    }
    if layers.len() == 1 {
        return VxlSprite {
            rgba: layers[0].rgba.clone(),
            depth: layers[0].depth.clone(),
            width: layers[0].width,
            height: layers[0].height,
            offset_x: layers[0].offset_x,
            offset_y: layers[0].offset_y,
        };
    }

    // Offsets are already integer-truncated from the fixed-point rasterizer,
    // so we can safely cast to i32 for pixel-exact compositing.
    let min_x_i: i32 = layers.iter().map(|s| s.offset_x as i32).min().unwrap_or(0);
    let min_y_i: i32 = layers.iter().map(|s| s.offset_y as i32).min().unwrap_or(0);
    let max_x_i: i32 = layers
        .iter()
        .map(|s| s.offset_x as i32 + s.width as i32)
        .max()
        .unwrap_or(1);
    let max_y_i: i32 = layers
        .iter()
        .map(|s| s.offset_y as i32 + s.height as i32)
        .max()
        .unwrap_or(1);

    let width: u32 = (max_x_i - min_x_i).max(1) as u32;
    let height: u32 = (max_y_i - min_y_i).max(1) as u32;
    let pixel_count: usize = (width * height) as usize;
    let mut rgba = vec![0u8; pixel_count * 4];
    let mut depth_buf = vec![f32::NEG_INFINITY; pixel_count];

    // Merge layers using shared depth buffer for correct occlusion.
    for layer in layers {
        let dx: i32 = layer.offset_x as i32 - min_x_i;
        let dy: i32 = layer.offset_y as i32 - min_y_i;
        for y in 0..layer.height as i32 {
            for x in 0..layer.width as i32 {
                let src_pix = (y as u32 * layer.width + x as u32) as usize;
                let src_idx = src_pix * 4;
                let a = layer.rgba[src_idx + 3];
                if a == 0 {
                    continue;
                }
                let px = dx + x;
                let py = dy + y;
                if px < 0 || py < 0 || px >= width as i32 || py >= height as i32 {
                    continue;
                }
                let dst_pix = (py as u32 * width + px as u32) as usize;
                let src_depth = layer.depth[src_pix];
                // Only write pixel if it's closer (or equal) to the camera.
                if src_depth >= depth_buf[dst_pix] {
                    depth_buf[dst_pix] = src_depth;
                    let dst_idx = dst_pix * 4;
                    rgba[dst_idx..dst_idx + 4].copy_from_slice(&layer.rgba[src_idx..src_idx + 4]);
                }
            }
        }
    }

    VxlSprite {
        rgba,
        depth: depth_buf,
        width,
        height,
        offset_x: min_x_i as f32,
        offset_y: min_y_i as f32,
    }
}

/// Pad a single VXL layer sprite into a canvas sized to the union bounding box
/// of all layers. This ensures body/turret/barrel share the same offset origin
/// so they align when drawn at the same screen position.
fn pad_layer_to_union_bounds(layer: &VxlSprite, all_layers: &[&VxlSprite]) -> VxlSprite {
    // Compute union bounding box across all layers (integer, same as composite_vxl_layers).
    let min_x_i: i32 = all_layers
        .iter()
        .map(|s| s.offset_x as i32)
        .min()
        .unwrap_or(0);
    let min_y_i: i32 = all_layers
        .iter()
        .map(|s| s.offset_y as i32)
        .min()
        .unwrap_or(0);
    let max_x_i: i32 = all_layers
        .iter()
        .map(|s| s.offset_x as i32 + s.width as i32)
        .max()
        .unwrap_or(1);
    let max_y_i: i32 = all_layers
        .iter()
        .map(|s| s.offset_y as i32 + s.height as i32)
        .max()
        .unwrap_or(1);

    let width: u32 = (max_x_i - min_x_i).max(1) as u32;
    let height: u32 = (max_y_i - min_y_i).max(1) as u32;
    let pixel_count: usize = (width * height) as usize;
    let mut rgba: Vec<u8> = vec![0u8; pixel_count * 4];
    let mut depth_buf: Vec<f32> = vec![f32::NEG_INFINITY; pixel_count];

    // Blit the requested layer into the union-sized canvas at its correct position.
    let dx: i32 = layer.offset_x as i32 - min_x_i;
    let dy: i32 = layer.offset_y as i32 - min_y_i;
    for y in 0..layer.height as i32 {
        for x in 0..layer.width as i32 {
            let src_pix: usize = (y as u32 * layer.width + x as u32) as usize;
            let src_idx: usize = src_pix * 4;
            if layer.rgba[src_idx + 3] == 0 {
                continue;
            }
            let px: i32 = dx + x;
            let py: i32 = dy + y;
            if px < 0 || py < 0 || px >= width as i32 || py >= height as i32 {
                continue;
            }
            let dst_pix: usize = (py as u32 * width + px as u32) as usize;
            let dst_idx: usize = dst_pix * 4;
            rgba[dst_idx..dst_idx + 4].copy_from_slice(&layer.rgba[src_idx..src_idx + 4]);
            depth_buf[dst_pix] = layer.depth[src_pix];
        }
    }

    VxlSprite {
        rgba,
        depth: depth_buf,
        width,
        height,
        offset_x: min_x_i as f32,
        offset_y: min_y_i as f32,
    }
}

/// Canonicalize body/composite facing to one of 32 rendered facing buckets (step=8).
pub fn canonical_unit_facing(facing: u8) -> u8 {
    (facing / UNIT_FACING_STEP) * UNIT_FACING_STEP
}

/// Canonicalize turret/barrel facing to one of 64 rendered facing buckets (step=4).
/// Accepts 16-bit DirStruct, converts to 8-bit for sprite frame selection.
/// This is the single u16→u8 conversion point for turret rendering.
pub fn canonical_turret_facing(facing_u16: u16) -> u8 {
    let facing_u8: u8 = (facing_u16 >> 8) as u8;
    (facing_u8 / TURRET_FACING_STEP) * TURRET_FACING_STEP
}

/// Get the facing quantization step and bucket count for a given VxlLayer.
fn facing_config_for_layer(layer: VxlLayer) -> (u8, u8) {
    match layer {
        VxlLayer::Body | VxlLayer::Composite => (UNIT_FACING_STEP, UNIT_FACING_BUCKETS),
        VxlLayer::Turret | VxlLayer::Barrel => (TURRET_FACING_STEP, TURRET_FACING_BUCKETS),
    }
}

/// Shelf-pack cached sprites into a GPU texture atlas.
fn pack_sprites(
    gpu: &GpuContext,
    batch: &BatchRenderer,
    sprites: &[CachedUnitSprite],
    frame_counts: BTreeMap<(String, VxlLayer), u32>,
) -> UnitAtlas {
    // Sort by height descending for shelf packing efficiency.
    let mut indices: Vec<usize> = (0..sprites.len()).collect();
    indices.sort_by(|&a, &b| sprites[b].height.cmp(&sprites[a].height));

    // Estimate atlas width from total pixel area.
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
        let mut cursor_x: u32 = 0;
        let mut cursor_y: u32 = 0;
        let mut shelf_height: u32 = 0;
        for &idx in &indices {
            let w: u32 = sprites[idx].width;
            let h: u32 = sprites[idx].height;
            if cursor_x + w > atlas_width {
                cursor_y += shelf_height + SPRITE_PADDING;
                cursor_x = 0;
                shelf_height = 0;
            }
            trial.push((idx, cursor_x, cursor_y));
            cursor_x += w + SPRITE_PADDING;
            shelf_height = shelf_height.max(h);
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
                "Unit atlas height {} exceeds GPU limit {} at max width",
                trial_height,
                max_texture_dim
            );
            placements = trial;
            atlas_height = trial_height.min(max_texture_dim);
            break;
        }
        atlas_width = (atlas_width.saturating_mul(2)).min(max_texture_dim);
    }

    // Allocate RGBA buffer and blit sprites.
    let mut rgba: Vec<u8> = vec![0u8; (atlas_width * atlas_height * 4) as usize];
    let mut entries: HashMap<UnitSpriteKey, UnitSpriteEntry> =
        HashMap::with_capacity(placements.len());
    let aw: f32 = atlas_width as f32;
    let ah: f32 = atlas_height as f32;

    for &(idx, px, py) in &placements {
        let rs: &CachedUnitSprite = &sprites[idx];
        let w: u32 = rs.width;
        let h: u32 = rs.height;

        // Blit sprite RGBA into atlas.
        for y in 0..h {
            let src_start: usize = (y * w * 4) as usize;
            let src_end: usize = src_start + (w * 4) as usize;
            let dst_start: usize = (((py + y) * atlas_width + px) * 4) as usize;
            let dst_end: usize = dst_start + (w * 4) as usize;
            if src_end <= rs.rgba.len() && dst_end <= rgba.len() {
                rgba[dst_start..dst_end].copy_from_slice(&rs.rgba[src_start..src_end]);
            }
        }

        entries.insert(
            rs.key.clone(),
            UnitSpriteEntry {
                uv_origin: [px as f32 / aw, py as f32 / ah],
                uv_size: [w as f32 / aw, h as f32 / ah],
                pixel_size: [w as f32, h as f32],
                offset_x: rs.offset_x,
                offset_y: rs.offset_y,
            },
        );
    }

    let texture: BatchTexture = batch.create_texture(gpu, &rgba, atlas_width, atlas_height);
    UnitAtlas {
        texture,
        entries,
        frame_counts,
        rendered_cache: Vec::new(), // caller sets this after packing
        gpu_rendered: 0,            // caller sets after rendering
        cpu_rendered: 0,
    }
}

// Tests extracted to unit_atlas_tests.rs to stay under 600 lines.
#[cfg(test)]
#[path = "unit_atlas_tests.rs"]
mod tests;
