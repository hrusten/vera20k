//! Shared type definitions and constants used across app_* modules.
//!
//! These types were extracted from app_render.rs because multiple sibling
//! modules (app_cursor, app_input, app_entity_pick, app_ui_overlays, etc.)
//! depend on them. Centralizing them here avoids coupling unrelated modules
//! to the rendering orchestration file.
//!
//! ## Dependency rules
//! - Part of the app layer — no sim/render dependencies.

use std::collections::HashMap;

use crate::render::batch::BatchTexture;

/// Background clear color — black, matching the shroud/fog of war in RA2.
/// Areas outside the isometric terrain diamond are not visible in the original game.
pub(crate) const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};

/// Fixed deterministic simulation rate — re-exported from util::fixed_math.
pub(crate) const SIM_TICK_HZ: u32 = crate::util::fixed_math::SIM_TICK_HZ;
/// Integer tick duration used by deterministic step execution.
pub(crate) const SIM_TICK_MS: u32 = 1000 / SIM_TICK_HZ; // 66ms
/// Next right-click order mode selected via hotkey.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OrderMode {
    Move,
    AttackMove,
    Guard,
}

/// Identifies a visual cursor from mouse.sha. Used as HashMap key in SoftwareCursor.
/// Frame ranges are hardcoded constants matching the vanilla RA2 exe (not INI-driven).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum CursorId {
    Default,
    Select,
    Move,
    NoMove,
    Attack,
    AttackOutOfRange,
    AttackMove,
    Deploy,
    NoDeploy,
    // Directional scroll cursors (move-allowed).
    ScrollN,
    ScrollNE,
    ScrollE,
    ScrollSE,
    ScrollS,
    ScrollSW,
    ScrollW,
    ScrollNW,
    // Directional scroll cursors (can't-scroll-further).
    NoMoveN,
    NoMoveNE,
    NoMoveE,
    NoMoveSE,
    NoMoveS,
    NoMoveSW,
    NoMoveW,
    NoMoveNW,
    MinimapMove,
    Enter,
    NoEnter,
    EngineerRepair,
    TogglePower,
    NoTogglePower,
    /// 4-way scroll arrow for middle-mouse pan (frame 385 in mouse.sha).
    Pan,
    // Sell / repair mode cursors.
    Sell,
    SellUnit,
    NoSell,
    Repair,
    NoRepair,
    // Special unit cursors.
    DesolatorDeploy,
    GIDeploy,
    Crush,
    Tote,
    IvanBomb,
    Detonate,
    Demolish,
    Disarm,
    InfantryHeal,
    // Spy / infiltration cursors.
    Disguise,
    SpyTech,
    SpyPower,
    // Mind control cursors.
    MindControl,
    NoMindControl,
    RemoveSquid,
    InfantryAbsorb,
    // Superweapon cursors.
    Nuke,
    Chronosphere,
    IronCurtain,
    LightningStorm,
    Paradrop,
    ForceShield,
    NoForceShield,
    GeneticMutator,
    AirStrike,
    PsychicDominator,
    PsychicReveal,
    SpyPlane,
    Beacon,
}

/// All loaded cursor animation sequences from mouse.sha, keyed by CursorId.
pub(crate) struct SoftwareCursor {
    pub(crate) sequences: HashMap<CursorId, SoftwareCursorSequence>,
}

impl SoftwareCursor {
    /// Look up a cursor sequence by id, falling back to Default if not found.
    pub(crate) fn get(&self, id: CursorId) -> Option<&SoftwareCursorSequence> {
        self.sequences
            .get(&id)
            .or_else(|| self.sequences.get(&CursorId::Default))
    }
}

pub(crate) struct SoftwareCursorFrame {
    pub(crate) texture: BatchTexture,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

pub(crate) struct SoftwareCursorSequence {
    pub(crate) frames: Vec<SoftwareCursorFrame>,
    pub(crate) interval_ms: u64,
    pub(crate) hotspot: [f32; 2],
}

/// Eight compass directions used for edge-scroll cursor selection.
/// Maps directly to the MoveN..MoveNW frames in mouse.sha (reference §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScrollDir {
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NW,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CursorFeedbackKind {
    Move,
    AttackMove,
    Guard,
    FriendlyUnit,
    FriendlyStructure,
    EnemyUnit,
    EnemyStructure,
    EnemyOutOfRange,
    Invalid,
    PlaceValid,
    PlaceInvalid,
    /// Edge-scroll arrow — shown when cursor is near a screen edge.
    Scroll(ScrollDir),
    /// Move cursor minimap variant (frames 42–51) — shown when hovering over the minimap.
    MinimapMove,
    /// Deploy/undeploy cursor — shown when a Deployer unit hovers over itself.
    Deploy,
    /// Enter cursor — garrison, capture, board transport, sabotage.
    Enter,
    /// Engineer repair cursor — engineer hovering a damaged friendly building.
    EngineerRepair,
    /// Pan cursor — shown while middle-mouse dragging to scroll the map.
    Pan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HoverTargetKind {
    FriendlyUnit,
    FriendlyStructure,
    EnemyUnit,
    EnemyStructure,
    HiddenEnemy,
}
