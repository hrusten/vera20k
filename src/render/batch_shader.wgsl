// Camera uniform: viewport size, scroll position, and depth parameters.
struct Camera {
    screen_size: vec2f,
    camera_pos: vec2f,
    // Zoom level: 1.0 = native, >1.0 = zoomed in, <1.0 = zoomed out.
    zoom: f32,
    pad0: f32,
};
@group(0) @binding(0) var<uniform> camera: Camera;

// Texture and sampler (nearest-neighbor for pixel art).
@group(1) @binding(0) var t_sprite: texture_2d<f32>;
@group(1) @binding(1) var s_sprite: sampler;

// Per-instance data from the instance buffer.
struct Instance {
    @location(0) position: vec2f,
    @location(1) size: vec2f,
    @location(2) uv_origin: vec2f,
    @location(3) uv_size: vec2f,
    @location(4) depth: f32,
    @location(5) tint: vec3f,
    @location(6) alpha: f32,
};

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
    @location(1) tint: vec3f,
    @location(2) alpha: f32,
};

@vertex
fn vs_main(
    @builtin(vertex_index) idx: u32,
    instance: Instance,
) -> VertexOutput {
    // Quad vertex positions: (0,0) top-left to (1,1) bottom-right.
    var quad_pos = array<vec2f, 6>(
        vec2f(0.0, 0.0), vec2f(1.0, 0.0), vec2f(0.0, 1.0),
        vec2f(0.0, 1.0), vec2f(1.0, 0.0), vec2f(1.0, 1.0),
    );
    var quad_uv = array<vec2f, 6>(
        vec2f(0.0, 0.0), vec2f(1.0, 0.0), vec2f(0.0, 1.0),
        vec2f(0.0, 1.0), vec2f(1.0, 0.0), vec2f(1.0, 1.0),
    );

    let local: vec2f = quad_pos[idx];
    // Screen-space pixel position of this vertex, scaled by zoom.
    // At native zoom (1.0) we pixel-snap to prevent sub-pixel seams between
    // adjacent tiles. At other zoom levels we skip the snap — floor() on
    // fractional screen positions causes 1px gaps between tiles because
    // adjacent world-space boundaries can round to different integers.
    // We also expand each quad by 0.5 screen pixels per side to eliminate
    // rasterization gaps at diamond edge midpoints (top-left fill rule).
    let is_zoomed: bool = abs(camera.zoom - 1.0) >= 0.001;
    let pad: f32 = select(0.0, 0.5 / camera.zoom, is_zoomed);
    let raw_pos: vec2f = (instance.position - vec2f(pad, pad) + local * (instance.size + vec2f(pad * 2.0, pad * 2.0)) - camera.camera_pos) * camera.zoom;
    let pixel_pos: vec2f = select(raw_pos, floor(raw_pos + vec2f(0.5, 0.5)), !is_zoomed);

    // Convert pixel coordinates to clip space.
    // Screen: (0,0) = top-left, (screen_size) = bottom-right.
    // Clip: (-1,-1) = bottom-left, (1,1) = top-right.
    let clip_x: f32 = (pixel_pos.x / camera.screen_size.x) * 2.0 - 1.0;
    let clip_y: f32 = 1.0 - (pixel_pos.y / camera.screen_size.y) * 2.0;

    var output: VertexOutput;
    output.position = vec4f(clip_x, clip_y, instance.depth, 1.0);
    output.uv = instance.uv_origin + quad_uv[idx] * instance.uv_size;
    output.tint = instance.tint;
    output.alpha = instance.alpha;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
    let color: vec4f = textureSample(t_sprite, s_sprite, input.uv);
    // Discard fully transparent pixels so they don't write to the depth buffer.
    // Without this, transparent regions of sprite quads would occlude objects behind them.
    if (color.a < 0.01) {
        discard;
    }
    // Apply map lighting tint and alpha. Tint (1,1,1) = no change.
    // Alpha 1.0 = fully opaque, 0.5 = 50% translucent (chrono warp effect).
    return vec4f(color.rgb * input.tint, color.a * input.alpha);
}
