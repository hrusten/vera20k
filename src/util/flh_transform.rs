//! FLH (Forward/Lateral/Height) to screen-space transform.
//!
//! Converts FLH lepton offsets into isometric screen-space pixel offsets,
//! given a facing direction. This is the generalized version of
//! `turret_screen_offset()` in app_instances/units.rs, which handles only
//! the Forward component (Lateral=0, Height=0).
//!
//! ## Math
//! 1. Rotate the (Forward, Lateral) vector by the facing angle:
//!    - RA2 facing: 0=N, 64=E, 128=S, 192=W
//!    - angle = TAU × (facing / 256.0)
//!    - world_x = Forward × sin(angle) + Lateral × cos(angle)
//!    - world_y = −Forward × cos(angle) + Lateral × sin(angle)
//! 2. Convert world leptons to isometric screen pixels:
//!    - screen_x = (world_x − world_y) × 30.0 / 256.0
//!    - screen_y = (world_x + world_y) × 15.0 / 256.0 − Height × HEIGHT_SCREEN_FACTOR
//!
//! ## Dependency rules
//! - Part of util/ — no dependencies on other game modules.

/// Pixels per lepton along the isometric X axis (half tile width / leptons per cell).
/// 60px tile width / 2 / 256 leptons = 30/256.
const SCREEN_X_PER_LEPTON: f32 = 30.0 / 256.0;

/// Pixels per lepton along the isometric Y axis (half tile height / leptons per cell).
/// 30px tile height / 2 / 256 leptons = 15/256.
const SCREEN_Y_PER_LEPTON: f32 = 15.0 / 256.0;

/// Height-to-screen factor. FLH height maps to a vertical screen offset.
/// Uses the same isometric Y ratio as ground coordinates — this matches
/// how RA2's engine projects height offsets onto the isometric plane.
const HEIGHT_SCREEN_FACTOR: f32 = SCREEN_Y_PER_LEPTON;

/// Convert an FLH lepton offset into an isometric screen-space pixel offset.
///
/// `forward`: distance along the unit's facing direction (positive = forward).
/// `lateral`: distance perpendicular to facing (positive = right of facing).
/// `height`: vertical offset (positive = up, produces negative screen Y).
/// `facing`: RA2 facing byte (0=N, 64=E, 128=S, 192=W).
///
/// Returns `(screen_dx, screen_dy)` in pixels, relative to the unit's center.
pub fn flh_to_screen_offset(forward: i32, lateral: i32, height: i32, facing: u8) -> (f32, f32) {
    if forward == 0 && lateral == 0 && height == 0 {
        return (0.0, 0.0);
    }

    // Convert facing (0–255) to radians.
    let angle: f32 = std::f32::consts::TAU * (facing as f32 / 256.0);
    let (sin, cos) = angle.sin_cos();

    let f: f32 = forward as f32;
    let l: f32 = lateral as f32;

    // Rotate (Forward, Lateral) by facing angle into world-space leptons.
    // Forward aligns with the facing direction (sin for X, -cos for Y).
    // Lateral is perpendicular (cos for X, sin for Y).
    let world_x: f32 = f * sin + l * cos;
    let world_y: f32 = -f * cos + l * sin;

    // Convert world leptons to isometric screen pixels.
    let screen_x: f32 = (world_x - world_y) * SCREEN_X_PER_LEPTON;
    let screen_y: f32 =
        (world_x + world_y) * SCREEN_Y_PER_LEPTON - height as f32 * HEIGHT_SCREEN_FACTOR;

    (screen_x, screen_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_flh_returns_zero() {
        let (sx, sy) = flh_to_screen_offset(0, 0, 0, 0);
        assert!((sx).abs() < 0.001);
        assert!((sy).abs() < 0.001);
    }

    #[test]
    fn test_forward_only_facing_north() {
        // Facing 0 (north): forward goes in -Y world direction.
        // world_x = 100 * sin(0) + 0 = 0
        // world_y = -100 * cos(0) + 0 = -100
        // screen_x = (0 - (-100)) * 30/256 = 100 * 0.117 ≈ 11.72
        // screen_y = (0 + (-100)) * 15/256 = -100 * 0.0586 ≈ -5.86
        let (sx, sy) = flh_to_screen_offset(100, 0, 0, 0);
        assert!((sx - 11.72).abs() < 0.1, "sx={sx}");
        assert!((sy - (-5.86)).abs() < 0.1, "sy={sy}");
    }

    #[test]
    fn test_forward_matches_turret_screen_offset_pattern() {
        // Facing 64 (east): angle = π/2, sin=1, cos=0
        // world_x = 100*1 + 0 = 100, world_y = -100*0 + 0 = 0
        // screen_x = (100-0) * 30/256 ≈ 11.72
        // screen_y = (100+0) * 15/256 ≈ 5.86
        let (sx, sy) = flh_to_screen_offset(100, 0, 0, 64);
        assert!((sx - 11.72).abs() < 0.1, "sx={sx}");
        assert!((sy - 5.86).abs() < 0.1, "sy={sy}");
    }

    #[test]
    fn test_facing_south_reverses_forward() {
        // Facing 128 (south): angle = π, sin≈0, cos=-1
        // world_x ≈ 0, world_y = -100*(-1) = 100
        // screen_x = (0-100) * 30/256 ≈ -11.72
        // screen_y = (0+100) * 15/256 ≈ 5.86
        let (sx, sy) = flh_to_screen_offset(100, 0, 0, 128);
        assert!((sx - (-11.72)).abs() < 0.1, "sx={sx}");
        assert!((sy - 5.86).abs() < 0.1, "sy={sy}");
    }

    #[test]
    fn test_lateral_only_facing_north() {
        // Facing 0 (north): lateral goes in +X world direction (right of north = east).
        // world_x = 0 + 50*cos(0) = 50
        // world_y = 0 + 50*sin(0) = 0
        // screen_x = (50-0) * 30/256 ≈ 5.86
        // screen_y = (50+0) * 15/256 ≈ 2.93
        let (sx, sy) = flh_to_screen_offset(0, 50, 0, 0);
        assert!((sx - 5.86).abs() < 0.1, "sx={sx}");
        assert!((sy - 2.93).abs() < 0.1, "sy={sy}");
    }

    #[test]
    fn test_height_only_produces_vertical_offset() {
        // Height only — no rotation, just vertical screen offset.
        // screen_y = -100 * 15/256 ≈ -5.86
        let (sx, sy) = flh_to_screen_offset(0, 0, 100, 0);
        assert!((sx).abs() < 0.001, "sx={sx}");
        assert!((sy - (-5.86)).abs() < 0.1, "sy={sy}");
    }

    #[test]
    fn test_combined_flh_east_facing() {
        // Facing 64 (east), F=150, L=0, H=100.
        // angle = π/2, sin=1, cos=0
        // world_x = 150, world_y = 0
        // screen_x = 150 * 30/256 ≈ 17.58
        // screen_y = 150 * 15/256 - 100 * 15/256 ≈ 8.79 - 5.86 ≈ 2.93
        let (sx, sy) = flh_to_screen_offset(150, 0, 100, 64);
        assert!((sx - 17.58).abs() < 0.1, "sx={sx}");
        assert!((sy - 2.93).abs() < 0.1, "sy={sy}");
    }

    #[test]
    fn test_negative_lateral() {
        // Facing 0 (north), L=-50 means left of facing direction.
        // world_x = -50*cos(0) = -50
        // screen_x = (-50) * 30/256 ≈ -5.86
        let (sx, _sy) = flh_to_screen_offset(0, -50, 0, 0);
        assert!((sx - (-5.86)).abs() < 0.1, "sx={sx}");
    }
}
