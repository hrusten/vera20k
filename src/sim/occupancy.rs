//! Persistent per-cell occupancy grid — tracks which entities occupy each map cell.
//!
//! Replaces the ephemeral `build_occupancy_maps()` approach with an incrementally
//! maintained grid. Entities are added on spawn/move-in, removed on death/move-out.
//! Structures occupy all their foundation cells.
//!
//! Unified single grid with layer-tagged occupants (no separate ground/bridge maps).
//! Equivalent to the original engine's CellClass::FirstObject/AltObject linked lists.
//!
//! ## Dependency rules
//! - Part of sim/ — depends on sim/movement/locomotor (MovementLayer).
//! - sim/ NEVER depends on render/, ui/, sidebar/, audio/, net/.

use std::collections::BTreeMap;

use crate::sim::movement::locomotor::MovementLayer;

/// Single occupant entry in a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellOccupant {
    pub entity_id: u64,
    pub layer: MovementLayer,
    /// Infantry sub-cell (2, 3, or 4). None for vehicles/structures.
    pub sub_cell: Option<u8>,
}

/// All occupants of a single cell.
#[derive(Debug, Clone, Default)]
pub struct CellOccupancy {
    /// Occupant list. Common case is 0-3 infantry or 1 vehicle per cell.
    pub occupants: Vec<CellOccupant>,
}

impl CellOccupancy {
    /// Non-infantry occupants (vehicles/structures) on a given layer.
    pub fn blockers(&self, layer: MovementLayer) -> impl Iterator<Item = u64> + '_ {
        self.occupants
            .iter()
            .filter(move |o| o.layer == layer && o.sub_cell.is_none())
            .map(|o| o.entity_id)
    }

    /// Infantry occupants on a given layer: (entity_id, sub_cell).
    pub fn infantry(&self, layer: MovementLayer) -> impl Iterator<Item = (u64, u8)> + '_ {
        self.occupants
            .iter()
            .filter(move |o| o.layer == layer && o.sub_cell.is_some())
            .map(|o| (o.entity_id, o.sub_cell.unwrap()))
    }

    /// Whether this cell has any occupants on the given layer.
    pub fn is_empty_on(&self, layer: MovementLayer) -> bool {
        !self.occupants.iter().any(|o| o.layer == layer)
    }

    /// Whether this cell has any non-infantry occupants on the given layer.
    pub fn has_blockers_on(&self, layer: MovementLayer) -> bool {
        self.occupants
            .iter()
            .any(|o| o.layer == layer && o.sub_cell.is_none())
    }

    /// Count occupants on a given layer.
    pub fn count_on(&self, layer: MovementLayer) -> usize {
        self.occupants.iter().filter(|o| o.layer == layer).count()
    }
}

/// Persistent per-cell occupancy index, stored on `Simulation`.
///
/// Mirrors entity positions: every entity that occupies a map cell has an entry.
/// Structures occupy all their foundation cells. Maintained incrementally — add
/// on spawn/move-in, remove on death/move-out.
pub struct OccupancyGrid {
    cells: BTreeMap<(u16, u16), CellOccupancy>,
}

impl OccupancyGrid {
    /// Create an empty occupancy grid.
    pub fn new() -> Self {
        Self {
            cells: BTreeMap::new(),
        }
    }

    /// Add an entity to a cell. For structures, caller must invoke once per
    /// foundation cell.
    pub fn add(
        &mut self,
        rx: u16,
        ry: u16,
        entity_id: u64,
        layer: MovementLayer,
        sub_cell: Option<u8>,
    ) {
        let occ = self.cells.entry((rx, ry)).or_default();
        occ.occupants.push(CellOccupant {
            entity_id,
            layer,
            sub_cell,
        });
    }

    /// Remove an entity from a cell. No-op if entity not found.
    /// For structures, caller must invoke once per foundation cell.
    pub fn remove(&mut self, rx: u16, ry: u16, entity_id: u64) {
        if let Some(occ) = self.cells.get_mut(&(rx, ry)) {
            occ.occupants.retain(|o| o.entity_id != entity_id);
            if occ.occupants.is_empty() {
                self.cells.remove(&(rx, ry));
            }
        }
    }

    /// Move an entity from one cell to another (remove + add).
    pub fn move_entity(
        &mut self,
        old_rx: u16,
        old_ry: u16,
        new_rx: u16,
        new_ry: u16,
        entity_id: u64,
        layer: MovementLayer,
        sub_cell: Option<u8>,
    ) {
        self.remove(old_rx, old_ry, entity_id);
        self.add(new_rx, new_ry, entity_id, layer, sub_cell);
    }

    /// Update an entity's sub-cell within the same cell.
    pub fn update_sub_cell(
        &mut self,
        rx: u16,
        ry: u16,
        entity_id: u64,
        new_sub_cell: Option<u8>,
    ) {
        if let Some(occ) = self.cells.get_mut(&(rx, ry)) {
            if let Some(o) = occ.occupants.iter_mut().find(|o| o.entity_id == entity_id) {
                o.sub_cell = new_sub_cell;
            }
        }
    }

    /// Get occupancy for a cell (all layers).
    pub fn get(&self, rx: u16, ry: u16) -> Option<&CellOccupancy> {
        self.cells.get(&(rx, ry))
    }

    /// Check if a cell has no occupants on a given layer.
    pub fn is_empty_on_layer(&self, rx: u16, ry: u16, layer: MovementLayer) -> bool {
        self.cells
            .get(&(rx, ry))
            .map_or(true, |occ| occ.is_empty_on(layer))
    }

    /// Count total occupants on a layer in a cell.
    pub fn count_on_layer(&self, rx: u16, ry: u16, layer: MovementLayer) -> usize {
        self.cells
            .get(&(rx, ry))
            .map_or(0, |occ| occ.count_on(layer))
    }

    /// Check if a specific entity is in a specific cell.
    pub fn contains_entity(&self, rx: u16, ry: u16, entity_id: u64) -> bool {
        self.cells
            .get(&(rx, ry))
            .is_some_and(|occ| occ.occupants.iter().any(|o| o.entity_id == entity_id))
    }

    /// Total number of occupied cells (for diagnostics).
    pub fn occupied_cell_count(&self) -> usize {
        self.cells.len()
    }
}
