//! SHP vehicle animation sequence builder.
//!
//! RA2 has a small number of SHP-based vehicles (Dolphin, Terror Drone, Giant Squid)
//! that use a tag-driven frame layout in art.ini rather than the arbitrary infantry
//! sequence system. Frame blocks are defined by paired Start/Count tags:
//! `WalkFrames=6`, `FiringFrames=4`, etc.
//!
//! Each block contains `Facings × FramesPerFacing` frames, organized with all frames
//! for facing 0 first, then all for facing 1, etc. SHP vehicle facings run **clockwise**
//! from North (opposite to infantry counter-clockwise convention).
//!
//! ## Dependency rules
//! - Part of rules/ — depends only on rules/art_data, sim/animation.
//! - Does NOT depend on sim/ game logic, render/, or any game module.

use crate::rules::art_data::ArtEntry;
use crate::sim::animation::{LoopMode, SequenceDef, SequenceKind, SequenceSet};

/// Default milliseconds per walk frame for SHP vehicles.
const DEFAULT_VEHICLE_WALK_TICK_MS: u32 = 100;
/// Default milliseconds per firing frame for SHP vehicles.
const DEFAULT_VEHICLE_FIRE_TICK_MS: u32 = 80;
/// Default milliseconds per standing frame for SHP vehicles.
const DEFAULT_VEHICLE_STAND_TICK_MS: u32 = 200;

/// Build a `SequenceSet` for an SHP vehicle from art.ini frame tags.
///
/// Frame layout is contiguous: Stand frames first, then Walk, then Fire.
/// If `StandingFrames` is absent, assumes 1 frame per facing for standing.
/// If `WalkFrames` or `FiringFrames` is absent, those sequences are omitted.
///
/// The DirStruct facing byte maps directly to the SHP frame order.
pub fn build_shp_vehicle_sequences(art: &ArtEntry) -> SequenceSet {
    let mut set = SequenceSet::new();
    let facings: u8 = art.shp_facings.max(1);

    let standing_frames: u16 = art.standing_frames.unwrap_or(1);
    let walk_frames: u16 = art.walk_frames.unwrap_or(0);
    let firing_frames: u16 = art.firing_frames.unwrap_or(0);

    // Stand: starts at frame 0, one block of standing_frames * facings.
    let stand_start: u16 = 0;
    set.insert(
        SequenceKind::Stand,
        SequenceDef {
            start_frame: stand_start,
            frame_count: standing_frames,
            facings,
            facing_multiplier: standing_frames,
            tick_ms: DEFAULT_VEHICLE_STAND_TICK_MS,
            loop_mode: LoopMode::Loop,
            clockwise_facings: true,
        },
    );

    // Walk: immediately after stand frames.
    if walk_frames > 0 {
        let walk_start: u16 = stand_start + standing_frames * facings as u16;
        set.insert(
            SequenceKind::Walk,
            SequenceDef {
                start_frame: walk_start,
                frame_count: walk_frames,
                facings,
                facing_multiplier: walk_frames,
                tick_ms: DEFAULT_VEHICLE_WALK_TICK_MS,
                loop_mode: LoopMode::Loop,
                clockwise_facings: true,
            },
        );

        // Attack (firing): immediately after walk frames.
        if firing_frames > 0 {
            let fire_start: u16 = walk_start + walk_frames * facings as u16;
            set.insert(
                SequenceKind::Attack,
                SequenceDef {
                    start_frame: fire_start,
                    frame_count: firing_frames,
                    facings,
                    facing_multiplier: firing_frames,
                    tick_ms: DEFAULT_VEHICLE_FIRE_TICK_MS,
                    loop_mode: LoopMode::TransitionTo(SequenceKind::Stand),
                    clockwise_facings: true,
                },
            );
        }
    }

    set
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_art_entry(walk: Option<u16>, firing: Option<u16>) -> ArtEntry {
        ArtEntry {
            image: None,
            cameo: None,
            alt_cameo: None,
            new_theater: false,
            theater: false,
            voxel: false,
            turret_offset: 0,
            y_draw_offset: 0,
            x_draw_offset: 0,
            building_anims: Vec::new(),
            foundation: None,
            bib_shape: None,
            palette: None,
            sequence: None,
            primary_fire_flh: Default::default(),
            secondary_fire_flh: Default::default(),
            elite_primary_fire_flh: None,
            elite_secondary_fire_flh: None,
            walk_frames: walk,
            firing_frames: firing,
            standing_frames: None,
            shp_facings: 8,
            fire_up: 0,
            extra_light: 0,
            queueing_cell: None,
            docking_offset: None,
            damage_fire_offsets: Vec::new(),
            height: 0,
            muzzle_flash_positions: Vec::new(),
        }
    }

    #[test]
    fn test_dolphin_sequences() {
        // DLPH: WalkFrames=6, FiringFrames=6, 8 facings
        let art = make_art_entry(Some(6), Some(6));
        let set = build_shp_vehicle_sequences(&art);

        let stand = set.get(&SequenceKind::Stand).expect("Stand");
        assert_eq!(stand.start_frame, 0);
        assert_eq!(stand.frame_count, 1);
        assert_eq!(stand.facings, 8);

        let walk = set.get(&SequenceKind::Walk).expect("Walk");
        // Stand: 1 * 8 = 8 frames → Walk starts at 8
        assert_eq!(walk.start_frame, 8);
        assert_eq!(walk.frame_count, 6);
        assert_eq!(walk.facing_multiplier, 6);

        let attack = set.get(&SequenceKind::Attack).expect("Attack");
        // Walk: 6 * 8 = 48 frames → Attack starts at 8 + 48 = 56
        assert_eq!(attack.start_frame, 56);
        assert_eq!(attack.frame_count, 6);
    }

    #[test]
    fn test_walk_only_no_firing() {
        let art = make_art_entry(Some(4), None);
        let set = build_shp_vehicle_sequences(&art);

        assert!(set.get(&SequenceKind::Stand).is_some());
        assert!(set.get(&SequenceKind::Walk).is_some());
        assert!(set.get(&SequenceKind::Attack).is_none());
    }

    #[test]
    fn test_no_walk_no_firing() {
        let art = make_art_entry(None, None);
        let set = build_shp_vehicle_sequences(&art);

        assert!(set.get(&SequenceKind::Stand).is_some());
        assert!(set.get(&SequenceKind::Walk).is_none());
        assert!(set.get(&SequenceKind::Attack).is_none());
    }
}
