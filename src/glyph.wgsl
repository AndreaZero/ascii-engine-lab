// The only raster primitive is a full-screen triangle. Its input is semantic
// glyph cells, never a color/depth image of a rendered world.
struct GlyphCell {
    glyph: u32,
    flags: u32,
    id: u32,
    pad: u32,
    color: vec4<f32>,
};

struct View {
    width: u32,
    height: u32,
    cols: u32,
    rows: u32,
};

@group(0) @binding(0) var<storage, read> cells: array<GlyphCell>;
@group(0) @binding(1) var glyph_atlas: texture_2d<f32>;
@group(0) @binding(2) var<uniform> view: View;

@vertex
fn vs_main(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4<f32> {
    let points = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    return vec4<f32>(points[vertex], 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let grid = pixel.xy * vec2<f32>(f32(view.cols), f32(view.rows))
        / vec2<f32>(f32(view.width), f32(view.height));
    let cell_xy = min(vec2<u32>(grid), vec2<u32>(view.cols - 1u, view.rows - 1u));
    let cell = cells[cell_xy.y * view.cols + cell_xy.x];
    // Invalid/control characters are blank. No Unicode or solid fallback.
    if (cell.glyph < 32u || cell.glyph > 126u) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let local = min(vec2<u32>(fract(grid) * 8.0), vec2<u32>(7u));
    let origin = vec2<u32>(cell.glyph % 16u, cell.glyph / 16u) * 8u;
    let ink = textureLoad(glyph_atlas, vec2<i32>(origin + local), 0).r;
    let color = clamp(cell.color.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(color * ink * clamp(cell.color.a, 0.0, 1.0), 1.0);
}
