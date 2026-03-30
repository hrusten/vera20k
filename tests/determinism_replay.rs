//! Determinism and replay parity tests for fixed-step simulation.

use std::collections::BTreeMap;

use vera20k::map::entities::{EntityCategory, MapEntity};
use vera20k::sim::command::{Command, CommandEnvelope};
use vera20k::sim::pathfinding::PathGrid;
use vera20k::sim::replay::{ReplayHeader, ReplayLog, ReplayRunner};
use vera20k::sim::world::Simulation;

const TICK_MS: u32 = 33;

fn make_test_sim() -> Simulation {
    let mut sim = Simulation::with_seed(123_456);
    let entity = MapEntity {
        owner: "Americans".to_string(),
        type_id: "MTNK".to_string(),
        health: 256,
        cell_x: 2,
        cell_y: 2,
        facing: 64,
        category: EntityCategory::Unit,
        sub_cell: 0,
        veterancy: 0,
        high: false,
    };
    let heights: BTreeMap<(u16, u16), u8> = BTreeMap::new();
    sim.spawn_from_map(&[entity], None, &heights);
    sim
}

fn make_move_command() -> CommandEnvelope {
    CommandEnvelope::new(
        vera20k::sim::intern::test_intern("Americans"),
        1,
        Command::Move {
            entity_id: 1,
            target_rx: 12,
            target_ry: 2,
            queue: false,
            group_id: None,
        },
    )
}

fn run_with_frame_profile(total_ms: u32, frame_ms: u32) -> (u64, Vec<u64>, ReplayLog) {
    let mut sim = make_test_sim();
    let grid = PathGrid::new(32, 32);
    let height_map: BTreeMap<(u16, u16), u8> = BTreeMap::new();
    let mut pending = vec![make_move_command()];
    let mut acc_ms: u64 = 0;
    let mut hashes: Vec<u64> = Vec::new();
    let mut replay = ReplayLog::new(ReplayHeader {
        version: 1,
        tick_hz: 30,
        seed: 123_456,
        map_name: "determinism_test".to_string(),
        rules_hash: 0,
    });

    let mut elapsed: u32 = 0;
    while elapsed < total_ms {
        let slice = frame_ms.min(total_ms - elapsed);
        elapsed += slice;
        acc_ms += slice as u64;

        while acc_ms >= TICK_MS as u64 {
            acc_ms -= TICK_MS as u64;
            let execute_tick = sim.tick + 1;
            let mut due: Vec<CommandEnvelope> = Vec::new();
            pending.retain(|cmd| {
                if cmd.execute_tick <= execute_tick {
                    due.push(cmd.clone());
                    false
                } else {
                    true
                }
            });

            let result = sim.advance_tick(&due, None, &height_map, Some(&grid), TICK_MS);
            hashes.push(result.state_hash);
            replay.record_tick(result.tick, due, result.state_hash);
        }
    }

    (sim.state_hash(), hashes, replay)
}

#[test]
fn fixed_step_invariance_across_frame_profiles() {
    let (hash_a, _, _) = run_with_frame_profile(2500, 16);
    let (hash_b, _, _) = run_with_frame_profile(2500, 50);
    assert_eq!(
        hash_a, hash_b,
        "Fixed-step simulation must be frame-rate invariant"
    );
}

#[test]
fn determinism_repeatability_same_inputs() {
    let (hash_a, timeline_a, _) = run_with_frame_profile(2500, 16);
    let (hash_b, timeline_b, _) = run_with_frame_profile(2500, 16);
    assert_eq!(hash_a, hash_b, "Final hash should be identical");
    assert_eq!(
        timeline_a, timeline_b,
        "Per-tick hash timeline should be identical"
    );
}

#[test]
fn replay_playback_matches_live_hash_timeline() {
    let (_, live_timeline, replay) = run_with_frame_profile(2500, 20);

    let mut replay_sim = make_test_sim();
    let grid = PathGrid::new(32, 32);
    let height_map: BTreeMap<(u16, u16), u8> = BTreeMap::new();
    let playback_timeline = ReplayRunner::run(
        &mut replay_sim,
        &replay,
        None,
        &height_map,
        Some(&grid),
        TICK_MS,
    );

    assert_eq!(
        live_timeline, playback_timeline,
        "Replay playback must match live hash timeline"
    );
}
