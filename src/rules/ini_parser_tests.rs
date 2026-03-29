//! Unit tests for the INI parser (IniFile + IniSection).

use super::*;

#[test]
fn test_basic_parse() {
    let ini: IniFile = IniFile::from_str("[General]\nName=Test\nCost=1000\n[Combat]\nDamage=50\n");

    assert_eq!(ini.section_count(), 2);

    let general: &IniSection = ini.section("General").expect("Should have General");
    assert_eq!(general.get("Name"), Some("Test"));
    assert_eq!(general.get("Cost"), Some("1000"));

    let combat: &IniSection = ini.section("Combat").expect("Should have Combat");
    assert_eq!(combat.get("Damage"), Some("50"));
}

#[test]
fn test_case_insensitive_lookup() {
    let ini: IniFile = IniFile::from_str("[General]\nName=Test\n");

    // Section lookup is case-insensitive.
    assert!(ini.section("general").is_some());
    assert!(ini.section("GENERAL").is_some());
    assert!(ini.section("General").is_some());

    // Key lookup is case-insensitive.
    let section: &IniSection = ini.section("general").unwrap();
    assert_eq!(section.get("name"), Some("Test"));
    assert_eq!(section.get("NAME"), Some("Test"));
}

#[test]
fn test_comments_and_blank_lines() {
    let text: &str = "\
; This is a comment
# This is also a comment

[Section1]
Key1=Value1

; Another comment
Key2=Value2
";
    let ini: IniFile = IniFile::from_str(text);

    assert_eq!(ini.section_count(), 1);
    let section: &IniSection = ini.section("Section1").unwrap();
    assert_eq!(section.get("Key1"), Some("Value1"));
    assert_eq!(section.get("Key2"), Some("Value2"));
    assert_eq!(section.entry_count(), 2);
}

#[test]
fn test_inline_comments() {
    let ini: IniFile = IniFile::from_str("[Test]\nCost=1000 ; credits\nName=hello\n");

    let section: &IniSection = ini.section("Test").unwrap();
    assert_eq!(section.get("Cost"), Some("1000"));
    assert_eq!(section.get("Name"), Some("hello"));
}

#[test]
fn test_get_i32() {
    let ini: IniFile = IniFile::from_str("[Stats]\nCost=1000\nDamage=-50\nName=tank\n");

    let section: &IniSection = ini.section("Stats").unwrap();
    assert_eq!(section.get_i32("Cost"), Some(1000));
    assert_eq!(section.get_i32("Damage"), Some(-50));
    assert_eq!(section.get_i32("Name"), None); // Not a number
    assert_eq!(section.get_i32("Missing"), None); // Key doesn't exist
}

#[test]
fn test_get_f32() {
    let ini: IniFile = IniFile::from_str("[Stats]\nSpeed=5.5\nROF=0.1\n");

    let section: &IniSection = ini.section("Stats").unwrap();
    let speed: f32 = section.get_f32("Speed").unwrap();
    assert!((speed - 5.5).abs() < f32::EPSILON);
    let rof: f32 = section.get_f32("ROF").unwrap();
    assert!((rof - 0.1).abs() < 0.001);
}

#[test]
fn test_get_bool() {
    let ini: IniFile = IniFile::from_str(
        "[Flags]\nDoubleOwned=yes\nCloakable=no\nActive=true\nDebug=false\nBit=1\nOff=0\n",
    );

    let section: &IniSection = ini.section("Flags").unwrap();
    assert_eq!(section.get_bool("DoubleOwned"), Some(true));
    assert_eq!(section.get_bool("Cloakable"), Some(false));
    assert_eq!(section.get_bool("Active"), Some(true));
    assert_eq!(section.get_bool("Debug"), Some(false));
    assert_eq!(section.get_bool("Bit"), Some(true));
    assert_eq!(section.get_bool("Off"), Some(false));
}

#[test]
fn test_get_list() {
    let ini: IniFile = IniFile::from_str("[Build]\nPrereq=GAWEAP,RADAR,TECH\nEmpty=\n");

    let section: &IniSection = ini.section("Build").unwrap();

    let prereq: Vec<&str> = section.get_list("Prereq").unwrap();
    assert_eq!(prereq, vec!["GAWEAP", "RADAR", "TECH"]);

    let empty: Vec<&str> = section.get_list("Empty").unwrap();
    assert!(empty.is_empty());

    assert!(section.get_list("Missing").is_none());
}

#[test]
fn test_duplicate_sections_merge() {
    let text: &str = "\
[General]
Key1=First
Key2=Original

[General]
Key2=Override
Key3=New
";
    let ini: IniFile = IniFile::from_str(text);

    // Duplicate sections should merge, not create two separate sections.
    assert_eq!(ini.section_count(), 1);

    let section: &IniSection = ini.section("General").unwrap();
    assert_eq!(section.get("Key1"), Some("First")); // From first occurrence
    assert_eq!(section.get("Key2"), Some("Override")); // Overridden
    assert_eq!(section.get("Key3"), Some("New")); // From second occurrence
}

#[test]
fn test_section_names_order() {
    let ini: IniFile = IniFile::from_str("[Zebra]\n[Alpha]\n[Middle]\n");

    let names: Vec<&str> = ini.section_names();
    assert_eq!(names, vec!["Zebra", "Alpha", "Middle"]);
}

#[test]
fn test_from_bytes() {
    let data: &[u8] = b"[Test]\nKey=Value\n";
    let ini: IniFile = IniFile::from_bytes(data).expect("Should parse UTF-8");
    assert_eq!(ini.section("Test").unwrap().get("Key"), Some("Value"));
}

#[test]
fn test_get_values_zero_indexed() {
    let ini: IniFile = IniFile::from_str("[Types]\n0=E1\n1=E2\n2=ENGINEER\n3=FLAKT\n");
    let section: &IniSection = ini.section("Types").unwrap();
    let values: Vec<&str> = section.get_values();
    assert_eq!(values, vec!["E1", "E2", "ENGINEER", "FLAKT"]);
}

#[test]
fn test_get_values_one_indexed() {
    // RA2 retail rules.ini uses 1-indexed type registries.
    let ini: IniFile = IniFile::from_str("[InfantryTypes]\n1=E1\n2=E2\n3=SHK\n");
    let section: &IniSection = ini.section("InfantryTypes").unwrap();
    let values: Vec<&str> = section.get_values();
    assert_eq!(values, vec!["E1", "E2", "SHK"]);
}

#[test]
fn test_get_values_with_numeric_gaps() {
    let ini: IniFile =
        IniFile::from_str("[VehicleTypes]\n1=HTNK\n2=MTNK\n5=SMIN\n36=CMIN\n40=HARV\n");
    let section: &IniSection = ini.section("VehicleTypes").unwrap();
    let values: Vec<&str> = section.get_values();
    assert_eq!(values, vec!["HTNK", "MTNK", "SMIN", "CMIN", "HARV"]);
}

#[test]
fn test_get_values_empty_section() {
    let ini: IniFile = IniFile::from_str("[Empty]\nName=Test\n");
    let section: &IniSection = ini.section("Empty").unwrap();
    let values: Vec<&str> = section.get_values();
    assert!(values.is_empty());
}

#[test]
fn test_whitespace_handling() {
    let ini: IniFile = IniFile::from_str("[Test]\n  Key  =  Value  \n");

    let section: &IniSection = ini.section("Test").unwrap();
    assert_eq!(section.get("Key"), Some("Value"));
}

#[test]
fn test_get_percent() {
    let ini: IniFile = IniFile::from_str(
        "[AudioVisual]\nConditionRed=25%\nConditionYellow=50%\nBare=0.75\nBad=abc\n",
    );
    let section: &IniSection = ini.section("AudioVisual").unwrap();
    let red: f32 = section.get_percent("ConditionRed").unwrap();
    assert!((red - 0.25).abs() < f32::EPSILON);
    let yellow: f32 = section.get_percent("ConditionYellow").unwrap();
    assert!((yellow - 0.50).abs() < f32::EPSILON);
    // Bare float without % suffix works too.
    let bare: f32 = section.get_percent("Bare").unwrap();
    assert!((bare - 0.75).abs() < f32::EPSILON);
    // Non-numeric returns None.
    assert!(section.get_percent("Bad").is_none());
    assert!(section.get_percent("Missing").is_none());
}
