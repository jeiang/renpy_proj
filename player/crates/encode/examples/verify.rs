//! End-to-end checks of the encode crate; exits non-zero on the first failure.
//!  - every available encoder: Annex B output, first AU is an IDR, SPS/PPS in every IDR,
//!    IDR on demand, size change, RGBA/BGRA/NV12 input, N of N frames decode (FFmpeg decoder),
//!    the burned timecode survives encoding and decodes by luma threshold 128
//!  - static content: bytes for 120 identical frames, and what FrameGate saves
//!  - Opus: 440 Hz tone round trip, frequency by zero crossings
mod common;
use encode::annexb::{nal_type, profile_level_id, split_nals};
use encode::{FrameGate, OpusEncoder, PixelFormat, RawFrame, VideoEncoder, burn_timecode, list_video_encoders, open_video_encoder};
use ffmpeg_next as ff;
use ff::{Packet, codec, frame};

struct Dec(ff::decoder::Video);

impl Dec {
    fn new() -> Self {
        let c = ff::decoder::find(codec::Id::H264).expect("h264 decoder");
        Dec(codec::context::Context::new_with_codec(c).decoder().video().unwrap())
    }
    /// Decodes one access unit, returns the decoded frame if one came out.
    fn feed(&mut self, au: &[u8]) -> Vec<frame::Video> {
        self.0.send_packet(&Packet::copy(au)).expect("decoder rejected access unit");
        let mut v = Vec::new();
        loop {
            let mut f = frame::Video::empty();
            if self.0.receive_frame(&mut f).is_err() {
                break;
            }
            v.push(f);
        }
        v
    }
    fn flush(&mut self) -> Vec<frame::Video> {
        let _ = self.0.send_eof();
        let mut v = Vec::new();
        loop {
            let mut f = frame::Video::empty();
            if self.0.receive_frame(&mut f).is_err() {
                break;
            }
            v.push(f);
        }
        v
    }
}

fn timecode_of(f: &frame::Video) -> u32 {
    let mut v = 0u32;
    for i in 0..32 {
        let y = f.data(0)[8 * f.stride(0) + 16 * i + 8];
        v = (v << 1) | (y >= 128) as u32;
    }
    v
}

fn check_au(name: &str, data: &[u8], keyframe: bool) {
    assert!(data.starts_with(&[0, 0, 0, 1]) || data.starts_with(&[0, 0, 1]), "{name}: no start code");
    let types: Vec<u8> = split_nals(data).iter().map(|n| nal_type(n)).collect();
    if keyframe {
        assert!(types.contains(&7) && types.contains(&8) && types.contains(&5), "{name}: IDR lacks SPS/PPS: {types:?}");
        let sps = types.iter().position(|t| *t == 7).unwrap();
        let idr = types.iter().position(|t| *t == 5).unwrap();
        assert!(sps < idr, "{name}: SPS after IDR slice");
    } else {
        assert!(!types.contains(&5), "{name}: non-key AU holds an IDR slice");
    }
}

fn run(enc: &mut dyn VideoEncoder, frames: Vec<RawFrame>, force_at: &[usize], check_tc: bool) -> (usize, Vec<usize>, usize) {
    let name = enc.name().to_string();
    let mut dec = Dec::new();
    let (mut aus, mut keys, mut bytes, mut decoded) = (0, Vec::new(), 0, Vec::new());
    let mut expect_tc = Vec::new();
    for (t, f) in frames.iter().enumerate() {
        expect_tc.push(f.capture_ms);
        for e in enc.encode(f, force_at.contains(&t)).unwrap() {
            check_au(&name, &e.data, e.keyframe);
            if aus == 0 {
                assert!(e.keyframe, "{name}: first AU not IDR");
            }
            if e.keyframe {
                keys.push(aus);
            }
            bytes += e.data.len();
            aus += 1;
            decoded.extend(dec.feed(&e.data));
        }
    }
    decoded.extend(dec.flush());
    assert_eq!(aus, frames.len(), "{name}: {} AUs for {} frames", aus, frames.len());
    assert_eq!(decoded.len(), frames.len(), "{name}: decoded {} of {}", decoded.len(), frames.len());
    for &t in force_at {
        assert!(keys.contains(&t), "{name}: force_idr at {t} gave no IDR (keys {keys:?})");
    }
    if check_tc {
        for (d, want) in decoded.iter().zip(&expect_tc) {
            assert_eq!(timecode_of(d), *want, "{name}: timecode lost");
        }
    }
    (aus, keys, bytes)
}

fn main() {
    let encoders = list_video_encoders();
    println!("available encoders: {encoders:?}");
    assert!(!encoders.is_empty());
    let (w, h) = (1280u32, 720u32);
    let base = common::base_scene(w, h);
    for name in &encoders {
        let mk = |w, h| open_video_encoder(w, h, 60, 6000, Some(name)).unwrap();
        // 1. Moving content, IDR on demand, timecode survives, all formats.
        for fmt in [PixelFormat::Rgba, PixelFormat::Bgra, PixelFormat::Nv12] {
            let frames: Vec<RawFrame> = (0..90)
                .map(|t| {
                    let mut f = common::frame_from(&base, w, h, if fmt == PixelFormat::Bgra { fmt } else { PixelFormat::Rgba }, t, true);
                    if fmt == PixelFormat::Nv12 {
                        f = common::to_nv12(&f);
                    }
                    f.capture_ms = 0x9000_0000 + t as u32 * 16 + 3;
                    burn_timecode(&mut f);
                    f
                })
                .collect();
            let mut enc = mk(w, h);
            let (aus, keys, bytes) = run(enc.as_mut(), frames, &[40, 41, 77], true);
            println!("{name:18} {fmt:?}: {aus}/90 AUs decode, {bytes} bytes, IDR at {keys:?} (forced at 40, 41, 77; timecode ok)");
        }
        // 2. SPS profile and level.
        let mut enc = mk(w, h);
        let e = enc.encode(&common::frame_from(&base, w, h, PixelFormat::Rgba, 0, false), false).unwrap();
        let p = profile_level_id(&e[0].data).expect("SPS");
        println!("{name:18} SPS profile_idc={} constraint=0x{:02x} level_idc={}", p[0], p[1], p[2]);
        assert!(p[0] == 66 || p[0] == 77, "profile must be baseline or main");
        // 3. Size change: IDR with new SPS, decodes at the new size.
        let small = common::base_scene(640, 360);
        let mut dec = Dec::new();
        let mut enc = mk(1280, 720);
        let mut got = Vec::new();
        for (t, (bw, bh, b)) in [(1280u32, 720u32, &base), (1280, 720, &base), (640, 360, &small), (640, 360, &small)].into_iter().enumerate() {
            for e in enc.encode(&common::frame_from(b, bw, bh, PixelFormat::Rgba, t, true), false).unwrap() {
                check_au(name, &e.data, e.keyframe);
                if t == 2 {
                    assert!(e.keyframe, "{name}: size change did not yield an IDR");
                }
                got.extend(dec.feed(&e.data));
            }
        }
        got.extend(dec.flush());
        assert_eq!(got.len(), 4);
        assert_eq!((got[3].width(), got[3].height()), (640, 360));
        println!("{name:18} size change 1280x720 -> 640x360: IDR + decodes");
        // 4. Static content bytes; what FrameGate saves.
        let mut enc = mk(w, h);
        let mut still = common::frame_from(&base, w, h, PixelFormat::Rgba, 0, false);
        let (mut first, mut rest) = (0, 0);
        let mut gated_bytes = 0;
        let mut gate = FrameGate::new();
        let mut enc_g = mk(w, h);
        for t in 0..120 {
            still.capture_ms = t * 16;
            burn_timecode(&mut still); // the timecode changes every frame
            for e in enc.encode(&still, false).unwrap() {
                if t == 0 { first += e.data.len() } else { rest += e.data.len() }
            }
            if gate.changed(&still) {
                gated_bytes += enc_g.encode(&still, false).unwrap().iter().map(|e| e.data.len()).sum::<usize>();
            }
        }
        let mut enc = mk(w, h);
        let still = common::frame_from(&base, w, h, PixelFormat::Rgba, 0, false);
        let mut sizes = Vec::new();
        for _ in 0..120 {
            sizes.push(enc.encode(&still, false).unwrap().iter().map(|e| e.data.len()).sum::<usize>());
        }
        println!(
            "{name:18} 120 identical frames (no timecode): first {} B, other 119 frames {} B total ({:.0} B/frame, max {})",
            sizes[0],
            sizes[1..].iter().sum::<usize>(),
            sizes[1..].iter().sum::<usize>() as f64 / 119.0,
            sizes[1..].iter().max().unwrap()
        );
        println!(
            "{name:18} 120 identical frames (timecode changing): first frame {first} B, other 119 frames {rest} B ({:.0} B/frame); \
             with FrameGate: {gated_bytes} B total ({} of 120 frames reach the encoder)",
            rest as f64 / 119.0,
            1
        );
    }
    // 5. Opus round trip.
    let mut enc = OpusEncoder::new(96).unwrap();
    let dec_codec = ff::decoder::find_by_name("libopus").or_else(|| ff::decoder::find_by_name("opus")).expect("opus decoder");
    let mut dctx = codec::context::Context::new_with_codec(dec_codec).decoder().audio().unwrap();
    let mut pcm = Vec::new();
    let mut sizes = Vec::new();
    for p in 0..100 {
        let mut buf = Vec::with_capacity(1920);
        for i in 0..960 {
            let t = (p * 960 + i) as f32 / 48000.0;
            let s = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5;
            buf.push(s);
            buf.push(s);
        }
        let pkt = enc.encode(&buf).unwrap();
        assert!(!pkt.is_empty() && pkt.len() < 1275);
        sizes.push(pkt.len());
        dctx.send_packet(&Packet::copy(&pkt)).unwrap();
        let mut f = frame::Audio::empty();
        while dctx.receive_frame(&mut f).is_ok() {
            let fmt = f.format();
            let n = f.samples();
            let ch = f.channels() as usize;
            use ff::format::sample::{Sample, Type};
            for i in 0..n {
                let d = f.data(0);
                let v = match fmt {
                    Sample::F32(Type::Planar) => f32::from_ne_bytes(d[i * 4..i * 4 + 4].try_into().unwrap()),
                    Sample::F32(Type::Packed) => f32::from_ne_bytes(d[i * ch * 4..i * ch * 4 + 4].try_into().unwrap()),
                    Sample::I16(Type::Planar) => i16::from_ne_bytes(d[i * 2..i * 2 + 2].try_into().unwrap()) as f32 / 32768.0,
                    Sample::I16(Type::Packed) => i16::from_ne_bytes(d[i * ch * 2..i * ch * 2 + 2].try_into().unwrap()) as f32 / 32768.0,
                    other => panic!("unexpected decoder format {other:?}"),
                };
                pcm.push(v);
            }
        }
    }
    let skip = 48000 / 10;
    let body = &pcm[skip..];
    let crossings = body.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
    let hz = crossings as f64 / 2.0 / (body.len() as f64 / 48000.0);
    println!("Opus: {} samples decoded, tone measured {hz:.1} Hz (sent 440), packet bytes mean {}", pcm.len(), sizes.iter().sum::<usize>() / sizes.len());
    assert!((hz - 440.0).abs() < 5.0, "Opus round trip frequency");
    assert!(enc.encode(&[0.0; 100]).is_err());
    println!("ALL OK");
}
