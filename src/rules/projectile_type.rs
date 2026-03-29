//! Projectile type definitions parsed from rules.ini.
//!
//! Each projectile in RA2 has its own `[ProjectileName]` section in rules.ini,
//! defining targeting capabilities (AA/AG), flight behavior (speed, arcing,
//! homing turn rate), and accuracy. Weapons reference projectiles via their
//! `Projectile=` key.
//!
//! ## Targeting flags
//! - `AA=yes` — projectile can hit aircraft (anti-air)
//! - `AG=yes` — projectile can hit ground units and buildings (default true)
//!
//! These flags determine which weapon (Primary vs Secondary) a unit uses
//! against a given target type. A typical tank has AG=yes Primary and
//! AA=yes Secondary, enabling automatic weapon switching.
//!
//! ## Dependency rules
//! - Part of rules/ — no dependencies on sim/, render/, ui/, etc.

use crate::rules::ini_parser::IniSection;

/// A projectile definition parsed from a rules.ini section.
///
/// Projectiles define how a weapon's shot travels and what it can target.
/// The targeting flags (AA/AG) are critical for weapon selection: if a
/// projectile can't hit the target type, the combat system falls back to
/// the unit's Secondary weapon.
///
/// All 37 fields are mapped from BulletTypeClass::ReadINI (0x46bee0).
#[derive(Debug, Clone)]
pub struct ProjectileType {
    /// Section name in rules.ini (e.g., "InvisibleLow", "MissileAA").
    pub id: String,

    // --- Targeting flags ---
    /// Can hit aircraft. Anti-air weapons need this flag.
    pub aa: bool,
    /// Can hit ground units and buildings. True by default for most projectiles.
    pub ag: bool,

    // --- Flight behavior ---
    /// Ballistic arc trajectory (e.g., artillery shells). Cannot be intercepted.
    pub arcing: bool,
    /// Homing missile turn rate (higher = tighter tracking). 0 = no homing.
    pub rot: i32,
    /// Random spread on impact (e.g., rapid-fire infantry weapons).
    pub inaccurate: bool,
    /// Projectile travel speed in pixels per game frame. 0 = instant hit.
    /// Binary offset: +0x2F0 (labeled "Arm" in the binary, read via "Speed" key).
    pub speed: i32,

    // --- Bool flags (offsets from BulletTypeClass base) ---
    /// Projectile explodes in the air, releasing sub-munitions. (+0x294)
    pub airburst: bool,
    /// Projectile floats above water instead of splashing. (+0x295)
    pub floater: bool,
    /// Trajectory affected by cliff elevation changes. (+0x296)
    pub subject_to_cliffs: bool,
    /// Trajectory affected by terrain elevation. (+0x297)
    pub subject_to_elevation: bool,
    /// Trajectory blocked by walls. (+0x298)
    pub subject_to_walls: bool,
    /// Flies at very high altitude (e.g., V3 rocket). (+0x299)
    pub very_high: bool,
    /// Casts a shadow on the ground while in flight. (+0x29A)
    pub shadow: bool,
    /// Projectile drops from above (e.g., paratroopers, bombs). (+0x29C)
    pub dropping: bool,
    /// Uses level flight trajectory. (+0x29D)
    pub level: bool,
    /// Invisible projectile (no sprite drawn). (+0x29E)
    pub inviso: bool,
    /// Detonates near the target rather than on contact. (+0x29F)
    pub proximity: bool,
    /// Has a maximum range (non-homing). (+0x2A0)
    pub ranged: bool,
    /// Whether the projectile sprite rotates to face travel direction. (+0x2A1)
    /// NOTE: Inverted in binary! The engine stores `!ReadBool("Rotates")`.
    /// Default is true (projectiles rotate by default).
    pub rotates: bool,
    /// Scatters like flak — random spread pattern for AA. (+0x2A3)
    pub flak_scatter: bool,
    /// Projectile loses health over time. (+0x2A6)
    pub degenerates: bool,
    /// Bounces off terrain on impact. (+0x2A7)
    pub bouncy: bool,
    /// Uses a custom animation palette instead of the unit palette. (+0x2A8)
    pub anim_palette: bool,
    /// Uses the firing unit's palette for rendering. (+0x2A9)
    pub firers_palette: bool,
    /// Projectile can be scaled (e.g., for perspective). (+0x2EC)
    pub scalable: bool,
    /// Launches vertically before turning toward target. (+0x2C0)
    pub vertical: bool,
    /// Flat trajectory (read from Image section in art.ini). (+0x2F7)
    pub flat: bool,

    // --- Integer fields ---
    /// Number of sub-projectiles on detonation. (+0x2AC)
    pub cluster: i32,
    /// Number of shrapnel fragments on impact. (+0x2B8)
    pub shrapnel_count: i32,
    /// Altitude at which the projectile detonates (for airburst). (+0x2BC)
    pub detonation_altitude: i32,
    /// Acceleration rate for missiles. (+0x2D0)
    pub acceleration: i32,
    /// Number of frames the projectile flies straight before homing. (+0x2E0)
    pub course_lock_duration: i32,
    /// Delay in frames between spawning sub-projectiles (read from Image section). (+0x2E4)
    pub spawn_delay: i32,
    /// Arming delay — frames before the projectile can detonate. (+0x2F0)
    /// This is the same offset as "Speed" in the binary struct.
    pub arm: i32,
    /// Lowest animation frame index for in-flight animation. (+0x2F4)
    pub anim_low: i32,
    /// Highest animation frame index for in-flight animation. (+0x2F5)
    pub anim_high: i32,
    /// Animation rate (frames between animation steps). (+0x2F6)
    pub anim_rate: i32,

    // --- Float fields ---
    /// Bounce elasticity coefficient (0.0 = no bounce, 1.0 = perfect bounce). (+0x2C8)
    pub elasticity: f64,

    // --- Color ---
    /// Projectile trail color as RGB. (+0x2D4)
    pub color: [u8; 3],

    // --- String/reference fields ---
    /// Weapon fired on airburst detonation (weapon type name).
    pub airburst_weapon: Option<String>,
    /// Weapon fired for each shrapnel fragment (weapon type name).
    pub shrapnel_weapon: Option<String>,
    /// Animation played as a trail behind the projectile (anim type name).
    pub trailer: Option<String>,
}

impl ProjectileType {
    /// Parse a ProjectileType from a rules.ini section.
    ///
    /// AG defaults to true because most projectiles can hit ground targets.
    /// Rotates defaults to true (projectiles rotate by default); the binary
    /// inverts this value when storing it.
    /// All other flags default to false/0.
    ///
    /// `image_section` is the optional art.ini section resolved via Image= key.
    /// Some fields (Flat, SpawnDelay) are read from this section when present.
    pub fn from_ini_section(
        id: &str,
        section: &IniSection,
        image_section: Option<&IniSection>,
    ) -> Self {
        // Parse "R,G,B" color string into [u8; 3], defaulting to [0,0,0].
        let color = section
            .get("Color")
            .and_then(|s| {
                let parts: Vec<&str> = s.split(',').collect();
                if parts.len() == 3 {
                    let r = parts[0].trim().parse::<u8>().ok()?;
                    let g = parts[1].trim().parse::<u8>().ok()?;
                    let b = parts[2].trim().parse::<u8>().ok()?;
                    Some([r, g, b])
                } else {
                    None
                }
            })
            .unwrap_or([0, 0, 0]);

        Self {
            id: id.to_string(),
            // Targeting
            aa: section.get_bool("AA").unwrap_or(false),
            ag: section.get_bool("AG").unwrap_or(true),
            // Flight behavior
            arcing: section.get_bool("Arcing").unwrap_or(false),
            rot: section.get_i32("ROT").unwrap_or(0),
            inaccurate: section.get_bool("Inaccurate").unwrap_or(false),
            speed: section.get_i32("Speed").unwrap_or(0),
            // Bool flags
            airburst: section.get_bool("Airburst").unwrap_or(false),
            floater: section.get_bool("Floater").unwrap_or(false),
            subject_to_cliffs: section.get_bool("SubjectToCliffs").unwrap_or(false),
            subject_to_elevation: section.get_bool("SubjectToElevation").unwrap_or(false),
            subject_to_walls: section.get_bool("SubjectToWalls").unwrap_or(false),
            very_high: section.get_bool("VeryHigh").unwrap_or(false),
            shadow: section.get_bool("Shadow").unwrap_or(false),
            dropping: section.get_bool("Dropping").unwrap_or(false),
            level: section.get_bool("Level").unwrap_or(false),
            inviso: section.get_bool("Inviso").unwrap_or(false),
            proximity: section.get_bool("Proximity").unwrap_or(false),
            ranged: section.get_bool("Ranged").unwrap_or(false),
            // Binary inverts Rotates: stored = !ReadBool("Rotates"). Default is true.
            rotates: section.get_bool("Rotates").map(|v| !v).unwrap_or(true),
            flak_scatter: section.get_bool("FlakScatter").unwrap_or(false),
            degenerates: section.get_bool("Degenerates").unwrap_or(false),
            bouncy: section.get_bool("Bouncy").unwrap_or(false),
            anim_palette: section.get_bool("AnimPalette").unwrap_or(false),
            firers_palette: section.get_bool("FirersPalette").unwrap_or(false),
            scalable: section.get_bool("Scalable").unwrap_or(false),
            vertical: section.get_bool("Vertical").unwrap_or(false),
            // Flat is read from the Image section in art.ini
            flat: image_section
                .and_then(|s| s.get_bool("Flat"))
                .unwrap_or(false),
            // Integer fields
            cluster: section.get_i32("Cluster").unwrap_or(0),
            shrapnel_count: section.get_i32("ShrapnelCount").unwrap_or(0),
            detonation_altitude: section.get_i32("DetonationAltitude").unwrap_or(0),
            acceleration: section.get_i32("Acceleration").unwrap_or(0),
            course_lock_duration: section.get_i32("CourseLockDuration").unwrap_or(0),
            // SpawnDelay is read from the Image section in art.ini
            spawn_delay: image_section
                .and_then(|s| s.get_i32("SpawnDelay"))
                .unwrap_or(0),
            arm: section.get_i32("Arm").unwrap_or(0),
            anim_low: section.get_i32("AnimLow").unwrap_or(0),
            anim_high: section.get_i32("AnimHigh").unwrap_or(0),
            anim_rate: section.get_i32("AnimRate").unwrap_or(0),
            // Float
            elasticity: section
                .get("Elasticity")
                .and_then(|s| s.trim().parse::<f64>().ok())
                .unwrap_or(0.0),
            // Color
            color,
            // String/reference fields
            airburst_weapon: section.get("AirburstWeapon").map(|s| s.trim().to_string()),
            shrapnel_weapon: section.get("ShrapnelWeapon").map(|s| s.trim().to_string()),
            trailer: section.get("Trailer").map(|s| s.trim().to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::ini_parser::IniFile;

    #[test]
    fn test_parse_aa_projectile() {
        let ini: IniFile = IniFile::from_str("[MissileAA]\nAA=yes\nAG=no\nSpeed=60\nROT=20\n");
        let section: &IniSection = ini.section("MissileAA").unwrap();
        let proj: ProjectileType = ProjectileType::from_ini_section("MissileAA", section, None);

        assert_eq!(proj.id, "MissileAA");
        assert!(proj.aa);
        assert!(!proj.ag);
        assert_eq!(proj.speed, 60);
        assert_eq!(proj.rot, 20);
        assert!(!proj.arcing);
        assert!(!proj.inaccurate);
        // Rotates defaults to true when not specified
        assert!(proj.rotates);
    }

    #[test]
    fn test_defaults() {
        let ini: IniFile = IniFile::from_str("[Empty]\n");
        let section: &IniSection = ini.section("Empty").unwrap();
        let proj: ProjectileType = ProjectileType::from_ini_section("Empty", section, None);

        assert!(!proj.aa);
        assert!(proj.ag, "AG should default to true");
        assert_eq!(proj.speed, 0);
        assert!(!proj.arcing);
        assert_eq!(proj.rot, 0);
        assert!(!proj.inaccurate);
        // All new bool fields default to false
        assert!(!proj.airburst);
        assert!(!proj.floater);
        assert!(!proj.subject_to_cliffs);
        assert!(!proj.subject_to_elevation);
        assert!(!proj.subject_to_walls);
        assert!(!proj.very_high);
        assert!(!proj.shadow);
        assert!(!proj.dropping);
        assert!(!proj.level);
        assert!(!proj.inviso);
        assert!(!proj.proximity);
        assert!(!proj.ranged);
        assert!(proj.rotates, "Rotates should default to true");
        assert!(!proj.flak_scatter);
        assert!(!proj.degenerates);
        assert!(!proj.bouncy);
        assert!(!proj.anim_palette);
        assert!(!proj.firers_palette);
        assert!(!proj.scalable);
        assert!(!proj.vertical);
        assert!(!proj.flat);
        // Int fields default to 0
        assert_eq!(proj.cluster, 0);
        assert_eq!(proj.shrapnel_count, 0);
        assert_eq!(proj.detonation_altitude, 0);
        assert_eq!(proj.acceleration, 0);
        assert_eq!(proj.course_lock_duration, 0);
        assert_eq!(proj.spawn_delay, 0);
        assert_eq!(proj.arm, 0);
        assert_eq!(proj.anim_low, 0);
        assert_eq!(proj.anim_high, 0);
        assert_eq!(proj.anim_rate, 0);
        // Float defaults
        assert_eq!(proj.elasticity, 0.0);
        // Color defaults
        assert_eq!(proj.color, [0, 0, 0]);
        // String fields default to None
        assert!(proj.airburst_weapon.is_none());
        assert!(proj.shrapnel_weapon.is_none());
        assert!(proj.trailer.is_none());
    }

    #[test]
    fn test_arcing_projectile() {
        let ini: IniFile =
            IniFile::from_str("[ArtilleryShell]\nAG=yes\nArcing=yes\nSpeed=30\nInaccurate=yes\n");
        let section: &IniSection = ini.section("ArtilleryShell").unwrap();
        let proj: ProjectileType =
            ProjectileType::from_ini_section("ArtilleryShell", section, None);

        assert!(proj.ag);
        assert!(proj.arcing);
        assert_eq!(proj.speed, 30);
        assert!(proj.inaccurate);
    }

    #[test]
    fn test_rotates_inversion() {
        // Rotates=yes in INI → stored as false (binary inverts)
        // But we expose the *inverted* value, matching the binary's stored form.
        // So Rotates=yes → rotates=false, Rotates=no → rotates=true.
        let ini: IniFile =
            IniFile::from_str("[RotYes]\nRotates=yes\n[RotNo]\nRotates=no\n[NoKey]\n");

        let sec_yes = ini.section("RotYes").unwrap();
        let proj_yes = ProjectileType::from_ini_section("RotYes", sec_yes, None);
        // Rotates=yes → binary stores false → our field = false
        assert!(
            !proj_yes.rotates,
            "Rotates=yes should invert to false (matching binary storage)"
        );

        let sec_no = ini.section("RotNo").unwrap();
        let proj_no = ProjectileType::from_ini_section("RotNo", sec_no, None);
        // Rotates=no → binary stores true → our field = true
        assert!(
            proj_no.rotates,
            "Rotates=no should invert to true (matching binary storage)"
        );

        let sec_none = ini.section("NoKey").unwrap();
        let proj_none = ProjectileType::from_ini_section("NoKey", sec_none, None);
        // No Rotates key → default true (binary default)
        assert!(
            proj_none.rotates,
            "Missing Rotates key should default to true"
        );
    }

    #[test]
    fn test_color_parsing() {
        let ini: IniFile = IniFile::from_str("[Tracer]\nColor=255,128,0\n");
        let section = ini.section("Tracer").unwrap();
        let proj = ProjectileType::from_ini_section("Tracer", section, None);

        assert_eq!(proj.color, [255, 128, 0]);
    }

    #[test]
    fn test_string_fields() {
        let ini: IniFile = IniFile::from_str(
            "[V3Rocket]\nAirburstWeapon=V3Warhead\nShrapnelWeapon=ShrapWep\nTrailer=V3TRAIL\n",
        );
        let section = ini.section("V3Rocket").unwrap();
        let proj = ProjectileType::from_ini_section("V3Rocket", section, None);

        assert_eq!(proj.airburst_weapon.as_deref(), Some("V3Warhead"));
        assert_eq!(proj.shrapnel_weapon.as_deref(), Some("ShrapWep"));
        assert_eq!(proj.trailer.as_deref(), Some("V3TRAIL"));
    }

    #[test]
    fn test_image_section_fields() {
        let ini: IniFile = IniFile::from_str("[Proj]\n[ProjImage]\nFlat=yes\nSpawnDelay=10\n");
        let proj_section = ini.section("Proj").unwrap();
        let image_section = ini.section("ProjImage").unwrap();
        let proj = ProjectileType::from_ini_section("Proj", proj_section, Some(image_section));

        assert!(proj.flat);
        assert_eq!(proj.spawn_delay, 10);
    }
}
