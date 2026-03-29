// Shroud multiply pass — GPU equivalent of the ABuffer brightness multiply.
//
// Draws a full-screen triangle. Each pixel samples the shroud buffer (R8Unorm)
// and outputs that brightness as RGB. Multiplicative blending (DstColor × Zero)
// darkens the existing framebuffer per-pixel, matching how gamemd.exe's
// TMP_TileBlitter reads the ABuffer to darken each rendered pixel.
//
// Shroud buffer values: 0x00 = black (full shroud), 0x7F = neutral (no darkening).
// R8Unorm maps 0x00→0.0 and 0xFF→1.0, so 0x7F→~0.498. We scale so 0x7F→1.0.

@group(0) @binding(0) var t_shroud: texture_2d<f32>;
@group(0) @binding(1) var s_shroud: sampler;

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    // Full-screen triangle: 3 vertices cover the entire clip space.
    // Oversized to avoid quad diagonal overdraw.
    var positions = array<vec2f, 6>(
        vec2f(0.0, 0.0), vec2f(1.0, 0.0), vec2f(0.0, 1.0),
        vec2f(0.0, 1.0), vec2f(1.0, 0.0), vec2f(1.0, 1.0),
    );

    let pos: vec2f = positions[idx];
    let clip_x: f32 = pos.x * 2.0 - 1.0;
    let clip_y: f32 = 1.0 - pos.y * 2.0;

    var output: VertexOutput;
    output.position = vec4f(clip_x, clip_y, 0.0, 1.0);
    output.uv = pos;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
    let shroud_val: f32 = textureSample(t_shroud, s_shroud, input.uv).r;
    // Scale so 0x7F (0.498 normalized) → 1.0 (full brightness).
    let brightness: f32 = clamp(shroud_val * 2.008, 0.0, 1.0);
    // The original engine multiplies in linear RGB565 space. Our sRGB surface
    // converts shader output from linear→sRGB before blending, making dark
    // gradients appear brighter/wider. To compensate, convert our linear
    // brightness back to "sRGB-encoded linear" so the GPU's linear→sRGB
    // conversion produces the actual linear value we want for the blend.
    // srgb_to_linear approximation: x^2.2
    let corrected: f32 = pow(brightness, 2.2);
    return vec4f(corrected, corrected, corrected, 1.0);
}
