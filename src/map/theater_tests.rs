//! Tests for theater INI parsing and tileset lookups.
//!
//! Extracted from theater.rs to stay under the 400-line limit.

use super::*;

fn make_test_ini() -> String {
    "[TileSet0000]\nSetName=Grass\nFileName=clear\nTilesInSet=1\n\n\
     [TileSet0001]\nSetName=Blank\nFileName=\nTilesInSet=1\n\n\
     [TileSet0002]\nSetName=Cliffs\nFileName=cliff\nTilesInSet=3\n"
        .to_string()
}

#[test]
fn test_parse_tileset_ini_basic() {
    let ini: &str = &make_test_ini();
    let lookup: TilesetLookup = parse_tileset_ini(ini.as_bytes(), "tem").expect("Should parse");

    assert_eq!(lookup.len(), 5); // 1 + 1 + 3
    assert_eq!(lookup.filename(0), Some("clear01.tem"));
    assert_eq!(lookup.filename(1), None); // blank
    assert_eq!(lookup.filename(2), Some("cliff01.tem"));
    assert_eq!(lookup.filename(4), Some("cliff03.tem"));

    // Tileset boundary/index lookups.
    assert_eq!(lookup.bounds().len(), 3);
    assert_eq!(lookup.tileset_index(0), Some(0)); // tile 0 → TileSet0000
    assert_eq!(lookup.tileset_index(1), Some(1)); // tile 1 → TileSet0001
    assert_eq!(lookup.tileset_index(2), Some(2)); // tile 2 → TileSet0002
    assert_eq!(lookup.tileset_index(4), Some(2)); // tile 4 → TileSet0002
    assert_eq!(lookup.tileset_index(99), None); // out of range

    // Edge cases: NO_TILE, negative, and far out-of-range.
    assert_eq!(lookup.filename(NO_TILE), None);
    assert_eq!(lookup.filename(-1), None);
    assert_eq!(lookup.filename(999), None);
}

#[test]
fn test_collect_used_tiles() {
    let cells: Vec<(i32, u8)> = vec![(0, 0), (1, 0), (0, 0), (NO_TILE, 0), (2, 1)];
    let used: HashSet<TileKey> = collect_used_tiles(&cells);
    assert_eq!(used.len(), 3); // (0,0), (1,0), (2,1) — deduped, NO_TILE excluded.
    assert!(used.contains(&TileKey {
        tile_id: 0,
        sub_tile: 0,
        variant: 0,
    }));
    assert!(used.contains(&TileKey {
        tile_id: 2,
        sub_tile: 1,
        variant: 0,
    }));
}

#[test]
fn test_theater_def_lookup() {
    assert!(theater_def("TEMPERATE").is_some());
    assert!(theater_def("temperate").is_some());
    assert!(theater_def("SNOW").is_some());
    assert!(theater_def("URBAN").is_some());
    assert!(theater_def("DESERT").is_none());
}

#[test]
fn test_is_water_and_cliff() {
    let ini_str: &str = "\
[TileSet0000]\nSetName=Grass\nFileName=clear\nTilesInSet=2\n\n\
[TileSet0001]\nSetName=Water\nFileName=water\nTilesInSet=3\n\n\
[TileSet0002]\nSetName=Water Cliffs\nFileName=wcliff\nTilesInSet=2\n\n\
[TileSet0003]\nSetName=Cliffs\nFileName=cliff\nTilesInSet=1\n";
    let lookup: TilesetLookup = parse_tileset_ini(ini_str.as_bytes(), "tem").expect("Should parse");

    // Grass (tile_ids 0-1): not water, not cliff.
    assert!(!lookup.is_water(0));
    assert!(!lookup.is_cliff(0));
    // Water (tile_ids 2-4): water but not cliff.
    assert!(lookup.is_water(2));
    assert!(lookup.is_water(4));
    assert!(!lookup.is_cliff(2));
    // Water Cliffs (tile_ids 5-6): both water and cliff.
    assert!(lookup.is_water(5));
    assert!(lookup.is_cliff(5));
    // Cliffs (tile_id 7): cliff but not water.
    assert!(!lookup.is_water(7));
    assert!(lookup.is_cliff(7));
    // Out of range: neither.
    assert!(!lookup.is_water(99));
    assert!(!lookup.is_cliff(99));
}
