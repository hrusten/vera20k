//! Internal decode helpers for VXL voxel model parsing.
//! Handles sparse column-based voxel span decoding.
//! Part of assets/ — no dependencies on game modules.

use crate::assets::error::AssetError;
use crate::assets::vxl_file::VxlVoxel;

/// Read a null-terminated ASCII string from a fixed-size byte slice.
pub(crate) fn read_null_string(bytes: &[u8]) -> String {
    let end: usize = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).to_string()
}

/// Decode all voxels for a single limb from the body data.
///
/// Uses the column start/end offset tables to locate each (x,y) column,
/// then walks voxel spans along the Z axis. Each span contains a z_skip
/// (gap before voxels), a count, then count×2 bytes of color+normal pairs.
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_limb_voxels(
    data: &[u8],
    body_start: usize,
    span_start_off: u32,
    span_end_off: u32,
    data_span_off: u32,
    size_x: u8,
    size_y: u8,
    size_z: u8,
) -> Result<Vec<VxlVoxel>, AssetError> {
    let col_count: usize = size_x as usize * size_y as usize;
    let start_table: usize = body_start + span_start_off as usize;
    let end_table: usize = body_start + span_end_off as usize;
    let data_base: usize = body_start + data_span_off as usize;

    // Validate offset tables fit in file.
    let table_bytes: usize = col_count * 4;
    if start_table + table_bytes > data.len() || end_table + table_bytes > data.len() {
        return Err(AssetError::InvalidVxlFile {
            reason: format!(
                "Column offset tables extend past file end (start={}, end={}, cols={}, file={})",
                start_table,
                end_table,
                col_count,
                data.len()
            ),
        });
    }

    let mut voxels: Vec<VxlVoxel> = Vec::new();

    for col_idx in 0..col_count {
        let x: u8 = (col_idx % size_x as usize) as u8;
        let y: u8 = (col_idx / size_x as usize) as u8;

        // Column start is a byte offset relative to data_base.
        let col_start: i32 = i32::from_le_bytes([
            data[start_table + col_idx * 4],
            data[start_table + col_idx * 4 + 1],
            data[start_table + col_idx * 4 + 2],
            data[start_table + col_idx * 4 + 3],
        ]);

        // -1 means empty column (no voxels).
        if col_start < 0 {
            continue;
        }

        decode_column(
            data,
            data_base,
            col_start as usize,
            x,
            y,
            size_z,
            &mut voxels,
        )?;
    }

    Ok(voxels)
}

/// Decode voxel spans for a single (x, y) column.
///
/// Walks span-by-span: z_skip bytes to advance Z, count voxels to read,
/// then a dup_count byte (unused validation). Continues until z >= size_z.
fn decode_column(
    data: &[u8],
    data_base: usize,
    col_offset: usize,
    x: u8,
    y: u8,
    size_z: u8,
    voxels: &mut Vec<VxlVoxel>,
) -> Result<(), AssetError> {
    let mut pos: usize = data_base + col_offset;
    let mut z: u8 = 0;

    while z < size_z {
        if pos + 3 > data.len() {
            break;
        }
        let z_skip: u8 = data[pos];
        let count: u8 = data[pos + 1];
        pos += 2;

        z = z.saturating_add(z_skip);

        if count == 0 {
            // Empty span — just skip. Read dup_count byte.
            if pos < data.len() {
                pos += 1;
            }
            continue;
        }

        // Read `count` voxels (2 bytes each: color + normal).
        let voxel_bytes: usize = count as usize * 2;
        if pos + voxel_bytes > data.len() {
            return Err(AssetError::InvalidVxlFile {
                reason: format!(
                    "Voxel span needs {} bytes at offset {}, but file is {} bytes",
                    voxel_bytes,
                    pos,
                    data.len()
                ),
            });
        }

        for v in 0..count {
            let vz: u8 = z.saturating_add(v);
            if vz < size_z {
                voxels.push(VxlVoxel {
                    x,
                    y,
                    z: vz,
                    color_index: data[pos + v as usize * 2],
                    normal_index: data[pos + v as usize * 2 + 1],
                });
            }
        }
        pos += voxel_bytes;

        // Skip dup_count byte (validation/padding — unused by us).
        if pos < data.len() {
            pos += 1;
        }

        z = z.saturating_add(count);
    }

    Ok(())
}
