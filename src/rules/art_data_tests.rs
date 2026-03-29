//! Tests for art.ini parsing and art-resolution layering.

use super::*;

#[test]
fn test_apply_theater_letter() {
    assert_eq!(apply_theater_letter("GAPOWR", "TEMPERATE"), "GTPOWR");
    assert_eq!(apply_theater_letter("GAPOWR", "SNOW"), "GAPOWR");
    assert_eq!(apply_theater_letter("NAHAND", "TEMPERATE"), "NTHAND");
    assert_eq!(apply_theater_letter("X", "TEMPERATE"), "X");
    assert_eq!(apply_theater_letter("GAPOWR", "UNKNOWN"), "GAPOWR");
}

#[test]
fn test_hardcoded_new_theater_prefixes() {
    let reg: ArtRegistry = ArtRegistry::empty();
    assert!(reg.should_use_new_theater("GAPOWR"));
    assert!(reg.should_use_new_theater("GTPOWR"));
    assert!(reg.should_use_new_theater("NAHAND"));
    assert!(reg.should_use_new_theater("NTHAND"));
    assert!(reg.should_use_new_theater("CAOUTP"));
    assert!(reg.should_use_new_theater("CTOUTP"));
    assert!(!reg.should_use_new_theater("MTNK"));
    assert!(!reg.should_use_new_theater("E1"));
    assert!(!reg.should_use_new_theater("HTNK"));
}

#[test]
fn test_object_shp_candidates_no_new_theater() {
    let reg: ArtRegistry = ArtRegistry::empty();
    let candidates: Vec<String> = object_shp_candidates(Some(&reg), "E1", "tem", "TEMPERATE");
    assert_eq!(candidates, vec!["E1.SHP", "E1.TEM"]);
}

#[test]
fn test_object_shp_candidates_with_new_theater() {
    let reg: ArtRegistry = ArtRegistry::empty();
    let candidates: Vec<String> = object_shp_candidates(Some(&reg), "GAPOWR", "tem", "TEMPERATE");
    assert_eq!(
        candidates,
        vec![
            "GTPOWR.SHP",
            "GTPOWR.TEM",
            "GGPOWR.SHP",
            "GGPOWR.TEM",
            "GAPOWR.SHP",
            "GAPOWR.TEM",
        ]
    );
}

#[test]
fn test_object_shp_candidates_dedupe_when_substitution_matches_original() {
    let reg: ArtRegistry = ArtRegistry::empty();
    let candidates: Vec<String> = object_shp_candidates(Some(&reg), "GAPOWR", "sno", "SNOW");
    assert_eq!(
        candidates,
        vec!["GAPOWR.SHP", "GAPOWR.SNO", "GGPOWR.SHP", "GGPOWR.SNO",]
    );
}

#[test]
fn test_apply_generic_letter() {
    assert_eq!(apply_generic_letter("GAPOWR"), "GGPOWR");
    assert_eq!(apply_generic_letter("NAHAND"), "NGHAND");
    assert_eq!(apply_generic_letter("X"), "X");
}

#[test]
fn test_voxel_asset_names() {
    let (vxl, hva) = voxel_asset_names("HTNK");
    assert_eq!(vxl, "HTNK.VXL");
    assert_eq!(hva, "HTNK.HVA");
}

#[test]
fn test_from_ini_parses_entries() {
    let ini: IniFile = IniFile::from_str(
        "[GAPOWR]\nNewTheater=yes\nCameo=GAPICON\n\n[HTNK]\nVoxel=yes\nAltCameo=HTKALT\n\n[NACNST]\nImage=CIVNC\n",
    );
    let reg: ArtRegistry = ArtRegistry::from_ini(&ini);
    assert_eq!(reg.len(), 3);

    let gapowr: &ArtEntry = reg.get("GAPOWR").expect("GAPOWR exists");
    assert!(gapowr.new_theater);
    assert!(!gapowr.voxel);
    assert!(gapowr.image.is_none());
    assert_eq!(gapowr.cameo.as_deref(), Some("GAPICON"));

    let htnk: &ArtEntry = reg.get("HTNK").expect("HTNK exists");
    assert!(!htnk.new_theater);
    assert!(htnk.voxel);
    assert_eq!(htnk.alt_cameo.as_deref(), Some("HTKALT"));

    let nacnst: &ArtEntry = reg.get("NACNST").expect("NACNST exists");
    assert_eq!(nacnst.image.as_deref(), Some("CIVNC"));
}

#[test]
fn test_resolve_effective_image_id_chain() {
    let ini: IniFile = IniFile::from_str("[NACNST]\nImage=CIVNC\n\n[E1]\n\n[MTNK]\nImage=MTNK\n");
    let reg: ArtRegistry = ArtRegistry::from_ini(&ini);

    assert_eq!(reg.resolve_effective_image_id("NACNST", "NACNST"), "CIVNC");
    assert_eq!(reg.resolve_effective_image_id("E1", "E1"), "E1");
    assert_eq!(reg.resolve_effective_image_id("MTNK", "MTNK"), "MTNK");
    assert_eq!(reg.resolve_effective_image_id("UNKNOWN", "FOO"), "FOO");
}

#[test]
fn test_resolve_object_art_exposes_exact_layers() {
    let ini: IniFile = IniFile::from_str("[NACNST]\nImage=CIVNC\nBibShape=NACNSTBB\n");
    let reg: ArtRegistry = ArtRegistry::from_ini(&ini);

    let resolved: ResolvedObjectArt<'_> = reg.resolve_object_art("NACNST", "NACNST");
    assert_eq!(resolved.base_art_id, "NACNST");
    assert_eq!(resolved.image_id, "CIVNC");
    assert_eq!(resolved.metadata_section_id, "NACNST");
    assert_eq!(
        resolved.entry.and_then(|e| e.bib_shape.as_deref()),
        Some("NACNSTBB")
    );
}

#[test]
fn test_resolve_metadata_entry_prefers_rules_image_section() {
    let ini: IniFile = IniFile::from_str(
        "[GAAIRC]\nBibShape=GAAIRCBB\nActiveAnim=GAAIRC_A\n\n[GAAIRC_A]\nLoopStart=0\nLoopEnd=4\nLoopCount=-1\n",
    );
    let reg: ArtRegistry = ArtRegistry::from_ini(&ini);

    let entry: &ArtEntry = reg
        .resolve_metadata_entry("AMRADR", "GAAIRC")
        .expect("AMRADR should use GAAIRC art metadata");
    assert_eq!(entry.bib_shape.as_deref(), Some("GAAIRCBB"));
    assert_eq!(entry.building_anims.len(), 1);
    assert_eq!(entry.building_anims[0].anim_type, "GAAIRC_A");
}

#[test]
fn test_resolve_metadata_entry_keeps_type_section_when_it_owns_metadata() {
    let ini: IniFile = IniFile::from_str("[NACNST]\nImage=CIVNC\nBibShape=NACNSTBB\n");
    let reg: ArtRegistry = ArtRegistry::from_ini(&ini);

    let entry: &ArtEntry = reg
        .resolve_metadata_entry("NACNST", "NACNST")
        .expect("NACNST metadata should stay on its own section");
    assert_eq!(entry.bib_shape.as_deref(), Some("NACNSTBB"));
    assert_eq!(entry.image.as_deref(), Some("CIVNC"));
}

#[test]
fn test_resolve_declared_cameo_id_prefers_art_data() {
    let ini: IniFile = IniFile::from_str(
        "[E1]\nCameo=E1CAMEO\n\n[MTNK]\nAltCameo=MTNKALT\n\n[NACNST]\nImage=CIVNC\n[CIVNC]\nCameo=CIVICON\n",
    );
    let reg: ArtRegistry = ArtRegistry::from_ini(&ini);

    assert_eq!(reg.resolve_declared_cameo_id("E1", "E1"), "E1CAMEO");
    assert_eq!(reg.resolve_declared_cameo_id("MTNK", "MTNK"), "MTNKALT");
    assert_eq!(reg.resolve_declared_cameo_id("NACNST", "NACNST"), "CIVICON");
    assert_eq!(reg.resolve_declared_cameo_id("UNKNOWN", "FOO"), "FOO");
}

#[test]
fn test_new_theater_from_ini_key() {
    let ini: IniFile = IniFile::from_str("[MYCIVBLD]\nNewTheater=yes\n");
    let reg: ArtRegistry = ArtRegistry::from_ini(&ini);
    assert!(reg.should_use_new_theater("MYCIVBLD"));
}

#[test]
fn test_parse_building_anims() {
    let ini: IniFile = IniFile::from_str(
        "[CAOILD]\nActiveAnim=CAOILD_A\nActiveAnimYSort=362\nActiveAnimTwo=CAOILD_F\nActiveAnimTwoZAdjust=-50\n",
    );
    let reg: ArtRegistry = ArtRegistry::from_ini(&ini);
    let entry: &ArtEntry = reg.get("CAOILD").expect("CAOILD exists");
    assert_eq!(entry.building_anims.len(), 2);

    assert_eq!(entry.building_anims[0].anim_type, "CAOILD_A");
    assert_eq!(entry.building_anims[0].y_sort, 362);
    assert_eq!(entry.building_anims[0].z_adjust, 0);

    assert_eq!(entry.building_anims[1].anim_type, "CAOILD_F");
    assert_eq!(entry.building_anims[1].y_sort, 0);
    assert_eq!(entry.building_anims[1].z_adjust, -50);
}

#[test]
fn test_no_building_anims_for_regular_entry() {
    let ini: IniFile = IniFile::from_str("[E1]\nVoxel=no\n");
    let reg: ArtRegistry = ArtRegistry::from_ini(&ini);
    let entry: &ArtEntry = reg.get("E1").expect("E1 exists");
    assert!(entry.building_anims.is_empty());
}

#[test]
fn test_parse_turret_offset() {
    let ini: IniFile = IniFile::from_str("[HTK]\nVoxel=yes\nTurretOffset=-80\n");
    let reg: ArtRegistry = ArtRegistry::from_ini(&ini);
    let entry: &ArtEntry = reg.get("HTK").expect("HTK exists");
    assert_eq!(entry.turret_offset, -80);
}

#[test]
fn test_resolve_declared_palette_id() {
    let ini: IniFile = IniFile::from_str("[TEST]\nImage=TESTART\n\n[TESTART]\nPalette=anim\n");
    let reg: ArtRegistry = ArtRegistry::from_ini(&ini);
    assert_eq!(
        reg.resolve_declared_palette_id("TEST", "TEST"),
        Some("anim".to_string())
    );
}

#[test]
fn test_anim_candidates_use_anim_section_flags() {
    let ini: IniFile = IniFile::from_str("[CAOILD_A]\nImage=CAOILDX\nNewTheater=yes\n");
    let reg: ArtRegistry = ArtRegistry::from_ini(&ini);
    let image_id: String = reg.resolve_effective_image_id("CAOILD_A", "CAOILD_A");
    let candidates: Vec<String> =
        anim_shp_candidates(Some(&reg), "CAOILD_A", &image_id, "urb", "NEWURBAN");
    assert_eq!(
        candidates,
        vec![
            "CNOILDX.SHP",
            "CNOILDX.URB",
            "CGOILDX.SHP",
            "CAOILDX.SHP",
            "CAOILDX.URB",
        ]
    );
}

#[test]
fn test_make_candidates_use_deduped_uppercase_names() {
    let reg: ArtRegistry = ArtRegistry::empty();
    let candidates: Vec<String> = make_shp_candidates(Some(&reg), "GAPOWR", "sno", "SNOW");
    assert_eq!(
        candidates,
        vec![
            "GAPOWRMK.SHP",
            "GAPOWRMK.SNO",
            "GGPOWRMK.SHP",
            "GGPOWRMK.SNO",
        ]
    );
}

#[test]
fn test_resolve_overlay_image_id_and_candidates() {
    let art_ini: IniFile = IniFile::from_str("[LOBRDG27]\nImage=LOBRDGX\nTheater=yes\n");
    let rules_ini: IniFile = IniFile::from_str("[LOBRDG27]\nImage=LOBRDGY\n");
    let reg: ArtRegistry = ArtRegistry::from_ini(&art_ini);

    let image_id: String = reg.resolve_overlay_image_id("LOBRDG27", &rules_ini);
    assert_eq!(image_id, "LOBRDGY");

    let candidates: Vec<String> =
        overlay_shp_candidates(Some(&reg), "LOBRDG27", &image_id, "tem", "TEMPERATE");
    assert_eq!(candidates, vec!["LOBRDGY.TEM", "LOBRDGY.SHP"]);
    assert!(!candidates.iter().any(|name| name.contains("LOBRDG26")));
}
