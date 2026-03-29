//! Radar event configuration parsed from rules.ini `[General]`.
//!
//! Controls the visual behavior of radar ping rectangles on the minimap.
//! Values from ModEnc: RadarEventMinRadius, RadarEventSpeed,
//! RadarEventRotationSpeed, RadarEventColorSpeed.
//!
//! ## Dependency rules
//! - Part of rules/ — depends only on rules/ini_parser.
//! - No dependencies on sim/, render/, ui/, etc.

use crate::rules::ini_parser::IniFile;

/// Radar event visual parameters from `[General]`.
///
/// These control the animated radar ping rectangles that appear on the
/// minimap when combat or other events occur.
#[derive(Debug, Clone, Copy)]
pub struct RadarEventConfig {
    /// Final rectangle size (in minimap pixels) after the zoom-in animation.
    /// Default 4. Larger = bigger final ping rectangle.
    pub min_radius: f32,
    /// Speed at which the ping rectangle shrinks from large to min_radius.
    /// Default 0.08. Higher = faster zoom-in.
    pub speed: f32,
    /// Rotation speed of the ping rectangle (radians per second).
    /// Default 0.12. RA2 rotates the rectangle slightly during animation.
    pub rotation_speed: f32,
    /// Speed of the brightness pulse effect (cycles per second).
    /// Default 0.05. Controls how fast the ping flashes.
    pub color_speed: f32,
    /// Duration in milliseconds that a radar event stays visible.
    /// Not directly a rules.ini key, but derived from typical RA2 behavior.
    pub event_duration_ms: u32,
    /// Maximum number of events kept in the ring buffer for Spacebar cycling.
    pub max_events: usize,
}

impl Default for RadarEventConfig {
    fn default() -> Self {
        Self {
            min_radius: 4.0,
            speed: 0.08,
            rotation_speed: 0.12,
            color_speed: 0.05,
            event_duration_ms: 13000,
            max_events: 8,
        }
    }
}

impl RadarEventConfig {
    /// Parse radar event config from `[General]` section of rules.ini.
    pub fn from_ini(ini: &IniFile) -> Self {
        let Some(general) = ini.section("General") else {
            return Self::default();
        };
        Self {
            min_radius: general.get_f32("RadarEventMinRadius").unwrap_or(4.0),
            speed: general.get_f32("RadarEventSpeed").unwrap_or(0.08),
            rotation_speed: general.get_f32("RadarEventRotationSpeed").unwrap_or(0.12),
            color_speed: general.get_f32("RadarEventColorSpeed").unwrap_or(0.05),
            event_duration_ms: general
                .get_i32("RadarEventDuration")
                .unwrap_or(13000)
                .max(0) as u32,
            max_events: 8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_reasonable_values() {
        let config = RadarEventConfig::default();
        assert!(config.min_radius > 0.0);
        assert!(config.speed > 0.0);
        assert!(config.event_duration_ms > 0);
        assert_eq!(config.max_events, 8);
    }

    #[test]
    fn parse_from_ini_overrides_defaults() {
        let ini = IniFile::from_str(
            "[General]\nRadarEventMinRadius=6.0\nRadarEventSpeed=0.15\n\
             RadarEventRotationSpeed=0.2\nRadarEventColorSpeed=0.1\n\
             RadarEventDuration=5000\n",
        );
        let config = RadarEventConfig::from_ini(&ini);
        assert!((config.min_radius - 6.0).abs() < 0.01);
        assert!((config.speed - 0.15).abs() < 0.01);
        assert!((config.rotation_speed - 0.2).abs() < 0.01);
        assert!((config.color_speed - 0.1).abs() < 0.01);
        assert_eq!(config.event_duration_ms, 5000);
    }

    #[test]
    fn missing_general_section_uses_defaults() {
        let ini = IniFile::from_str("[Map]\nTheater=TEMPERATE\n");
        let config = RadarEventConfig::from_ini(&ini);
        assert!((config.min_radius - 4.0).abs() < 0.01);
        assert!((config.speed - 0.08).abs() < 0.01);
    }
}
