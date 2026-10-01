//! Benchmarks every available encoder at 1080p60: ms per frame, bytes, keyframe positions,
//! and bytes for 120 identical frames. Also times FrameGate. Run with --release.
//! Usage: encode_bench [frames=240]
mod common;
use encode::{FrameGate, PixelFormat, list_video_encoders, open_video_encoder};
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let n: usize = std::env::args().nth(1).map_or(240, |s| s.parse().unwrap());
    let (w, h) = (1920, 1080);
    let base = common::base_scene(w, h);
    let names = list_video_encoders();
    println!("available encoders: {names:?}");
    let frames: Vec<_> = (0..n).map(|t| common::frame_from(&base, w, h, PixelFormat::Rgba, t, true)).collect();
    for name in &names {
        for (label, moving) in [("moving", true), ("static", false)] {
            let mut enc = open_video_encoder(w, h, 60, 8000, Some(name))?;
            let mut bytes = 0;
            let mut keys = Vec::new();
            let mut units = 0;
            let mut times = Vec::new();
            let count = if moving { n } else { 120 };
            let still = common::frame_from(&base, w, h, PixelFormat::Rgba, 0, false);
            for t in 0..count {
                let f = if moving { &frames[t] } else { &still };
                let t0 = Instant::now();
                let out = enc.encode(f, false)?;
                times.push(t0.elapsed().as_secs_f64() * 1000.0);
                for e in out {
                    if e.keyframe {
                        keys.push(units);
                    }
                    units += 1;
                    bytes += e.data.len();
                }
            }
            // Skip the first frames (encoder warm-up, first IDR) in the mean.
            let steady = &times[times.len().min(10)..];
            let mean = steady.iter().sum::<f64>() / steady.len().max(1) as f64;
            let max = steady.iter().cloned().fold(0.0, f64::max);
            println!(
                "{name:20} {label:6} {count} frames: {mean:.2} ms/frame (max {max:.2}), {bytes} bytes in {units} units, keyframes at {keys:?}"
            );
        }
    }
    // FrameGate cost: identical frames (full compare) and a change at the last byte.
    let mut gate = FrameGate::new();
    let f = &frames[0];
    gate.changed(f);
    let t0 = Instant::now();
    for _ in 0..100 {
        assert!(!gate.changed(f));
    }
    println!("FrameGate identical 1080p RGBA: {:.3} ms/frame", t0.elapsed().as_secs_f64() * 10.0);
    let t0 = Instant::now();
    let mut changed = 0;
    for t in 0..100 {
        changed += gate.changed(&frames[t % 2 + 1]) as usize;
    }
    println!("FrameGate changed 1080p RGBA (compare + copy): {:.3} ms/frame ({changed}/100 changed)", t0.elapsed().as_secs_f64() * 10.0);
    Ok(())
}
