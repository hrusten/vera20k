//! Shared helpers for instance builders — depth sorting, interpolation, visibility.
//!
//! These utilities are used by the unit, SHP, and overlay instance builders.
//! Extracted from app_instances.rs to keep files under the 600-line limit.
//!
//! ## Dependency rules
//! - Part of the app layer — may depend on everything.

use crate::app::AppState;
use crate::map::terrain;
use crate::sim::components::Position;
use crate::sim::game_entity::GameEntity;
use crate::sim::intern::InternedId;
use crate::sim::vision::FogState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CellVisibilityState {
    Visible,
    Shrouded,
}

/// Compute depth for a sprite from screen position.
///
/// Used ONLY for terrain occlusion (cliff depth test). Sprites do not write
/// to the depth buffer — sprite-vs-sprite ordering is handled by draw order
/// (painter's algorithm). The depth value determines whether a sprite pixel
/// passes the LessEqual test against terrain Z-data.
///
/// Lower screen_y → larger depth (further from camera).
/// Higher elevation (z) → slightly smaller depth (closer to camera).
pub(crate) fn compute_sprite_depth(state: &AppState, screen_y: f32, z: u8) -> f32 {
    let (origin_y, world_height) = state
        .terrain_grid
        .as_ref()
        .map(|g| (g.origin_y, g.world_height))
        .unwrap_or((0.0, 1.0));
    compute_sprite_depth_params(origin_y, world_height, screen_y, z)
}

/// Compute sprite depth from explicit parameters.
/// Same formula as `compute_sprite_depth` but for callers that already have
/// origin_y and world_height (avoids re-extracting from AppState).
pub(crate) fn compute_sprite_depth_params(
    origin_y: f32,
    world_height: f32,
    screen_y: f32,
    z: u8,
) -> f32 {
    let iso_row: f32 = screen_y + z as f32 * terrain::HEIGHT_STEP;
    let normalized: f32 = ((iso_row - origin_y) / world_height).clamp(0.0, 1.0);
    let z_bias: f32 = z as f32 * 0.0001;
    (1.0 - normalized - z_bias).clamp(0.001, 0.999)
}

pub(crate) fn is_near_bridge_cell(state: &AppState, rx: u16, ry: u16) -> bool {
    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            let nx = rx as i32 + dx;
            let ny = ry as i32 + dy;
            if nx < 0 || ny < 0 {
                continue;
            }
            if state
                .bridge_height_map
                .contains_key(&(nx as u16, ny as u16))
            {
                return true;
            }
        }
    }
    false
}

pub(crate) fn is_under_bridge_render_state(state: &AppState, entity: &GameEntity) -> bool {
    entity.bridge_occupancy.is_none()
        && is_near_bridge_cell(state, entity.position.rx, entity.position.ry)
}

pub(crate) fn apply_bridge_depth_bias(state: &AppState, entity: &GameEntity, depth: f32) -> f32 {
    if !is_under_bridge_render_state(state, entity) {
        return depth;
    }
    (depth + entity.zfudge_bridge.max(0) as f32 * 0.00002).clamp(0.001, 0.999)
}

/// Convenience wrapper that takes a `GameEntity` directly.
/// Sub-cell offsets are already baked into `screen_x`/`screen_y` via
/// `lepton_to_screen()` in the sim tick, so no extra offset is needed.
pub(crate) fn interpolated_screen_position_entity(
    entity: &crate::sim::game_entity::GameEntity,
) -> (f32, f32) {
    (entity.position.screen_x, entity.position.screen_y)
}

/// Check whether an entity is visible to the local player based on shroud.
///
/// In standard YR (FogOfWar=false), once a cell is explored it stays fully
/// visible forever. Friendly entities are always visible. Enemy entities are
/// visible if the cell they occupy has been explored (revealed).
pub(crate) fn is_entity_visible_for_local_owner(
    local_owner: Option<&str>,
    fog: &FogState,
    pos: &Position,
    owner: &str,
    ignore_visibility: bool,
    local_owner_id: Option<InternedId>,
) -> bool {
    if ignore_visibility {
        return true;
    }
    let Some(local_owner) = local_owner else {
        return true;
    };
    if fog.is_friendly(local_owner, owner) {
        return true;
    }
    let owner_id = local_owner_id.unwrap_or_default();
    fog.is_cell_revealed(owner_id, pos.rx, pos.ry)
        && !fog.is_cell_gap_covered(owner_id, pos.rx, pos.ry)
}

pub(crate) fn cell_visibility_for_local_owner(
    local_owner_id: Option<InternedId>,
    fog: Option<&FogState>,
    rx: u16,
    ry: u16,
    ignore_visibility: bool,
) -> CellVisibilityState {
    if ignore_visibility {
        return CellVisibilityState::Visible;
    }
    let Some(local_owner_id) = local_owner_id else {
        return CellVisibilityState::Visible;
    };
    let Some(fog) = fog else {
        return CellVisibilityState::Visible;
    };
    // Standard YR (FogOfWar=false): explored = fully visible, no intermediate state.
    if fog.is_cell_revealed(local_owner_id, rx, ry) {
        CellVisibilityState::Visible
    } else {
        CellVisibilityState::Shrouded
    }
}

/// Viewport frustum cull check: is the entity's bounding box visible on screen?
pub(crate) fn in_view(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    cam_x: f32,
    cam_y: f32,
    sw: f32,
    sh: f32,
    m: f32,
) -> bool {
    x + w >= cam_x - m && x <= cam_x + sw + m && y + h >= cam_y - m && y <= cam_y + sh + m
}
