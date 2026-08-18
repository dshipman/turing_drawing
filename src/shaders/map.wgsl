struct Uniforms {
    origin: vec4<f32>,
    inv_scale: vec4<f32>,
    map_dims: vec4<f32>,
    letterbox: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(0) @binding(1)
var canvas_tex: texture_2d<f32>;

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
    let drawn = uniforms.inv_scale.yz;
    if (drawn.x <= 0.0 || drawn.y <= 0.0) {
        return uniforms.letterbox;
    }

    let pixel = in.uv * uniforms.map_dims.zw;
    let local = pixel - uniforms.origin.xy;

    if (local.x < 0.0 || local.y < 0.0 || local.x >= drawn.x || local.y >= drawn.y) {
        return uniforms.letterbox;
    }

    let cell = vec2<i32>(local * uniforms.inv_scale.x);
    let dims = vec2<i32>(uniforms.map_dims.xy);
    if (cell.x < 0 || cell.y < 0 || cell.x >= dims.x || cell.y >= dims.y) {
        return uniforms.letterbox;
    }

    return textureLoad(canvas_tex, cell, 0);
}
