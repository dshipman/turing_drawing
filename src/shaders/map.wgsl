struct Uniforms {
    palette: array<vec4<f32>, 8>,
    map_size: vec2<f32>,
    widget_size: vec2<f32>,
    letterbox: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(0) @binding(1)
var map_tex: texture_2d<u32>;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    var pos = array<vec2<f32>, 3>(
        vec2(-1.0, -1.0),
        vec2(3.0, -1.0),
        vec2(-1.0, 3.0),
    );
    let clip = pos[i];
    var out: VsOut;
    out.clip = vec4(clip, 0.0, 1.0);
    out.uv = vec2(clip.x, -clip.y) * 0.5 + 0.5;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let map = uniforms.map_size;
    let widget = uniforms.widget_size;
    if (map.x <= 0.0 || map.y <= 0.0 || widget.x <= 0.0 || widget.y <= 0.0) {
        return uniforms.letterbox;
    }

    let scale = min(widget.x / map.x, widget.y / map.y);
    let drawn = map * scale;
    let origin = (widget - drawn) * 0.5;
    let local = in.uv * widget - origin;

    if (local.x < 0.0 || local.y < 0.0 || local.x >= drawn.x || local.y >= drawn.y) {
        return uniforms.letterbox;
    }

    let cell = vec2<i32>(local / scale);
    let dims = vec2<i32>(map);
    if (cell.x < 0 || cell.y < 0 || cell.x >= dims.x || cell.y >= dims.y) {
        return uniforms.letterbox;
    }

    let sy = min(textureLoad(map_tex, cell, 0).r, 7u);
    return uniforms.palette[sy];
}
