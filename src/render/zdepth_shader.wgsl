// Per-pixel Z-depth shader for terrain tiles and cliff redraw.
//
// Same vertex shader as batch_shader.wgsl. Fragment shader samples an R8 depth
// atlas (binding 2) at the same UV as the color texture, then writes per-pixel
// frag_depth. This allows cliff tiles to have correct per-pixel occlusion
// instead of uniform depth per quad.

struct Camera {
    screen_size: vec2f,
    camera_pos: vec2f,
    // Zoom level: 1.0 = native, >1.0 = zoomed in, <1.0 = zoomed out.
    zoom: f32,
    pad0: f32,
};
@group(0) @binding(0) var<uniform> camera: Camera;

// Color texture + sampler (same as batch_shader).
@group(1) @binding(0) var t_sprite: texture_2d<f32>;
@group(1) @binding(1) var s_sprite: sampler;
// R8 depth atlas — parallel to color atlas, same UV layout.
@group(1) @binding(2) var t_zdepth: texture_2d<f32>;

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
    @location(2) base_depth: f32,
};

@vertex
fn vs_main(
    @builtin(vertex_index) idx: u32,
    instance: Instance,
) -> VertexOutput {
    var quad_pos = array<vec2f, 6>(
        vec2f(0.0, 0.0), vec2f(1.0, 0.0), vec2f(0.0, 1.0),
        vec2f(0.0, 1.0), vec2f(1.0, 0.0), vec2f(1.0, 1.0),
    );
    var quad_uv = array<vec2f, 6>(
        vec2f(0.0, 0.0), vec2f(1.0, 0.0), vec2f(0.0, 1.0),
        vec2f(0.0, 1.0), vec2f(1.0, 0.0), vec2f(1.0, 1.0),
    );

    let local: vec2f = quad_pos[idx];
    let is_zoomed: bool = abs(camera.zoom - 1.0) >= 0.001;
    let pad: f32 = select(0.0, 0.5 / camera.zoom, is_zoomed);
    let raw_pos: vec2f = (instance.position - vec2f(pad, pad) + local * (instance.size + vec2f(pad * 2.0, pad * 2.0)) - camera.camera_pos) * camera.zoom;
    let pixel_pos: vec2f = select(raw_pos, floor(raw_pos + vec2f(0.5, 0.5)), !is_zoomed);

    let clip_x: f32 = (pixel_pos.x / camera.screen_size.x) * 2.0 - 1.0;
    let clip_y: f32 = 1.0 - (pixel_pos.y / camera.screen_size.y) * 2.0;

    var output: VertexOutput;
    // Set position.z to 0.5 (midpoint) — frag_depth overrides the actual depth.
    output.position = vec4f(clip_x, clip_y, 0.5, 1.0);
    output.uv = instance.uv_origin + quad_uv[idx] * instance.uv_size;
    output.tint = instance.tint;
    output.base_depth = instance.depth;
    return output;
}

struct FragOutput {
    @location(0) color: vec4f,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_main(input: VertexOutput) -> FragOutput {
    let color: vec4f = textureSample(t_sprite, s_sprite, input.uv);
    if (color.a < 0.01) {
        discard;
    }

    // Sample R8 depth atlas: value 0..1 (from 0..255 byte).
    let z_sample: f32 = textureSample(t_zdepth, s_sprite, input.uv).r;

    // Depth formula: base_depth is the tile's Y-sorted depth (0=near, 1=far).
    // z_sample offsets per-pixel: higher z_sample values push terrain pixels
    // closer to the camera (lower depth), creating cliff occlusion. Sprites
    // behind a cliff fail the depth test because their depth > cliff depth.
    let depth_scale: f32 = 0.0002;
    let frag_depth: f32 = clamp(input.base_depth - z_sample * depth_scale, 0.001, 0.999);

    var output: FragOutput;
    output.color = vec4f(color.rgb * input.tint, color.a);
    output.depth = frag_depth;
    return output;
}
