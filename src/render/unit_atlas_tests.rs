use super::*;
use crate::render::vxl_raster::VxlSprite;

#[test]
fn test_unit_sprite_key_hash_equality() {
    let hc = HouseColorIndex::default();
    let key1 = UnitSpriteKey {
        type_id: "HTNK".into(),
        facing: 64,
        house_color: hc,
        layer: VxlLayer::Composite,
        frame: 0,
        slope_type: 0,
    };
    let key2 = UnitSpriteKey {
        type_id: "HTNK".into(),
        facing: 64,
        house_color: hc,
        layer: VxlLayer::Composite,
        frame: 0,
        slope_type: 0,
    };
    let key3 = UnitSpriteKey {
        type_id: "HTNK".into(),
        facing: 128,
        house_color: hc,
        layer: VxlLayer::Composite,
        frame: 0,
        slope_type: 0,
    };
    assert_eq!(key1, key2);
    assert_ne!(key1, key3);
    let mut set: HashSet<UnitSpriteKey> = HashSet::new();
    set.insert(key1);
    set.insert(key2); // duplicate
    set.insert(key3);
    assert_eq!(set.len(), 2);
}

#[test]
fn test_empty_world_returns_none() {
    let needed: HashSet<UnitSpriteKey> = HashSet::new();
    assert!(needed.is_empty());
}

#[test]
fn test_key_collection_deduplicates() {
    let hc = HouseColorIndex::default();
    let mut needed: HashSet<UnitSpriteKey> = HashSet::new();
    for facing in [64u8, 64, 128] {
        needed.insert(UnitSpriteKey {
            type_id: "HTNK".to_string(),
            facing,
            house_color: hc,
            layer: VxlLayer::Composite,
            frame: 0,
            slope_type: 0,
        });
    }
    assert_eq!(needed.len(), 2);
}

#[test]
fn test_composite_layers_depth_correct() {
    // Body: 2x2, all at depth 1.0
    let body = VxlSprite {
        rgba: vec![
            10, 10, 10, 255, 10, 10, 10, 255, 10, 10, 10, 255, 10, 10, 10, 255,
        ],
        depth: vec![1.0, 1.0, 1.0, 1.0],
        width: 2,
        height: 2,
        offset_x: 0.0,
        offset_y: 0.0,
    };
    // Turret: 1x1 at (1,1), depth 2.0 (closer) — should overwrite body.
    let turret = VxlSprite {
        rgba: vec![255, 0, 0, 255],
        depth: vec![2.0],
        width: 1,
        height: 1,
        offset_x: 1.0,
        offset_y: 1.0,
    };
    let out = composite_vxl_layers(&[body.clone(), turret]);
    assert_eq!(out.width, 2);
    assert_eq!(out.height, 2);
    let idx = ((1 * out.width + 1) * 4) as usize;
    assert_eq!(&out.rgba[idx..idx + 4], &[255, 0, 0, 255]);

    // Turret behind body (depth 0.5 < body's 1.0) — body pixel wins.
    let turret_behind = VxlSprite {
        rgba: vec![0, 255, 0, 255],
        depth: vec![0.5],
        width: 1,
        height: 1,
        offset_x: 1.0,
        offset_y: 1.0,
    };
    let out2 = composite_vxl_layers(&[body, turret_behind]);
    let idx2 = ((1 * out2.width + 1) * 4) as usize;
    // Body pixel should remain (depth 1.0 > 0.5).
    assert_eq!(&out2.rgba[idx2..idx2 + 4], &[10, 10, 10, 255]);
}

#[test]
fn test_pad_layer_to_union_bounds() {
    // Body at offset (0,0), 2x2
    let body = VxlSprite {
        rgba: vec![
            10, 10, 10, 255, 10, 10, 10, 255, 10, 10, 10, 255, 10, 10, 10, 255,
        ],
        depth: vec![1.0; 4],
        width: 2,
        height: 2,
        offset_x: 0.0,
        offset_y: 0.0,
    };
    // Turret at offset (5,3), 1x1 — different origin from body.
    let turret = VxlSprite {
        rgba: vec![255, 0, 0, 255],
        depth: vec![2.0],
        width: 1,
        height: 1,
        offset_x: 5.0,
        offset_y: 3.0,
    };

    let all_layers: Vec<&VxlSprite> = vec![&body, &turret];

    // Pad body into union bounds.
    let padded_body = pad_layer_to_union_bounds(&body, &all_layers);
    // Pad turret into union bounds.
    let padded_turret = pad_layer_to_union_bounds(&turret, &all_layers);

    // Both should have the same dimensions and offset (shared origin).
    assert_eq!(padded_body.width, padded_turret.width);
    assert_eq!(padded_body.height, padded_turret.height);
    assert!((padded_body.offset_x - padded_turret.offset_x).abs() < 0.01);
    assert!((padded_body.offset_y - padded_turret.offset_y).abs() < 0.01);

    // Union bounds: min_x=0, min_y=0, max_x=6, max_y=4 → 6x4
    assert_eq!(padded_body.width, 6);
    assert_eq!(padded_body.height, 4);
    assert!((padded_body.offset_x - 0.0).abs() < 0.01);
    assert!((padded_body.offset_y - 0.0).abs() < 0.01);

    // Body pixels should be at (0,0) in the padded canvas.
    assert_eq!(padded_body.rgba[3], 255); // alpha of pixel (0,0)

    // Turret pixel should be at (5,3) in the padded canvas.
    let turret_pix: usize = (3 * padded_turret.width + 5) as usize;
    assert_eq!(padded_turret.rgba[turret_pix * 4 + 3], 255); // alpha
    assert_eq!(padded_turret.rgba[turret_pix * 4], 255); // red channel
}

#[test]
fn test_canonical_turret_facing() {
    use super::canonical_turret_facing;
    // canonical_turret_facing takes u16 (16-bit DirStruct), converts via >>8 to u8,
    // then quantizes to multiples of 2 (128 buckets).
    // u8 value 0 → u16 value 0<<8 = 0
    assert_eq!(canonical_turret_facing(0u16), 0);
    // u8 value 1 → u16 = 1<<8 = 256
    assert_eq!(canonical_turret_facing(256), 0);
    // u8 value 2 → u16 = 2<<8 = 512
    assert_eq!(canonical_turret_facing(512), 2);
    // u8 value 3 → u16 = 3<<8 = 768
    assert_eq!(canonical_turret_facing(768), 2);
    // u8 value 4 → u16 = 4<<8 = 1024
    assert_eq!(canonical_turret_facing(1024), 4);
    // u8 value 255 → u16 = 255<<8 = 65280
    assert_eq!(canonical_turret_facing(65280), 254);
    // Verify finer than body facing (step=4).
    assert_eq!(canonical_unit_facing(3), 0); // snaps to 0
    assert_eq!(canonical_turret_facing(768), 2); // u8=3, snaps to 2
}

#[test]
fn test_facing_config_for_layer() {
    let (step, buckets) = super::facing_config_for_layer(VxlLayer::Body);
    assert_eq!(step, 4);
    assert_eq!(buckets, 64);

    let (step, buckets) = super::facing_config_for_layer(VxlLayer::Composite);
    assert_eq!(step, 4);
    assert_eq!(buckets, 64);

    let (step, buckets) = super::facing_config_for_layer(VxlLayer::Turret);
    assert_eq!(step, 2);
    assert_eq!(buckets, 128);

    let (step, buckets) = super::facing_config_for_layer(VxlLayer::Barrel);
    assert_eq!(step, 2);
    assert_eq!(buckets, 128);
}
