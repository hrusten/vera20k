//! Tests for drive track data validation and lookup functions.

use super::*;

#[test]
fn turn_tracks_have_valid_raw_track_indices() {
    for (i, tt) in TURN_TRACKS.iter().enumerate() {
        assert!(
            (tt.normal_track as usize) < RAW_TRACKS.len(),
            "TurnTrack[{}].normal_track={} exceeds RAW_TRACKS len {}",
            i,
            tt.normal_track,
            RAW_TRACKS.len()
        );
        assert!(
            (tt.short_track as usize) < RAW_TRACKS.len(),
            "TurnTrack[{}].short_track={} exceeds RAW_TRACKS len {}",
            i,
            tt.short_track,
            RAW_TRACKS.len()
        );
    }
}

#[test]
fn turn_tracks_count_is_72() {
    assert_eq!(TURN_TRACKS.len(), 72);
}

#[test]
fn raw_tracks_count_is_16() {
    assert_eq!(RAW_TRACKS.len(), 16);
}

#[test]
fn raw_track_0_is_empty() {
    let track = &RAW_TRACKS[0];
    assert_eq!(track.points_count, 0);
}

#[test]
fn raw_track_1_is_straight_north() {
    let points = raw_track_points(1);
    assert_eq!(points.len(), 23);
    // All points have x=0 (straight), face=0 (north)
    for (i, p) in points.iter().enumerate() {
        assert_eq!(p.x, 0, "Track1 point {} should have x=0", i);
        assert_eq!(p.facing, 0, "Track1 point {} should face north", i);
    }
    // Y should decrease (moving northward)
    assert!(
        points[0].y > points[22].y,
        "Track1 Y should decrease (northward)"
    );
    // First point near cell edge, last near/past center
    assert!(points[0].y > 200, "Track1 starts near cell edge");
    assert!(points[22].y < 10, "Track1 ends near/past cell center");
}

#[test]
fn track1_y_step_is_consistent() {
    let points = raw_track_points(1);
    // Each step decreases Y by ~11 leptons
    for i in 1..points.len() {
        let step = points[i - 1].y - points[i].y;
        assert!(
            step >= 10 && step <= 12,
            "Track1 step {} has Y delta={}, expected ~11",
            i,
            step
        );
    }
}

#[test]
fn raw_track_2_is_straight_ne_diagonal() {
    let points = raw_track_points(2);
    assert_eq!(points.len(), 31);
    // All points face NE (0x20 = 32)
    for (i, p) in points.iter().enumerate() {
        assert_eq!(p.facing, 32, "Track2 point {} should face NE (32)", i);
    }
    // X increases by 8 per point (from -248 toward 0)
    // Y decreases by 8 per point (from 248 toward 0)
    for i in 1..points.len() {
        let dx = points[i].x - points[i - 1].x;
        let dy = points[i - 1].y - points[i].y;
        assert_eq!(dx, 8, "Track2 point {} X step should be +8, got {}", i, dx);
        assert_eq!(dy, 8, "Track2 point {} Y step should be -8, got {}", i, dy);
    }
    assert_eq!(points[0].x, -248);
    assert_eq!(points[0].y, 248);
    assert_eq!(points[30].x, -8);
    assert_eq!(points[30].y, 8);
}

#[test]
fn raw_track_3_is_north_to_ne_curve() {
    let points = raw_track_points(3);
    assert_eq!(points.len(), 54);
    // Phase 1 (0-13): straight north, x=-256, face=0
    for i in 0..=13 {
        assert_eq!(points[i].x, -256, "Track3 phase1 point {} x", i);
        assert_eq!(points[i].facing, 0, "Track3 phase1 point {} face", i);
    }
    // Entry index is 12: vehicle starts here
    assert_eq!(points[12].y, 373);
    // Phase 2 (14-36): turning — face increases from 1 toward 31
    assert_eq!(points[14].facing, 1, "first turn point");
    assert_eq!(points[36].facing, 31, "last turn point before jump");
    // Jump index 37: face reaches 32 (NE), cell transition
    assert_eq!(points[37].facing, 32, "jump point faces NE");
    assert_eq!(points[37].x, -136);
    assert_eq!(points[37].y, 136);
    // Phase 3 (37-53): straight NE exit, face=32
    for i in 37..=53 {
        assert_eq!(points[i].facing, 32, "Track3 phase3 point {} face", i);
    }
    // Final point near origin (sentinel removed, last real point is index 53)
    assert_eq!(points[53].x, -8);
    assert_eq!(points[53].y, 8);
}

#[test]
fn track_3_begin_starts_at_entry_12() {
    let state = begin_drive_track(3, 0, 1, -1).unwrap();
    assert_eq!(state.point_index, 12, "Track 3 entry_index is 12");
}

#[test]
fn select_drive_track_ne_diagonal_gives_track_2() {
    // Facing NE (32), moving NE (32) → entry 9: normal_track=2 (straight diagonal).
    let sel = select_drive_track(32, 32, false);
    assert!(sel.is_some(), "NE diagonal should find Track 2");
    let sel = sel.unwrap();
    assert_eq!(
        sel.raw_track_index, 2,
        "should be Track 2 (straight diagonal)"
    );
}

#[test]
fn turning_tracks_have_valid_cell_cross_indices() {
    // Tracks 3-6 have positive cell_cross indices (cell crossings)
    for idx in 3..=6 {
        let track = &RAW_TRACKS[idx];
        assert!(
            track.cell_cross_index >= 0,
            "RawTrack[{}] should have positive cell_cross_index, got {}",
            idx,
            track.cell_cross_index
        );
        assert!(
            track.cell_cross_index < track.points_count as i16,
            "RawTrack[{}] cell_cross_index {} exceeds points_count {}",
            idx,
            track.cell_cross_index,
            track.points_count
        );
        assert!(
            track.entry_index < track.points_count,
            "RawTrack[{}] entry_index {} exceeds points_count {}",
            idx,
            track.entry_index,
            track.points_count
        );
    }
}

#[test]
fn select_raw_track_index_picks_correct_variant() {
    let tt = &TURN_TRACKS[1]; // normal=3, short=7
    assert_eq!(select_raw_track_index(tt, false), 3);
    assert_eq!(select_raw_track_index(tt, true), 7);
}

#[test]
fn turn_track_lookup_in_range() {
    assert!(turn_track_at(0).is_some());
    assert!(turn_track_at(71).is_some());
    assert!(turn_track_at(72).is_none());
}

// ---------------------------------------------------------------------------
// facing_to_dir tests
// ---------------------------------------------------------------------------

#[test]
fn facing_to_dir_quantizes_8_directions() {
    // Exact facing boundaries: 0=N, 32=NE, 64=E, 96=SE, 128=S, 160=SW, 192=W, 224=NW
    assert_eq!(facing_to_dir(0), 0, "0 → N (dir 0)");
    assert_eq!(facing_to_dir(32), 1, "32 → NE (dir 1)");
    assert_eq!(facing_to_dir(64), 2, "64 → E (dir 2)");
    assert_eq!(facing_to_dir(96), 3, "96 → SE (dir 3)");
    assert_eq!(facing_to_dir(128), 4, "128 → S (dir 4)");
    assert_eq!(facing_to_dir(160), 5, "160 → SW (dir 5)");
    assert_eq!(facing_to_dir(192), 6, "192 → W (dir 6)");
    assert_eq!(facing_to_dir(224), 7, "224 → NW (dir 7)");
}

#[test]
fn facing_to_dir_rounds_near_boundaries() {
    // 15 is within 16 of 0 → should round to N (dir 0)
    assert_eq!(facing_to_dir(15), 0, "15 → N (rounds down)");
    // 16 is the boundary, rounds to NE (dir 1)
    assert_eq!(facing_to_dir(16), 1, "16 → NE (rounds up)");
    // 240 is 16 units from 224 (NW) and 16 units from 256/0 (N)
    // 240 + 16 = 256 = 0 wrapping, 0 / 32 = 0 → N
    assert_eq!(facing_to_dir(240), 0, "240 → N (wraps around)");
    // 241 wraps to 1, 1/32 = 0 → N
    assert_eq!(facing_to_dir(241), 0, "241 → N (wraps around)");
}

// ---------------------------------------------------------------------------
// select_drive_track tests
// ---------------------------------------------------------------------------

#[test]
fn select_drive_track_straight_north_gives_track_1() {
    // Facing N (0), moving N (0) → entry 0: normal_track=1 (straight north).
    // Track 1 has point data → should succeed.
    let sel = select_drive_track(0, 0, false);
    // Entry 0 has normal_track=1, but no facing change means same dir.
    // Actually entry 0 is straight ahead — it should give Track 1.
    assert!(sel.is_some(), "straight north should find Track 1");
    let sel = sel.unwrap();
    assert_eq!(sel.raw_track_index, 1, "should be Track 1 (straight north)");
    assert_eq!(sel.target_facing, 0x00, "target facing should be 0 (north)");
}

#[test]
fn select_drive_track_null_track_returns_none() {
    // Facing N (0), moving SE (96) → entry 3: normal_track=0 (null).
    // Too sharp a turn — should return None.
    let sel = select_drive_track(0, 96, false);
    assert!(sel.is_none(), "135° turn should return None (null track)");
}

#[test]
fn select_drive_track_north_to_ne_gives_track_3() {
    // Facing N (0), moving NE (32) → entry 1: normal_track=3 (turning curve A).
    let sel = select_drive_track(0, 32, false);
    assert!(sel.is_some(), "N→NE slight turn should give Track 3");
    let sel = sel.unwrap();
    assert_eq!(sel.raw_track_index, 3);
    assert_eq!(sel.target_facing, 0x20);
    assert_eq!(sel.chain_index, 37);
    assert_eq!(sel.cell_cross_index, 22);
    assert_eq!(sel.entry_index, 12);
}

#[test]
fn select_drive_track_all_cardinal_straights_give_track_1() {
    // All 8 cardinal/diagonal straight-ahead cases should resolve to Track 1 or 2.
    // Cardinals (N, E, S, W) → Track 1; diagonals (NE, SE, SW, NW) → Track 2.
    for facing in [0u8, 64, 128, 192] {
        let sel = select_drive_track(facing, facing, false);
        assert!(
            sel.is_some(),
            "cardinal facing {} should have a track",
            facing
        );
        assert_eq!(
            sel.unwrap().raw_track_index,
            1,
            "cardinal {} → Track 1",
            facing
        );
    }
    for facing in [32u8, 96, 160, 224] {
        let sel = select_drive_track(facing, facing, false);
        assert!(
            sel.is_some(),
            "diagonal facing {} should have a track",
            facing
        );
        assert_eq!(
            sel.unwrap().raw_track_index,
            2,
            "diagonal {} → Track 2",
            facing
        );
    }
}

// ---------------------------------------------------------------------------
// begin_drive_track tests
// ---------------------------------------------------------------------------

#[test]
fn begin_drive_track_1_starts_at_entry() {
    let state = begin_drive_track(1, 0, 0, 0);
    assert!(state.is_some(), "Track 1 should be startable");
    let state = state.unwrap();
    assert_eq!(state.raw_track_index, 1);
    assert_eq!(state.point_index, 0, "Track 1 entry_index is 0");
}

#[test]
fn begin_drive_track_0_returns_none() {
    // Track 0 is the null track (no points).
    let state = begin_drive_track(0, 0, 0, 0);
    assert!(state.is_none(), "Track 0 (null) should not be startable");
}

#[test]
fn begin_drive_track_missing_data_returns_none() {
    // Out-of-range track index (only 0-15 exist) should return None.
    let state = begin_drive_track(16, 0, 0, 0);
    assert!(state.is_none(), "Track with no metadata should return None");
    // Track 5 now has point data and should be startable.
    let state5 = begin_drive_track(5, 0, 1, -1);
    assert!(
        state5.is_some(),
        "Track 5 should be startable (has 61 points)"
    );
}

#[test]
fn raw_track_4_is_north_to_east_90_degree() {
    let points = raw_track_points(4);
    assert_eq!(points.len(), 38);
    // Starts facing north (0), ends facing east (64)
    assert_eq!(points[0].facing, 0);
    assert_eq!(points[37].facing, 64);
    // Entry at point 11
    assert_eq!(points[11].facing, 5);
    // Jump at point 26: face near 64 (east)
    assert_eq!(points[26].facing, 60);
    // Phase 3 exit: face=64 (east), y≈0
    for i in 29..=37 {
        assert!(
            points[i].facing >= 64,
            "Track4 exit point {} face={}",
            i,
            points[i].facing
        );
        assert!(
            points[i].y <= 9,
            "Track4 exit point {} y={}",
            i,
            points[i].y
        );
    }
}

// ---------------------------------------------------------------------------
// advance_drive_track tests
// ---------------------------------------------------------------------------

#[test]
fn advance_drive_track_1_progresses() {
    // Track 1 (straight north) with head_to = one cell north (dx=0, dy=-1).
    // head_offset = (128, -128). Point 0: sub = (128, -128+245=117).
    let mut state = begin_drive_track(1, 0, 0, -1).unwrap();
    let dt = SimFixed::lit("0.066"); // ~66ms tick (15fps)
    let speed = SimFixed::from_num(256); // 256 leptons/sec = 1 cell/sec

    // Advance one tick.
    let result = advance_drive_track(&mut state, speed, dt);
    // Budget = 256 * 0.066 ≈ 16, cost per step = 7, so 2 steps this tick.
    assert!(!result.finished, "track should not be done after 1 tick");
    assert_eq!(
        result.facing, 0,
        "facing should stay 0 (north) on straight track"
    );
    // sub_x should be 128 (center, since track x=0, head_offset_x=128).
    assert_eq!(
        result.sub_x.to_num::<i32>(),
        128,
        "sub_x should be centered"
    );
    // sub_y should be positive and decreasing (northward).
    let sy = result.sub_y.to_num::<i32>();
    assert!(sy > 0 && sy < 120, "sub_y should decrease: got {}", sy);
}

#[test]
fn advance_drive_track_1_completes() {
    // Track 1 (straight north) with head_to one cell north.
    let mut state = begin_drive_track(1, 0, 0, -1).unwrap();
    let dt = SimFixed::lit("0.066");
    let speed = SimFixed::from_num(256);

    // Advance many ticks until track finishes.
    let mut finished = false;
    for _ in 0..100 {
        let result = advance_drive_track(&mut state, speed, dt);
        if result.finished {
            finished = true;
            // Final y point is 3 → sub_y = -128 + 3 = -125.
            // After cell_jump offset (+256): sub_y ≈ 131.
            let sy = result.sub_y.to_num::<i32>();
            assert!(
                sy > 100 && sy < 160,
                "final sub_y should be ~131 after cell offset: got {}",
                sy
            );
            break;
        }
    }
    assert!(finished, "track should complete within 100 ticks");
}

#[test]
fn advance_drive_track_1_cell_jump_fires_once() {
    // Track 1 (straight north) with head_to one cell north.
    // Coordinate-based detection should fire cell_jump exactly once
    // when sub_y crosses below 0 (around step 11 where y drops below 128).
    let mut state = begin_drive_track(1, 0, 0, -1).unwrap();
    let dt = SimFixed::lit("0.066");
    let speed = SimFixed::from_num(256);

    let mut jump_count = 0;
    for _ in 0..100 {
        let result = advance_drive_track(&mut state, speed, dt);
        if result.cell_jump {
            jump_count += 1;
        }
        if result.finished {
            break;
        }
    }
    assert_eq!(
        jump_count, 1,
        "straight north track should cross exactly one cell boundary"
    );
}
