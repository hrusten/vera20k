//! Game screen state machine — which UI screen is active.
//!
//! The App checks this each frame to decide what to render (menu vs game).
//! Transitions are triggered by UI actions (e.g., clicking "Quick Play")
//! or loading completion.
//!
//! ## Dependency rules
//! - Part of ui/ — no dependencies on render/, assets/, sim/, etc.
//! - Pure data enum. App.rs reads and mutates this.

/// Which screen the application is currently displaying.
///
/// Transitions:
/// - MainMenu → Loading (user clicks "Start Game")
/// - Loading → SpawnPick (skirmish maps with 2+ start waypoints)
/// - Loading → InGame (campaign/sandbox maps without spawn pick)
/// - SpawnPick → InGame (player clicks a waypoint to place MCV)
/// - InGame → MainMenu (user presses Escape)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameScreen {
    /// The main menu. No map loaded yet. Only egui is rendered.
    MainMenu,

    /// Transitional state: map is being loaded.
    /// Displays "Loading..." text via egui while the app loads map data
    /// on the next frame (synchronous load, deferred from initialize).
    Loading {
        /// Name of the map file to load, chosen from the menu.
        map_name: String,
    },

    /// Spawn-pick phase: entire map is loaded and rendered without fog.
    /// Player sees waypoint markers and clicks one to choose their start.
    /// Simulation is NOT ticking — pure map preview with camera controls.
    SpawnPick,

    /// In-game: terrain, units, sprites are rendered.
    /// egui is NOT rendered in this state (future: pause menu overlay).
    InGame,

    /// Mission ended: victory, defeat, or script-forced end state.
    MissionResult { title: String, detail: String },
}

impl Default for GameScreen {
    fn default() -> Self {
        Self::MainMenu
    }
}
