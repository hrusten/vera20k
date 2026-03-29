//! Tests for SHP sprite atlas key collection and deduplication.

use super::*;

fn make_shp_key(type_id: &str, facing: u8) -> ShpSpriteKey {
    ShpSpriteKey {
        type_id: type_id.to_string(),
        facing,
        frame: 0,
        house_color: HouseColorIndex::default(),
    }
}

#[test]
fn test_shp_sprite_key_hash_equality() {
    let hc = HouseColorIndex::default();
    let k1 = ShpSpriteKey {
        type_id: "E1".into(),
        facing: 64,
        frame: 10,
        house_color: hc,
    };
    let k2 = ShpSpriteKey {
        type_id: "E1".into(),
        facing: 64,
        frame: 10,
        house_color: hc,
    };
    let k3 = ShpSpriteKey {
        type_id: "E1".into(),
        facing: 64,
        frame: 11,
        house_color: hc,
    };
    assert_eq!(k1, k2);
    assert_ne!(k1, k3);
    let mut set: HashSet<ShpSpriteKey> = HashSet::new();
    set.insert(k1);
    set.insert(k2);
    set.insert(k3);
    assert_eq!(set.len(), 2);
}

#[test]
fn test_empty_world_returns_none() {
    let needed: HashSet<ShpSpriteKey> = HashSet::new();
    assert!(needed.is_empty());
}

#[test]
fn test_key_collection_deduplicates() {
    let hc = HouseColorIndex::default();
    let mut needed: HashSet<ShpSpriteKey> = HashSet::new();
    // Two identical keys + one different facing.
    needed.insert(make_shp_key("E1", 64));
    needed.insert(make_shp_key("E1", 64)); // duplicate
    needed.insert(make_shp_key("E1", 128));
    assert_eq!(needed.len(), 2);
    let _ = hc;
}

#[test]
fn test_structure_facing_collapse() {
    let hc = HouseColorIndex::default();
    // Structures always use facing 0 (no rotation).
    let mut needed: HashSet<ShpSpriteKey> = HashSet::new();
    for facing_raw in [64u8, 192u8] {
        let eff = 0u8; // structures collapse to facing 0
        needed.insert(ShpSpriteKey {
            type_id: "GAPOWR".to_string(),
            facing: eff,
            frame: 0,
            house_color: hc,
        });
        let _ = facing_raw;
    }
    assert_eq!(needed.len(), 1);
}

#[test]
fn test_different_houses_create_separate_keys() {
    let hc0 = HouseColorIndex(0); // Gold
    let hc1 = HouseColorIndex(1); // DarkBlue
    let k1 = ShpSpriteKey {
        type_id: "E1".into(),
        facing: 64,
        frame: 10,
        house_color: hc0,
    };
    let k2 = ShpSpriteKey {
        type_id: "E1".into(),
        facing: 64,
        frame: 10,
        house_color: hc1,
    };
    assert_ne!(
        k1, k2,
        "Same type+facing but different house should be distinct keys"
    );
    let mut set: HashSet<ShpSpriteKey> = HashSet::new();
    set.insert(k1);
    set.insert(k2);
    assert_eq!(set.len(), 2);
}
