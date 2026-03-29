// Catmull-Rom bicubic upscale pass.
//
// Draws a full-screen quad sampling the intermediate render target through a
// 5-tap optimized Catmull-Rom bicubic filter. Exploits hardware bilinear
// filtering by combining adjacent weight pairs and offsetting sample positions.
//
// Ported from cnc-ddraw (MIT, TheRealMJP):
// https://gist.github.com/TheRealMJP/c83b8c0f46b63f3a88a5986f4fa982b1
// http://vec3.ca/bicubic-filtering-in-fewer-taps/

struct Params {
    texture_size: vec2f,   // source texture dimensions in pixels
};

@group(0) @binding(0) var t_source: texture_2d<f32>;
@group(0) @binding(1) var s_source: sampler;  // must be Linear for the bilinear trick
@group(0) @binding(2) var<uniform> params: Params;

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
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
    let tex_size: vec2f = params.texture_size;
    let inv_size: vec2f = 1.0 / tex_size;

    // Convert UV to texel coordinates.
    let sample_pos: vec2f = input.uv * tex_size;
    let tex_pos1: vec2f = floor(sample_pos - 0.5) + 0.5;

    // Fractional offset within the texel.
    let f: vec2f = sample_pos - tex_pos1;

    // Catmull-Rom basis weights for the 4 taps per axis.
    let w0: vec2f = f * (-0.5 + f * (1.0 - 0.5 * f));
    let w1: vec2f = 1.0 + f * f * (-2.5 + 1.5 * f);
    let w2: vec2f = f * (0.5 + f * (2.0 - 1.5 * f));
    let w3: vec2f = f * f * (-0.5 + 0.5 * f);

    // Combine the two center weights and compute the bilinear offset.
    let w12: vec2f = w1 + w2;
    let offset12: vec2f = w2 / w12;

    // Texel center positions for the three sample columns/rows.
    let tex_pos0: vec2f = (tex_pos1 - 1.0) * inv_size;
    let tex_pos3: vec2f = (tex_pos1 + 2.0) * inv_size;
    let tex_pos12: vec2f = (tex_pos1 + offset12) * inv_size;

    // 2D weights for the 5 samples (cross pattern).
    let wtm: f32 = w12.x * w0.y;   // top-middle
    let wml: f32 = w0.x * w12.y;   // middle-left
    let wmm: f32 = w12.x * w12.y;  // middle-middle (center)
    let wmr: f32 = w3.x * w12.y;   // middle-right
    let wbm: f32 = w12.x * w3.y;   // bottom-middle

    // 5-tap sample using hardware bilinear filtering.
    var result: vec3f = vec3f(0.0);
    result += textureSample(t_source, s_source, vec2f(tex_pos12.x, tex_pos0.y)).rgb * wtm;
    result += textureSample(t_source, s_source, vec2f(tex_pos0.x, tex_pos12.y)).rgb * wml;
    result += textureSample(t_source, s_source, vec2f(tex_pos12.x, tex_pos12.y)).rgb * wmm;
    result += textureSample(t_source, s_source, vec2f(tex_pos3.x, tex_pos12.y)).rgb * wmr;
    result += textureSample(t_source, s_source, vec2f(tex_pos12.x, tex_pos3.y)).rgb * wbm;

    // Normalize by total weight.
    let norm: f32 = 1.0 / (wtm + wml + wmm + wmr + wbm);
    return vec4f(result * norm, 1.0);
}
