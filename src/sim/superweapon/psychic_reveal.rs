//! PsychicReveal superweapon launch handler.
//!
//! Reveals shroud in a radius around the target cell for the owning house.
//! Matches binary's double-call to MapClass::RevealAroundCell (verified).
//!
//! ## Dependency rules
//! - Part of sim/ — depends on rules/, sim/vision, sim/world.
//! - sim/ NEVER depends on render/, ui/, sidebar/, audio/, net/.

use crate::rules::ruleset::RuleSet;
use crate::sim::intern::InternedId;
use crate::sim::vision;
use crate::sim::world::{SimSoundEvent, Simulation};

/// Launch PsychicReveal at (target_rx, target_ry). Reveals shroud in radius.
pub fn launch(
    sim: &mut Simulation,
    rules: &RuleSet,
    owner: InternedId,
    target_rx: u16,
    target_ry: u16,
) -> bool {
    let radius = rules.general.psychic_reveal_radius as u16;

    // Double call matches binary (verified). Both calls pass identical args.
    vision::reveal_radius(&mut sim.fog, owner, target_rx, target_ry, radius);
    vision::reveal_radius(&mut sim.fog, owner, target_rx, target_ry, radius);

    sim.sound_events.push(SimSoundEvent::SuperWeaponLaunched {
        owner,
        rx: target_rx,
        ry: target_ry,
    });

    log::info!(
        "PsychicReveal launched at ({}, {}) by '{}', radius={}",
        target_rx,
        target_ry,
        sim.interner.resolve(owner),
        radius
    );

    true
}
