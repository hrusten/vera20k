// VXL compute splat shader — one thread per voxel.
//
// Projects each voxel through the combined transform matrix, converts to
// screen-space via 16.16 fixed-point truncation (matching RA2/TS pixel
// snapping), packs (depth | vpl_page | color_index) into a u32, and writes
// via atomicMin to the atomic framebuffer. Fill rectangle covers fill_size²
// pixels centered on the projected point.
//
// Depth is inverted (65535 - quantized) so that closer voxels produce
// smaller packed values, making atomicMin equivalent to a depth test.

struct SplatParams {
    // Combined transform matrix columns (model → screen world space).
    mat_col0: vec4f,
    mat_col1: vec4f,
    mat_col2: vec4f,
    mat_col3: vec4f,
    // Rendering parameters.
    scale: f32,
    fp_scale: f32,        // 65536.0
    fb_width: u32,
    fb_height: u32,
    buf_off_x_fp: i32,
    buf_off_y_fp: i32,
    fill_size: i32,
    half_fill: i32,
    voxel_count: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(0) @binding(0) var<uniform> params: SplatParams;
// Packed voxel positions: x | (y << 8) | (z << 16).
@group(0) @binding(1) var<storage, read> voxel_positions: array<u32>;
// Packed voxel data: color_index | (normal_index << 8).
@group(0) @binding(2) var<storage, read> voxel_data: array<u32>;
// Normal index → VPL page lookup, packed 4 per u32.
@group(0) @binding(3) var<storage, read> vpl_pages: array<u32>;
// Atomic framebuffer: packed (depth_u16 << 16) | (page << 8) | color_index.
@group(0) @binding(4) var<storage, read_write> atomic_fb: array<atomic<u32>>;

fn get_vpl_page(normal_index: u32) -> u32 {
    let word_idx = normal_index / 4u;
    let byte_idx = normal_index % 4u;
    return (vpl_pages[word_idx] >> (byte_idx * 8u)) & 0xFFu;
}

@compute @workgroup_size(256)
fn splat_main(@builtin(global_invocation_id) gid: vec3u) {
    let idx = gid.x;
    if (idx >= params.voxel_count) {
        return;
    }

    // Unpack voxel position.
    let pos_packed = voxel_positions[idx];
    let vx = f32(pos_packed & 0xFFu);
    let vy = f32((pos_packed >> 8u) & 0xFFu);
    let vz = f32((pos_packed >> 16u) & 0xFFu);

    // Unpack color + normal.
    let data = voxel_data[idx];
    let color_index = data & 0xFFu;
    let normal_index = (data >> 8u) & 0xFFu;

    // Skip empty voxels (color_index 0 = transparent).
    if (color_index == 0u) {
        return;
    }

    // Transform voxel position through the combined matrix.
    let pos = vec4f(vx, vy, vz, 1.0);
    let world_x = dot(params.mat_col0, pos);
    let world_y = dot(params.mat_col1, pos);
    let world_z = dot(params.mat_col2, pos);

    // 16.16 fixed-point projection with truncation (matching CPU `as i32`).
    let sx_fp = i32(world_x * params.scale * params.fp_scale);
    let sy_fp = i32(-world_y * params.scale * params.fp_scale);
    let px = (sx_fp + params.buf_off_x_fp) >> 16;
    let py = (sy_fp + params.buf_off_y_fp) >> 16;

    // Quantize depth to 16 bits. Invert so closer (higher world_z) = smaller packed value.
    // Depth range for VXL sprites is roughly -50..+50; map to 0..65535.
    let depth_norm = clamp((world_z + 50.0) / 100.0, 0.0, 1.0);
    let depth_u16 = 65535u - u32(depth_norm * 65535.0);

    // Look up VPL page for this normal.
    let page = get_vpl_page(normal_index);

    // Pack: depth(16) | page(8) | color_index(8).
    let packed = (depth_u16 << 16u) | ((page & 0xFFu) << 8u) | (color_index & 0xFFu);

    // Write fill rectangle centered on projected point.
    let fill = params.fill_size;
    let half = params.half_fill;
    let w = i32(params.fb_width);
    let h = i32(params.fb_height);

    for (var dy = -half; dy <= fill - 1 - half; dy = dy + 1) {
        for (var dx = -half; dx <= fill - 1 - half; dx = dx + 1) {
            let fx = px + dx;
            let fy = py + dy;
            if (fx >= 0 && fy >= 0 && fx < w && fy < h) {
                let buf_idx = u32(fy) * params.fb_width + u32(fx);
                atomicMin(&atomic_fb[buf_idx], packed);
            }
        }
    }
}
