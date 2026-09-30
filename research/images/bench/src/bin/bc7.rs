//! BC7 / BC1 encode cost of one RGBA frame with intel_tex_2 (ISPC kernels). Usage: bc7 <image>
use std::time::Instant;
use intel_tex_2::{bc7, RgbaSurface};
fn main() {
    let p = std::env::args().nth(1).unwrap();
    let im = image::open(&p).unwrap().into_rgba8();
    let (w, h) = (im.width(), im.height());
    let surf = RgbaSurface { width: w, height: h, stride: w * 4, data: im.as_raw() };
    let bc7_sz = bc7::calc_output_size(w, h);
    let mut out = vec![0u8; bc7_sz];
    for (name, set) in [("bc7 ultrafast", bc7::alpha_ultra_fast_settings()), ("bc7 fast", bc7::alpha_fast_settings()), ("bc7 basic", bc7::alpha_basic_settings())] {
        let t = Instant::now();
        bc7::compress_blocks_into(&set, &surf, &mut out);
        println!("{p}\t{w}x{h}\t{name}\t{:.1} ms\t{} MB (RGBA8 {} MB)", t.elapsed().as_secs_f64() * 1e3, bc7_sz >> 20, (w * h * 4) >> 20);
    }
}
