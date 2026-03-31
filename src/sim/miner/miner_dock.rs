//! Refinery dock reservation and queue management.
//!
//! Only one miner may occupy a refinery dock at a time. Additional miners
//! queue up and are promoted in FIFO order when the dock is released.
//! State lives in `ProductionState.dock_reservations` (shared across entities).
//!
//! ## Dependency rules
//! - Part of sim/ -- no dependencies outside sim/.
//! - sim/ NEVER depends on render/, ui/, sidebar/, audio/, net/.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Tracks which refinery docks are occupied and who is waiting.
///
/// Keyed by refinery StableEntityId. Each refinery has at most one occupant
/// (the miner currently unloading) and a FIFO queue of waiting miners.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DockReservations {
    /// Maps refinery StableEntityId -> currently docked miner StableEntityId.
    pub occupied: BTreeMap<u64, u64>,
    /// Maps refinery StableEntityId -> ordered queue of waiting miner StableEntityIds.
    pub queues: BTreeMap<u64, VecDeque<u64>>,
}

impl DockReservations {
    /// Try to reserve the dock at `refinery_sid` for `miner_sid`.
    ///
    /// Returns `true` if the miner now occupies the dock (immediately granted).
    /// Returns `false` if the dock is occupied — the miner is enqueued instead.
    pub fn try_reserve(&mut self, refinery_sid: u64, miner_sid: u64) -> bool {
        if let Some(&occupant) = self.occupied.get(&refinery_sid) {
            if occupant == miner_sid {
                return true; // already occupying
            }
            // Dock busy — enqueue if not already queued.
            let queue = self.queues.entry(refinery_sid).or_default();
            if !queue.contains(&miner_sid) {
                queue.push_back(miner_sid);
            }
            return false;
        }
        // Dock free — grant immediately.
        self.occupied.insert(refinery_sid, miner_sid);
        true
    }

    /// Release the dock at `refinery_sid`. Returns the next miner promoted
    /// from the queue (if any), which should transition to Dock/Unload.
    pub fn release(&mut self, refinery_sid: u64) -> Option<u64> {
        self.occupied.remove(&refinery_sid);
        let next = self
            .queues
            .get_mut(&refinery_sid)
            .and_then(|q| q.pop_front());
        if let Some(next_miner) = next {
            self.occupied.insert(refinery_sid, next_miner);
            Some(next_miner)
        } else {
            None
        }
    }

    /// Cancel a miner's reservation or queue position at a specific refinery.
    pub fn cancel(&mut self, refinery_sid: u64, miner_sid: u64) {
        if self.occupied.get(&refinery_sid) == Some(&miner_sid) {
            self.occupied.remove(&refinery_sid);
            // Promote next in queue.
            if let Some(next) = self
                .queues
                .get_mut(&refinery_sid)
                .and_then(|q| q.pop_front())
            {
                self.occupied.insert(refinery_sid, next);
            }
        } else if let Some(queue) = self.queues.get_mut(&refinery_sid) {
            queue.retain(|&sid| sid != miner_sid);
        }
    }

    /// Whether the dock at `refinery_sid` is currently occupied.
    pub fn is_occupied(&self, refinery_sid: u64) -> bool {
        self.occupied.contains_key(&refinery_sid)
    }

    /// Remove any references to dead entities (miners or refineries).
    ///
    /// Call at the start of each tick with the set of all alive StableEntityIds
    /// to prevent stale reservations from blocking docks forever.
    pub fn cleanup_dead(&mut self, alive: &BTreeSet<u64>) {
        // Remove dead refineries entirely.
        self.occupied.retain(|ref_sid, _| alive.contains(ref_sid));
        self.queues.retain(|ref_sid, _| alive.contains(ref_sid));

        // Remove dead miners from occupant slots and promote next.
        let dead_occupants: Vec<u64> = self
            .occupied
            .iter()
            .filter(|(_, miner_sid)| !alive.contains(miner_sid))
            .map(|(&ref_sid, _)| ref_sid)
            .collect();
        for ref_sid in dead_occupants {
            self.occupied.remove(&ref_sid);
            if let Some(next) = self.queues.get_mut(&ref_sid).and_then(|q| q.pop_front()) {
                if alive.contains(&next) {
                    self.occupied.insert(ref_sid, next);
                }
            }
        }

        // Remove dead miners from queues.
        for queue in self.queues.values_mut() {
            queue.retain(|sid| alive.contains(sid));
        }
        // Clean up empty queue entries.
        self.queues.retain(|_, q| !q.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserve_free_dock() {
        let mut docks = DockReservations::default();
        assert!(docks.try_reserve(100, 1));
        assert!(docks.is_occupied(100));
    }

    #[test]
    fn second_miner_queues() {
        let mut docks = DockReservations::default();
        assert!(docks.try_reserve(100, 1));
        assert!(!docks.try_reserve(100, 2));
        assert_eq!(docks.queues[&100].len(), 1);
    }

    #[test]
    fn release_promotes_next() {
        let mut docks = DockReservations::default();
        docks.try_reserve(100, 1);
        docks.try_reserve(100, 2);
        docks.try_reserve(100, 3);
        let promoted = docks.release(100);
        assert_eq!(promoted, Some(2));
        assert_eq!(docks.occupied[&100], 2);
    }

    #[test]
    fn cancel_occupant_promotes() {
        let mut docks = DockReservations::default();
        docks.try_reserve(100, 1);
        docks.try_reserve(100, 2);
        docks.cancel(100, 1);
        assert_eq!(docks.occupied.get(&100), Some(&2));
    }

    #[test]
    fn cleanup_removes_dead() {
        let mut docks = DockReservations::default();
        docks.try_reserve(100, 1);
        docks.try_reserve(100, 2);
        let alive: BTreeSet<u64> = [100, 2].into_iter().collect();
        docks.cleanup_dead(&alive);
        // Miner 1 is dead, miner 2 should be promoted.
        assert_eq!(docks.occupied.get(&100), Some(&2));
    }

    #[test]
    fn idempotent_reserve() {
        let mut docks = DockReservations::default();
        assert!(docks.try_reserve(100, 1));
        assert!(docks.try_reserve(100, 1)); // already occupying
    }
}
