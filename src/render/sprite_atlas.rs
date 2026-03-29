//! SHP sprite atlas — pre-renders SHP sprites into a packed GPU texture.
//!
//! At map load time, all SpriteModel-tagged entities (infantry, buildings) are
//! identified by (type_id, facing). Each unique combination has its SHP frame
//! rendered to RGBA and packed into a single GPU texture atlas.
//!
//! Mirrors the unit_atlas.rs approach: one texture, one draw call per frame.
//!
//! ## Frame selection
//! - Buildings (Structure): always frame 0 (no rotation).
//! - Infantry: `(facing / 32) % num_facings` for 8-direction standing pose.
//! - Facing is collapsed to 0 for structures in the cache key (dedup).
//!
//! ## Dependency rules
//! - Part of render/ — depends on assets/ (SHP/Palette), render/batch (GPU upload).
//! - Reads from sim/ via EntityStore iteration (GameEntity fields).

use std::collections::{HashMap, HashSet};

use crate::assets::asset_manager::AssetManager;
use crate::assets::pal_file::Palette;
use crate::assets::shp_file::ShpFile;
use crate::map::entities::EntityCategory;
use crate::map::houses::HouseColorMap;
use crate::render::batch::{BatchRenderer, BatchTexture};
use crate::render::gpu::GpuContext;
use crate::rules::art_data::{self, ArtRegistry};
use crate::rules::house_colors::{self, HouseColorIndex};
use crate::rules::ruleset::RuleSet;

/// Maximum atlas texture width for SHP sprites (pixels).

/// Padding between sprites in the atlas to prevent texture bleeding.
const SPRITE_PADDING: u32 = 1;
const INFANTRY_FACING_STEP: u8 = 32;
const INFANTRY_FACING_BUCKETS: u8 = 8;

/// Cache key: unique combination of object type, facing, and house color.
/// For structures, facing is always 0 (buildings don't rotate).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShpSpriteKey {
    /// Object type ID from rules.ini (e.g., "E1", "GAPOWR").
    pub type_id: String,
    /// Facing direction (0–255). Collapsed to 0 for structures.
    pub facing: u8,
    /// Absolute SHP frame index.
    pub frame: u16,
    /// House color index for palette remapping (different per player).
    pub house_color: HouseColorIndex,
}

/// UV and offset data for one sprite within the SHP atlas.
#[derive(Debug, Clone, Copy)]
pub struct ShpSpriteEntry {
    /// Top-left UV coordinate in the atlas page (0.0..1.0).
    pub uv_origin: [f32; 2],
    /// UV width and height (0.0..1.0).
    pub uv_size: [f32; 2],
    /// Sprite dimensions in pixels.
    pub pixel_size: [f32; 2],
    /// X offset from the cell center to the sprite's top-left corner.
    pub offset_x: f32,
    /// Y offset from the cell center to the sprite's top-left corner.
    pub offset_y: f32,
    /// Atlas page index (0-based). Each page is a separate GPU texture.
    pub page: u8,
}

/// A single page of the multi-page sprite atlas.
/// Each page is a separate GPU texture with its own bind group.
pub struct SpriteAtlasPage {
    /// The packed GPU texture for this page.
    pub texture: BatchTexture,
}

/// Per-building-type bounding box for selection brackets and click picking.
///
/// Computed by unioning all SHP frames (main sprite + animation overlays) of the
/// building type.
///
/// Coordinates are relative to `(cell_center_x, screen_y)` where
/// `cell_center_x = screen_x + TILE_WIDTH / 2`.
#[derive(Debug, Clone, Copy)]
pub struct BuildingBounds {
    /// Left edge offset from cell center X.
    pub min_x: f32,
    /// Top edge offset from screen_y.
    pub min_y: f32,
    /// Total width in pixels.
    pub width: f32,
    /// Total height in pixels.
    pub height: f32,
}

/// A multi-page GPU texture atlas containing pre-rendered SHP sprites.
///
/// When the total sprite area exceeds the GPU texture limit, sprites are
/// split across multiple pages. Each page is an independent GPU texture
/// with its own bind group. The entry lookup returns a `page` index that
/// identifies which page's texture to bind when drawing that sprite.
///
/// Created once at map load, rebuilt incrementally when new entity types appear.
pub struct SpriteAtlas {
    /// Atlas pages — each is a separate GPU texture.
    /// Most maps need only 1 page; large games with many building types may use 2+.
    pub pages: Vec<SpriteAtlasPage>,
    /// Lookup: (type_id, facing, frame, house_color) → UV rectangle + offset + page.
    entries: HashMap<ShpSpriteKey, ShpSpriteEntry>,
    /// Building type → number of make (build-up) animation frames.
    /// Key is the base type_id (e.g., "GACNST"), not the "_MAKE" suffixed key.
    pub make_frame_counts: HashMap<String, u16>,
    /// ActiveAnim/ProductionAnim type → total frame count (non-shadow half).
    /// Used by the renderer to cycle crane animations during production.
    pub active_anim_frame_counts: HashMap<String, u16>,
    /// Per-building-type bounding boxes for selection brackets and click picking.
    /// Computed by unioning all SHP frame rects for each building type.
    pub building_bounds: HashMap<String, BuildingBounds>,
    /// Cached rendered sprites for incremental rebuild. On subsequent rebuilds,
    /// only genuinely new sprite keys are rendered; cached sprites are reused
    /// and everything is repacked.
    rendered_cache: Vec<RenderedShpSprite>,
}

impl SpriteAtlas {
    /// Look up the atlas entry for a given sprite key.
    /// The returned entry includes a `page` field identifying which atlas page
    /// holds this sprite's texture data.
    pub fn get(&self, key: &ShpSpriteKey) -> Option<&ShpSpriteEntry> {
        self.entries.get(key)
    }

    /// Number of unique sprites across all pages.
    pub fn sprite_count(&self) -> usize {
        self.entries.len()
    }

    /// Number of atlas pages.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Get a specific atlas page by index.
    pub fn page(&self, idx: usize) -> Option<&SpriteAtlasPage> {
        self.pages.get(idx)
    }

    /// Iterate over all atlas entries (key + sprite data).
    pub fn entries_iter(&self) -> impl Iterator<Item = (&ShpSpriteKey, &ShpSpriteEntry)> {
        self.entries.iter()
    }

    /// Check whether the atlas already contains all sprite keys needed by the
    /// current ECS world. Returns true if no rebuild is necessary.
    pub fn has_all_keys(&self, needed: &HashSet<ShpSpriteKey>) -> bool {
        needed.iter().all(|k| self.entries.contains_key(k))
    }
}

/// Intermediate rendered sprite before atlas packing.
struct RenderedShpSprite {
    key: ShpSpriteKey,
    /// RGBA pixel data blitted into full (width × height) bounds.
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    /// Offset from cell center to top-left of sprite.
    offset_x: f32,
    offset_y: f32,
}

/// Collect the base set of (type_id, house_color) pairs from the ECS world.
///
/// Returns the set of unique (type_id, color) combos that would trigger atlas entries.
/// Used by the incremental rebuild path: if every combo already has entries in the
/// existing atlas, we can skip the expensive full rebuild.
pub fn collect_needed_base_keys(
    entities: &crate::sim::entity_store::EntityStore,
    house_colors: &HouseColorMap,
    extra_building_types: &[&str],
    interner: Option<&crate::sim::intern::StringInterner>,
) -> HashSet<(String, HouseColorIndex)> {
    let mut base_keys: HashSet<(String, HouseColorIndex)> = HashSet::new();
    for entity in entities.values() {
        if entity.is_voxel {
            continue;
        }
        let owner_str = interner.map_or("", |i| i.resolve(entity.owner));
        let type_str = interner.map_or("", |i| i.resolve(entity.type_ref));
        let color_idx: HouseColorIndex =
            house_colors.get(owner_str).copied()
                .unwrap_or(crate::rules::house_colors::NO_REMAP);
        base_keys.insert((type_str.to_string(), color_idx));
    }
    // Include extra building types (deployable ConYards etc.).
    if !extra_building_types.is_empty() {
        let all_colors: Vec<HouseColorIndex> = house_colors.values().copied().collect();
        for &type_id in extra_building_types {
            for &color in &all_colors {
                base_keys.insert((type_id.to_string(), color));
            }
        }
    }
    base_keys
}

/// Check if an existing sprite atlas already covers all the given base keys.
///
/// A base key (type_id, color) is "covered" if the atlas contains at least one
/// entry with that type_id and house_color (e.g., the frame=0 entry).
pub fn atlas_covers_base_keys(
    atlas: &SpriteAtlas,
    base_keys: &HashSet<(String, HouseColorIndex)>,
) -> bool {
    for (type_id, color) in base_keys {
        let probe = ShpSpriteKey {
            type_id: type_id.clone(),
            facing: 0,
            frame: 0,
            house_color: *color,
        };
        if atlas.get(&probe).is_none() {
            // Also check non-zero facings for infantry (facing=0 might be a structure probe).
            let infantry_probe = ShpSpriteKey {
                type_id: type_id.clone(),
                facing: 0,
                frame: 0,
                house_color: *color,
            };
            if atlas.get(&infantry_probe).is_none() {
                return false;
            }
        }
    }
    true
}

/// Build a SHP sprite atlas from all SpriteModel entities in the ECS world.
///
/// Uses incremental rendering: if `existing` is provided, its cached rendered
/// sprites are reused and only genuinely new keys are rendered. This avoids
/// expensive SHP loading and frame blitting for sprites already in the atlas.
///
/// 1. Queries the world for all (TypeRef, Facing, Category, SpriteModel) entities.
/// 2. Collects unique (type_id, facing) pairs (facing=0 for structures).
/// 3. Diffs against cached sprites — renders only new keys.
/// 4. Shelf-packs all sprites (cached + new) into a single atlas texture.
///
/// Returns None if no sprite entities exist or all fail to load.
///
/// `theater_ext` is the file extension for theater-specific SHP files
/// (e.g., "tem" for TEMPERATE). Civilian buildings use `{TYPE_ID}.{ext}`
/// instead of `{TYPE_ID}.SHP`.
pub fn build_sprite_atlas(
    gpu: &GpuContext,
    batch: &BatchRenderer,
    entities: &crate::sim::entity_store::EntityStore,
    asset_manager: &AssetManager,
    palette: &Palette,
    theater_ext: &str,
    theater_name: &str,
    rules: Option<&RuleSet>,
    art: Option<&ArtRegistry>,
    house_colors: &HouseColorMap,
    extra_building_types: &[&str],
    infantry_sequences: &crate::rules::infantry_sequence::InfantrySequenceRegistry,
    existing: Option<SpriteAtlas>,
    interner: Option<&crate::sim::intern::StringInterner>,
) -> Option<SpriteAtlas> {
    // Step 1: Collect unique (type_id, facing, frame, house_color) keys.
    // Structures get facing=0 since buildings don't rotate.
    let mut needed: HashSet<ShpSpriteKey> = HashSet::new();
    for entity in entities.values() {
        if entity.is_voxel {
            continue;
        }
        let owner_str = interner.map_or("", |i| i.resolve(entity.owner));
        let type_str = interner.map_or("", |i| i.resolve(entity.type_ref));
        let color_idx: HouseColorIndex =
            house_colors.get(owner_str).copied()
                .unwrap_or(crate::rules::house_colors::NO_REMAP);
        match entity.category {
            EntityCategory::Structure => {
                needed.insert(ShpSpriteKey {
                    type_id: type_str.to_string(),
                    facing: 0,
                    frame: 0,
                    house_color: color_idx,
                });
            }
            _ => {
                // Look up per-type sequence definition from art.ini.
                // Use it to collect exactly the SHP frames needed for each animation.
                let seq_set: Option<crate::sim::animation::SequenceSet> = art
                    .and_then(|a| a.get(type_str))
                    .and_then(|e| e.sequence.as_deref())
                    .and_then(|name| infantry_sequences.get(&name.to_uppercase()))
                    .map(|def| crate::rules::infantry_sequence::build_sequence_set(def));

                if let Some(ref set) = seq_set {
                    // Data-driven: iterate Stand + Walk sequences (and others)
                    // to collect exactly the right frames.
                    use crate::sim::animation::SequenceKind;
                    let relevant_kinds: &[SequenceKind] = &[
                        SequenceKind::Stand,
                        SequenceKind::Walk,
                        SequenceKind::Attack,
                        SequenceKind::Idle1,
                        SequenceKind::Idle2,
                        SequenceKind::Die1,
                        SequenceKind::Die2,
                        SequenceKind::Die3,
                        SequenceKind::Die4,
                        SequenceKind::Die5,
                        SequenceKind::Prone,
                        SequenceKind::Crawl,
                        SequenceKind::FireProne,
                        SequenceKind::Down,
                        SequenceKind::Up,
                        SequenceKind::Cheer,
                        SequenceKind::Paradrop,
                        SequenceKind::Panic,
                        SequenceKind::Deploy,
                        SequenceKind::Undeploy,
                        SequenceKind::Deployed,
                        SequenceKind::DeployedFire,
                        SequenceKind::DeployedIdle,
                        SequenceKind::SecondaryFire,
                        SequenceKind::SecondaryProne,
                        SequenceKind::Swim,
                        SequenceKind::Fly,
                        SequenceKind::FireFly,
                        SequenceKind::Hover,
                        SequenceKind::Tread,
                        SequenceKind::WetAttack,
                        SequenceKind::WetIdle1,
                        SequenceKind::WetIdle2,
                    ];
                    for kind in relevant_kinds {
                        if let Some(seq_def) = set.get(kind) {
                            for f_idx in 0..seq_def.facings {
                                for frame_offset in 0..seq_def.frame_count {
                                    let frame: u16 = seq_def.start_frame
                                        + f_idx as u16 * seq_def.facing_multiplier
                                        + frame_offset;
                                    // Use facing=0 for all infantry keys — the absolute
                                    // frame index already encodes the facing direction.
                                    // This avoids cache key mismatches for non-8-facing
                                    // sequences (most RA2 infantry use 6 facings).
                                    needed.insert(ShpSpriteKey {
                                        type_id: type_str.to_string(),
                                        facing: 0,
                                        frame,
                                        house_color: color_idx,
                                    });
                                }
                            }
                        }
                    }
                } else {
                    // Fallback: hardcoded default layout (stand=0-7, walk=8-55).
                    // Use facing=0 for all keys — frame index encodes direction.
                    for bucket in 0..INFANTRY_FACING_BUCKETS {
                        needed.insert(ShpSpriteKey {
                            type_id: type_str.to_string(),
                            facing: 0,
                            frame: bucket as u16,
                            house_color: color_idx,
                        });
                        for walk_frame in 0..6u16 {
                            needed.insert(ShpSpriteKey {
                                type_id: type_str.to_string(),
                                facing: 0,
                                frame: 8 + bucket as u16 * 6 + walk_frame,
                                house_color: color_idx,
                            });
                        }
                    }
                }
            }
        }
    }

    // Step 1a: Pre-load extra building types (e.g., ConYards for MCV deployment).
    // These may not exist on the map initially but can be spawned at runtime.
    if !extra_building_types.is_empty() {
        let all_colors: Vec<HouseColorIndex> = house_colors.values().copied().collect();
        for &type_id in extra_building_types {
            for &color in &all_colors {
                needed.insert(ShpSpriteKey {
                    type_id: type_id.to_string(),
                    facing: 0,
                    frame: 0,
                    house_color: color,
                });
            }
        }
    }

    // Step 1b: Also collect building animation overlay SHPs (ActiveAnim, IdleAnim, etc.).
    // IdleAnim/SuperAnim/SpecialAnim: only frame 0 (static or not yet animated).
    // ActiveAnim/ProductionAnim: all frames (crane animation plays during production).
    let mut active_anim_frame_counts: HashMap<String, u16> = HashMap::new();
    if let Some(art_reg) = art {
        let building_keys: Vec<ShpSpriteKey> = needed
            .iter()
            .filter(|k| k.facing == 0) // structures only
            .cloned()
            .collect();
        for key in &building_keys {
            let rules_image: String = rules
                .and_then(|r| r.object(&key.type_id))
                .map(|o| o.image.clone())
                .unwrap_or_else(|| key.type_id.clone());
            let art_entry: Option<&crate::rules::art_data::ArtEntry> =
                art_reg.resolve_metadata_entry(&key.type_id, &rules_image);
            if let Some(entry) = art_entry {
                for anim in &entry.building_anims {
                    let is_active: bool = matches!(
                        anim.kind,
                        crate::rules::art_data::BuildingAnimKind::Active
                            | crate::rules::art_data::BuildingAnimKind::Production
                    );
                    if is_active {
                        // Load all frames for active/production anims (crane, etc.).
                        // Pre-scan the SHP to count frames, similar to make SHPs.
                        let anim_upper: String = anim.anim_type.to_uppercase();
                        if !active_anim_frame_counts.contains_key(&anim_upper) {
                            let anim_image: String = art_reg
                                .resolve_effective_image_id(&anim.anim_type, &anim.anim_type);
                            let candidates: Vec<String> = art_data::anim_shp_candidates(
                                Some(art_reg),
                                &anim.anim_type,
                                &anim_image,
                                theater_ext,
                                theater_name,
                            );
                            if let Some(data) = candidates.iter().find_map(|c| asset_manager.get(c))
                            {
                                if let Ok(shp) = ShpFile::from_bytes(&data) {
                                    // RA2 anim SHPs have shadow frames in second half.
                                    let real: u16 = (shp.frames.len() as u16) / 2;
                                    // Ensure we load at least loop_end frames so every
                                    // frame in LoopStart..LoopEnd (exclusive) exists in
                                    // the atlas. Guards against odd total frame counts
                                    // where integer division would drop frames.
                                    let count: u16 = if real >= anim.loop_end {
                                        real
                                    } else if (shp.frames.len() as u16) >= anim.loop_end {
                                        anim.loop_end
                                    } else {
                                        shp.frames.len() as u16
                                    };
                                    active_anim_frame_counts.insert(anim_upper.clone(), count);
                                    log::info!(
                                        "ActiveAnim {} for {}: {} frames (shp total={}, shadow-split={}, loop_end={})",
                                        anim.anim_type,
                                        key.type_id,
                                        count,
                                        shp.frames.len(),
                                        real,
                                        anim.loop_end,
                                    );
                                }
                            }
                        }
                        let count: u16 = active_anim_frame_counts
                            .get(&anim.anim_type.to_uppercase())
                            .copied()
                            .unwrap_or(1);
                        for f in 0..count {
                            needed.insert(ShpSpriteKey {
                                type_id: anim.anim_type.clone(),
                                facing: 0,
                                frame: f,
                                house_color: key.house_color,
                            });
                        }
                    } else if matches!(anim.kind, crate::rules::art_data::BuildingAnimKind::Idle) {
                        // IdleAnim loops continuously — load all frames (same as Active).
                        let anim_upper: String = anim.anim_type.to_uppercase();
                        if !active_anim_frame_counts.contains_key(&anim_upper) {
                            let anim_image: String = art_reg
                                .resolve_effective_image_id(&anim.anim_type, &anim.anim_type);
                            let candidates: Vec<String> = art_data::anim_shp_candidates(
                                Some(art_reg),
                                &anim.anim_type,
                                &anim_image,
                                theater_ext,
                                theater_name,
                            );
                            if let Some(data) = candidates.iter().find_map(|c| asset_manager.get(c))
                            {
                                if let Ok(shp) = ShpFile::from_bytes(&data) {
                                    let real: u16 = (shp.frames.len() as u16) / 2;
                                    // Same loop_end guard as ActiveAnim above.
                                    let count: u16 = if real >= anim.loop_end {
                                        real
                                    } else if (shp.frames.len() as u16) >= anim.loop_end {
                                        anim.loop_end
                                    } else {
                                        shp.frames.len() as u16
                                    };
                                    active_anim_frame_counts.insert(anim_upper.clone(), count);
                                    log::info!(
                                        "IdleAnim {} for {}: {} frames (shp total={}, shadow-split={}, loop_end={})",
                                        anim.anim_type,
                                        key.type_id,
                                        count,
                                        shp.frames.len(),
                                        real,
                                        anim.loop_end,
                                    );
                                }
                            }
                        }
                        let count: u16 = active_anim_frame_counts
                            .get(&anim.anim_type.to_uppercase())
                            .copied()
                            .unwrap_or(1);
                        for f in 0..count {
                            needed.insert(ShpSpriteKey {
                                type_id: anim.anim_type.clone(),
                                facing: 0,
                                frame: f,
                                house_color: key.house_color,
                            });
                        }
                    } else {
                        // Super/Special anims: frame 0 only (not yet animated).
                        needed.insert(ShpSpriteKey {
                            type_id: anim.anim_type.clone(),
                            facing: 0,
                            frame: 0,
                            house_color: key.house_color,
                        });
                    }
                }
                // BibShape: ground-level pad SHP (e.g., refinery dock GAREFNBB).
                // Use bib name directly as atlas key so render can look it up.
                if let Some(ref bib) = entry.bib_shape {
                    needed.insert(ShpSpriteKey {
                        type_id: bib.to_uppercase(),
                        facing: 0,
                        frame: 0,
                        house_color: key.house_color,
                    });
                    log::info!("BibShape for {}: {}", key.type_id, bib);
                }
            }
        }
    }

    // Step 1c: Pre-scan for building "make" (build-up) SHPs and add all their frames.
    // The make SHP is a separate file (e.g., GTCNSTMK.SHP) with N frames showing
    // the building assembling. We need to load it briefly to count frames, then add
    // keys for each frame so they get rendered into the atlas.
    let mut make_frame_counts: HashMap<String, u16> = HashMap::new();
    if let Some(art_reg) = art {
        let building_type_ids: Vec<(String, HouseColorIndex)> = needed
            .iter()
            .filter(|k| k.facing == 0 && k.frame == 0)
            .map(|k| (k.type_id.clone(), k.house_color))
            .collect();
        for (type_id, color) in &building_type_ids {
            // Skip anim overlay types (they don't have make SHPs).
            if type_id.contains('_') {
                continue;
            }
            let make_key: String = format!("{}_MAKE", type_id);
            if make_frame_counts.contains_key(&make_key) {
                // Already scanned this type — just add keys for this color.
                let count: u16 = make_frame_counts[&make_key];
                for f in 0..count {
                    needed.insert(ShpSpriteKey {
                        type_id: make_key.clone(),
                        facing: 0,
                        frame: f,
                        house_color: *color,
                    });
                }
                continue;
            }
            let rules_image: String = rules
                .and_then(|r| r.object(type_id))
                .map(|o| o.image.clone())
                .unwrap_or_else(|| type_id.clone());
            let image: String = art_reg.resolve_effective_image_id(type_id, &rules_image);
            let candidates: Vec<String> =
                art_data::make_shp_candidates(Some(art_reg), &image, theater_ext, theater_name);
            let shp_data: Option<Vec<u8>> = candidates.iter().find_map(|c| asset_manager.get(c));
            if let Some(data) = shp_data {
                if let Ok(shp) = ShpFile::from_bytes(&data) {
                    // RA2 make SHPs have shadow frames in the second half — only use the first half.
                    let real_frames: u16 = (shp.frames.len() as u16) / 2;
                    let frame_count: u16 = if real_frames > 0 {
                        real_frames
                    } else {
                        shp.frames.len() as u16
                    };
                    log::info!(
                        "Make SHP for {}: {} frames ({})",
                        type_id,
                        frame_count,
                        candidates
                            .iter()
                            .find(|c| asset_manager.get(c).is_some())
                            .unwrap_or(&String::new()),
                    );
                    make_frame_counts.insert(make_key.clone(), frame_count);
                    for f in 0..frame_count {
                        needed.insert(ShpSpriteKey {
                            type_id: make_key.clone(),
                            facing: 0,
                            frame: f,
                            house_color: *color,
                        });
                    }
                }
            }
        }
    }

    // Step 1d: Pre-load world effect SHPs (warp animations, explosions, etc.).
    // Names come from rules.ini [General] WarpIn=/WarpOut=/WarpAway= — NOT hardcoded.
    // These use anim.pal (effect palette), not unit.pal — tracked in effect_type_ids
    // so step 2 can pick the correct palette.
    let mut effect_type_ids: HashSet<String> = HashSet::new();
    {
        let mut effect_names: Vec<String> = Vec::new();
        if let Some(r) = rules {
            effect_names.push(r.general.warp_in.name.clone());
            effect_names.push(r.general.warp_out.name.clone());
            effect_names.push(r.general.warp_away.name.clone());
            // Add damage fire types (FIRE01, FIRE02, FIRE03 by default).
            for fire_ref in &r.general.damage_fire_types {
                if !effect_names
                    .iter()
                    .any(|n| n.eq_ignore_ascii_case(&fire_ref.name))
                {
                    effect_names.push(fire_ref.name.clone());
                }
            }
            // Collect explosion animation names from all warhead AnimList= fields.
            for wh in r.warheads_iter() {
                for anim_name in &wh.anim_list {
                    if !effect_names
                        .iter()
                        .any(|n| n.eq_ignore_ascii_case(anim_name))
                    {
                        effect_names.push(anim_name.clone());
                    }
                }
            }
            // Collect OccupantAnim names from weapons (garrison muzzle flashes).
            for weapon in r.weapons_iter() {
                if let Some(ref anim_name) = weapon.occupant_anim {
                    if !effect_names
                        .iter()
                        .any(|n| n.eq_ignore_ascii_case(anim_name))
                    {
                        effect_names.push(anim_name.clone());
                    }
                }
            }
        }
        for name in &effect_names {
            let lower: String = name.to_ascii_lowercase();
            let candidates: Vec<String> = vec![format!("{}.shp", lower), format!("{}.SHP", name)];
            if let Some(data) = candidates.iter().find_map(|c| asset_manager.get(c)) {
                if let Ok(shp) = ShpFile::from_bytes(&data) {
                    let real: u16 = (shp.frames.len() as u16) / 2;
                    let count: u16 = if real > 0 {
                        real
                    } else {
                        shp.frames.len() as u16
                    };
                    for f in 0..count {
                        needed.insert(ShpSpriteKey {
                            type_id: name.clone(),
                            facing: 0,
                            frame: f,
                            house_color: HouseColorIndex(0),
                        });
                    }
                    // Store frame count so sim systems (miner chrono-teleport) can
                    // look up real frame counts instead of hardcoding them.
                    active_anim_frame_counts.insert(name.to_uppercase(), count);
                    effect_type_ids.insert(name.clone());
                    log::info!("WorldEffect SHP {}: {} frames loaded", name, count);
                }
            }
        }
    }

    if needed.is_empty() {
        log::info!("No SHP sprite entities found — skipping sprite atlas");
        return None;
    }

    // Step 1e: Extract cached rendered sprites from existing atlas, diff against needed.
    let mut cached: Vec<RenderedShpSprite> = existing
        .map(|atlas| atlas.rendered_cache)
        .unwrap_or_default();
    let cached_keys: HashSet<ShpSpriteKey> = cached.iter().map(|s| s.key.clone()).collect();
    let new_count: usize = needed.iter().filter(|k| !cached_keys.contains(k)).count();

    log::info!(
        "Sprite atlas: {} cached, {} new to render, {} total needed",
        cached.len(),
        new_count,
        needed.len(),
    );

    // Load anim.pal for world effect SHPs. Anim types (explosions, warp flashes,
    // etc.) are drawn with anim.pal, not unit.pal.
    let effect_palette: Option<Palette> = asset_manager
        .get("anim.pal")
        .and_then(|d| Palette::from_bytes(&d).ok());
    if effect_palette.is_none() && !effect_type_ids.is_empty() {
        log::warn!("anim.pal not found — world effect SHPs will use unit.pal (wrong colors)");
    }

    // Step 2: Render only new sprites (skip cached ones).
    for key in &needed {
        if cached_keys.contains(key) {
            continue;
        }
        // World effect SHPs use anim.pal; everything else uses unit.pal.
        let pal: &Palette = if effect_type_ids.contains(&key.type_id) {
            effect_palette.as_ref().unwrap_or(palette)
        } else {
            palette
        };
        match render_shp_sprite(
            asset_manager,
            pal,
            key,
            theater_ext,
            theater_name,
            rules,
            art,
        ) {
            Some(sprite) => cached.push(sprite),
            None => log::debug!(
                "No SHP for {} (facing {}, frame {})",
                key.type_id,
                key.facing,
                key.frame
            ),
        }
    }

    // Step 2b: Render oregath.shp harvest overlay frames (if any harvesters exist).
    // Uses anim.pal (effect palette) — no house color remap. Skip if already cached.
    let has_harvesters: bool = entities.values().any(|e| e.miner.is_some());
    let oregath_cached: bool = cached_keys.contains(&ShpSpriteKey {
        type_id: "OREGATH".to_string(),
        facing: 0,
        frame: 0,
        house_color: HouseColorIndex::default(),
    });
    if has_harvesters && !oregath_cached {
        let oregath_sprites: Vec<RenderedShpSprite> =
            render_harvest_overlay_frames(asset_manager);
        if !oregath_sprites.is_empty() {
            log::info!(
                "Rendered {} oregath.shp harvest overlay frames",
                oregath_sprites.len()
            );
            cached.extend(oregath_sprites);
        }
    }

    if cached.is_empty() {
        log::warn!("No SHP sprites rendered — sprite atlas will be empty");
        return None;
    }

    log::info!(
        "Packing {} SHP sprites into atlas ({} reused from cache)",
        cached.len(),
        cached.len().saturating_sub(new_count),
    );

    // Step 3: Shelf-pack all sprites (cached + newly rendered) into atlas.
    let mut atlas: SpriteAtlas = pack_sprites(gpu, batch, &cached);
    atlas.make_frame_counts = make_frame_counts;
    atlas.active_anim_frame_counts = active_anim_frame_counts;
    atlas.building_bounds = compute_building_bounds(&atlas, entities, art, rules, interner);
    atlas.rendered_cache = cached;
    log::info!(
        "SHP sprite atlas built: {} sprites, {} pages, {} make anims, {} active anims, {} building bounds",
        atlas.sprite_count(),
        atlas.page_count(),
        atlas.make_frame_counts.len(),
        atlas.active_anim_frame_counts.len(),
        atlas.building_bounds.len(),
    );
    Some(atlas)
}

/// Compute per-building-type bounding boxes by unioning all SHP frame rects.
///
/// For each structure type in the entity store, unions the main sprite rect with
/// any building animation overlay rects (ActiveAnim, IdleAnim, etc. from art.ini).
/// This matches the original engine's `BuildingTypeClass::CalculateBoundingBox`.
fn compute_building_bounds(
    atlas: &SpriteAtlas,
    entities: &crate::sim::entity_store::EntityStore,
    art: Option<&ArtRegistry>,
    rules: Option<&RuleSet>,
    interner: Option<&crate::sim::intern::StringInterner>,
) -> HashMap<String, BuildingBounds> {
    let mut bounds: HashMap<String, BuildingBounds> = HashMap::new();

    // Collect unique building type_ids — include both existing structures and
    // any buildings reachable via DeploysInto (e.g. MCV → ConYard) so they are
    // clickable even if they didn't exist when the atlas was first built.
    let mut building_types: HashSet<String> = entities
        .values()
        .filter(|e| e.category == EntityCategory::Structure && !e.is_voxel)
        .map(|e| interner.map_or("".to_string(), |i| i.resolve(e.type_ref).to_string()))
        .collect();
    if let Some(r) = rules {
        let deploy_targets: Vec<String> = entities
            .values()
            .filter_map(|e| {
                let t = interner.map_or("", |i| i.resolve(e.type_ref));
                r.object(t).and_then(|o| o.deploys_into.clone())
            })
            .collect();
        building_types.extend(deploy_targets);
    }

    for type_id in &building_types {
        // Find any atlas entry for the main building sprite (frame 0, any house color).
        let main_entry = atlas
            .entries_iter()
            .find(|(k, _)| k.type_id == *type_id && k.frame == 0)
            .map(|(_, v)| *v);
        let Some(main) = main_entry else { continue };

        // Initialize bbox from main sprite.
        let mut min_x: f32 = main.offset_x;
        let mut min_y: f32 = main.offset_y;
        let mut max_x: f32 = main.offset_x + main.pixel_size[0];
        let mut max_y: f32 = main.offset_y + main.pixel_size[1];

        // Union all other frames of the main sprite (animation frames).
        for (k, v) in atlas.entries_iter() {
            if k.type_id == *type_id {
                min_x = min_x.min(v.offset_x);
                min_y = min_y.min(v.offset_y);
                max_x = max_x.max(v.offset_x + v.pixel_size[0]);
                max_y = max_y.max(v.offset_y + v.pixel_size[1]);
            }
        }

        // Union with building animation overlay sprites (ActiveAnim, IdleAnim, etc.).
        if let Some(art_reg) = art {
            let rules_image: String = rules
                .and_then(|r| r.object(type_id))
                .map(|o| o.image.clone())
                .unwrap_or_else(|| type_id.clone());
            if let Some(art_entry) = art_reg.resolve_metadata_entry(type_id, &rules_image) {
                for anim in &art_entry.building_anims {
                    // Anim overlays are offset by (anim.x, anim.y) pixels from building origin.
                    for (k, v) in atlas.entries_iter() {
                        if k.type_id == anim.anim_type {
                            let ax: f32 = anim.x as f32 + v.offset_x;
                            let ay: f32 = anim.y as f32 + v.offset_y;
                            min_x = min_x.min(ax);
                            min_y = min_y.min(ay);
                            max_x = max_x.max(ax + v.pixel_size[0]);
                            max_y = max_y.max(ay + v.pixel_size[1]);
                        }
                    }
                }
            }
        }

        bounds.insert(
            type_id.clone(),
            BuildingBounds {
                min_x,
                min_y,
                width: max_x - min_x,
                height: max_y - min_y,
            },
        );
    }

    bounds
}

/// Load and render a single SHP sprite to RGBA pixels.
///
/// Uses ArtRegistry to resolve the correct filename (with NewTheater substitution).
/// Falls back to direct {TYPE_ID}.SHP if art data is unavailable.
/// Selects the appropriate frame based on facing (8-direction for infantry).
fn render_shp_sprite(
    asset_manager: &AssetManager,
    palette: &Palette,
    key: &ShpSpriteKey,
    theater_ext: &str,
    theater_name: &str,
    rules: Option<&RuleSet>,
    art: Option<&ArtRegistry>,
) -> Option<RenderedShpSprite> {
    // Check if this is a make (build-up) SHP — type_id ends with "_MAKE".
    let is_make: bool = key.type_id.ends_with("_MAKE");
    let base_type_id: &str = if is_make {
        &key.type_id[..key.type_id.len() - 5]
    } else {
        &key.type_id
    };

    // Resolve image name: type_id → rules.ini Image= → art.ini Image= override.
    let rules_image: String = rules
        .and_then(|r| r.object(base_type_id))
        .map(|o| o.image.clone())
        .unwrap_or_else(|| base_type_id.to_string());
    let image: String = art
        .map(|a| a.resolve_effective_image_id(base_type_id, &rules_image))
        .unwrap_or_else(|| rules_image.to_uppercase());

    // Build filename candidates — use make candidates for _MAKE types.
    let candidates: Vec<String> = if is_make {
        art_data::make_shp_candidates(art, &image, theater_ext, theater_name)
    } else {
        art_data::object_shp_candidates(art, &image, theater_ext, theater_name)
    };

    // Try each candidate in order until one is found.
    let mut lookup_result: Option<(Vec<u8>, String)> = None;
    for name in &candidates {
        if let Some(data) = asset_manager.get(name) {
            lookup_result = Some((data, name.clone()));
            break;
        }
    }
    let (shp_data, found_name) = match lookup_result {
        Some(pair) => pair,
        None => {
            log::warn!("SHP not found for {}: tried {:?}", key.type_id, candidates);
            return None;
        }
    };
    // Log when a non-first candidate was selected (generic 'G' fallback is normal).
    if candidates.len() > 2 && found_name != candidates[0] {
        log::debug!(
            "Theater fallback for {}: wanted '{}' but loaded '{}' (tried {:?})",
            key.type_id,
            candidates[0],
            found_name,
            candidates,
        );
    }
    log::trace!("Loaded SHP: {} ({} bytes)", found_name, shp_data.len());
    let shp: ShpFile = match ShpFile::from_bytes(&shp_data) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("Failed to parse {}: {}", found_name, e);
            return None;
        }
    };

    if shp.frames.is_empty() {
        log::warn!("{} has no frames", found_name);
        return None;
    }

    log::debug!(
        "{}: {}x{}, {} frames",
        found_name,
        shp.width,
        shp.height,
        shp.frames.len()
    );

    // Frame selection: use facing to pick among the first 8 frames (standing pose).
    // If SHP has fewer frames, use frame 0.
    let frame_idx: usize = if (key.frame as usize) < shp.frames.len() {
        key.frame as usize
    } else if shp.frames.len() >= 8 {
        (INFANTRY_FACING_BUCKETS as usize
            - (canonical_infantry_facing(key.facing) as usize / INFANTRY_FACING_STEP as usize))
            % INFANTRY_FACING_BUCKETS as usize
    } else {
        0
    };

    let frame = &shp.frames[frame_idx];
    if frame.frame_width == 0 || frame.frame_height == 0 {
        log::warn!(
            "{} frame {} is empty ({}x{})",
            found_name,
            frame_idx,
            frame.frame_width,
            frame.frame_height
        );
        return None;
    }

    // Apply house color remapping: replace palette indices 16–31 with house ramp.
    let ramp = house_colors::house_color_ramp(key.house_color);
    let remapped_pal: Palette = palette.with_house_colors(ramp);

    let frame_rgba: Vec<u8> = match shp.frame_to_rgba(frame_idx, &remapped_pal) {
        Ok(rgba) => rgba,
        Err(e) => {
            log::warn!(
                "Failed to convert {} frame {}: {}",
                found_name,
                frame_idx,
                e
            );
            return None;
        }
    };

    // Blit the sub-frame into the full (shp.width × shp.height) bounds.
    // This ensures consistent sprite dimensions and correct positioning.
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
        let src_start: usize = (y * fw * 4) as usize;
        let src_end: usize = src_start + (fw * 4) as usize;
        let dst_start: usize = ((dst_y * full_w + fx) * 4) as usize;
        let copy_w: u32 = fw.min(full_w.saturating_sub(fx));
        let dst_end: usize = dst_start + (copy_w * 4) as usize;
        if src_end <= frame_rgba.len() && dst_end <= full_rgba.len() {
            let actual_bytes: usize = (copy_w * 4) as usize;
            full_rgba[dst_start..dst_start + actual_bytes]
                .copy_from_slice(&frame_rgba[src_start..src_start + actual_bytes]);
        }
    }

    log::debug!(
        "SHP {} facing={}: {}x{} frame {}/{}, sub {}x{} at ({},{})",
        found_name,
        key.facing,
        full_w,
        full_h,
        frame_idx,
        shp.frames.len(),
        fw,
        fh,
        fx,
        fy,
    );

    // Center sprite on cell center. FrameOffset embedded via blit.
    // DrawOffset from art.ini XDrawOffset/YDrawOffset for per-type fine-tuning.
    // Uses integer division: -ShapeWidth/2 (truncated), to avoid sub-pixel drift
    // on odd-dimension SHPs.
    let (xdo, ydo) = art.map(|a| a.draw_offsets(&key.type_id)).unwrap_or((0, 0));
    let offset_x: f32 = -((full_w / 2) as f32) + xdo as f32;
    let offset_y: f32 = -((full_h / 2) as f32) + ydo as f32;

    Some(RenderedShpSprite {
        key: key.clone(),
        rgba: full_rgba,
        width: full_w,
        height: full_h,
        offset_x,
        offset_y,
    })
}

/// Canonicalize RA2 facing byte to one of the 8 infantry-facing buckets.
pub fn canonical_infantry_facing(facing: u8) -> u8 {
    (facing / INFANTRY_FACING_STEP) * INFANTRY_FACING_STEP
}

/// Render oregath.shp harvest overlay frames using the effect palette (anim.pal).
///
/// OREGATH uses anim.pal (the same palette used for explosion/effect SHPs),
/// with no house color remap.
///
/// Returns all 120 SHP frames (15 animation frames x 8 facings) as rendered sprites.
/// Each frame is keyed as ShpSpriteKey { type_id: "OREGATH", facing: 0, frame: <shp_idx> }.
/// At render time, the correct frame is: `facing_index * 15 + anim_frame`.
fn render_harvest_overlay_frames(asset_manager: &AssetManager) -> Vec<RenderedShpSprite> {
    // Load effect palette (anim.pal).
    let pal_data: Vec<u8> = match asset_manager.get("anim.pal") {
        Some(d) => d,
        None => {
            log::warn!("anim.pal not found — skipping harvest overlay");
            return Vec::new();
        }
    };
    let palette: Palette = match Palette::from_bytes(&pal_data) {
        Ok(p) => p,
        Err(e) => {
            log::warn!("Failed to parse anim.pal: {} — skipping harvest overlay", e);
            return Vec::new();
        }
    };

    // Load oregath.shp.
    let shp_data: Vec<u8> = match asset_manager.get("oregath.shp") {
        Some(d) => d,
        None => {
            log::warn!("oregath.shp not found — skipping harvest overlay");
            return Vec::new();
        }
    };
    let shp: ShpFile = match ShpFile::from_bytes(&shp_data) {
        Ok(s) => s,
        Err(e) => {
            log::warn!(
                "Failed to parse oregath.shp: {} — skipping harvest overlay",
                e
            );
            return Vec::new();
        }
    };

    // RA2 SHP files often have shadow frames in the second half.
    // Use only the first half (real frames).
    let total_frames: usize = shp.frames.len();
    let real_frames: usize = if total_frames > 120 {
        total_frames / 2
    } else {
        total_frames
    };
    log::info!(
        "oregath.shp: {}x{}, {} total frames, {} real frames",
        shp.width,
        shp.height,
        total_frames,
        real_frames,
    );

    let hc: HouseColorIndex = HouseColorIndex::default();
    let mut sprites: Vec<RenderedShpSprite> = Vec::with_capacity(real_frames);

    let canvas_w: u32 = shp.width as u32;
    let canvas_h: u32 = shp.height as u32;

    for frame_idx in 0..real_frames {
        let frame = &shp.frames[frame_idx];
        if frame.frame_width == 0 || frame.frame_height == 0 {
            continue;
        }
        // Render with effect palette — no house color remap.
        let frame_rgba: Vec<u8> = match shp.frame_to_rgba(frame_idx, &palette) {
            Ok(rgba) => rgba,
            Err(_) => continue,
        };

        // Use per-frame dimensions instead of the full SHP canvas.
        // oregath.shp's canvas encompasses all 8 facings, so it's much larger
        // than any single frame. Using the canvas size makes the overlay huge.
        let fw: u32 = frame.frame_width as u32;
        let fh: u32 = frame.frame_height as u32;
        let fx: u32 = frame.frame_x as u32;
        let fy: u32 = frame.frame_y as u32;

        // Offset: frame position within canvas, relative to canvas center.
        // This positions the sub-frame correctly relative to the unit center.
        let offset_x: f32 = fx as f32 - (canvas_w as f32) / 2.0;
        let offset_y: f32 = fy as f32 - (canvas_h as f32) / 2.0;

        sprites.push(RenderedShpSprite {
            key: ShpSpriteKey {
                type_id: "OREGATH".to_string(),
                facing: 0,
                frame: frame_idx as u16,
                house_color: hc,
            },
            rgba: frame_rgba,
            width: fw,
            height: fh,
            offset_x,
            offset_y,
        });
    }

    sprites
}

/// Shelf-pack rendered SHP sprites into a multi-page GPU texture atlas.
///
/// Sprites are packed into pages of at most `max_texture_dim × max_texture_dim`
/// pixels each. Most maps fit in a single page; pages are added only when the
/// total sprite area exceeds what one GPU texture can hold.
fn pack_sprites(
    gpu: &GpuContext,
    batch: &BatchRenderer,
    sprites: &[RenderedShpSprite],
) -> SpriteAtlas {
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

    // Try widening atlas to fit everything in a single page.
    loop {
        let trial_height: u32 = simulate_shelf_height(&indices, sprites, atlas_width);
        if trial_height <= max_texture_dim || atlas_width >= max_texture_dim {
            break;
        }
        atlas_width = (atlas_width.saturating_mul(2)).min(max_texture_dim);
    }

    // Shelf-pack with page splitting when height exceeds GPU limit.
    struct Placement {
        idx: usize,
        page: u8,
        px: u32,
        py: u32,
    }
    let mut placements: Vec<Placement> = Vec::with_capacity(sprites.len());
    let mut cursor_x: u32 = 0;
    let mut cursor_y: u32 = 0;
    let mut shelf_height: u32 = 0;
    let mut current_page: u8 = 0;

    for &idx in &indices {
        let w: u32 = sprites[idx].width;
        let h: u32 = sprites[idx].height;
        if cursor_x + w > atlas_width {
            let new_y: u32 = cursor_y + shelf_height + SPRITE_PADDING;
            if new_y + h > max_texture_dim {
                // Current page is full — start a new page.
                current_page += 1;
                cursor_x = 0;
                cursor_y = 0;
                shelf_height = 0;
            } else {
                cursor_y = new_y;
                cursor_x = 0;
                shelf_height = 0;
            }
        }
        placements.push(Placement {
            idx,
            page: current_page,
            px: cursor_x,
            py: cursor_y,
        });
        cursor_x += w + SPRITE_PADDING;
        shelf_height = shelf_height.max(h);
    }

    let num_pages: usize = current_page as usize + 1;
    if num_pages > 1 {
        log::info!(
            "Sprite atlas split into {} pages (GPU texture limit {})",
            num_pages,
            max_texture_dim,
        );
    }

    // Build each page's GPU texture.
    let mut pages: Vec<SpriteAtlasPage> = Vec::with_capacity(num_pages);
    let mut entries: HashMap<ShpSpriteKey, ShpSpriteEntry> =
        HashMap::with_capacity(placements.len());
    for page_idx in 0..num_pages as u8 {
        let page_height: u32 = placements
            .iter()
            .filter(|p| p.page == page_idx)
            .map(|p| p.py + sprites[p.idx].height)
            .max()
            .unwrap_or(1);

        let mut rgba: Vec<u8> = vec![0u8; (atlas_width * page_height * 4) as usize];
        let aw: f32 = atlas_width as f32;
        let ah: f32 = page_height as f32;

        for p in placements.iter().filter(|p| p.page == page_idx) {
            let rs: &RenderedShpSprite = &sprites[p.idx];
            let w: u32 = rs.width;
            let h: u32 = rs.height;

            // Blit sprite RGBA into page buffer.
            for y in 0..h {
                let src_start: usize = (y * w * 4) as usize;
                let src_end: usize = src_start + (w * 4) as usize;
                let dst_start: usize = (((p.py + y) * atlas_width + p.px) * 4) as usize;
                let dst_end: usize = dst_start + (w * 4) as usize;
                if src_end <= rs.rgba.len() && dst_end <= rgba.len() {
                    rgba[dst_start..dst_end].copy_from_slice(&rs.rgba[src_start..src_end]);
                }
            }

            entries.insert(
                rs.key.clone(),
                ShpSpriteEntry {
                    uv_origin: [p.px as f32 / aw, p.py as f32 / ah],
                    uv_size: [w as f32 / aw, h as f32 / ah],
                    pixel_size: [w as f32, h as f32],
                    offset_x: rs.offset_x,
                    offset_y: rs.offset_y,
                    page: page_idx,
                },
            );
        }

        let texture: BatchTexture = batch.create_texture(gpu, &rgba, atlas_width, page_height);
        pages.push(SpriteAtlasPage { texture });
    }

    SpriteAtlas {
        pages,
        entries,
        make_frame_counts: HashMap::new(),
        active_anim_frame_counts: HashMap::new(),
        building_bounds: HashMap::new(),
        rendered_cache: Vec::new(), // caller sets this after packing
    }
}

/// Simulate shelf-packing to determine total height without allocating buffers.
fn simulate_shelf_height(
    indices: &[usize],
    sprites: &[RenderedShpSprite],
    atlas_width: u32,
) -> u32 {
    let mut cursor_x: u32 = 0;
    let mut cursor_y: u32 = 0;
    let mut shelf_height: u32 = 0;
    for &idx in indices {
        let w: u32 = sprites[idx].width;
        let h: u32 = sprites[idx].height;
        if cursor_x + w > atlas_width {
            cursor_y += shelf_height + SPRITE_PADDING;
            cursor_x = 0;
            shelf_height = 0;
        }
        cursor_x += w + SPRITE_PADDING;
        shelf_height = shelf_height.max(h);
    }
    cursor_y + shelf_height
}

#[cfg(test)]
#[path = "sprite_atlas_tests.rs"]
mod tests;
