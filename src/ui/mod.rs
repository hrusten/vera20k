//! egui-based menus and dialogs.
//!
//! Uses egui for screens that DON'T need pixel-perfect RA2 art:
//! main menu, skirmish setup, settings, confirmation dialogs.
//!
//! The in-game sidebar is NOT here — it uses custom wgpu rendering
//! in the sidebar/ module because it needs original RA2 art assets
//! (cameo icons, custom buttons, progress bars).
//!
//! ## Dependency rules
//! - ui/ depends on: sim/ (reads game state, produces commands)
//! - ui/ does NOT depend on: assets/, render/, sidebar/, audio/, net/

pub mod client_theme;
pub mod game_screen;
pub mod in_game_hud;
pub mod main_menu;
pub mod mission_status;
pub mod pause_menu;
// pub mod skirmish;
// pub mod dialog;
// pub mod settings;
