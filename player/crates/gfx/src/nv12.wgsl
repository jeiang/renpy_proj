// Screen capture to NV12 (BT.709, limited range).
//
// The target is an R8Unorm texture of W x (H * 3 / 2) texels, where W and H are the source
// size rounded down to even. Rows 0..H hold Y. Rows H..H*3/2 hold interleaved U, V: even texels
// are U and odd texels are V. Each chroma sample averages a 2 x 2 block of source texels.
// The source holds premultiplied alpha. Straight RGB is recovered as the old CPU path did.

@group(0) @binding(0) var src: texture_2d<f32>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
}

// One oversized triangle covers the whole target.
@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    var o: VsOut;
    o.pos = vec4(p[i], 0.0, 1.0);
    return o;
}

// Straight (non-premultiplied) RGB in 0..1 at source texel (x, y).
fn rgb_at(x: i32, y: i32) -> vec3<f32> {
    let c = textureLoad(src, vec2<i32>(x, y), 0);
    if (c.a > 0.0 && c.a < 1.0) {
        return min(c.rgb / c.a, vec3(1.0));
    }
    return c.rgb;
}

@fragment
fn fs(in: VsOut) -> @location(0) f32 {
    let sz = textureDimensions(src);
    let h = sz.y & 0xfffffffeu;
    let x = u32(in.pos.x);
    let y = u32(in.pos.y);
    if (y < h) {
        let c = rgb_at(i32(x), i32(y));
        // Y = 16 + 219 * luma, in 0..255 units.
        return (16.0 + 219.0 * dot(c, vec3(0.2126, 0.7152, 0.0722))) / 255.0;
    }
    // Chroma row cy covers source rows 2cy and 2cy+1. Pair p covers source columns 2p and 2p+1.
    let cy = y - h;
    let sx = i32(x & 0xfffffffeu);
    let sy = i32(cy << 1u);
    let avg = (rgb_at(sx, sy) + rgb_at(sx + 1, sy) + rgb_at(sx, sy + 1) + rgb_at(sx + 1, sy + 1)) * 0.25;
    let cb = 128.0 + 224.0 * dot(avg, vec3(-0.1146, -0.3854, 0.5));
    let cr = 128.0 + 224.0 * dot(avg, vec3(0.5, -0.4542, -0.0458));
    var v = cb;
    if ((x & 1u) == 1u) {
        v = cr;
    }
    return v / 255.0;
}
