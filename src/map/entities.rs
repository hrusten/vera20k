//! Parser for map-placed entities: units, infantry, structures, and aircraft.
//!
//! RA2 maps store entity placements in four INI sections:
//! - `[Units]`: vehicles (14 comma-separated fields per line)
//! - `[Infantry]`: soldiers (14 fields, includes sub-cell position)
//! - `[Structures]`: buildings (17 fields, includes upgrades)
//! - [Aircraft]: air units (12 fields)
//!
//! Each line: `INDEX=OWNER,TYPE_ID,HEALTH,X,Y,...` with category-specific trailing fields.
//!
//! ## Dependency rules
//! - Part of map/ — depends on rules/ (IniFile/IniSection for parsing).

use crate::rules::ini_parser::IniFile;

/// Which category of game object this entity represents.
/// Determines rendering approach and available behaviors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum EntityCategory {
    /// Vehicles — rendered as VXL voxel models.
    Unit,
    /// Soldiers — rendered as SHP sprites, support sub-cell positioning.
    Infantry,
    /// Buildings — rendered as SHP sprites, have foundations.
    Structure,
    /// Air units — rendered as VXL voxel models, drawn above ground.
    Aircraft,
}

/// A single entity placement parsed from a map file.
///
/// Contains the minimum data needed to spawn an ECS entity.
/// Advanced fields (trigger tags, AI flags, upgrades) are not yet parsed
/// — they'll be added when trigger/AI systems are implemented.
#[derive(Debug, Clone)]
pub struct MapEntity {
    /// House/faction name (e.g., "Americans", "Soviet", "Neutral").
    pub owner: String,
    /// Object type ID from rules.ini (e.g., "HTNK", "E1", "GAPOWR").
    pub type_id: String,
    /// Health value (0–256, where 256 = 100% health).
    pub health: u16,
    /// Isometric cell X coordinate.
    pub cell_x: u16,
    /// Isometric cell Y coordinate.
    pub cell_y: u16,
    /// Facing direction (0–255, where 0=north, 64=east, 128=south, 192=west).
    pub facing: u8,
    /// Entity category (determines rendering and behavior).
    pub category: EntityCategory,
    /// Sub-cell position for infantry (0–4). Always 0 for other categories.
    pub sub_cell: u8,
    /// Veterancy level: 0=rookie, 100=veteran, 200=elite.
    pub veterancy: u16,
    /// Spawn on the bridge deck / high layer when the map placement marks it.
    pub high: bool,
}

/// Parse all entity placements from a map's INI data.
///
/// Reads [Units], [Infantry], [Structures], and [Aircraft] sections.
/// Malformed lines are skipped with a warning log. Returns an empty Vec
/// if none of these sections exist (e.g., empty skirmish maps).
pub fn parse_map_entities(ini: &IniFile) -> Vec<MapEntity> {
    let mut entities: Vec<MapEntity> = Vec::new();

    if let Some(section) = ini.section("Units") {
        parse_units_section(section, &mut entities);
    }
    if let Some(section) = ini.section("Infantry") {
        parse_infantry_section(section, &mut entities);
    }
    if let Some(section) = ini.section("Structures") {
        parse_structures_section(section, &mut entities);
    }
    if let Some(section) = ini.section("Aircraft") {
        parse_aircraft_section(section, &mut entities);
    }

    log::info!(
        "Parsed {} map entities ({} units, {} infantry, {} structures, {} aircraft)",
        entities.len(),
        entities
            .iter()
            .filter(|e| e.category == EntityCategory::Unit)
            .count(),
        entities
            .iter()
            .filter(|e| e.category == EntityCategory::Infantry)
            .count(),
        entities
            .iter()
            .filter(|e| e.category == EntityCategory::Structure)
            .count(),
        entities
            .iter()
            .filter(|e| e.category == EntityCategory::Aircraft)
            .count(),
    );

    entities
}

/// Parse [Units] section: INDEX=OWNER,ID,HEALTH,X,Y,FACING,MISSION,TAG,...
/// Minimum 6 fields needed (owner, id, health, x, y, facing).
fn parse_units_section(
    section: &crate::rules::ini_parser::IniSection,
    entities: &mut Vec<MapEntity>,
) {
    for key in section.keys() {
        let Some(value) = section.get(key) else {
            continue;
        };
        let fields: Vec<&str> = value.split(',').map(|s| s.trim()).collect();
        if fields.len() < 6 {
            log::warn!(
                "[Units] key {}: expected >= 6 fields, got {}",
                key,
                fields.len()
            );
            continue;
        }
        let Some(entity) = parse_common_fields(&fields, EntityCategory::Unit, key) else {
            continue;
        };
        entities.push(entity);
    }
}

/// Parse [Infantry] section: INDEX=OWNER,ID,HEALTH,X,Y,SUB_CELL,MISSION,FACING,...
/// Note: infantry has SUB_CELL at index 5 and FACING at index 7 (different from units).
fn parse_infantry_section(
    section: &crate::rules::ini_parser::IniSection,
    entities: &mut Vec<MapEntity>,
) {
    for key in section.keys() {
        let Some(value) = section.get(key) else {
            continue;
        };
        let fields: Vec<&str> = value.split(',').map(|s| s.trim()).collect();
        if fields.len() < 8 {
            log::warn!(
                "[Infantry] key {}: expected >= 8 fields, got {}",
                key,
                fields.len()
            );
            continue;
        }
        let owner: String = fields[0].to_string();
        let type_id: String = fields[1].to_string();
        let health: u16 = fields[2].parse::<u16>().unwrap_or(256).min(256);
        let Some(cell_x) = fields[3].parse::<u16>().ok() else {
            log::warn!("[Infantry] key {}: invalid X '{}'", key, fields[3]);
            continue;
        };
        let Some(cell_y) = fields[4].parse::<u16>().ok() else {
            log::warn!("[Infantry] key {}: invalid Y '{}'", key, fields[4]);
            continue;
        };
        let sub_cell: u8 = fields[5].parse::<u8>().unwrap_or(0).min(4);
        // Infantry facing is at field index 7 (after MISSION at index 6).
        let facing: u8 = if fields.len() > 7 {
            fields[7].parse::<u16>().unwrap_or(0).min(255) as u8
        } else {
            0
        };
        let veterancy: u16 = if fields.len() > 9 {
            fields[9].parse::<u16>().unwrap_or(0)
        } else {
            0
        };

        entities.push(MapEntity {
            owner,
            type_id,
            health,
            cell_x,
            cell_y,
            facing,
            category: EntityCategory::Infantry,
            sub_cell,
            veterancy,
            high: parse_boolish_field(fields.get(11).copied()),
        });
    }
}

/// Parse [Structures] section: INDEX=OWNER,ID,HEALTH,X,Y,FACING,TAG,...
/// Minimum 6 fields needed.
fn parse_structures_section(
    section: &crate::rules::ini_parser::IniSection,
    entities: &mut Vec<MapEntity>,
) {
    for key in section.keys() {
        let Some(value) = section.get(key) else {
            continue;
        };
        let fields: Vec<&str> = value.split(',').map(|s| s.trim()).collect();
        if fields.len() < 6 {
            log::warn!(
                "[Structures] key {}: expected >= 6 fields, got {}",
                key,
                fields.len()
            );
            continue;
        }
        let Some(entity) = parse_common_fields(&fields, EntityCategory::Structure, key) else {
            continue;
        };
        entities.push(entity);
    }
}

/// Parse [Aircraft] section: INDEX=OWNER,ID,HEALTH,X,Y,FACING,MISSION,TAG,...
/// Minimum 6 fields needed.
fn parse_aircraft_section(
    section: &crate::rules::ini_parser::IniSection,
    entities: &mut Vec<MapEntity>,
) {
    for key in section.keys() {
        let Some(value) = section.get(key) else {
            continue;
        };
        let fields: Vec<&str> = value.split(',').map(|s| s.trim()).collect();
        if fields.len() < 6 {
            log::warn!(
                "[Aircraft] key {}: expected >= 6 fields, got {}",
                key,
                fields.len()
            );
            continue;
        }
        let Some(entity) = parse_common_fields(&fields, EntityCategory::Aircraft, key) else {
            continue;
        };
        entities.push(entity);
    }
}

/// Parse the common fields shared by Units, Structures, and Aircraft.
///
/// Field layout: OWNER(0), ID(1), HEALTH(2), X(3), Y(4), FACING(5).
/// Veterancy at index 8 for units/aircraft, index 9 for structures — we try both.
fn parse_common_fields(fields: &[&str], category: EntityCategory, key: &str) -> Option<MapEntity> {
    let owner: String = fields[0].to_string();
    let type_id: String = fields[1].to_string();
    let health: u16 = fields[2].parse::<u16>().unwrap_or(256).min(256);

    let cell_x: u16 = match fields[3].parse::<u16>() {
        Ok(v) => v,
        Err(_) => {
            log::warn!("[{:?}] key {}: invalid X '{}'", category, key, fields[3]);
            return None;
        }
    };
    let cell_y: u16 = match fields[4].parse::<u16>() {
        Ok(v) => v,
        Err(_) => {
            log::warn!("[{:?}] key {}: invalid Y '{}'", category, key, fields[4]);
            return None;
        }
    };

    let facing: u8 = fields[5].parse::<u16>().unwrap_or(0).min(255) as u8;

    // Veterancy is at different indices depending on category.
    let vet_index: usize = match category {
        EntityCategory::Unit | EntityCategory::Aircraft => 8,
        EntityCategory::Structure => 8, // structures don't really have veterancy, but parse defensively
        EntityCategory::Infantry => 9,  // not used here (infantry has its own parser)
    };
    let veterancy: u16 = if fields.len() > vet_index {
        fields[vet_index].parse::<u16>().unwrap_or(0)
    } else {
        0
    };

    Some(MapEntity {
        owner,
        type_id,
        health,
        cell_x,
        cell_y,
        facing,
        category,
        sub_cell: 0,
        veterancy,
        high: matches!(category, EntityCategory::Unit)
            && parse_boolish_field(fields.get(10).copied()),
    })
}

fn parse_boolish_field(value: Option<&str>) -> bool {
    let Some(value) = value else { return false };
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::ini_parser::IniFile;

    #[test]
    fn test_parse_units() {
        let ini: IniFile = IniFile::from_str(
            "[Units]\n\
             0=Americans,MTNK,256,30,40,64,Guard,None,0,-1,false,-1,true,false\n\
             1=Soviet,HTNK,200,50,60,128,Guard,None,100,-1,false,-1,false,false\n",
        );
        let entities: Vec<MapEntity> = parse_map_entities(&ini);
        assert_eq!(entities.len(), 2);

        assert_eq!(entities[0].owner, "Americans");
        assert_eq!(entities[0].type_id, "MTNK");
        assert_eq!(entities[0].health, 256);
        assert_eq!(entities[0].cell_x, 30);
        assert_eq!(entities[0].cell_y, 40);
        assert_eq!(entities[0].facing, 64);
        assert_eq!(entities[0].category, EntityCategory::Unit);
        assert_eq!(entities[0].veterancy, 0);
        assert!(!entities[0].high);

        assert_eq!(entities[1].owner, "Soviet");
        assert_eq!(entities[1].type_id, "HTNK");
        assert_eq!(entities[1].health, 200);
        assert_eq!(entities[1].facing, 128);
        assert_eq!(entities[1].veterancy, 100);
        assert!(!entities[1].high);
    }

    #[test]
    fn test_parse_infantry() {
        let ini: IniFile = IniFile::from_str(
            "[Infantry]\n\
             0=Soviet,E1,256,10,20,2,Guard,192,None,200,-1,false,true,false\n",
        );
        let entities: Vec<MapEntity> = parse_map_entities(&ini);
        assert_eq!(entities.len(), 1);

        assert_eq!(entities[0].type_id, "E1");
        assert_eq!(entities[0].cell_x, 10);
        assert_eq!(entities[0].cell_y, 20);
        assert_eq!(entities[0].sub_cell, 2);
        assert_eq!(entities[0].facing, 192);
        assert_eq!(entities[0].category, EntityCategory::Infantry);
        assert_eq!(entities[0].veterancy, 200);
        assert!(!entities[0].high);
    }

    #[test]
    fn test_parse_structures() {
        let ini: IniFile = IniFile::from_str(
            "[Structures]\n\
             0=Americans,GAPOWR,256,15,25,0,None,true,false,true,0,0,None,None,None,false,true\n",
        );
        let entities: Vec<MapEntity> = parse_map_entities(&ini);
        assert_eq!(entities.len(), 1);

        assert_eq!(entities[0].type_id, "GAPOWR");
        assert_eq!(entities[0].cell_x, 15);
        assert_eq!(entities[0].cell_y, 25);
        assert_eq!(entities[0].facing, 0);
        assert_eq!(entities[0].category, EntityCategory::Structure);
    }

    #[test]
    fn test_parse_aircraft() {
        let ini: IniFile = IniFile::from_str(
            "[Aircraft]\n\
             0=Soviet,DRON,256,50,50,0,Guard,None,0,-1,false,false\n",
        );
        let entities: Vec<MapEntity> = parse_map_entities(&ini);
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].type_id, "DRON");
        assert_eq!(entities[0].category, EntityCategory::Aircraft);
        assert!(!entities[0].high);
    }

    #[test]
    fn test_parse_high_for_units_and_infantry() {
        let ini: IniFile = IniFile::from_str(
            "[Units]\n\
             0=Americans,MTNK,256,30,40,64,Guard,None,0,-1,true,-1,false,false\n\
             [Infantry]\n\
             0=Soviet,E1,256,10,20,2,Guard,192,None,200,-1,true,false\n",
        );
        let entities: Vec<MapEntity> = parse_map_entities(&ini);
        assert_eq!(entities.len(), 2);
        assert!(entities[0].high);
        assert!(entities[1].high);
    }

    #[test]
    fn test_malformed_lines_skipped() {
        let ini: IniFile = IniFile::from_str(
            "[Units]\n\
             0=Americans,MTNK\n\
             1=Soviet,HTNK,256,50,60,128,Guard,None,0,-1,false,-1,false,false\n",
        );
        let entities: Vec<MapEntity> = parse_map_entities(&ini);
        // First line has only 2 fields (< 6 minimum), should be skipped.
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].type_id, "HTNK");
    }

    #[test]
    fn test_empty_map_returns_empty() {
        let ini: IniFile = IniFile::from_str("[Map]\nTheater=TEMPERATE\n");
        let entities: Vec<MapEntity> = parse_map_entities(&ini);
        assert!(entities.is_empty());
    }

    #[test]
    fn test_mixed_sections() {
        let ini: IniFile = IniFile::from_str(
            "[Units]\n\
             0=Americans,MTNK,256,30,40,64,Guard,None,0,-1,false,-1,true,false\n\
             [Infantry]\n\
             0=Soviet,E1,256,10,20,0,Guard,0,None,0,-1,false,true,false\n\
             [Structures]\n\
             0=Americans,GAPOWR,256,15,25,0,None,true,false,true,0,0,None,None,None,false,true\n\
             [Aircraft]\n\
             0=Soviet,DRON,256,50,50,0,Guard,None,0,-1,false,false\n",
        );
        let entities: Vec<MapEntity> = parse_map_entities(&ini);
        assert_eq!(entities.len(), 4);

        let units: usize = entities
            .iter()
            .filter(|e| e.category == EntityCategory::Unit)
            .count();
        let infantry: usize = entities
            .iter()
            .filter(|e| e.category == EntityCategory::Infantry)
            .count();
        let structures: usize = entities
            .iter()
            .filter(|e| e.category == EntityCategory::Structure)
            .count();
        let aircraft: usize = entities
            .iter()
            .filter(|e| e.category == EntityCategory::Aircraft)
            .count();
        assert_eq!(units, 1);
        assert_eq!(infantry, 1);
        assert_eq!(structures, 1);
        assert_eq!(aircraft, 1);
    }
}
