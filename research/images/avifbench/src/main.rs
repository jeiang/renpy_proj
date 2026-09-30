use std::{fs, time::Instant};
fn t<F: FnMut() -> (u32, u32)>(mut f: F) -> (f64, u32, u32) {
    let (w, h) = f();
    let mut v: Vec<f64> = (0..7).map(|_| { let s = Instant::now(); std::hint::black_box(f()); s.elapsed().as_secs_f64() * 1e3 }).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (v[0], w, h)
}
fn main() {
    println!("file\tdecoder\tmin_ms\tw\th");
    for e in fs::read_dir(std::env::args().nth(1).unwrap()).unwrap() {
        let p = e.unwrap().path();
        let d = fs::read(&p).unwrap();
        let n = p.file_name().unwrap().to_string_lossy().to_string();
        let (ms, w, h) = t(|| { let im = image::load_from_memory(&d).unwrap().into_rgba8(); (im.width(), im.height()) });
        println!("{n}\timage(avif-native,dav1d)\t{ms:.2}\t{w}\t{h}");
        let (ms, w, h) = t(|| { let dec = avif_decode::Decoder::from_avif(&d).unwrap(); let im = dec.to_image().unwrap(); match im { avif_decode::Image::Rgba8(i) => (i.width() as u32, i.height() as u32), avif_decode::Image::Rgb8(i) => (i.width() as u32, i.height() as u32), _ => (0, 0) } });
        println!("{n}\tavif-decode(dav1d)\t{ms:.2}\t{w}\t{h}");
    }
}
