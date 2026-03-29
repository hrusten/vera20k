use std::collections::HashMap;

use crate::app_types::CursorId;
use crate::assets::asset_manager::AssetManager;
use crate::assets::pal_file::Palette;
use crate::assets::shp_file::ShpFile;
use crate::render::batch::BatchRenderer;
use crate::render::gpu::GpuContext;
use image::RgbaImage;
use std::path::Path;

/// Hotspot position within the FullWidth × FullHeight canvas.
/// Matches the Left/Center/Right + Top/Middle/Bottom combos in the reference table.
/// The hotspot pixel is the one that registers as the actual click point.
#[derive(Clone, Copy)]
enum CursorHotspot {
    /// Left, Top — tip of an arrow pointing upper-left (Default cursor).
    TopLeft,
    /// Center, Top — top-center edge (scroll north).
    CenterTop,
    /// Right, Top — top-right corner (scroll north-east).
    RightTop,
    /// Right, Middle — right edge (scroll east).
    RightMiddle,
    /// Right, Bottom — bottom-right corner (scroll south-east).
    RightBottom,
    /// Center, Bottom — bottom-center edge (scroll south).
    CenterBottom,
    /// Left, Bottom — bottom-left corner (scroll south-west).
    LeftBottom,
    /// Left, Middle — left edge (scroll west).
    LeftMiddle,
    /// Center, Middle — center of frame (attack, move, select, deploy).
    CenterMiddle,
}

/// Animation interval: vanilla RA2 uses Interval=4 ticks at ~30 display FPS.
/// 4 ticks × (1000ms / 30fps) ≈ 133ms per frame advance.
const ANIM_INTERVAL_MS: u64 = 133;

/// Complete cursor definition table. Frame ranges are hardcoded in the vanilla RA2 exe
/// (not INI-driven). Verified against ra2_cursors_reference.md §4.
/// Format: (CursorId, start_frame, frame_count, interval_ms, hotspot).
const CURSOR_DEFS: &[(CursorId, usize, usize, u64, CursorHotspot)] = &[
    // --- Core gameplay cursors ---
    (CursorId::Default, 0, 1, 0, CursorHotspot::TopLeft),
    (
        CursorId::Select,
        18,
        13,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::Move,
        31,
        10,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (CursorId::NoMove, 41, 1, 0, CursorHotspot::CenterMiddle),
    (
        CursorId::Attack,
        53,
        5,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::AttackOutOfRange,
        58,
        5,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::AttackMove,
        404,
        9,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::Deploy,
        110,
        9,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (CursorId::NoDeploy, 119, 1, 0, CursorHotspot::CenterMiddle),
    (
        CursorId::MinimapMove,
        42,
        10,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::Enter,
        89,
        10,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (CursorId::NoEnter, 99, 1, 0, CursorHotspot::CenterMiddle),
    (
        CursorId::EngineerRepair,
        170,
        20,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::TogglePower,
        339,
        6,
        0,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::NoTogglePower,
        384,
        1,
        0,
        CursorHotspot::CenterMiddle,
    ),
    // --- Directional scroll cursors (can scroll) ---
    (CursorId::ScrollN, 2, 1, 0, CursorHotspot::CenterTop),
    (CursorId::ScrollNE, 3, 1, 0, CursorHotspot::RightTop),
    (CursorId::ScrollE, 4, 1, 0, CursorHotspot::RightMiddle),
    (CursorId::ScrollSE, 5, 1, 0, CursorHotspot::RightBottom),
    (CursorId::ScrollS, 6, 1, 0, CursorHotspot::CenterBottom),
    (CursorId::ScrollSW, 7, 1, 0, CursorHotspot::LeftBottom),
    (CursorId::ScrollW, 8, 1, 0, CursorHotspot::LeftMiddle),
    (CursorId::ScrollNW, 9, 1, 0, CursorHotspot::TopLeft),
    // --- Directional scroll cursors (can't scroll further) ---
    (CursorId::NoMoveN, 10, 1, 0, CursorHotspot::CenterTop),
    (CursorId::NoMoveNE, 11, 1, 0, CursorHotspot::RightTop),
    (CursorId::NoMoveE, 12, 1, 0, CursorHotspot::RightMiddle),
    (CursorId::NoMoveSE, 13, 1, 0, CursorHotspot::RightBottom),
    (CursorId::NoMoveS, 14, 1, 0, CursorHotspot::CenterBottom),
    (CursorId::NoMoveSW, 15, 1, 0, CursorHotspot::LeftBottom),
    (CursorId::NoMoveW, 16, 1, 0, CursorHotspot::LeftMiddle),
    (CursorId::NoMoveNW, 17, 1, 0, CursorHotspot::TopLeft),
    // --- 4-way scroll arrow for middle-mouse pan ---
    (CursorId::Pan, 385, 1, 0, CursorHotspot::CenterMiddle),
    // --- Sell / repair mode ---
    (
        CursorId::Sell,
        129,
        10,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::SellUnit,
        139,
        10,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (CursorId::NoSell, 149, 1, 0, CursorHotspot::CenterMiddle),
    (
        CursorId::Repair,
        150,
        20,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (CursorId::NoRepair, 190, 1, 0, CursorHotspot::CenterMiddle),
    // --- Special unit cursors ---
    (
        CursorId::DesolatorDeploy,
        78,
        10,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (CursorId::GIDeploy, 239, 10, 0, CursorHotspot::CenterMiddle),
    (CursorId::Crush, 219, 5, 0, CursorHotspot::CenterMiddle),
    (
        CursorId::Tote,
        329,
        10,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::IvanBomb,
        204,
        5,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::Detonate,
        299,
        10,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::Demolish,
        309,
        10,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (CursorId::Disarm, 369, 15, 0, CursorHotspot::CenterMiddle),
    (
        CursorId::InfantryHeal,
        355,
        1,
        0,
        CursorHotspot::CenterMiddle,
    ),
    // --- Spy / infiltration ---
    (CursorId::Disguise, 199, 5, 0, CursorHotspot::CenterMiddle),
    (CursorId::SpyTech, 224, 5, 0, CursorHotspot::CenterMiddle),
    (CursorId::SpyPower, 229, 5, 0, CursorHotspot::CenterMiddle),
    // --- Mind control ---
    (
        CursorId::MindControl,
        209,
        5,
        0,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::NoMindControl,
        431,
        1,
        0,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::RemoveSquid,
        214,
        5,
        0,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::InfantryAbsorb,
        422,
        9,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    // --- Superweapons ---
    (
        CursorId::Nuke,
        319,
        10,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::Chronosphere,
        357,
        12,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::IronCurtain,
        346,
        5,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::LightningStorm,
        279,
        20,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::Paradrop,
        259,
        10,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::ForceShield,
        450,
        10,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::NoForceShield,
        460,
        10,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::GeneticMutator,
        470,
        10,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::AirStrike,
        480,
        8,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::PsychicDominator,
        488,
        8,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::PsychicReveal,
        496,
        8,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::SpyPlane,
        504,
        8,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
    (
        CursorId::Beacon,
        435,
        15,
        ANIM_INTERVAL_MS,
        CursorHotspot::CenterMiddle,
    ),
];

pub(crate) fn build_software_cursor(
    gpu: &GpuContext,
    batch: &BatchRenderer,
    asset_manager: &AssetManager,
) -> Option<crate::app_render::SoftwareCursor> {
    let palette = asset_manager
        .get("mousepal.pal")
        .and_then(|data| Palette::from_bytes(&data).ok())?;
    let shp_data = asset_manager
        .get("mouse.sha")
        .or_else(|| asset_manager.get("mouse.shp"))?;
    let shp = ShpFile::from_bytes(&shp_data).ok()?;
    if shp.frames.is_empty() {
        return None;
    }

    maybe_export_mouse_sheet(&shp, &palette);

    let mut sequences: HashMap<CursorId, crate::app_render::SoftwareCursorSequence> =
        HashMap::new();
    for &(id, start, count, interval_ms, hotspot) in CURSOR_DEFS {
        if let Some(seq) = build_cursor_sequence(
            gpu,
            batch,
            &shp,
            &palette,
            start,
            count,
            interval_ms,
            hotspot,
        ) {
            sequences.insert(id, seq);
        } else if id == CursorId::Default {
            // Default cursor must load successfully — bail if it fails.
            return None;
        }
        // Non-default cursors that fail silently skip; SoftwareCursor::get() falls back to Default.
    }
    Some(crate::app_render::SoftwareCursor { sequences })
}

fn build_cursor_sequence(
    gpu: &GpuContext,
    batch: &BatchRenderer,
    shp: &ShpFile,
    palette: &Palette,
    start: usize,
    count: usize,
    interval_ms: u64,
    hotspot: CursorHotspot,
) -> Option<crate::app_render::SoftwareCursorSequence> {
    let mut frames = Vec::new();
    for frame_idx in start..start + count {
        if let Some(frame) = build_cursor_frame(gpu, batch, shp, palette, frame_idx) {
            frames.push(frame);
        }
    }
    if frames.is_empty() {
        return None;
    }
    Some(crate::app_render::SoftwareCursorSequence {
        frames,
        interval_ms,
        hotspot: resolve_hotspot(hotspot, shp),
    })
}

fn resolve_hotspot(hotspot: CursorHotspot, shp: &ShpFile) -> [f32; 2] {
    // Hotspot pixel positions within FullWidth × FullHeight canvas (reference §7.3).
    let w: f32 = shp.width as f32;
    let h: f32 = shp.height as f32;
    match hotspot {
        CursorHotspot::TopLeft => [0.0, 0.0],
        CursorHotspot::CenterTop => [w / 2.0, 0.0],
        CursorHotspot::RightTop => [w - 1.0, 0.0],
        CursorHotspot::RightMiddle => [w - 1.0, h / 2.0],
        CursorHotspot::RightBottom => [w - 1.0, h - 1.0],
        CursorHotspot::CenterBottom => [w / 2.0, h - 1.0],
        CursorHotspot::LeftBottom => [0.0, h - 1.0],
        CursorHotspot::LeftMiddle => [0.0, h / 2.0],
        CursorHotspot::CenterMiddle => [w / 2.0, h / 2.0],
    }
}

fn build_cursor_frame(
    gpu: &GpuContext,
    batch: &BatchRenderer,
    shp: &ShpFile,
    palette: &Palette,
    frame_idx: usize,
) -> Option<crate::app_render::SoftwareCursorFrame> {
    if frame_idx >= shp.frames.len() {
        return None;
    }
    let rgba = frame_to_canvas_rgba(shp, frame_idx, palette).ok()?;
    let texture = batch.create_texture(gpu, &rgba, shp.width as u32, shp.height as u32);
    Some(crate::app_render::SoftwareCursorFrame {
        texture,
        width: shp.width as f32,
        height: shp.height as f32,
    })
}

fn maybe_export_mouse_sheet(shp: &ShpFile, palette: &Palette) {
    let enabled = std::env::var("RA2_DEBUG_MOUSE_CURSOR_SHEET")
        .ok()
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            v == "1" || v == "true" || v == "yes" || v == "on"
        })
        .unwrap_or(false);
    if !enabled {
        return;
    }

    let max_frames = shp.frames.len();
    let cols = 10u32;
    let rows = ((max_frames as u32) + cols - 1) / cols;
    let cell_w = shp.width as u32;
    let cell_h = shp.height as u32;
    let sheet_w = cols * cell_w;
    let sheet_h = rows * cell_h;
    let mut rgba = vec![0u8; (sheet_w * sheet_h * 4) as usize];
    fill_checkerboard(&mut rgba, sheet_w, sheet_h);

    for frame_idx in 0..max_frames {
        let Ok(frame_rgba) = frame_to_canvas_rgba(shp, frame_idx, palette) else {
            continue;
        };
        let col = (frame_idx as u32) % cols;
        let row = (frame_idx as u32) / cols;
        let dst_x = col * cell_w;
        let dst_y = row * cell_h;
        blit_rgba(
            &mut rgba,
            sheet_w,
            &frame_rgba,
            cell_w,
            cell_h,
            dst_x,
            dst_y,
        );
        draw_cell_border(&mut rgba, sheet_w, sheet_h, dst_x, dst_y, cell_w, cell_h);
    }

    if let Some(img) = RgbaImage::from_raw(sheet_w, sheet_h, rgba) {
        let _ = img.save(Path::new("debug_mouse_cursor_sheet.png"));
        log::info!(
            "Saved debug_mouse_cursor_sheet.png ({} frames, {}x{})",
            max_frames,
            sheet_w,
            sheet_h
        );
    }
}

fn fill_checkerboard(dst: &mut [u8], width: u32, height: u32) {
    for y in 0..height {
        for x in 0..width {
            let light = ((x / 4) + (y / 4)) % 2 == 0;
            let value = if light { 54 } else { 32 };
            let idx = ((y * width + x) * 4) as usize;
            dst[idx] = value;
            dst[idx + 1] = value;
            dst[idx + 2] = value;
            dst[idx + 3] = 255;
        }
    }
}

fn draw_cell_border(
    dst: &mut [u8],
    dst_width: u32,
    dst_height: u32,
    dst_x: u32,
    dst_y: u32,
    cell_w: u32,
    cell_h: u32,
) {
    let max_x = (dst_x + cell_w).min(dst_width);
    let max_y = (dst_y + cell_h).min(dst_height);
    for x in dst_x..max_x {
        set_rgba(dst, dst_width, x, dst_y, [255, 0, 255, 255]);
        if max_y > 0 {
            set_rgba(dst, dst_width, x, max_y - 1, [255, 0, 255, 255]);
        }
    }
    for y in dst_y..max_y {
        set_rgba(dst, dst_width, dst_x, y, [255, 0, 255, 255]);
        if max_x > 0 {
            set_rgba(dst, dst_width, max_x - 1, y, [255, 0, 255, 255]);
        }
    }
}

fn set_rgba(dst: &mut [u8], dst_width: u32, x: u32, y: u32, rgba: [u8; 4]) {
    let idx = ((y * dst_width + x) * 4) as usize;
    if idx + 3 >= dst.len() {
        return;
    }
    dst[idx..idx + 4].copy_from_slice(&rgba);
}

fn blit_rgba(
    dst: &mut [u8],
    dst_width: u32,
    src: &[u8],
    src_width: u32,
    src_height: u32,
    dst_x: u32,
    dst_y: u32,
) {
    for y in 0..src_height {
        for x in 0..src_width {
            let src_i = ((y * src_width + x) * 4) as usize;
            let dst_i = (((dst_y + y) * dst_width + (dst_x + x)) * 4) as usize;
            if src_i + 3 >= src.len() || dst_i + 3 >= dst.len() {
                continue;
            }
            dst[dst_i..dst_i + 4].copy_from_slice(&src[src_i..src_i + 4]);
        }
    }
}

fn frame_to_canvas_rgba(shp: &ShpFile, frame_idx: usize, palette: &Palette) -> Result<Vec<u8>, ()> {
    let frame = shp.frames.get(frame_idx).ok_or(())?;
    let frame_rgba = shp.frame_to_rgba(frame_idx, palette).map_err(|_| ())?;
    let canvas_w = shp.width as u32;
    let canvas_h = shp.height as u32;
    let mut canvas = vec![0u8; (canvas_w * canvas_h * 4) as usize];
    blit_rgba(
        &mut canvas,
        canvas_w,
        &frame_rgba,
        frame.frame_width as u32,
        frame.frame_height as u32,
        frame.frame_x as u32,
        frame.frame_y as u32,
    );
    Ok(canvas)
}
