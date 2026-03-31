//! Theater system: maps tile_id → TMP filenames and loads tile images.
//!
//! Each RA2 map specifies a theater (temperate, snow, urban) which determines
//! the terrain tileset (.tmp files) and palette. Theater INI files define
//! numbered tilesets ([TileSet0000], [TileSet0001], ...) with filename prefix
//! and count. The global tile_id from IsoMapPack5 is a cumulative index.
//!
//! ## Dependency rules
//! - Part of map/ — depends on assets/, rules/ (for INI parsing).
//! - Does NOT depend on render/ or sim/.

use std::collections::{HashMap, HashSet};

use crate::assets::asset_manager::AssetManager;
use crate::assets::pal_file::Palette;
use crate::assets::tmp_file::TmpFile;
use crate::map::map_file::MapError;
use crate::rules::ini_parser::{IniFile, IniSection};

/// Marker for "no tile" in IsoMapPack5 data.
/// The raw field is i32; -1 (0xFFFFFFFF) means clear ground.
/// We also treat the legacy u16 0xFFFF as no-tile for compatibility.
pub const NO_TILE: i32 = -1;

/// Identifies a specific sub-tile within a TMP template, including variant.
/// Used as a key for atlas lookups. Variant 0 = main tile; 1-4 = visual
/// replacements loaded from `{name}a.{ext}` through `{name}d.{ext}`.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct TileKey {
    pub tile_id: u16,
    pub sub_tile: u8,
    /// Visual replacement index: 0 = main tile, 1-4 = variant a-d.
    pub variant: u8,
}

/// RGBA pixel data for a single rendered tile, before atlas packing.
///
/// For tiles with extra data (cliff faces, shores), width/height include the
/// extra region. offset_x/offset_y indicate where the standard 60×30 diamond
/// origin sits within this enlarged buffer.
pub struct TileImage {
    pub rgba: Vec<u8>,
    /// Per-pixel Z-depth from TMP file (same dimensions as rgba, one byte per pixel).
    /// Non-zero values indicate depth offset for occlusion (cliffs, ramps).
    /// Flat tiles have all zeros.
    pub depth: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// X offset of diamond origin within this buffer (negative = extra data extends left).
    pub offset_x: i32,
    /// Y offset of diamond origin within this buffer (negative = extra data extends above).
    pub offset_y: i32,
}

/// Static definition for a theater.
struct TheaterDef {
    /// INI filenames to try (YR md variant first, base RA2 fallback).
    ini_names: &'static [&'static str],
    /// File extension for TMP files (e.g., "tem" for temperate).
    extension: &'static str,
    /// Palette filenames to try (iso palette first, then unit, then generic).
    palette_names: &'static [&'static str],
    /// Unit palette filenames to try (theater-specific unit palette).
    unit_palette_names: &'static [&'static str],
    /// Tiberium palette filenames to try (used for ore/gem overlays).
    /// Tiberium uses a separate theater palette: temperat.pal, snow.pal, etc.
    tiberium_palette_names: &'static [&'static str],
    /// Theater-specific MIX archives to load for highest priority file access.
    /// YR md variants first, then base RA2 variants. Loaded via load_nested().
    mix_archives: &'static [&'static str],
}

/// All known theater definitions. First match wins for INI/palette lookup.
const THEATER_DEFS: &[(&str, TheaterDef)] = &[
    // Try YR (md) INI first — YR maps use YR tileset indices.
    // Fall back to base RA2 INI if md version not found.
    (
        "TEMPERATE",
        TheaterDef {
            ini_names: &["temperatmd.ini", "temperat.ini"],
            extension: "tem",
            palette_names: &["isotem.pal", "temperat.pal"],
            unit_palette_names: &["unittem.pal", "unit.pal"],
            tiberium_palette_names: &["temperat.pal", "isotem.pal"],
            mix_archives: &["isotemmd.mix", "isotemp.mix", "tem.mix", "temperat.mix"],
        },
    ),
    (
        "SNOW",
        TheaterDef {
            ini_names: &["snowmd.ini", "snow.ini"],
            extension: "sno",
            palette_names: &["isosno.pal", "snow.pal"],
            unit_palette_names: &["unitsno.pal", "unit.pal"],
            tiberium_palette_names: &["snow.pal", "isosno.pal"],
            mix_archives: &["isosnowmd.mix", "isosnow.mix", "sno.mix", "snow.mix"],
        },
    ),
    (
        "URBAN",
        TheaterDef {
            ini_names: &["urbanmd.ini", "urban.ini"],
            extension: "urb",
            palette_names: &["isourb.pal", "urban.pal"],
            unit_palette_names: &["uniturb.pal", "unit.pal"],
            tiberium_palette_names: &["urban.pal", "isourb.pal"],
            mix_archives: &["isourbnmd.mix", "isourb.mix", "urb.mix", "urban.mix"],
        },
    ),
];

/// Start tile_id and count for one tileset section (e.g., [TileSet0013]).
#[derive(Debug, Clone, Copy)]
pub struct TilesetBounds {
    /// First tile_id belonging to this tileset.
    pub start: u16,
    /// Number of tiles in this tileset.
    pub count: u16,
}

/// Maps tile_id → TMP filename. Built by parsing a theater INI file.
pub struct TilesetLookup {
    /// tile_id → TMP filename (e.g., "clear01.tem"). None = blank/empty tileset.
    entries: Vec<Option<String>>,
    /// tile_id → variant TMP filenames (e.g., ["clear01a.tem", "clear01b.tem"]).
    /// FA2 loads up to 4 visual replacements per tile by inserting 'a'-'d' before
    /// the extension. Empty vec = no variants for that tile_id.
    variant_filenames: Vec<Vec<String>>,
    /// Tileset index → bounds (start tile_id and count).
    /// Index 0 corresponds to [TileSet0000], etc.
    tileset_bounds: Vec<TilesetBounds>,
    /// Tileset index → SetName from theater INI (e.g., "Water", "Cliffs", "Grass").
    /// Used to classify tiles for walkability (water/cliff detection).
    set_names: Vec<String>,
}

impl TilesetLookup {
    /// Get the TMP filename for a given tile index.
    /// Returns None for NO_TILE (-1), out-of-range, or blank tilesets.
    pub fn filename(&self, tile_index: i32) -> Option<&str> {
        if tile_index < 0 {
            return None;
        }
        self.entries
            .get(tile_index as usize)
            .and_then(|opt| opt.as_deref())
    }

    /// Total number of tile_id slots (including blanks).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Get the tileset bounds table (tileset_index → start, count).
    pub fn bounds(&self) -> &[TilesetBounds] {
        &self.tileset_bounds
    }

    /// Find which tileset index a tile_id belongs to.
    /// Returns None for negative or out-of-range tile IDs.
    pub fn tileset_index(&self, tile_id: u16) -> Option<u16> {
        for (idx, b) in self.tileset_bounds.iter().enumerate() {
            if tile_id >= b.start && tile_id < b.start + b.count {
                return Some(idx as u16);
            }
        }
        None
    }

    /// Get the SetName for a tileset index (e.g., "Rough", "Cliff Set").
    pub fn set_name(&self, tileset_idx: u16) -> Option<&str> {
        self.set_names.get(tileset_idx as usize).map(|s| s.as_str())
    }

    /// Check if a tile belongs to a water tileset (impassable for ground units).
    ///
    /// Looks up the tileset's SetName from the theater INI and checks if it
    /// contains "Water" (case-insensitive). This covers tilesets named
    /// "Water", "Water Cliffs", "Water Bridge", etc.
    pub fn is_water(&self, tile_id: u16) -> bool {
        let idx: u16 = match self.tileset_index(tile_id) {
            Some(i) => i,
            None => return false,
        };
        if let Some(name) = self.set_names.get(idx as usize) {
            let lower: String = name.to_ascii_lowercase();
            lower.contains("water")
        } else {
            false
        }
    }

    /// Check if a tile belongs to a cliff tileset (impassable for ground units).
    ///
    /// Looks up the tileset's SetName and checks for "Cliff" (case-insensitive).
    /// Note: some cliffs are passable ramps — this is a conservative check.
    /// Number of visual replacement variants for a tile_id (0 = no variants).
    /// FA2 loads up to 4 variants per TMP: {base}a.{ext} through {base}d.{ext}.
    pub fn variant_count(&self, tile_id: u16) -> u8 {
        self.variant_filenames
            .get(tile_id as usize)
            .map(|v| v.len() as u8)
            .unwrap_or(0)
    }

    /// Get the variant TMP filenames for a tile_id (may be empty).
    pub fn variant_filenames(&self, tile_id: u16) -> &[String] {
        self.variant_filenames
            .get(tile_id as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn is_cliff(&self, tile_id: u16) -> bool {
        let idx: u16 = match self.tileset_index(tile_id) {
            Some(i) => i,
            None => return false,
        };
        if let Some(name) = self.set_names.get(idx as usize) {
            let lower: String = name.to_ascii_lowercase();
            lower.contains("cliff")
        } else {
            false
        }
    }
}

/// Parse a theater INI file into a TilesetLookup.
///
/// Iterates [TileSet0000], [TileSet0001], ... sections in order.
/// Each section's `TilesInSet` (default 1) advances the global tile_id counter.
/// The filename is `{FileName}{NN:02}.{extension}` where NN is 1-indexed.
/// Blank FileName entries consume tile_id slots but map to None.
pub fn parse_tileset_ini(ini_data: &[u8], extension: &str) -> Result<TilesetLookup, MapError> {
    let ini: IniFile = IniFile::from_bytes(ini_data).map_err(MapError::Ini)?;

    let mut entries: Vec<Option<String>> = Vec::new();
    let mut variant_filenames: Vec<Vec<String>> = Vec::new();
    let mut tileset_bounds: Vec<TilesetBounds> = Vec::new();
    let mut set_names: Vec<String> = Vec::new();

    // Iterate tileset sections in numerical order: TileSet0000, TileSet0001, ...
    for idx in 0..10000u32 {
        let section_name: String = format!("TileSet{:04}", idx);
        let section: &IniSection = match ini.section(&section_name) {
            Some(s) => s,
            None => break, // No more tilesets.
        };

        let filename: &str = section.get("FileName").unwrap_or("");
        let set_name: &str = section.get("SetName").unwrap_or("");
        let raw_tiles: Option<&str> = section.get("TilesInSet");
        // TilesInSet=0 means empty tileset — consume 0 tile_id slots.
        // Do NOT clamp to 1; that shifts all subsequent tile indices.
        let tiles_in_set: u32 = section.get_i32("TilesInSet").unwrap_or(0).max(0) as u32;

        let start: u16 = entries.len() as u16;
        tileset_bounds.push(TilesetBounds {
            start,
            count: tiles_in_set as u16,
        });
        set_names.push(set_name.to_string());

        // Diagnostic: log ALL tileset raw TilesInSet values for debugging.
        log::debug!(
            "  TileSet{:04} raw_TilesInSet={:?} parsed={} start={} file={} name={}",
            idx,
            raw_tiles,
            tiles_in_set,
            start,
            filename,
            set_name
        );

        if filename.is_empty() {
            // Blank tileset — consume slots but produce None entries.
            for _ in 0..tiles_in_set {
                entries.push(None);
                variant_filenames.push(Vec::new());
            }
        } else {
            // Each tile is named {prefix}{NN:02}.{ext}, 1-indexed.
            // FA2 also loads up to 4 replacement TMP files per tile by inserting
            // 'a'-'d' before the extension: clear01a.tem, clear01b.tem, etc.
            // (Loading.cpp:4499-4520). We generate the candidate names here;
            // actual existence is checked at image load time.
            for i in 1..=tiles_in_set {
                let main_name = format!("{}{:02}.{}", filename, i, extension);
                let variants: Vec<String> = ['a', 'b', 'c', 'd']
                    .iter()
                    .map(|c| format!("{}{:02}{}.{}", filename, i, c, extension))
                    .collect();
                entries.push(Some(main_name));
                variant_filenames.push(variants);
            }
        }
    }

    // Diagnostic: log first 15 tilesets for debugging tile mapping.
    for (idx, (bounds, name)) in tileset_bounds
        .iter()
        .zip(set_names.iter())
        .enumerate()
        .take(15)
    {
        let fname: &str = entries
            .get(bounds.start as usize)
            .and_then(|o| o.as_deref())
            .unwrap_or("(blank)");
        log::info!(
            "  TileSet{:04}: start={:4}, count={:3}, name={:20} file={}",
            idx,
            bounds.start,
            bounds.count,
            name,
            fname,
        );
    }
    log::info!(
        "  ... {} total tilesets, {} total tile_id slots",
        tileset_bounds.len(),
        entries.len()
    );

    Ok(TilesetLookup {
        entries,
        variant_filenames,
        tileset_bounds,
        set_names,
    })
}

/// Look up the theater definition for a theater name (e.g., "TEMPERATE").
fn theater_def(name: &str) -> Option<&'static TheaterDef> {
    let upper: String = name.to_ascii_uppercase();
    THEATER_DEFS
        .iter()
        .find(|(n, _)| *n == upper)
        .map(|(_, def)| def)
}

/// Result of loading theater data: tilesets, palettes, extension, raw INI bytes.
pub struct TheaterData {
    pub lookup: TilesetLookup,
    /// Isometric terrain palette (for tile rendering).
    pub iso_palette: Palette,
    /// Unit palette (for sprites on this theater).
    pub unit_palette: Palette,
    /// Tiberium palette (for ore/gem overlays). Falls back to iso_palette if not found.
    pub tiberium_palette: Palette,
    /// File extension for TMP files (e.g., "tem").
    pub extension: &'static str,
    /// Raw INI bytes (needed by LAT config parsing).
    pub ini_data: Vec<u8>,
    /// Tileset index for concrete/stone bridgehead tiles (BridgeSet= in theater INI).
    pub bridge_set: Option<u16>,
    /// Tileset index for wooden bridgehead tiles (WoodBridgeSet= in theater INI).
    pub wood_bridge_set: Option<u16>,
}

/// Load tileset data for a theater.
///
/// Loads theater-specific MIX archives (e.g., isotemmd.mix) at highest priority,
/// then parses the theater INI for tileset definitions and loads palettes.
/// The AssetManager is mutable because theater MIX archives are loaded on demand.
pub fn load_theater(asset_manager: &mut AssetManager, theater_name: &str) -> Option<TheaterData> {
    let def: &TheaterDef = theater_def(theater_name)?;

    // Load theater-specific MIX archives at highest priority.
    // These contain the .tmp terrain tiles and theater-specific SHP sprites.
    for &mix_name in def.mix_archives {
        match asset_manager.load_nested(mix_name) {
            Ok(()) => log::info!("Theater {}: loaded MIX '{}'", theater_name, mix_name),
            Err(_) => log::debug!(
                "Theater {}: MIX '{}' not found (optional)",
                theater_name,
                mix_name
            ),
        }
    }

    // Find the theater INI file (try each name in order, log which one matched).
    let mut ini_data: Option<Vec<u8>> = None;
    let mut ini_name: &str = "";
    for &name in def.ini_names {
        if let Some((data, source)) = asset_manager.get_with_source(name) {
            log::info!("Theater {}: INI '{}' from {}", theater_name, name, source);
            ini_name = name;
            ini_data = Some(data);
            break;
        }
    }
    let ini_data: Vec<u8> = ini_data?;

    let lookup: TilesetLookup = parse_tileset_ini(&ini_data, def.extension).ok()?;
    log::info!(
        "Theater {}: loaded {} from INI '{}' ({} tile_id slots, {} tilesets)",
        theater_name,
        def.extension,
        ini_name,
        lookup.len(),
        lookup.bounds().len()
    );

    // Find the iso palette (for terrain tile rendering).
    let iso_palette: Palette = find_palette(asset_manager, def.palette_names, theater_name, "iso")?;

    // Find the unit palette (for unit/overlay sprites on this theater).
    let unit_palette: Palette =
        find_palette(asset_manager, def.unit_palette_names, theater_name, "unit")?;

    // Find the tiberium palette (for ore/gem overlays).
    // Tiberium uses a dedicated palette (e.g., temperat.pal) distinct from the unit palette.
    // Fall back to iso palette if the dedicated tiberium palette is not found.
    let tiberium_palette: Palette = find_palette(
        asset_manager,
        def.tiberium_palette_names,
        theater_name,
        "tiberium",
    )
    .unwrap_or_else(|| {
        log::warn!(
            "Theater {}: tiberium palette not found, falling back to iso palette",
            theater_name
        );
        iso_palette.clone()
    });

    // Parse BridgeSet and WoodBridgeSet from the theater INI global section.
    // These are at the top of the file before any [TileSet...] section, so
    // the IniFile parser skips them. Parse directly from the raw text.
    let ini_text = String::from_utf8_lossy(&ini_data);
    let bridge_set = parse_general_int(&ini_text, "BridgeSet");
    let wood_bridge_set = parse_general_int(&ini_text, "WoodBridgeSet");
    if bridge_set.is_some() || wood_bridge_set.is_some() {
        log::info!(
            "Theater {}: BridgeSet={:?}, WoodBridgeSet={:?}",
            theater_name,
            bridge_set,
            wood_bridge_set,
        );
    }

    Some(TheaterData {
        lookup,
        iso_palette,
        unit_palette,
        tiberium_palette,
        extension: def.extension,
        ini_data,
        bridge_set,
        wood_bridge_set,
    })
}

/// Parse a key=value integer from the `[General]` section of a theater INI file.
/// BridgeSet and WoodBridgeSet are defined inside `[General]`, not in the
/// global scope before any section header.
fn parse_general_int(text: &str, key: &str) -> Option<u16> {
    let mut in_general = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            if line.to_ascii_lowercase().starts_with("[general]") {
                in_general = true;
                continue;
            } else if in_general {
                // Left [General], entered another section — stop.
                break;
            }
            continue;
        }
        if !in_general {
            continue;
        }
        if line.starts_with(';') || line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim().eq_ignore_ascii_case(key) {
                let v = v.split(';').next().unwrap_or("").trim();
                return v.parse().ok();
            }
        }
    }
    None
}

/// Try palette filenames in order, returning the first valid palette found.
fn find_palette(
    asset_manager: &AssetManager,
    names: &[&str],
    theater_name: &str,
    palette_kind: &str,
) -> Option<Palette> {
    for &name in names {
        if let Some((data, source)) = asset_manager.get_with_source(name) {
            if let Ok(pal) = Palette::from_bytes(&data) {
                log::info!(
                    "Theater {}: {} palette '{}' from {}",
                    theater_name,
                    palette_kind,
                    name,
                    source
                );
                return Some(pal);
            }
        }
    }
    log::warn!(
        "Theater {}: no {} palette found",
        theater_name,
        palette_kind
    );
    None
}

/// Collect all unique TileKey values used by a terrain grid.
/// Filters out NO_TILE (-1) and negative indices, truncates to u16 for TileKey.
pub fn collect_used_tiles(cells: &[(i32, u8)]) -> HashSet<TileKey> {
    cells
        .iter()
        .filter(|(id, _)| *id >= 0)
        .map(|&(tile_index, sub_tile)| TileKey {
            variant: 0,
            tile_id: tile_index as u16,
            sub_tile,
        })
        .collect()
}

/// Load RGBA tile images for tiles needed by the map. Groups by tile_id
/// to batch-load TMP files. Skips missing or unparseable TMP files.
pub fn load_tile_images(
    asset_manager: &AssetManager,
    lookup: &TilesetLookup,
    palette: &Palette,
    needed: &HashSet<TileKey>,
) -> HashMap<TileKey, TileImage> {
    let mut images: HashMap<TileKey, TileImage> = HashMap::new();
    let mut blank_slot_count: u32 = 0;
    let mut out_of_range_count: u32 = 0;
    let mut missing_file_count: u32 = 0;
    let mut empty_cell_count: u32 = 0;
    let mut parse_error_count: u32 = 0;

    // Group needed tiles by tile_id to batch-load TMP files.
    let mut by_tile_id: HashMap<u16, Vec<u8>> = HashMap::new();
    for key in needed {
        by_tile_id
            .entry(key.tile_id)
            .or_default()
            .push(key.sub_tile);
    }

    for (tile_id, sub_tiles) in &by_tile_id {
        let tile_id_i32: i32 = *tile_id as i32;

        // Distinguish out-of-range from blank slots.
        if tile_id_i32 >= lookup.len() as i32 {
            out_of_range_count += sub_tiles.len() as u32;
            continue;
        }

        let filename: &str = match lookup.filename(tile_id_i32) {
            Some(f) => f,
            None => {
                blank_slot_count += sub_tiles.len() as u32;
                continue;
            }
        };

        let tmp_data: &[u8] = match asset_manager.get_ref(filename) {
            Some(d) => d,
            None => {
                missing_file_count += sub_tiles.len() as u32;
                log::trace!("TMP not found: {} (tile_id {})", filename, tile_id);
                continue;
            }
        };

        let tmp: TmpFile = match TmpFile::from_bytes(tmp_data) {
            Ok(t) => t,
            Err(e) => {
                parse_error_count += sub_tiles.len() as u32;
                log::warn!("TMP parse error {}: {:#}", filename, e);
                continue;
            }
        };

        // Log tile dimensions from first successfully parsed TMP file.
        if images.is_empty() {
            log::info!(
                "First TMP '{}': tile_width={}, tile_height={}, template={}x{}",
                filename,
                tmp.tile_width,
                tmp.tile_height,
                tmp.template_width,
                tmp.template_height,
            );
        }

        let cell_count: usize = (tmp.template_width * tmp.template_height) as usize;

        for &sub in sub_tiles {
            if (sub as usize) >= cell_count {
                empty_cell_count += 1;
                continue;
            }

            let Some(tile) = tmp.tiles[sub as usize].as_ref() else {
                empty_cell_count += 1;
                continue;
            };
            match tmp.tile_to_rgba(sub as usize, palette) {
                Ok(rgba) => {
                    images.insert(
                        TileKey {
                            tile_id: *tile_id,
                            sub_tile: sub,
                            variant: 0,
                        },
                        TileImage {
                            rgba,
                            depth: tile.depth.clone(),
                            width: tile.pixel_width,
                            height: tile.pixel_height,
                            offset_x: tile.offset_x,
                            offset_y: tile.offset_y,
                        },
                    );
                }
                Err(e) => {
                    log::warn!("RGBA convert error {} sub {}: {:#}", filename, sub, e);
                }
            }
        }
    }

    // Load variant TMP files (FA2 replacements: {base}a.{ext} through {base}d.{ext}).
    // Each variant that exists in the MIX archive gets its own TileKey with variant=1..4.
    let mut variant_count: u32 = 0;
    for (tile_id, sub_tiles) in &by_tile_id {
        let var_names = lookup.variant_filenames(*tile_id);
        if var_names.is_empty() {
            continue;
        }
        for (var_idx, var_name) in var_names.iter().enumerate() {
            let Some(var_data) = asset_manager.get_ref(var_name) else {
                break; // Stop at first missing variant (same as FA2)
            };
            let Ok(var_tmp) = TmpFile::from_bytes(var_data) else {
                break;
            };
            let var_cell_count = (var_tmp.template_width * var_tmp.template_height) as usize;
            for &sub in sub_tiles {
                if (sub as usize) >= var_cell_count {
                    continue;
                }
                let Some(tile) = var_tmp.tiles[sub as usize].as_ref() else {
                    continue;
                };
                if let Ok(rgba) = var_tmp.tile_to_rgba(sub as usize, palette) {
                    images.insert(
                        TileKey {
                            tile_id: *tile_id,
                            sub_tile: sub,
                            variant: (var_idx + 1) as u8,
                        },
                        TileImage {
                            rgba,
                            depth: tile.depth.clone(),
                            width: tile.pixel_width,
                            height: tile.pixel_height,
                            offset_x: tile.offset_x,
                            offset_y: tile.offset_y,
                        },
                    );
                    variant_count += 1;
                }
            }
        }
    }

    log::info!(
        "Tile loading: {} loaded ({} variants), {} empty cells (expected), {} blank slots, \
         {} missing files, {} out-of-range, {} parse errors (of {} needed)",
        images.len(),
        variant_count,
        empty_cell_count,
        blank_slot_count,
        missing_file_count,
        out_of_range_count,
        parse_error_count,
        needed.len()
    );

    images
}

// Tests extracted to map/theater_tests.rs to stay under 400 lines.
#[cfg(test)]
#[path = "theater_tests.rs"]
mod theater_tests;
