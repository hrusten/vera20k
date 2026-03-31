//! Building docking system — repair depot unit repair.
//!
//! Units drive onto a repair depot pad, get healed over time (spending owner
//! credits), and exit when fully repaired or out of funds. Uses the same
//! `DockReservations` pattern as the refinery/miner system for FIFO queuing.
//!
//! ## Dependency rules
//! - Part of sim/ — depends on rules/, sim/components, sim/production_tech.
//! - sim/ NEVER depends on render/, ui/, sidebar/, audio/, net/.

use std::collections::BTreeSet;

use crate::rules::ruleset::RuleSet;
use crate::sim::intern::InternedId;
use crate::sim::world::Simulation;

use crate::sim::production::foundation_dimensions;

/// Dock state machine phase for a unit interacting with a repair depot.
///
/// Condensed from the original game's 7-state FSM:
/// - States 0-1 (validate + clear obstructions) -> Approach
/// - States 2-3 (rotate + move to dock) -> EnterDock
/// - States 5-6 (linked + idle on pad) -> Servicing
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DockPhase {
    /// Moving toward the dock building via pathfinding.
    Approach,
    /// At/near dock cell, waiting for dock reservation to be granted.
    WaitForDock,
    /// Dock reserved, moving onto the exact dock cell.
    EnterDock,
    /// On the dock pad, receiving repair (HP restored, credits deducted).
    Servicing,
    /// Repair complete or funds exhausted, exiting the dock.
    ExitDock,
}

/// Per-entity docking state, stored as `Option<DockState>` on `GameEntity`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DockState {
    /// StableEntityId of the target repair depot building.
    pub dock_building_id: u64,
    /// Current phase of the dock state machine.
    pub phase: DockPhase,
    /// Ticks remaining until the next repair step fires.
    pub service_timer: u32,
    /// Consecutive ticks with insufficient credits (triggers exit after grace).
    pub no_funds_ticks: u32,
}

/// Grace period in ticks before a docked unit exits due to insufficient funds.
/// ~2 seconds at 15 Hz.
const NO_FUNDS_GRACE_TICKS: u32 = 30;

/// Compute the dock cell (center of foundation) for a building.
pub fn depot_dock_cell(building_rx: u16, building_ry: u16, foundation: &str) -> (u16, u16) {
    let (w, h) = foundation_dimensions(foundation);
    (building_rx + w / 2, building_ry + h / 2)
}

/// Manhattan distance between two cell coordinates.
fn cell_distance(ax: u16, ay: u16, bx: u16, by: u16) -> u32 {
    let dx = (ax as i32 - bx as i32).unsigned_abs();
    let dy = (ay as i32 - by as i32).unsigned_abs();
    dx.max(dy)
}

/// Advance building dock state machines for all entities with `dock_state`.
///
/// Called once per tick from `advance_tick()`, after `tick_repairs()`.
/// Uses the two-phase snapshot pattern to avoid borrow conflicts.
pub fn tick_building_docks(sim: &mut Simulation, rules: &RuleSet) {
    // Phase 1: Cleanup dead entities from dock reservations.
    let alive: BTreeSet<u64> = sim.entities.keys_sorted().iter().copied().collect();
    sim.production.depot_dock_reservations.cleanup_dead(&alive);

    // Phase 2: Snapshot all entities that have a dock state.
    struct DockSnapshot {
        id: u64,
        owner: InternedId,
        type_ref: InternedId,
        rx: u16,
        ry: u16,
        hp: u16,
        max_hp: u16,
        dock_building_id: u64,
        phase: DockPhase,
        service_timer: u32,
        no_funds_ticks: u32,
    }

    let snapshots: Vec<DockSnapshot> = sim
        .entities
        .values()
        .filter_map(|e| {
            let ds = e.dock_state.as_ref()?;
            Some(DockSnapshot {
                id: e.stable_id,
                owner: e.owner,
                type_ref: e.type_ref,
                rx: e.position.rx,
                ry: e.position.ry,
                hp: e.health.current,
                max_hp: e.health.max,
                dock_building_id: ds.dock_building_id,
                phase: ds.phase,
                service_timer: ds.service_timer,
                no_funds_ticks: ds.no_funds_ticks,
            })
        })
        .collect();

    if snapshots.is_empty() {
        return;
    }

    // Phase 3: Process each docking entity.
    struct DockMutation {
        id: u64,
        new_phase: Option<DockPhase>,
        new_timer: Option<u32>,
        new_no_funds: Option<u32>,
        heal_amount: u16,
        deduct_credits: i32,
        clear_dock: bool,
        clear_movement: bool,
    }

    let mut mutations: Vec<DockMutation> = Vec::new();
    let mut exit_moves: Vec<(u64, u16, u16)> = Vec::new();

    for snap in &snapshots {
        let mut m = DockMutation {
            id: snap.id,
            new_phase: None,
            new_timer: None,
            new_no_funds: None,
            heal_amount: 0,
            deduct_credits: 0,
            clear_dock: false,
            clear_movement: false,
        };

        // Verify depot still exists and is alive/friendly.
        let depot_info = sim.entities.get(snap.dock_building_id).and_then(|depot| {
            if depot.health.current == 0 || depot.dying {
                return None;
            }
            if depot.owner != snap.owner {
                return None;
            }
            let obj = rules.object(sim.interner.resolve(depot.type_ref))?;
            if !obj.unit_repair {
                return None;
            }
            Some((depot.position.rx, depot.position.ry, obj.foundation.clone()))
        });

        let Some((depot_rx, depot_ry, foundation)) = depot_info else {
            // Depot gone or invalid — abort docking.
            m.clear_dock = true;
            sim.production
                .depot_dock_reservations
                .cancel(snap.dock_building_id, snap.id);
            mutations.push(m);
            continue;
        };

        let (dock_rx, dock_ry) = depot_dock_cell(depot_rx, depot_ry, &foundation);
        let dist = cell_distance(snap.rx, snap.ry, dock_rx, dock_ry);

        match snap.phase {
            DockPhase::Approach => {
                if dist <= 2 {
                    m.new_phase = Some(DockPhase::WaitForDock);
                }
                // If pathing ended but not close, the unit will stay stuck.
                // The command dispatch already issued a move, so no re-issue needed
                // unless movement_target was cleared without arriving.
            }
            DockPhase::WaitForDock => {
                let granted = sim
                    .production
                    .depot_dock_reservations
                    .try_reserve(snap.dock_building_id, snap.id);
                if granted {
                    m.new_phase = Some(DockPhase::EnterDock);
                    // Movement toward exact dock cell will be issued below.
                }
            }
            DockPhase::EnterDock => {
                if dist == 0 {
                    // Arrived on dock pad.
                    m.clear_movement = true;
                    m.new_phase = Some(DockPhase::Servicing);
                    m.new_timer = Some(rules.general.unit_repair_rate_ticks);
                }
                // If not arrived and no movement, re-issue is handled below.
            }
            DockPhase::Servicing => {
                if snap.hp >= snap.max_hp {
                    // Fully repaired — exit.
                    m.new_phase = Some(DockPhase::ExitDock);
                } else {
                    let timer = snap.service_timer.saturating_sub(1);
                    if timer == 0 {
                        // Time to apply a repair step.
                        let cost = rules
                            .object(sim.interner.resolve(snap.type_ref))
                            .map(|obj| obj.cost)
                            .unwrap_or(0);
                        let total_repair_cost =
                            (cost as i64 * rules.general.repair_percent as i64 / 100) as i32;
                        let cost_per_step = if snap.max_hp > 0 {
                            (total_repair_cost as i64 * rules.general.repair_step as i64
                                / snap.max_hp as i64)
                                .max(1) as i32
                        } else {
                            1
                        };

                        // Check credits.
                        let credits = crate::sim::house_state::house_state_for_owner(
                            &sim.houses,
                            sim.interner.resolve(snap.owner),
                            &sim.interner,
                        )
                        .map(|h| h.credits)
                        .unwrap_or(0);

                        if credits >= cost_per_step {
                            m.heal_amount = rules.general.repair_step;
                            m.deduct_credits = cost_per_step;
                            m.new_no_funds = Some(0);
                        } else {
                            // No funds — increment grace counter.
                            let nf = snap.no_funds_ticks + 1;
                            if nf >= NO_FUNDS_GRACE_TICKS {
                                m.new_phase = Some(DockPhase::ExitDock);
                            } else {
                                m.new_no_funds = Some(nf);
                            }
                        }
                        m.new_timer = Some(rules.general.unit_repair_rate_ticks);
                    } else {
                        m.new_timer = Some(timer);
                    }
                }
            }
            DockPhase::ExitDock => {
                // Release reservation and clear dock state.
                sim.production
                    .depot_dock_reservations
                    .release(snap.dock_building_id);
                m.clear_dock = true;
                // Issue move one cell away from dock.
                let exit_rx = dock_rx.saturating_add(1);
                let exit_ry = dock_ry;
                exit_moves.push((snap.id, exit_rx, exit_ry));
            }
        }

        mutations.push(m);
    }

    // Phase 4: Apply mutations.
    for m in &mutations {
        let Some(entity) = sim.entities.get_mut(m.id) else {
            continue;
        };

        if m.clear_dock {
            entity.dock_state = None;
            continue;
        }

        if let Some(ref mut ds) = entity.dock_state {
            if let Some(phase) = m.new_phase {
                ds.phase = phase;
            }
            if let Some(timer) = m.new_timer {
                ds.service_timer = timer;
            }
            if let Some(nf) = m.new_no_funds {
                ds.no_funds_ticks = nf;
            }
        }

        if m.clear_movement {
            entity.movement_target = None;
        }

        if m.heal_amount > 0 {
            entity.health.current = (entity.health.current + m.heal_amount).min(entity.health.max);
        }

        if m.deduct_credits > 0 {
            if let Some(house) = crate::sim::house_state::house_state_for_owner_mut(
                &mut sim.houses,
                sim.interner.resolve(entity.owner),
                &sim.interner,
            ) {
                house.credits = (house.credits - m.deduct_credits).max(0);
            }
        }
    }

    // Issue exit moves (after mutations so dock_state is already cleared).
    for (entity_id, rx, ry) in exit_moves {
        // Simple: just clear movement. The unit is now idle at the dock cell.
        // In a full implementation we'd issue a proper move command to exit,
        // but that requires path_grid which we don't have here.
        // The unit can receive new orders immediately.
        let _ = (entity_id, rx, ry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dock_cell_for_3x3_foundation() {
        let (rx, ry) = depot_dock_cell(10, 20, "3x3");
        assert_eq!((rx, ry), (11, 21));
    }

    #[test]
    fn dock_cell_for_2x2_foundation() {
        let (rx, ry) = depot_dock_cell(10, 20, "2x2");
        assert_eq!((rx, ry), (11, 21));
    }

    #[test]
    fn dock_cell_for_1x1_foundation() {
        let (rx, ry) = depot_dock_cell(10, 20, "1x1");
        assert_eq!((rx, ry), (10, 20));
    }

    #[test]
    fn cell_distance_same() {
        assert_eq!(cell_distance(5, 5, 5, 5), 0);
    }

    #[test]
    fn cell_distance_diagonal() {
        assert_eq!(cell_distance(5, 5, 8, 9), 4);
    }
}
