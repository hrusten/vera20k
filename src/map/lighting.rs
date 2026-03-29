//! Map lighting — parses [Lighting] section and computes per-cell RGB tint.
//!
//! RA2 maps define global lighting parameters (Ambient, Red, Green, Blue, Ground,
//! Level) in their INI [Lighting] section. These determine a per-cell color
//! multiplier that tints terrain tiles and entity sprites.
//!
//! Point light sources (lamp posts, buildings with LightVisibility) add localized
//! brightness using linear falloff: `contribution = ((range - distance) / range) * intensity`.
//! This matches the original engine's point light calculation.
//!
//! ## Dependency rules
//! - Part of map/ — depends on rules/ini_parser for IniFile, map/entities for MapEntity.

use std::collections::HashMap;

use crate::map::entities::{EntityCategory, MapEntity};
use crate::map::map_file::MapCell;
use crate::rules::art_data::ArtRegistry;
use crate::rules::ini_parser::IniFile;
use crate::rules::ruleset::RuleSet;

/// Maximum combined lighting value per channel.
pub const TOTAL_AMBIENT_CAP: f32 = 2.0;

/// Leptons per cell in RA2's coordinate system.
const LEPTONS_PER_CELL: f32 = 256.0;

/// Default white tint (no lighting effect).
pub const DEFAULT_TINT: [f32; 3] = [1.0, 1.0, 1.0];

/// Global lighting parameters from the map's [Lighting] INI section.
#[derive(Debug, Clone)]
pub struct LightingConfig {
    /// Base brightness level (default 1.0).
    pub ambient: f32,
    /// Red channel multiplier (default 1.0).
    pub red: f32,
    /// Green channel multiplier (default 1.0).
    pub green: f32,
    /// Blue channel multiplier (default 1.0).
    pub blue: f32,
    /// Darkening factor: ambient is multiplied by (1.0 - ground). Default 0.0.
    pub ground: f32,
    /// Height-based ambient boost per elevation level. Default 0.032.
    pub level: f32,
}

impl Default for LightingConfig {
    fn default() -> Self {
        Self {
            ambient: 1.0,
            red: 1.0,
            green: 1.0,
            blue: 1.0,
            ground: 0.0,
            level: 0.032,
        }
    }
}

/// Per-cell RGB tint grid: (rx, ry) → [r, g, b] multiplier.
pub type LightingGrid = HashMap<(u16, u16), [f32; 3]>;

/// Parse [Lighting] section from a map INI file.
pub fn parse_lighting(ini: &IniFile) -> LightingConfig {
    let section = match ini.section("Lighting") {
        Some(s) => s,
        None => return LightingConfig::default(),
    };
    LightingConfig {
        ambient: section.get_f32("Ambient").unwrap_or(1.0),
        red: section.get_f32("Red").unwrap_or(1.0),
        green: section.get_f32("Green").unwrap_or(1.0),
        blue: section.get_f32("Blue").unwrap_or(1.0),
        ground: section.get_f32("Ground").unwrap_or(0.0),
        level: section.get_f32("Level").unwrap_or(0.032),
    }
}

/// Compute the RGB tint for a single cell given its elevation.
pub fn cell_tint(config: &LightingConfig, z: u8) -> [f32; 3] {
    let cell_ambient: f32 = config.ambient * (1.0 - config.ground) + config.level * z as f32;
    let mut r: f32 = config.red * cell_ambient;
    let mut g: f32 = config.green * cell_ambient;
    let mut b: f32 = config.blue * cell_ambient;

    // Cap: if any channel exceeds TOTAL_AMBIENT_CAP, scale all down proportionally.
    let max_val: f32 = r.max(g).max(b);
    if max_val > TOTAL_AMBIENT_CAP {
        let scale: f32 = TOTAL_AMBIENT_CAP / max_val;
        r *= scale;
        g *= scale;
        b *= scale;
    }

    [r, g, b]
}

/// Compute the uniform terrain tint for the map.
///
/// Terrain uses the ground-level lighting value across all cells so repeating
/// tile textures do not expose the map grid through per-cell tint boundaries.
pub fn terrain_tint(config: &LightingConfig) -> [f32; 3] {
    cell_tint(config, 0)
}

/// Build a per-cell lighting tint grid from map INI and cell data.
///
/// Returns a HashMap keyed by (rx, ry) with RGB tint values for each cell.
pub fn build_lighting_grid(ini: &IniFile, cells: &[MapCell]) -> LightingGrid {
    let config: LightingConfig = parse_lighting(ini);
    log::info!(
        "Lighting: ambient={:.2} R={:.2} G={:.2} B={:.2} ground={:.2} level={:.3}",
        config.ambient,
        config.red,
        config.green,
        config.blue,
        config.ground,
        config.level,
    );

    let mut grid: LightingGrid = HashMap::with_capacity(cells.len());
    for cell in cells {
        let tint: [f32; 3] = cell_tint(&config, cell.z);
        grid.insert((cell.rx, cell.ry), tint);
    }

    grid
}

/// A point light source placed on the map (lamp post, lit building, etc.).
///
/// Created from map entity data during map load. Each light contributes
/// localized brightness to nearby cells using linear distance falloff.
#[derive(Debug, Clone)]
pub struct PointLight {
    /// Cell position of the light source.
    pub rx: u16,
    pub ry: u16,
    /// Visibility range in cells (LightVisibility / 256).
    pub range_cells: f32,
    /// Brightness intensity (LightIntensity). Can be negative for darkening.
    pub intensity: f32,
    /// RGB tint color [LightRedTint, LightGreenTint, LightBlueTint].
    pub tint: [f32; 3],
}

/// Collect point light sources from map-placed buildings with LightVisibility > 0.
///
/// Iterates all structure entities on the map and checks their ObjectType
/// for light emission properties parsed from rules.ini.
pub fn collect_building_lights(entities: &[MapEntity], rules: Option<&RuleSet>) -> Vec<PointLight> {
    let Some(rules) = rules else {
        return Vec::new();
    };
    let mut lights = Vec::new();
    for ent in entities {
        if ent.category != EntityCategory::Structure {
            continue;
        }
        let Some(obj) = rules.object(&ent.type_id) else {
            continue;
        };
        if obj.light_visibility <= 0 || obj.light_intensity == 0.0 {
            continue;
        }
        lights.push(PointLight {
            rx: ent.cell_x,
            ry: ent.cell_y,
            range_cells: obj.light_visibility as f32 / LEPTONS_PER_CELL,
            intensity: obj.light_intensity,
            tint: [
                obj.light_red_tint,
                obj.light_green_tint,
                obj.light_blue_tint,
            ],
        });
    }
    lights
}

/// Accumulate point light contributions into an existing LightingGrid.
///
/// For each light source, iterates cells within its range and adds the
/// attenuated contribution. Uses RA2's linear falloff formula:
///   `factor = ((range - distance) / range) * intensity`
/// Distance is Euclidean in cell coordinates. Clamped to [0, TOTAL_AMBIENT_CAP].
pub fn accumulate_point_lights(grid: &mut LightingGrid, lights: &[PointLight]) {
    for light in lights {
        let range = light.range_cells;
        let range_i = range.ceil() as i32;
        let cx = light.rx as i32;
        let cy = light.ry as i32;

        for dy in -range_i..=range_i {
            for dx in -range_i..=range_i {
                let nx = cx + dx;
                let ny = cy + dy;
                if nx < 0 || ny < 0 {
                    continue;
                }
                let key = (nx as u16, ny as u16);
                let Some(tint) = grid.get_mut(&key) else {
                    continue;
                };
                let dist = ((dx * dx + dy * dy) as f32).sqrt();
                if dist >= range {
                    continue;
                }
                let factor = (range - dist) / range * light.intensity;
                tint[0] = (tint[0] + factor * light.tint[0]).clamp(0.0, TOTAL_AMBIENT_CAP);
                tint[1] = (tint[1] + factor * light.tint[1]).clamp(0.0, TOTAL_AMBIENT_CAP);
                tint[2] = (tint[2] + factor * light.tint[2]).clamp(0.0, TOTAL_AMBIENT_CAP);
            }
        }
    }
}

/// Apply ExtraLight contributions from art.ini to the lighting grid.
///
/// ExtraLight is a flat brightness adjustment applied to a building's own cell.
/// Positive values brighten, negative values darken. Scale: 1000 ≈ 1.0 brightness.
pub fn apply_extra_light(
    grid: &mut LightingGrid,
    entities: &[MapEntity],
    art: Option<&ArtRegistry>,
    rules: Option<&RuleSet>,
) {
    let Some(art) = art else { return };
    let Some(rules) = rules else { return };
    for ent in entities {
        if ent.category != EntityCategory::Structure {
            continue;
        }
        // Resolve art section: use Image= override from rules if present, else type_id.
        let image_key = rules
            .object(&ent.type_id)
            .map(|obj| {
                if obj.image.trim().is_empty() {
                    ent.type_id.to_ascii_uppercase()
                } else {
                    obj.image.to_ascii_uppercase()
                }
            })
            .unwrap_or_else(|| ent.type_id.to_ascii_uppercase());
        let Some(art_entry) = art.get(&image_key) else {
            continue;
        };
        if art_entry.extra_light == 0 {
            continue;
        }
        let boost = art_entry.extra_light as f32 / 1000.0;
        let key = (ent.cell_x, ent.cell_y);
        if let Some(tint) = grid.get_mut(&key) {
            tint[0] = (tint[0] + boost).clamp(0.0, TOTAL_AMBIENT_CAP);
            tint[1] = (tint[1] + boost).clamp(0.0, TOTAL_AMBIENT_CAP);
            tint[2] = (tint[2] + boost).clamp(0.0, TOTAL_AMBIENT_CAP);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_lighting_produces_white() {
        let config: LightingConfig = LightingConfig::default();
        let tint: [f32; 3] = cell_tint(&config, 0);
        assert!((tint[0] - 1.0).abs() < 0.001);
        assert!((tint[1] - 1.0).abs() < 0.001);
        assert!((tint[2] - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_elevation_boost() {
        let config: LightingConfig = LightingConfig::default();
        let tint_z0: [f32; 3] = cell_tint(&config, 0);
        let tint_z4: [f32; 3] = cell_tint(&config, 4);
        // Level=0.032, z=4 → ambient boost = 0.128 → tint ≈ 1.128
        assert!(tint_z4[0] > tint_z0[0]);
        assert!((tint_z4[0] - 1.128).abs() < 0.01);
    }

    #[test]
    fn test_dark_map() {
        let config: LightingConfig = LightingConfig {
            ambient: 0.5,
            red: 1.0,
            green: 0.8,
            blue: 0.6,
            ground: 0.0,
            level: 0.0,
        };
        let tint: [f32; 3] = cell_tint(&config, 0);
        assert!((tint[0] - 0.5).abs() < 0.001);
        assert!((tint[1] - 0.4).abs() < 0.001);
        assert!((tint[2] - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_cap_at_two() {
        let config: LightingConfig = LightingConfig {
            ambient: 3.0,
            red: 1.0,
            green: 1.0,
            blue: 1.0,
            ground: 0.0,
            level: 0.0,
        };
        let tint: [f32; 3] = cell_tint(&config, 0);
        // 3.0 > 2.0 cap → scaled to 2.0
        assert!((tint[0] - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_ground_darkening() {
        let config: LightingConfig = LightingConfig {
            ambient: 1.0,
            red: 1.0,
            green: 1.0,
            blue: 1.0,
            ground: 0.5,
            level: 0.0,
        };
        let tint: [f32; 3] = cell_tint(&config, 0);
        // ambient * (1.0 - 0.5) = 0.5
        assert!((tint[0] - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_terrain_tint_matches_ground_level_cell_tint() {
        let config: LightingConfig = LightingConfig {
            ambient: 1.0,
            red: 1.0,
            green: 0.88,
            blue: 0.88,
            ground: 0.0,
            level: 0.039,
        };
        let terrain: [f32; 3] = terrain_tint(&config);
        let ground_cell: [f32; 3] = cell_tint(&config, 0);
        assert_eq!(terrain, ground_cell);
    }

    /// Helper: build a small grid with uniform tint for testing point lights.
    fn test_grid(size: u16, base_tint: [f32; 3]) -> LightingGrid {
        let mut grid = LightingGrid::new();
        for y in 0..size {
            for x in 0..size {
                grid.insert((x, y), base_tint);
            }
        }
        grid
    }

    #[test]
    fn test_point_light_linear_falloff() {
        let mut grid = test_grid(20, [0.5, 0.5, 0.5]);
        let light = PointLight {
            rx: 10,
            ry: 10,
            range_cells: 5.0,
            intensity: 1.0,
            tint: [1.0, 1.0, 1.0],
        };
        accumulate_point_lights(&mut grid, &[light]);

        // Center cell gets full intensity: 0.5 + 1.0 = 1.5
        let center = grid[&(10, 10)];
        assert!((center[0] - 1.5).abs() < 0.01, "center={:.3}", center[0]);

        // Cell at distance 2.5 gets (5-2.5)/5 = 0.5 intensity: 0.5 + 0.5 = 1.0
        // (2, 1.5 diagonal ≈ 2.5 cells — use (12, 10) which is distance 2.0)
        let d2 = grid[&(12, 10)];
        let expected_d2 = 0.5 + (5.0 - 2.0) / 5.0;
        assert!(
            (d2[0] - expected_d2).abs() < 0.01,
            "d2={:.3} expected={:.3}",
            d2[0],
            expected_d2
        );

        // Cell at distance 5+ is unchanged: still 0.5
        let far = grid[&(15, 10)];
        assert!((far[0] - 0.5).abs() < 0.01, "far={:.3}", far[0]);
    }

    #[test]
    fn test_point_light_colored_tint() {
        let mut grid = test_grid(10, [0.3, 0.3, 0.3]);
        let light = PointLight {
            rx: 5,
            ry: 5,
            range_cells: 3.0,
            intensity: 0.6,
            tint: [1.0, 0.0, 0.5], // Red light with half blue
        };
        accumulate_point_lights(&mut grid, &[light]);

        let center = grid[&(5, 5)];
        // Red: 0.3 + 0.6*1.0 = 0.9
        assert!((center[0] - 0.9).abs() < 0.01, "r={:.3}", center[0]);
        // Green: 0.3 + 0.6*0.0 = 0.3 (unchanged)
        assert!((center[1] - 0.3).abs() < 0.01, "g={:.3}", center[1]);
        // Blue: 0.3 + 0.6*0.5 = 0.6
        assert!((center[2] - 0.6).abs() < 0.01, "b={:.3}", center[2]);
    }

    #[test]
    fn test_accumulation_clamps_at_cap() {
        let mut grid = test_grid(5, [1.8, 1.8, 1.8]);
        let light = PointLight {
            rx: 2,
            ry: 2,
            range_cells: 3.0,
            intensity: 1.0,
            tint: [1.0, 1.0, 1.0],
        };
        accumulate_point_lights(&mut grid, &[light]);

        let center = grid[&(2, 2)];
        // 1.8 + 1.0 = 2.8 → clamped to 2.0
        assert!((center[0] - TOTAL_AMBIENT_CAP).abs() < 0.001);
    }

    #[test]
    fn test_extra_light_boost() {
        let mut grid = test_grid(5, [0.4, 0.4, 0.4]);
        // Simulate: cell (2,2) gets +350 extra light (0.35 brightness)
        let key = (2u16, 2u16);
        if let Some(tint) = grid.get_mut(&key) {
            let boost: f32 = 350.0 / 1000.0;
            tint[0] = (tint[0] + boost).clamp(0.0, TOTAL_AMBIENT_CAP);
            tint[1] = (tint[1] + boost).clamp(0.0, TOTAL_AMBIENT_CAP);
            tint[2] = (tint[2] + boost).clamp(0.0, TOTAL_AMBIENT_CAP);
        }
        let result = grid[&key];
        assert!((result[0] - 0.75).abs() < 0.01);
    }
}
