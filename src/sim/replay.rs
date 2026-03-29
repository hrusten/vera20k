//! Replay log format and playback helpers.
//!
//! Replay data = initial metadata + per-tick command batches + state hashes.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::rules::ruleset::RuleSet;
use crate::sim::command::CommandEnvelope;
use crate::sim::pathfinding::PathGrid;
use crate::sim::world::Simulation;

/// Replay file header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayHeader {
    pub version: u32,
    pub tick_hz: u32,
    pub seed: u64,
    pub map_name: String,
    pub rules_hash: u64,
}

/// One recorded simulation tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayTick {
    pub tick: u64,
    pub commands: Vec<CommandEnvelope>,
    pub state_hash: u64,
}

/// Full replay document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayLog {
    pub header: ReplayHeader,
    pub ticks: Vec<ReplayTick>,
}

impl ReplayLog {
    pub fn new(header: ReplayHeader) -> Self {
        Self {
            header,
            ticks: Vec::new(),
        }
    }

    pub fn record_tick(&mut self, tick: u64, commands: Vec<CommandEnvelope>, state_hash: u64) {
        self.ticks.push(ReplayTick {
            tick,
            commands,
            state_hash,
        });
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let bytes =
            serde_json::to_vec_pretty(self).context("Failed to serialize replay log to JSON")?;
        std::fs::write(path, bytes)
            .with_context(|| format!("Failed to write replay file: {}", path.display()))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("Failed to read replay file: {}", path.display()))?;
        serde_json::from_slice(&bytes).context("Failed to parse replay JSON")
    }
}

/// Headless replay playback runner.
pub struct ReplayRunner;

impl ReplayRunner {
    /// Re-run simulation ticks from a replay log and return the resulting hash timeline.
    pub fn run(
        sim: &mut Simulation,
        replay: &ReplayLog,
        rules: Option<&RuleSet>,
        height_map: &BTreeMap<(u16, u16), u8>,
        path_grid: Option<&PathGrid>,
        tick_ms: u32,
    ) -> Vec<u64> {
        let mut hashes: Vec<u64> = Vec::with_capacity(replay.ticks.len());
        for entry in &replay.ticks {
            let result = sim.advance_tick(&entry.commands, rules, height_map, path_grid, tick_ms);
            hashes.push(result.state_hash);
        }
        hashes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::command::Command;

    #[test]
    fn test_replay_json_roundtrip() {
        let mut log = ReplayLog::new(ReplayHeader {
            version: 1,
            tick_hz: 30,
            seed: 42,
            map_name: "test".to_string(),
            rules_hash: 123,
        });
        log.record_tick(
            1,
            vec![CommandEnvelope::new(
                crate::sim::intern::test_intern("Americans"),
                1,
                Command::SetRally {
                    owner: crate::sim::intern::test_intern("Americans"),
                    rx: 10,
                    ry: 11,
                },
            )],
            999,
        );
        let json = serde_json::to_string(&log).expect("serialize");
        let parsed: ReplayLog = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.header.tick_hz, 30);
        assert_eq!(parsed.ticks.len(), 1);
        assert_eq!(parsed.ticks[0].tick, 1);
        assert_eq!(parsed.ticks[0].state_hash, 999);
    }
}
