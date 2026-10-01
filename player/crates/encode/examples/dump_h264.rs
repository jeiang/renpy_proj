//! Writes an Annex B .h264 file from synthetic frames and checks its structure.
//! Usage: dump_h264 <out.h264> [encoder] [frames=120] [WxH=1280x720] [moving|static] [rgba|bgra|nv12]
//! Validate the file with: ffprobe -count_frames -show_entries stream=nb_read_frames,profile,level out.h264
mod common;
use encode::annexb::{nal_type, profile_level_id, split_nals};
use encode::{PixelFormat, open_video_encoder};
use std::io::Write;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let out = a.get(1).cloned().unwrap_or("out.h264".into());
    let prefer = a.get(2).filter(|s| s.as_str() != "default").map(|s| s.as_str());
    let n: usize = a.get(3).map_or(120, |s| s.parse().unwrap());
    let (w, h) = a.get(4).map_or((1280, 720), |s| {
        let (w, h) = s.split_once('x').unwrap();
        (w.parse().unwrap(), h.parse().unwrap())
    });
    let moving = a.get(5).is_none_or(|s| s == "moving");
    let fmt = match a.get(6).map(|s| s.as_str()) {
        Some("bgra") => PixelFormat::Bgra,
        Some("nv12") => PixelFormat::Nv12,
        _ => PixelFormat::Rgba,
    };
    let mut enc = open_video_encoder(w, h, 60, 6000, prefer)?;
    let base = common::base_scene(w, h);
    let mut file = std::fs::File::create(&out)?;
    let mut total = 0;
    let mut access_units = 0;
    let mut idrs = Vec::new();
    for t in 0..n {
        let mut f = common::frame_from(&base, w, h, PixelFormat::Rgba, t, moving);
        f = match fmt {
            PixelFormat::Nv12 => common::to_nv12(&f),
            PixelFormat::Bgra => common::frame_from(&base, w, h, PixelFormat::Bgra, t, moving),
            _ => f,
        };
        let force = t == 30 || t == 61; // IDR on demand
        if let Ok(ms) = std::env::var("PACE_MS") { std::thread::sleep(std::time::Duration::from_millis(ms.parse().unwrap())); }
        let outs = enc.encode(&f, force)?;
        if std::env::var_os("LAG").is_some() { print!("{}", outs.len()); }
        for e in outs {
            let nals: Vec<u8> = split_nals(&e.data).iter().map(|n| nal_type(n)).collect();
            if e.keyframe {
                assert!(nals.contains(&7) && nals.contains(&8) && nals.contains(&5), "IDR without SPS/PPS: {nals:?}");
                idrs.push(access_units);
            }
            if access_units == 0 {
                assert!(e.keyframe, "first access unit must be an IDR");
                let p = profile_level_id(&e.data).unwrap();
                println!("SPS profile-level-id = {:02x}{:02x}{:02x}", p[0], p[1], p[2]);
            }
            assert!(e.data.starts_with(&[0, 0, 0, 1]) || e.data.starts_with(&[0, 0, 1]));
            total += e.data.len();
            access_units += 1;
            file.write_all(&e.data)?;
        }
    }
    println!("{}: {n} frames in, {access_units} access units, {total} bytes, IDR at access units {idrs:?} -> {out}", enc.name());
    Ok(())
}
