// YUV -> RGB. `semi` is the mode: 0 planar YUV, 1 semi-planar YUV, 2 planar RGB (ty = G, tu = B, tv = R). Planes: ty = luma, tu = U (planar) or interleaved UV (semi-planar), tv = V (planar only).
struct P {
    y_off: f32, y_scale: f32, c_off: f32, c_scale: f32,
    kr: f32, kb: f32, sample_scale: f32, semi: f32,
    // letterbox: x0,y0,x1,y1 in clip space
    rect: vec4<f32>,
};
@group(0) @binding(0) var ty: texture_2d<f32>;
@group(0) @binding(1) var tu: texture_2d<f32>;
@group(0) @binding(2) var tv: texture_2d<f32>;
@group(0) @binding(3) var smp: sampler;
@group(0) @binding(4) var<uniform> p: P;

struct VOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VOut {
    // two triangles covering the letterbox rect
    var c = array<vec2<f32>, 6>(vec2(0.,0.), vec2(1.,0.), vec2(0.,1.), vec2(0.,1.), vec2(1.,0.), vec2(1.,1.));
    let q = c[i];
    var o: VOut;
    o.pos = vec4(mix(p.rect.x, p.rect.z, q.x), mix(p.rect.y, p.rect.w, q.y), 0., 1.);
    o.uv = q;
    return o;
}

@fragment
fn fs(in: VOut) -> @location(0) vec4<f32> {
    if (p.semi > 1.5) {
        let g = textureSampleLevel(ty, smp, in.uv, 0.).r;
        let b = textureSampleLevel(tu, smp, in.uv, 0.).r;
        let r = textureSampleLevel(tv, smp, in.uv, 0.).r;
        return vec4(clamp(vec3(r, g, b) * p.sample_scale, vec3(0.), vec3(1.)), 1.);
    }
    let y = (textureSampleLevel(ty, smp, in.uv, 0.).r * p.sample_scale - p.y_off) * p.y_scale;
    var cbcr: vec2<f32>;
    if (p.semi > 0.5) {
        cbcr = textureSampleLevel(tu, smp, in.uv, 0.).rg;
    } else {
        cbcr = vec2(textureSampleLevel(tu, smp, in.uv, 0.).r, textureSampleLevel(tv, smp, in.uv, 0.).r);
    }
    cbcr = (cbcr * p.sample_scale - p.c_off) * p.c_scale;
    let r = y + 2. * (1. - p.kr) * cbcr.y;
    let b = y + 2. * (1. - p.kb) * cbcr.x;
    let g = (y - p.kr * r - p.kb * b) / (1. - p.kr - p.kb);
    return vec4(clamp(vec3(r, g, b), vec3(0.), vec3(1.)), 1.);
}
