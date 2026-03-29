//! Internal decode helpers for TMP terrain tile parsing.
//! Handles per-tile cell parsing, diamond pixel unpacking, and extra data overlay.
//! Part of assets/ — no dependencies on game modules.

use crate::assets::error::AssetError;
use crate::assets::tmp_file::TmpTile;
use crate::util::read_helpers::{read_i32_le, read_u32_le};

/// Size of the per-tile cell header in bytes (before pixel data).
const TILE_HEADER_SIZE: usize = 52;

/// Bit flag: tile has extra pixel data (cliff faces, shadows).
const FLAG_HAS_EXTRA_DATA: u32 = 0x01;

/// Bit flag: tile has per-pixel Z-buffer data (depth for occlusion).
/// When NOT set, ZData bytes do not exist after diamond pixel data.
const FLAG_HAS_Z_DATA: u32 = 0x02;

/// Bit flag: tile variants are deterministic damaged states (bridges),
/// not randomly-selected visual diversity picks (normal terrain).
const FLAG_HAS_DAMAGED_DATA: u32 = 0x04;

/// Initial diamond row width in pixels. All TS/RA2 tiles start with 4 pixels.
pub(crate) const DIAMOND_INITIAL_WIDTH: u32 = 4;

/// Diamond row width increment/decrement per row.
pub(crate) const DIAMOND_WIDTH_STEP: u32 = 4;

/// Parse a single tile cell at the given file offset.
///
/// Reads the 52-byte header, unpacks diamond pixel and depth data,
/// and handles optional extra data (cliff faces, shadows).
pub(crate) fn parse_tile_cell(
    data: &[u8],
    offset: usize,
    tile_width: u32,
    tile_height: u32,
    col: u32,
    row: u32,
) -> Result<TmpTile, AssetError> {
    if offset + TILE_HEADER_SIZE > data.len() {
        return Err(AssetError::InvalidTmpFile {
            reason: format!(
                "Tile cell at offset {} extends past file end ({})",
                offset,
                data.len()
            ),
        });
    }

    // Bytes 0-19: metadata we skip (world positions, internal offsets).
    let raw_extra_x: i32 = read_i32_le(data, offset + 20);
    let raw_extra_y: i32 = read_i32_le(data, offset + 24);
    let extra_width: u32 = read_u32_le(data, offset + 28);
    let extra_height: u32 = read_u32_le(data, offset + 32);
    let flags: u32 = read_u32_le(data, offset + 36);

    let height: u8 = data[offset + 40];
    let terrain_type: u8 = data[offset + 41];
    let ramp_type: u8 = data[offset + 42];
    let radar_left: [u8; 3] = [data[offset + 43], data[offset + 44], data[offset + 45]];
    let radar_right: [u8; 3] = [data[offset + 46], data[offset + 47], data[offset + 48]];

    // Convert extra data positions from template-relative to tile-relative.
    // Each tile's origin: x = (col-row)*tile_width/2, y = (col+row)*tile_height/2.
    let has_extra: bool = (flags & FLAG_HAS_EXTRA_DATA) != 0;
    let has_z_data: bool = (flags & FLAG_HAS_Z_DATA) != 0;
    let has_damaged_data: bool = (flags & FLAG_HAS_DAMAGED_DATA) != 0;
    let extra_x: i32 = raw_extra_x - (col as i32 - row as i32) * (tile_width as i32) / 2;
    let extra_y: i32 = raw_extra_y - (col as i32 + row as i32) * (tile_height as i32) / 2;

    // Compute bounding rectangle encompassing diamond + any extra data.
    let (pixel_width, pixel_height, offset_x, offset_y) = if has_extra {
        let min_x: i32 = 0i32.min(extra_x);
        let min_y: i32 = 0i32.min(extra_y);
        let max_x: i32 = (tile_width as i32).max(extra_x + extra_width as i32);
        let max_y: i32 = (tile_height as i32).max(extra_y + extra_height as i32);
        ((max_x - min_x) as u32, (max_y - min_y) as u32, min_x, min_y)
    } else {
        (tile_width, tile_height, 0, 0)
    };

    let buf_size: usize = (pixel_width * pixel_height) as usize;
    let mut pixels: Vec<u8> = vec![0u8; buf_size];
    let mut depth: Vec<u8> = vec![0u8; buf_size];

    let pixel_data_start: usize = offset + TILE_HEADER_SIZE;

    // Unpack diamond pixel data (always present).
    let diamond_bytes: usize = unpack_diamond(
        data,
        pixel_data_start,
        tile_width,
        tile_height,
        &mut pixels,
        pixel_width,
        -offset_x,
        -offset_y,
    )?;

    // ZData (per-pixel depth) only exists when HasZData flag (bit 1) is set.
    // Without this check, we'd consume bytes belonging to ExtraData, corrupting
    // cliff face graphics. See ra2_yr_map_terrain.md §1.4.
    let depth_bytes: usize = if has_z_data {
        unpack_diamond(
            data,
            pixel_data_start + diamond_bytes,
            tile_width,
            tile_height,
            &mut depth,
            pixel_width,
            -offset_x,
            -offset_y,
        )?
    } else {
        0 // No ZData — depth buffer stays zeroed.
    };

    // Handle extra data (cliff faces, etc.) if present.
    if has_extra {
        let extra_start: usize = pixel_data_start + diamond_bytes + depth_bytes;
        let extra_pixel_count: usize = (extra_width * extra_height) as usize;

        // ExtraZData only exists when BOTH HasExtraData AND HasZData are set
        // (ra2_yr_map_terrain.md §1.5).
        let extra_total_bytes: usize = if has_z_data {
            extra_pixel_count * 2 // ExtraData + ExtraZData
        } else {
            extra_pixel_count // ExtraData only
        };

        if extra_start + extra_total_bytes > data.len() {
            return Err(AssetError::InvalidTmpFile {
                reason: format!(
                    "Extra data at offset {} extends past file end (need {} bytes)",
                    extra_start, extra_total_bytes
                ),
            });
        }

        // Overlay extra pixels BEHIND diamond data — only fill where the diamond
        // didn't write (corners and regions outside the diamond shape). The diamond
        // surface texture takes priority; extra data (cliff faces, shadows) fills gaps.
        overlay_rect(
            &data[extra_start..],
            extra_x,
            extra_y,
            extra_width,
            extra_height,
            &mut pixels,
            pixel_width,
            offset_x,
            offset_y,
            |v| v != 0,
            true,
        );
        // Overlay extra depth behind diamond depth — only when ZData flag is set.
        if has_z_data {
            overlay_rect(
                &data[extra_start + extra_pixel_count..],
                extra_x,
                extra_y,
                extra_width,
                extra_height,
                &mut depth,
                pixel_width,
                offset_x,
                offset_y,
                |v| v < 32,
                true,
            );
        }
    }

    Ok(TmpTile {
        height,
        terrain_type,
        ramp_type,
        radar_left,
        radar_right,
        pixels,
        depth,
        pixel_width,
        pixel_height,
        offset_x,
        offset_y,
        has_damaged_data,
    })
}

/// Unpack diamond-shaped tile data into a rectangular pixel buffer.
///
/// Diamond rows expand from 4 pixels wide, growing by 4 each row until the
/// midpoint, then shrinking back. Returns the number of bytes consumed.
#[allow(clippy::too_many_arguments)]
fn unpack_diamond(
    data: &[u8],
    offset: usize,
    tile_width: u32,
    tile_height: u32,
    buf: &mut [u8],
    buf_width: u32,
    buf_x_offset: i32,
    buf_y_offset: i32,
) -> Result<usize, AssetError> {
    let mut read_pos: usize = offset;
    let mut row_width: u32 = DIAMOND_INITIAL_WIDTH;
    let half_minus_one: u32 = tile_height / 2 - 1;

    for j in 0..tile_height {
        if row_width > 0 {
            let x_start: i32 = ((tile_width - row_width) / 2) as i32 + buf_x_offset;
            let y: i32 = j as i32 + buf_y_offset;

            if read_pos + row_width as usize > data.len() {
                return Err(AssetError::InvalidTmpFile {
                    reason: format!(
                        "Diamond row {} needs {} bytes at offset {}, but file is {} bytes",
                        j,
                        row_width,
                        read_pos,
                        data.len()
                    ),
                });
            }

            if y >= 0 && (y as u32) < (buf.len() as u32 / buf_width) {
                let dest: usize = y as usize * buf_width as usize + x_start as usize;
                let end: usize = dest + row_width as usize;
                if end <= buf.len() {
                    buf[dest..end].copy_from_slice(&data[read_pos..read_pos + row_width as usize]);
                }
            }
            read_pos += row_width as usize;
        }

        if j < half_minus_one {
            row_width += DIAMOND_WIDTH_STEP;
        } else {
            row_width = row_width.saturating_sub(DIAMOND_WIDTH_STEP);
        }
    }

    Ok(read_pos - offset)
}

/// Overlay rectangular extra data onto a pixel/depth buffer.
///
/// The `filter` predicate controls which source values are written —
/// e.g., `|v| v != 0` for pixel data, `|v| v < 32` for depth data.
///
/// When `behind` is true, only writes to buffer positions that are still 0
/// (untouched by the diamond). This ensures the diamond surface takes priority
/// and extra data (cliff faces, shadows) only fills the gaps outside the diamond.
#[allow(clippy::too_many_arguments)]
fn overlay_rect(
    extra_data: &[u8],
    extra_x: i32,
    extra_y: i32,
    extra_width: u32,
    extra_height: u32,
    buf: &mut [u8],
    buf_width: u32,
    buf_origin_x: i32,
    buf_origin_y: i32,
    filter: impl Fn(u8) -> bool,
    behind: bool,
) {
    for ey in 0..extra_height {
        for ex in 0..extra_width {
            let src_idx: usize = ey as usize * extra_width as usize + ex as usize;
            let val: u8 = extra_data[src_idx];
            if filter(val) {
                let bx: i32 = extra_x + ex as i32 - buf_origin_x;
                let by: i32 = extra_y + ey as i32 - buf_origin_y;
                if bx >= 0 && by >= 0 {
                    let dest: usize = by as usize * buf_width as usize + bx as usize;
                    if dest < buf.len() && (!behind || buf[dest] == 0) {
                        buf[dest] = val;
                    }
                }
            }
        }
    }
}
