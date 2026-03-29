//! FLH (Forward/Lateral/Height) firing offset triplet.
//!
//! FLH defines where a projectile visually originates on a unit, relative to
//! the unit's turret pivot (or body center for turretless units).
//!
//! - **Forward**: along the unit's facing direction (positive = forward)
//! - **Lateral**: perpendicular to facing (positive = right of facing direction)
//! - **Height**: vertical offset (positive = up)
//!
//! Values are in leptons (256 leptons = 1 cell). Parsed from art.ini keys:
//! `PrimaryFireFLH=`, `SecondaryFireFLH=`, `ElitePrimaryFireFLH=`,
//! `EliteSecondaryFireFLH=`.
//!
//! ## Dependency rules
//! - Part of rules/ — no dependencies on sim/, render/, ui/, etc.

/// FLH (Forward, Lateral, Height) lepton offset triplet.
///
/// Defines where a projectile visually originates relative to the unit's
/// turret pivot (or body center for turretless units).
///
/// Values are in leptons (256 leptons = 1 cell).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flh {
    /// Distance along the unit's facing direction. Positive = forward.
    pub forward: i32,
    /// Distance perpendicular to facing. Positive = right of facing direction.
    pub lateral: i32,
    /// Vertical offset. Positive = up.
    pub height: i32,
}

/// Parse an FLH triplet from a comma-separated INI value string.
///
/// Accepts `"F,L,H"` (three integers) or `"F,L"` (height defaults to 0).
/// Returns `Flh::default()` (all zeros) if the value is None, empty, or malformed.
///
/// Uses the same pattern as `parse_exit_coord()` in object_type.rs.
pub fn parse_flh(value: Option<&str>) -> Flh {
    let val: &str = match value {
        Some(v) if !v.trim().is_empty() => v,
        _ => return Flh::default(),
    };

    let parts: Vec<&str> = val.split(',').collect();
    if parts.len() < 2 {
        return Flh::default();
    }

    let forward: i32 = match parts[0].trim().parse() {
        Ok(v) => v,
        Err(_) => return Flh::default(),
    };
    let lateral: i32 = match parts[1].trim().parse() {
        Ok(v) => v,
        Err(_) => return Flh::default(),
    };
    let height: i32 = parts
        .get(2)
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    Flh {
        forward,
        lateral,
        height,
    }
}

/// Resolve the effective FLH for a weapon slot, considering veterancy.
///
/// Elite units (veterancy >= 200) use the elite FLH override if the art.ini
/// section defines one. Otherwise falls back to the normal FLH.
///
/// `is_primary`: true for primary weapon, false for secondary.
/// `veterancy`: entity veterancy level (0=rookie, 100=veteran, 200=elite).
pub fn resolve_flh(
    primary_flh: Flh,
    secondary_flh: Flh,
    elite_primary_flh: Option<Flh>,
    elite_secondary_flh: Option<Flh>,
    is_primary: bool,
    veterancy: u16,
) -> Flh {
    /// Veterancy level at which elite FLH overrides apply.
    const ELITE_VETERANCY_THRESHOLD: u16 = 200;

    let is_elite: bool = veterancy >= ELITE_VETERANCY_THRESHOLD;

    if is_primary {
        if is_elite {
            elite_primary_flh.unwrap_or(primary_flh)
        } else {
            primary_flh
        }
    } else if is_elite {
        elite_secondary_flh.unwrap_or(secondary_flh)
    } else {
        secondary_flh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_flh_full_triplet() {
        let flh: Flh = parse_flh(Some("150,0,100"));
        assert_eq!(
            flh,
            Flh {
                forward: 150,
                lateral: 0,
                height: 100
            }
        );
    }

    #[test]
    fn test_parse_flh_two_elements() {
        let flh: Flh = parse_flh(Some("100,-25"));
        assert_eq!(
            flh,
            Flh {
                forward: 100,
                lateral: -25,
                height: 0
            }
        );
    }

    #[test]
    fn test_parse_flh_with_spaces() {
        let flh: Flh = parse_flh(Some(" 80 , 0 , 120 "));
        assert_eq!(
            flh,
            Flh {
                forward: 80,
                lateral: 0,
                height: 120
            }
        );
    }

    #[test]
    fn test_parse_flh_negative_values() {
        let flh: Flh = parse_flh(Some("-160,0,75"));
        assert_eq!(
            flh,
            Flh {
                forward: -160,
                lateral: 0,
                height: 75
            }
        );
    }

    #[test]
    fn test_parse_flh_none() {
        let flh: Flh = parse_flh(None);
        assert_eq!(flh, Flh::default());
    }

    #[test]
    fn test_parse_flh_empty_string() {
        let flh: Flh = parse_flh(Some(""));
        assert_eq!(flh, Flh::default());
    }

    #[test]
    fn test_parse_flh_single_value() {
        // Only one number — not a valid FLH, needs at least F,L.
        let flh: Flh = parse_flh(Some("100"));
        assert_eq!(flh, Flh::default());
    }

    #[test]
    fn test_parse_flh_malformed() {
        let flh: Flh = parse_flh(Some("abc,def,ghi"));
        assert_eq!(flh, Flh::default());
    }

    #[test]
    fn test_parse_flh_zero_triplet() {
        let flh: Flh = parse_flh(Some("0,0,0"));
        assert_eq!(
            flh,
            Flh {
                forward: 0,
                lateral: 0,
                height: 0
            }
        );
    }

    #[test]
    fn test_parse_flh_large_values() {
        // Real-world: [DEST] Destroyer has PrimaryFireFLH=280,0,120
        let flh: Flh = parse_flh(Some("280,0,120"));
        assert_eq!(
            flh,
            Flh {
                forward: 280,
                lateral: 0,
                height: 120
            }
        );
    }

    // --- resolve_flh tests ---

    const PRI: Flh = Flh {
        forward: 150,
        lateral: 0,
        height: 100,
    };
    const SEC: Flh = Flh {
        forward: 80,
        lateral: -25,
        height: 120,
    };
    const ELITE_PRI: Flh = Flh {
        forward: 200,
        lateral: 10,
        height: 150,
    };

    #[test]
    fn test_resolve_flh_normal_primary() {
        let flh: Flh = resolve_flh(PRI, SEC, Some(ELITE_PRI), None, true, 0);
        assert_eq!(flh, PRI);
    }

    #[test]
    fn test_resolve_flh_normal_secondary() {
        let flh: Flh = resolve_flh(PRI, SEC, Some(ELITE_PRI), None, false, 0);
        assert_eq!(flh, SEC);
    }

    #[test]
    fn test_resolve_flh_elite_with_override() {
        let flh: Flh = resolve_flh(PRI, SEC, Some(ELITE_PRI), None, true, 200);
        assert_eq!(flh, ELITE_PRI);
    }

    #[test]
    fn test_resolve_flh_elite_without_override_falls_back() {
        let flh: Flh = resolve_flh(PRI, SEC, None, None, true, 200);
        // Falls back to normal primary FLH.
        assert_eq!(flh, PRI);
    }

    #[test]
    fn test_resolve_flh_veteran_uses_normal() {
        // Veteran (100) is not elite — should use normal FLH.
        let flh: Flh = resolve_flh(PRI, SEC, Some(ELITE_PRI), None, true, 100);
        assert_eq!(flh, PRI);
    }

    #[test]
    fn test_resolve_flh_elite_secondary_with_override() {
        let elite_sec: Flh = Flh {
            forward: 100,
            lateral: -10,
            height: 140,
        };
        let flh: Flh = resolve_flh(PRI, SEC, None, Some(elite_sec), false, 200);
        assert_eq!(flh, elite_sec);
    }
}
