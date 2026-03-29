//! Resource economy: harvester ticking and ore-node utilities.
//!
//! Dispatches to the Miner-component systems (War, Chrono, Slave miners)
//! and provides shared helpers like `pick_best_resource_node`.

use std::collections::BTreeMap;

use crate::rules::ruleset::RuleSet;
use crate::sim::miner::{MinerConfig, ResourceNode, ResourceType};
use crate::sim::pathfinding;
use crate::sim::world::Simulation;

pub(super) fn tick_resource_economy(
    sim: &mut Simulation,
    rules: &RuleSet,
    config: &MinerConfig,
    path_grid: Option<&pathfinding::PathGrid>,
) {
    // Tick Miner-component-based system (War + Chrono miners).
    super::super::miner::miner_system::tick_miners(sim, rules, config, path_grid);

    // Tick Slave Miner subsystems: slave harvest AI + slave regeneration.
    super::super::slave_miner::tick_slave_harvesters(sim, rules, config);
    super::super::slave_miner::tick_slave_regen(sim, rules);
}

pub fn is_harvester_type(rules: &RuleSet, type_id: &str) -> bool {
    rules
        .object_case_insensitive(type_id)
        .is_some_and(|obj| obj.harvester)
}

/// Find the nearest non-empty resource node to `from`.
pub fn pick_best_resource_node(
    nodes: &BTreeMap<(u16, u16), ResourceNode>,
    from: (u16, u16),
) -> Option<(u16, u16)> {
    // RA2 cell selection priority (ref doc §3):
    //   1. Gems over ore (type_rank: 0=gem, 1=ore)
    //   2. Highest density (density_rank: inverted remaining so more = lower = better)
    //   3. Nearest (dist_sq)
    //   4. Deterministic tie-break (ry, rx)
    let mut best: Option<((u8, u32, u32, u16, u16), (u16, u16))> = None;
    for (&(rx, ry), node) in nodes {
        if node.remaining == 0 {
            continue;
        }
        let dx = rx as i64 - from.0 as i64;
        let dy = ry as i64 - from.1 as i64;
        let dist_sq = (dx * dx + dy * dy) as u32;
        let type_rank: u8 = if node.resource_type == ResourceType::Ore {
            1
        } else {
            0
        };
        // Invert remaining so higher density = lower rank = preferred.
        let density_rank: u32 = u32::MAX - node.remaining as u32;
        let rank = (type_rank, density_rank, dist_sq, ry, rx);
        match best {
            Some((ref cur, _)) if rank >= *cur => {}
            _ => best = Some((rank, (rx, ry))),
        }
    }
    best.map(|(_, cell)| cell)
}
