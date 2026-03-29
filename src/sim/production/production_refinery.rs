//! Refinery detection, harvester spawning, and ore dropoff cell finding.
//!
//! Extracted from production_placement.rs for file-size limits.

use std::collections::BTreeMap;

use crate::rules::ruleset::RuleSet;
use crate::sim::world::Simulation;

use super::production_tech::foundation_dimensions;

/// Spawn a free harvester when a refinery is placed (RA2 standard behavior).
pub(super) fn maybe_spawn_refinery_harvester(
    sim: &mut Simulation,
    rules: &RuleSet,
    owner: &str,
    building_type_id: &str,
    building_rx: u16,
    building_ry: u16,
    path_grid: Option<&crate::sim::pathfinding::PathGrid>,
    height_map: &BTreeMap<(u16, u16), u8>,
) {
    if !rules.is_refinery_type(building_type_id) {
        return;
    }

    let Some(harvester_type) = rules.refinery_free_unit(building_type_id) else {
        return;
    };

    let obj = rules.object_case_insensitive(building_type_id);
    let (width, height) = obj
        .map(|o| foundation_dimensions(&o.foundation))
        .unwrap_or((1, 1));
    let qc = obj.and_then(|o| o.queueing_cell);

    // Spawn the free harvester at the refinery dock cell — the platform pad
    // just below the building entrance. In original RA2, the free harvester
    // appears on this pad, not at a random adjacent cell.
    let dock: (u16, u16) = super::super::miner::miner_system::refinery_dock_cell(
        building_rx,
        building_ry,
        width,
        height,
        qc,
    );
    // Use the dock cell if walkable, otherwise fall back to adjacent search.
    let (rx, ry) = if path_grid.is_none_or(|g| g.is_walkable(dock.0, dock.1)) {
        dock
    } else {
        match find_adjacent_spawn_cell(building_rx, building_ry, width, height, path_grid) {
            Some(cell) => cell,
            None => {
                log::warn!(
                    "No walkable cell near refinery ({},{}) to spawn {}",
                    building_rx,
                    building_ry,
                    harvester_type
                );
                return;
            }
        }
    };
    if sim
        .spawn_object(harvester_type, owner, rx, ry, 64, rules, height_map)
        .is_some()
    {
        log::info!(
            "Refinery {} spawned free {} at ({},{}) for {}",
            building_type_id,
            harvester_type,
            rx,
            ry,
            owner
        );
    } else {
        log::warn!(
            "Refinery {} resolved free unit {} but spawn_object failed at ({},{}) for {}",
            building_type_id,
            harvester_type,
            rx,
            ry,
            owner
        );
    }
}

fn find_adjacent_spawn_cell(
    cx: u16,
    cy: u16,
    width: u16,
    height: u16,
    path_grid: Option<&crate::sim::pathfinding::PathGrid>,
) -> Option<(u16, u16)> {
    let Some(grid) = path_grid else {
        return Some((cx.saturating_add(width), cy.saturating_add(height / 2)));
    };

    let building_max_x = i32::from(cx) + i32::from(width) - 1;
    let building_max_y = i32::from(cy) + i32::from(height) - 1;
    for radius in 1..=5_i32 {
        let min_x = i32::from(cx) - radius;
        let max_x = building_max_x + radius;
        let min_y = i32::from(cy) - radius;
        let max_y = building_max_y + radius;
        for ry in min_y..=max_y {
            for rx in min_x..=max_x {
                let on_perimeter = rx == min_x || rx == max_x || ry == min_y || ry == max_y;
                if !on_perimeter || rx < 0 || ry < 0 {
                    continue;
                }
                let (rx_u16, ry_u16) = (rx as u16, ry as u16);
                if grid.is_walkable(rx_u16, ry_u16) {
                    return Some((rx_u16, ry_u16));
                }
            }
        }
    }
    None
}
