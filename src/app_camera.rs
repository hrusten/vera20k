//! Camera positioning — keyboard scroll, mouse edge scroll, zoom, and clamping.
//!
//! Extracted from app_sim_tick.rs to separate camera control from sim advancement.
//!
//! ## Dependency rules
//! - Part of the app layer — may depend on everything.

use crate::app::AppState;
use crate::map::terrain;

/// Camera scroll speed in pixels per frame (arrow keys).
const CAMERA_SCROLL_SPEED: f32 = 8.0;
/// Screen edge margin for mouse-based scrolling (pixels from window edge).
const EDGE_SCROLL_MARGIN: f32 = 10.0;

/// Minimum zoom level — zoomed out enough to see a large portion of the map.
const MIN_ZOOM: f32 = 0.25;
/// Maximum zoom level — zoomed in close to pixel-level detail.
const MAX_ZOOM: f32 = 4.0;
/// Multiplicative zoom step per mouse wheel notch (smooth exponential zoom).
const ZOOM_STEP: f32 = 1.15;

/// Update camera position based on keyboard and mouse edge scrolling.
pub(crate) fn update_camera(state: &mut AppState) {
    let sw: f32 = state.render_width() as f32;
    let sh: f32 = state.render_height() as f32;

    if state
        .keys_held
        .contains(&winit::keyboard::KeyCode::ArrowLeft)
    {
        state.camera_x -= CAMERA_SCROLL_SPEED / state.zoom_level;
    }
    if state
        .keys_held
        .contains(&winit::keyboard::KeyCode::ArrowRight)
    {
        state.camera_x += CAMERA_SCROLL_SPEED / state.zoom_level;
    }
    if state.keys_held.contains(&winit::keyboard::KeyCode::ArrowUp) {
        state.camera_y -= CAMERA_SCROLL_SPEED / state.zoom_level;
    }
    if state
        .keys_held
        .contains(&winit::keyboard::KeyCode::ArrowDown)
    {
        state.camera_y += CAMERA_SCROLL_SPEED / state.zoom_level;
    }

    if !state.minimap_dragging {
        let sidebar_x = sw - state.sidebar_layout_spec.sidebar_width;
        let over_sidebar = state.cursor_x >= sidebar_x;

        if state.cursor_x < EDGE_SCROLL_MARGIN {
            state.camera_x -= CAMERA_SCROLL_SPEED / state.zoom_level;
        }
        if !over_sidebar && state.cursor_x > sidebar_x - EDGE_SCROLL_MARGIN {
            state.camera_x += CAMERA_SCROLL_SPEED / state.zoom_level;
        }
        if state.cursor_y < EDGE_SCROLL_MARGIN {
            state.camera_y -= CAMERA_SCROLL_SPEED / state.zoom_level;
        }
        if state.cursor_y > sh - EDGE_SCROLL_MARGIN {
            state.camera_y += CAMERA_SCROLL_SPEED / state.zoom_level;
        }
    }

    clamp_camera_to_playable_area(state, sw, sh);

    // Smoothly animate zoom_level toward zoom_target each frame.
    animate_zoom(state);
}

/// Smoothing factor for zoom animation. Each frame, zoom_level moves this
/// fraction of the remaining distance toward zoom_target. 0.35 ≈ snappy ease-out.
const ZOOM_EASE: f32 = 0.35;
/// Snap threshold — when zoom_level is this close to zoom_target, jump to it.
const ZOOM_SNAP: f32 = 0.002;

/// Set zoom target from mouse wheel input, anchored on the cursor position.
///
/// Records the world point under the cursor so `animate_zoom` can keep it
/// pinned at that screen position during the smooth ease.
pub(crate) fn apply_zoom(state: &mut AppState, delta_lines: f32) {
    let old_target = state.zoom_target;
    let factor = ZOOM_STEP.powf(delta_lines);
    let new_target = (old_target * factor).clamp(MIN_ZOOM, MAX_ZOOM);
    if (new_target - old_target).abs() < 1e-6 {
        return;
    }

    // Record the world point under the cursor — animate_zoom keeps it stable.
    let z = state.zoom_level;
    state.zoom_anchor_world = [
        state.cursor_x / z + state.camera_x,
        state.cursor_y / z + state.camera_y,
    ];
    state.zoom_anchor_screen = [state.cursor_x, state.cursor_y];
    state.zoom_target = new_target;
}

/// Animate zoom_level toward zoom_target each frame, adjusting the camera so
/// the anchor world point stays at the anchor screen position.
pub(crate) fn animate_zoom(state: &mut AppState) {
    let diff = state.zoom_target - state.zoom_level;
    if diff.abs() < ZOOM_SNAP {
        if (state.zoom_level - state.zoom_target).abs() > 1e-7 {
            state.zoom_level = state.zoom_target;
            let sw = state.render_width() as f32;
            let sh = state.render_height() as f32;
            clamp_camera_to_playable_area(state, sw, sh);
        }
        return;
    }

    state.zoom_level += diff * ZOOM_EASE;

    // Adjust camera so the anchor world point stays at the anchor screen position:
    //   anchor_world_x = anchor_screen_x / zoom + camera_x
    //   camera_x = anchor_world_x - anchor_screen_x / zoom
    state.camera_x = state.zoom_anchor_world[0] - state.zoom_anchor_screen[0] / state.zoom_level;
    state.camera_y = state.zoom_anchor_world[1] - state.zoom_anchor_screen[1] / state.zoom_level;

    let sw = state.render_width() as f32;
    let sh = state.render_height() as f32;
    clamp_camera_to_playable_area(state, sw, sh);
}

pub(crate) fn center_camera_on_cell(state: &mut AppState, rx: u16, ry: u16) {
    let z = state.height_map.get(&(rx, ry)).copied().unwrap_or(0);
    let (sx, sy) = terrain::iso_to_screen(rx, ry, z);
    let sw = state.render_width() as f32;
    let sh = state.render_height() as f32;
    // Center the cell on screen: the visible world width is sw/zoom, so half is sw/(2*zoom).
    state.camera_x = sx - sw / (2.0 * state.zoom_level);
    state.camera_y = sy - sh / (2.0 * state.zoom_level);
    clamp_camera_to_playable_area(state, sw, sh);
}

pub(crate) fn clamp_camera_to_playable_area(state: &mut AppState, sw: f32, sh: f32) {
    let Some(grid) = &state.terrain_grid else {
        return;
    };
    let (area_x, area_y, area_w, area_h) = match grid.local_bounds {
        Some(b) => (b.pixel_x, b.pixel_y, b.pixel_w, b.pixel_h),
        None => (
            grid.origin_x,
            grid.origin_y,
            grid.world_width,
            grid.world_height,
        ),
    };
    // Visible world area = screen pixels / zoom.
    let zoom = state.zoom_level;
    let clamp_axis = |origin: f32, world_size: f32, viewport: f32| -> (f32, f32) {
        let visible = viewport / zoom;
        if world_size <= visible {
            let center: f32 = origin + (world_size - visible) / 2.0;
            (center, center)
        } else {
            (origin, origin + world_size - visible)
        }
    };
    // Use game viewport width (excluding sidebar) for X clamping, not full window width.
    // The sidebar covers the right portion of the window and isn't part of the game view.
    let game_viewport_w: f32 = sw - state.sidebar_layout_spec.sidebar_width;
    let (cx_min, cx_max) = clamp_axis(area_x, area_w, game_viewport_w);
    let (cy_min, cy_max) = clamp_axis(area_y, area_h, sh);
    state.camera_x = state.camera_x.clamp(cx_min, cx_max);
    state.camera_y = state.camera_y.clamp(cy_min, cy_max);
}
