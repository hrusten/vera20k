//! Tests for infantry animation sequence parsing from art.ini.

use super::*;
use crate::rules::ini_parser::IniFile;

#[test]
fn test_parse_sequence_value_basic() {
    let entry: InfantrySequenceEntry =
        parse_sequence_value("8,6,6").expect("Should parse Walk=8,6,6");
    assert_eq!(entry.start_frame, 8);
    assert_eq!(entry.frames_per_facing, 6);
    assert_eq!(entry.facings, 6);
    assert_eq!(entry.facing_hint, None);
}

#[test]
fn test_parse_sequence_value_with_facing_hint() {
    let entry: InfantrySequenceEntry =
        parse_sequence_value("56,15,0,S").expect("Should parse Idle1=56,15,0,S");
    assert_eq!(entry.start_frame, 56);
    assert_eq!(entry.frames_per_facing, 15);
    assert_eq!(entry.facings, 0);
    assert_eq!(entry.facing_hint, Some(FacingHint::S));
}

#[test]
fn test_parse_sequence_value_facing_hint_ne() {
    let entry: InfantrySequenceEntry =
        parse_sequence_value("71,15,0,NE").expect("Should parse with NE hint");
    assert_eq!(entry.facing_hint, Some(FacingHint::NE));
}

#[test]
fn test_parse_sequence_value_single_frame() {
    let entry: InfantrySequenceEntry =
        parse_sequence_value("0,1,1").expect("Should parse Ready=0,1,1");
    assert_eq!(entry.start_frame, 0);
    assert_eq!(entry.frames_per_facing, 1);
    assert_eq!(entry.facings, 1);
    assert_eq!(entry.facing_hint, None);
}

#[test]
fn test_parse_sequence_value_zero_frames() {
    let entry: InfantrySequenceEntry =
        parse_sequence_value("0,0,0").expect("Should parse zero-frame entry");
    assert_eq!(entry.start_frame, 0);
    assert_eq!(entry.frames_per_facing, 0);
    assert_eq!(entry.facings, 0);
}

#[test]
fn test_parse_sequence_value_too_few_fields() {
    assert!(parse_sequence_value("8,6").is_none());
    assert!(parse_sequence_value("8").is_none());
    assert!(parse_sequence_value("").is_none());
}

#[test]
fn test_parse_sequence_value_invalid_number() {
    assert!(parse_sequence_value("abc,6,6").is_none());
    assert!(parse_sequence_value("8,xyz,6").is_none());
}

#[test]
fn test_sequence_kind_from_ini_key_core_mappings() {
    assert_eq!(
        sequence_kind_from_ini_key("Ready"),
        Some(SequenceKind::Stand)
    );
    assert_eq!(
        sequence_kind_from_ini_key("Guard"),
        Some(SequenceKind::Stand)
    );
    assert_eq!(sequence_kind_from_ini_key("Walk"), Some(SequenceKind::Walk));
    assert_eq!(
        sequence_kind_from_ini_key("FireUp"),
        Some(SequenceKind::Attack)
    );
    assert_eq!(
        sequence_kind_from_ini_key("Idle1"),
        Some(SequenceKind::Idle1)
    );
    assert_eq!(sequence_kind_from_ini_key("Die1"), Some(SequenceKind::Die1));
    assert_eq!(sequence_kind_from_ini_key("Die5"), Some(SequenceKind::Die5));
    assert_eq!(
        sequence_kind_from_ini_key("Prone"),
        Some(SequenceKind::Prone)
    );
    assert_eq!(
        sequence_kind_from_ini_key("Crawl"),
        Some(SequenceKind::Crawl)
    );
    assert_eq!(
        sequence_kind_from_ini_key("FireProne"),
        Some(SequenceKind::FireProne)
    );
    assert_eq!(sequence_kind_from_ini_key("Down"), Some(SequenceKind::Down));
    assert_eq!(sequence_kind_from_ini_key("Up"), Some(SequenceKind::Up));
    assert_eq!(
        sequence_kind_from_ini_key("Cheer"),
        Some(SequenceKind::Cheer)
    );
    assert_eq!(
        sequence_kind_from_ini_key("Paradrop"),
        Some(SequenceKind::Paradrop)
    );
    assert_eq!(
        sequence_kind_from_ini_key("Panic"),
        Some(SequenceKind::Panic)
    );
}

#[test]
fn test_sequence_kind_from_ini_key_case_insensitive() {
    assert_eq!(sequence_kind_from_ini_key("walk"), Some(SequenceKind::Walk));
    assert_eq!(sequence_kind_from_ini_key("WALK"), Some(SequenceKind::Walk));
    assert_eq!(sequence_kind_from_ini_key("Walk"), Some(SequenceKind::Walk));
}

#[test]
fn test_sequence_kind_from_ini_key_unknown_returns_none() {
    assert_eq!(sequence_kind_from_ini_key("Tumble"), None);
    assert_eq!(sequence_kind_from_ini_key("Carry"), None);
    assert_eq!(sequence_kind_from_ini_key("Shovel"), None);
    assert_eq!(sequence_kind_from_ini_key("Bogus"), None);
}

#[test]
fn test_sequence_kind_from_ini_key_deploy_variants() {
    assert_eq!(
        sequence_kind_from_ini_key("Deploy"),
        Some(SequenceKind::Deploy)
    );
    assert_eq!(
        sequence_kind_from_ini_key("Undeploy"),
        Some(SequenceKind::Undeploy)
    );
    assert_eq!(
        sequence_kind_from_ini_key("Deployed"),
        Some(SequenceKind::Deployed)
    );
    assert_eq!(
        sequence_kind_from_ini_key("DeployedFire"),
        Some(SequenceKind::DeployedFire)
    );
    assert_eq!(
        sequence_kind_from_ini_key("DeployedIdle"),
        Some(SequenceKind::DeployedIdle)
    );
    assert_eq!(sequence_kind_from_ini_key("Swim"), Some(SequenceKind::Swim));
    assert_eq!(sequence_kind_from_ini_key("Fly"), Some(SequenceKind::Fly));
    assert_eq!(
        sequence_kind_from_ini_key("WetAttack"),
        Some(SequenceKind::WetAttack)
    );
}

#[test]
fn test_parse_con_sequence_from_ini() {
    let ini_text: &str = "\
[ConSequence]
Ready=0,1,1
Guard=0,1,1
Walk=8,6,6
FireUp=164,6,6
Idle1=56,15,0,S
Idle2=71,15,0,E
Die1=134,15,0
Die2=149,15,0
Prone=86,1,6
Crawl=86,6,6
Down=260,2,2
Up=276,2,2
FireProne=212,6,6
Cheer=293,8,0,E
Paradrop=292,1,0
Panic=8,6,6
";
    let ini: IniFile = IniFile::from_str(ini_text);
    let registry: InfantrySequenceRegistry = parse_infantry_sequence_registry(&ini);

    assert_eq!(registry.len(), 1, "Should parse exactly one sequence def");
    let def: &InfantrySequenceDef = registry
        .get("CONSEQUENCE")
        .expect("Should find ConSequence");

    // Walk: start=8, 6 frames, 6 facings.
    let walk: &InfantrySequenceEntry = def.entries.get("WALK").expect("Should have Walk");
    assert_eq!(walk.start_frame, 8);
    assert_eq!(walk.frames_per_facing, 6);
    assert_eq!(walk.facings, 6);

    // Idle1: non-directional with facing hint S.
    let idle1: &InfantrySequenceEntry = def.entries.get("IDLE1").expect("Should have Idle1");
    assert_eq!(idle1.start_frame, 56);
    assert_eq!(idle1.frames_per_facing, 15);
    assert_eq!(idle1.facings, 0);
    assert_eq!(idle1.facing_hint, Some(FacingHint::S));

    // FireUp: start=164, 6 frames, 6 facings.
    let fire: &InfantrySequenceEntry = def.entries.get("FIREUP").expect("Should have FireUp");
    assert_eq!(fire.start_frame, 164);
    assert_eq!(fire.frames_per_facing, 6);
    assert_eq!(fire.facings, 6);
}

#[test]
fn test_build_sequence_set_from_con_sequence() {
    let ini_text: &str = "\
[ConSequence]
Ready=0,1,1
Guard=0,1,1
Walk=8,6,6
FireUp=164,6,6
Idle1=56,15,0,S
Idle2=71,15,0,E
Die1=134,15,0
Die2=149,15,0
Prone=86,1,6
Crawl=86,6,6
Down=260,2,2
Up=276,2,2
FireProne=212,6,6
Cheer=293,8,0,E
Paradrop=292,1,0
Panic=8,6,6
";
    let ini: IniFile = IniFile::from_str(ini_text);
    let registry: InfantrySequenceRegistry = parse_infantry_sequence_registry(&ini);
    let def: &InfantrySequenceDef = registry
        .get("CONSEQUENCE")
        .expect("Should find ConSequence");
    let set: SequenceSet = build_sequence_set(def);

    // Stand (from Ready=0,1,1): INI multiplier=1 → facings=8, facing_multiplier=1.
    let stand: &SequenceDef = set.get(&SequenceKind::Stand).expect("Should have Stand");
    assert_eq!(stand.start_frame, 0);
    assert_eq!(stand.frame_count, 1);
    assert_eq!(stand.facings, 8);
    assert_eq!(stand.facing_multiplier, 1);

    // Walk (from Walk=8,6,6): facings=8, facing_multiplier=6.
    let walk: &SequenceDef = set.get(&SequenceKind::Walk).expect("Should have Walk");
    assert_eq!(walk.start_frame, 8);
    assert_eq!(walk.frame_count, 6);
    assert_eq!(walk.facings, 8);
    assert_eq!(walk.facing_multiplier, 6);

    // Attack (from FireUp=164,6,6): facings=8, facing_multiplier=6.
    let attack: &SequenceDef = set.get(&SequenceKind::Attack).expect("Should have Attack");
    assert_eq!(attack.start_frame, 164);
    assert_eq!(attack.frame_count, 6);
    assert_eq!(attack.facings, 8);
    assert_eq!(attack.facing_multiplier, 6);
    assert!(matches!(
        attack.loop_mode,
        LoopMode::TransitionTo(SequenceKind::Stand)
    ));

    // Idle1: facings=0 → engine facings=1 (non-directional).
    let idle1: &SequenceDef = set.get(&SequenceKind::Idle1).expect("Should have Idle1");
    assert_eq!(idle1.start_frame, 56);
    assert_eq!(idle1.frame_count, 15);
    assert_eq!(idle1.facings, 1);

    // Die1: start=134, holds last frame.
    let die1: &SequenceDef = set.get(&SequenceKind::Die1).expect("Should have Die1");
    assert_eq!(die1.start_frame, 134);
    assert_eq!(die1.frame_count, 15);
    assert!(matches!(die1.loop_mode, LoopMode::HoldLast));

    // Down: transition to Prone.
    let down: &SequenceDef = set.get(&SequenceKind::Down).expect("Should have Down");
    assert_eq!(down.start_frame, 260);
    assert!(matches!(
        down.loop_mode,
        LoopMode::TransitionTo(SequenceKind::Prone)
    ));

    // Verify total count: Ready+Guard map to Stand (deduped), so 16 INI keys → 15 SequenceKinds.
    assert!(
        set.len() >= 14,
        "Should have at least 14 sequences, got {}",
        set.len()
    );
}

#[test]
fn test_build_sequence_set_facings_zero_becomes_one() {
    let ini_text: &str = "\
[TestSequence]
Idle1=56,15,0,S
";
    let ini: IniFile = IniFile::from_str(ini_text);
    let registry: InfantrySequenceRegistry = parse_infantry_sequence_registry(&ini);
    let def: &InfantrySequenceDef = registry.get("TESTSEQUENCE").expect("def");
    let set: SequenceSet = build_sequence_set(def);

    let idle: &SequenceDef = set.get(&SequenceKind::Idle1).expect("Idle1");
    assert_eq!(
        idle.facings, 1,
        "facings=0 in INI should become 1 in engine"
    );
}

#[test]
fn test_multiple_sequences_in_registry() {
    let ini_text: &str = "\
[ConSequence]
Ready=0,1,1
Walk=8,6,6

[BruteSequence]
Ready=0,1,1
Walk=8,6,6
FireUp=116,10,10
";
    let ini: IniFile = IniFile::from_str(ini_text);
    let registry: InfantrySequenceRegistry = parse_infantry_sequence_registry(&ini);

    assert_eq!(registry.len(), 2);
    assert!(registry.contains_key("CONSEQUENCE"));
    assert!(registry.contains_key("BRUTESEQUENCE"));

    // BruteSequence FireUp has 10 facings (unusual).
    let brute: &InfantrySequenceDef = registry.get("BRUTESEQUENCE").expect("brute");
    let fire: &InfantrySequenceEntry = brute.entries.get("FIREUP").expect("fireup");
    assert_eq!(fire.facings, 10);
}

#[test]
fn test_non_sequence_sections_ignored() {
    let ini_text: &str = "\
[CONS]
Cameo=E2ICON
Voxel=yes

[ConSequence]
Walk=8,6,6
";
    let ini: IniFile = IniFile::from_str(ini_text);
    let registry: InfantrySequenceRegistry = parse_infantry_sequence_registry(&ini);

    // Only ConSequence should be parsed, not [CONS].
    assert_eq!(registry.len(), 1);
    assert!(registry.contains_key("CONSEQUENCE"));
}
