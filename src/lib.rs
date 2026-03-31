//! RA2 Engine — library root.
//!
//! Re-exports all modules so integration tests and future binary targets
//! can access the engine's functionality. The binary entry point (main.rs)
//! delegates to this library for all logic.

// Asset parsers — .mix, .shp, .vxl, .pal, .tmp, .hva, .csf, .aud
// No dependencies on game modules. Standalone parser library.
pub mod assets;

// GPU rendering — wgpu-based sprite, terrain, voxel rendering.
// Reads from sim/ state but never mutates it.
pub mod render;

// Game data from rules.ini and art.ini.
// Defines every unit type, building, weapon, warhead.
pub mod rules;

// Game simulation — EntityStore, fixed-point math, deterministic logic.
// NEVER depends on render/, ui/, sidebar/, audio/, net/.
pub mod sim;

// egui menus and dialogs (NOT the in-game sidebar).
pub mod ui;

// Custom wgpu sidebar — pixel-perfect RA2 art, not egui.
pub mod sidebar;

// Sound/music via rodio.
pub mod audio;

// Map loading, terrain tiles, theater system.
pub mod map;

// Multiplayer — deterministic lockstep command transport.
pub mod net;

// Shared utilities — config, fixed-point math, color helpers.
pub mod util;

// The app orchestrator. Public for testing but not intended for direct use.
pub mod app;

// App initialization helpers — map loading, entity spawning, asset loading.
// Extracted from app.rs to keep the orchestrator under 600 lines.
pub mod app_init;
pub mod app_init_helpers;
pub mod app_list_maps;
pub mod app_skirmish;

// Shared type definitions and constants used across app_* modules.
// Extracted from app_render.rs to decouple type imports from rendering.
pub mod app_types;

// In-game rendering — terrain tiles, unit sprites, SHP sprites.
// Extracted from app.rs to keep the orchestrator under 600 lines.
pub mod app_render;

// In-game input handling — mouse clicks, hotkeys, sidebar interactions.
// Extracted from app_render.rs to keep files under 400 lines.
pub mod app_input;

// Context-sensitive order resolution — click-to-command decision tree.
// Extracted from app_input.rs to separate order logic from raw input handling.
pub mod app_context_order;

// Entity picking, hover-target resolution, and selection snapshots.
// Extracted from app_render.rs to keep files under 400 lines.
pub mod app_entity_pick;

// Sidebar rendering — view construction, minimap, chrome helpers.
// Extracted from app_render.rs to keep files under 400 lines.
pub mod app_sidebar_render;

// Sidebar sprite instance builders — slots, chrome, cameos, text.
// Extracted from app_sidebar_render.rs to keep files under 400 lines.
pub mod app_sidebar_build;

// Build/production commands and owner management.
// Extracted from app_render.rs.
pub mod app_commands;

// Simulation tick loop, triggers, atlas refresh, path grid rebuild.
// Extracted from app_render.rs.
pub mod app_sim_tick;

// Camera positioning — keyboard scroll, edge scroll, clamping.
// Extracted from app_sim_tick.rs.
pub mod app_camera;

// Building animation lifecycle, damage fires, sidebar UI tick, sound playback.
// Extracted from app_sim_tick.rs.
pub mod app_building_anim;

// App state transitions: map loading, screen clearing.
pub mod app_transitions;

// Cursor feedback analysis and software cursor frame selection.
pub mod app_cursor;

// UI overlay builders — status bars, software cursor.
// Extracted from app_render.rs.
pub mod app_ui_overlays;

// Isometric 3D selection bracket lines for buildings.
pub mod app_selection_brackets;

// Target/action lines — colored lines from units to command destinations.
pub mod app_target_lines;

// Legacy egui-based sidebar text overlay kept as fallback/reference.
pub mod app_sidebar_text;

// Per-frame instance builders — units, sprites, overlays.
// Extracted from app_render.rs to keep files under 600 lines.
pub mod app_instances;

// Spawn-pick phase — player chooses their starting position on the map.
pub mod app_spawn_pick;

// Debug visualization overlays — pathgrid walkability, terrain costs.
// Toggled via hotkeys (P / F9 = pathgrid).
pub mod app_debug_overlays;
// Debug info panel — egui overlay with PathGrid/entity info (shown with pathgrid overlay).
pub mod app_debug_panel;
// Save/load panel — egui overlay for managing save files (F5).
pub mod app_save_load_panel;
