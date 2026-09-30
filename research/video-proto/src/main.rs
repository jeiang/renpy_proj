// THROWAWAY PROTOTYPE (ticket #23): FFmpeg 7.1 (LGPL) + VideoToolbox/dav1d -> wgpu YUV upload -> WGSL colour conversion,
// presented on a simple clock. No audio, no seek API, no error polish.
//
//   video-proto FILE [--mode play|decode] [--secs 20] [--warmup 3] [--hw auto|off] [--av1 dav1d|vt]
//                    [--size 1280x720] [--prebuf 4] [--queue 8]
//
// `play`   : decoder thread -> bounded queue -> main thread (winit window, wgpu surface, Fifo present).
//            Frames are shown when due on a clock started at the first shown frame; when several frames
//            are due at once only the newest is shown and the rest count as `skipped` (late) drops.
// `decode` : same decoder thread, frames discarded on receipt; measures decode+transfer throughput and cores.
// The clip loops (seek to start on EOF) so any clip can fill the window.
use ffmpeg_next as ff;
use ff::ffi;
use std::collections::HashSet;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

// ------------------------------------------------------------------ args

#[derive(Clone)]
struct Args {
    file: String,
    mode: String,
    secs: f64,
    warmup: f64,
    hw: bool,
    av1_vt: bool,
    size: (u32, u32),
    prebuf: usize,
    queue: usize,
    label: String,
    dump: Option<String>,
}

fn parse_args() -> Args {
    let mut a = Args { file: String::new(), mode: "play".into(), secs: 20., warmup: 3., hw: true, av1_vt: false, size: (1280, 720), prebuf: 4, queue: 8, label: String::new(), dump: None };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut v = || it.next().expect("missing value");
        match k.as_str() {
            "--mode" => a.mode = v(),
            "--secs" => a.secs = v().parse().unwrap(),
            "--warmup" => a.warmup = v().parse().unwrap(),
            "--hw" => a.hw = v() != "off",
            "--av1" => a.av1_vt = v() == "vt",
            "--size" => { let s = v(); let (w, h) = s.split_once('x').unwrap(); a.size = (w.parse().unwrap(), h.parse().unwrap()) }
            "--prebuf" => a.prebuf = v().parse().unwrap(),
            "--queue" => a.queue = v().parse().unwrap(),
            "--label" => a.label = v(),
            "--dump" => a.dump = Some(v()),
            _ => a.file = k,
        }
    }
    assert!(!a.file.is_empty(), "usage: video-proto FILE [options]");
    a
}

// ------------------------------------------------------------------ decoder thread

struct Item {
    frame: ff::frame::Video,
    pts: f64, // seconds, monotonically increasing across loops
}

#[derive(Default)]
struct DecInfo {
    hw_frames: AtomicBool,
    got_frame: AtomicBool,
    buffered: AtomicUsize,
    done: AtomicBool,
}

unsafe extern "C" fn get_fmt_vt(_: *mut ffi::AVCodecContext, mut f: *const ffi::AVPixelFormat) -> ffi::AVPixelFormat {
    // hw formats are listed first, software fallbacks last; take VideoToolbox if offered, else the last (software) entry.
    let mut last = ffi::AVPixelFormat::AV_PIX_FMT_NONE;
    while *f != ffi::AVPixelFormat::AV_PIX_FMT_NONE {
        if *f == ffi::AVPixelFormat::AV_PIX_FMT_VIDEOTOOLBOX { return *f; }
        last = *f;
        f = f.add(1);
    }
    last
}

fn decode_thread(a: Args, tx: SyncSender<Item>, info: Arc<DecInfo>) {
    let r = run_decoder(&a, a.hw, &tx, &info);
    let r = match r {
        Err(e) if a.hw && !info.got_frame.load(Ordering::Relaxed) => {
            eprintln!("hw decode failed before first frame ({e}); retrying in software");
            run_decoder(&a, false, &tx, &info)
        }
        r => r,
    };
    if let Err(e) = r { eprintln!("decoder ended: {e}"); }
    info.done.store(true, Ordering::Release);
}

fn run_decoder(a: &Args, hw: bool, tx: &SyncSender<Item>, info: &DecInfo) -> Result<(), ff::Error> {
    let mut input = ff::format::input(&a.file)?;
    let (idx, tb, fps, params) = {
        let s = input.streams().best(ff::media::Type::Video).ok_or(ff::Error::StreamNotFound)?;
        let fr = s.avg_frame_rate();
        let fps = if fr.denominator() > 0 { fr.numerator() as f64 / fr.denominator() as f64 } else { 30. };
        (s.index(), f64::from(s.time_base()), fps, s.parameters())
    };
    let mut ctx = ff::codec::Context::from_parameters(params)?;
    let id = ctx.id();
    let mut hw_on = false;
    unsafe {
        let p = ctx.as_mut_ptr();
        if hw && !(id == ff::codec::Id::AV1 && !a.av1_vt) {
            let mut dev: *mut ffi::AVBufferRef = ptr::null_mut();
            let r = ffi::av_hwdevice_ctx_create(&mut dev, ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX, ptr::null(), ptr::null_mut(), 0);
            if r >= 0 {
                (*p).hw_device_ctx = dev;
                (*p).get_format = Some(get_fmt_vt);
                hw_on = true;
            }
        }
        // hwaccel decoders are driven by the hardware; software gets frame+slice threads with auto count.
        (*p).thread_count = if hw_on { 1 } else { 0 };
        (*p).thread_type = (ffi::FF_THREAD_FRAME | ffi::FF_THREAD_SLICE) as i32;
    }
    let dec = ctx.decoder();
    let codec = if id == ff::codec::Id::AV1 && a.av1_vt { ff::decoder::find_by_name("av1") } else { ff::decoder::find(id) };
    let codec = codec.ok_or(ff::Error::DecoderNotFound)?;
    eprintln!("decoder: {} (hwaccel requested: {hw_on})", codec.name());
    let mut dec = dec.open_as(codec)?.video()?;

    let mut base = 0.0f64; // offset added to this loop iteration
    loop {
        let (mut iter_first, mut iter_max) = (f64::NAN, f64::MIN);
        let mut emit = |dec: &mut ff::decoder::Video, flush: bool| -> Result<bool, ff::Error> {
            if flush { dec.send_eof()?; }
            loop {
                let mut f = ff::frame::Video::empty();
                match dec.receive_frame(&mut f) {
                    Ok(()) => {}
                    Err(ff::Error::Other { errno }) if errno == libc::EAGAIN => return Ok(true),
                    Err(ff::Error::Eof) => return Ok(false),
                    Err(e) => return Err(e),
                }
                let raw = f.timestamp().or(f.pts()).unwrap_or(0) as f64 * tb;
                if iter_first.is_nan() { iter_first = raw; }
                iter_max = iter_max.max(raw);
                let sw = if f.format() == ff::format::Pixel::VIDEOTOOLBOX {
                    let mut sw = ff::frame::Video::empty();
                    let r = unsafe { ffi::av_hwframe_transfer_data(sw.as_mut_ptr(), f.as_ptr(), 0) };
                    if r < 0 { return Err(ff::Error::from(r)); }
                    unsafe { ffi::av_frame_copy_props(sw.as_mut_ptr(), f.as_ptr()); }
                    info.hw_frames.store(true, Ordering::Relaxed);
                    sw
                } else { f };
                info.got_frame.store(true, Ordering::Relaxed);
                info.buffered.fetch_add(1, Ordering::AcqRel);
                if tx.send(Item { frame: sw, pts: base + raw - if iter_first.is_nan() { 0. } else { iter_first } }).is_err() {
                    return Ok(false);
                }
            }
        };
        let mut alive = true;
        for (s, pkt) in input.packets() {
            if s.index() != idx { continue; }
            dec.send_packet(&pkt)?;
            if !emit(&mut dec, false)? { alive = false; break; }
            // `emit` returns Ok(true) on EAGAIN (need more input) and Ok(false) when the consumer went away.
        }
        if !alive || tx_closed(tx) { return Ok(()); }
        emit(&mut dec, true)?; // drain
        dec.flush();
        base += (iter_max - iter_first) + 1.0 / fps;
        input.seek(0, ..i64::MAX)?;
    }
}

fn tx_closed(_tx: &SyncSender<Item>) -> bool { false }

// ------------------------------------------------------------------ pixel formats

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Layout {
    semi: bool,
    wide: bool, // 16-bit samples
    bits: u32,
    msb_aligned: bool, // P010 style
    csx: u32,
    csy: u32,
    force_full: bool, // yuvj*
}

fn layout_of(p: ff::format::Pixel) -> Option<Layout> {
    use ff::format::Pixel::*;
    let l = |semi, wide, bits, msb, csx, csy, full| Layout { semi, wide, bits, msb_aligned: msb, csx, csy, force_full: full };
    Some(match p {
        NV12 => l(true, false, 8, false, 1, 1, false),
        P010LE => l(true, true, 10, true, 1, 1, false),
        YUV420P => l(false, false, 8, false, 1, 1, false),
        YUVJ420P => l(false, false, 8, false, 1, 1, true),
        YUV422P => l(false, false, 8, false, 1, 0, false),
        YUV444P => l(false, false, 8, false, 0, 0, false),
        YUV420P10LE => l(false, true, 10, false, 1, 1, false),
        YUV422P10LE => l(false, true, 10, false, 1, 0, false),
        YUV444P10LE => l(false, true, 10, false, 0, 0, false),
        _ => return Option::None,
    })
}

// ------------------------------------------------------------------ gpu

struct Planes {
    key: (Layout, u32, u32),
    tex: Vec<wgpu::Texture>,
    bind: wgpu::BindGroup,
}

struct Gpu {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipe: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    ubuf: wgpu::Buffer,
    dummy: wgpu::TextureView,
    planes: Option<Planes>,
    dump: Option<String>,
    nframes: u64,
    spare: Option<wgpu::Texture>,
}

impl Gpu {
    fn new(window: Arc<Window>) -> Gpu {
        let inst = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = inst.create_surface(window.clone()).expect("surface");
        let adapter = pollster::block_on(inst.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            apply_limit_buckets: false,
        })).expect("adapter");
        eprintln!("adapter: {:?}", adapter.get_info().name);
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: wgpu::Features::TEXTURE_FORMAT_16BIT_NORM,
            ..Default::default()
        })).expect("device");
        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
        let sz = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: sz.width.max(1),
            height: sz.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0], color_space: wgpu::SurfaceColorSpace::Auto,
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        let sh = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()) });
        let tex_entry = |b| wgpu::BindGroupLayoutEntry { binding: b, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None };
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[
            tex_entry(0), tex_entry(1), tex_entry(2),
            wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            wgpu::BindGroupLayoutEntry { binding: 4, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
        ] });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&bgl)], immediate_size: 0 });
        let pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &sh, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState { module: &sh, entry_point: Some("fs"), compilation_options: Default::default(), targets: &[Some(format.into())] }),
            primitive: Default::default(), depth_stencil: None, multisample: Default::default(), multiview_mask: None, cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });
        let ubuf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let dummy = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::R8Unorm, usage: wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] }).create_view(&Default::default());
        Gpu { window, surface, config, device, queue, pipe, bgl, sampler, ubuf, dummy, planes: None, dump: None, nframes: 0, spare: None }
    }

    fn ensure_planes(&mut self, l: Layout, w: u32, h: u32) {
        if self.planes.as_ref().map(|p| p.key) == Some((l, w, h)) { return; }
        let cw = (w + (1 << l.csx) - 1) >> l.csx;
        let ch = (h + (1 << l.csy) - 1) >> l.csy;
        let mk = |w, h, fmt| self.device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: fmt, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[] });
        let (r, rg) = if l.wide { (wgpu::TextureFormat::R16Unorm, wgpu::TextureFormat::Rg16Unorm) } else { (wgpu::TextureFormat::R8Unorm, wgpu::TextureFormat::Rg8Unorm) };
        let tex: Vec<wgpu::Texture> = if l.semi { vec![mk(w, h, r), mk(cw, ch, rg)] } else { vec![mk(w, h, r), mk(cw, ch, r), mk(cw, ch, r)] };
        let views: Vec<wgpu::TextureView> = tex.iter().map(|t| t.create_view(&Default::default())).collect();
        let v2 = if l.semi { &self.dummy } else { &views[2] };
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.bgl, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&views[0]) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&views[1]) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(v2) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            wgpu::BindGroupEntry { binding: 4, resource: self.ubuf.as_entire_binding() },
        ] });
        self.planes = Some(Planes { key: (l, w, h), tex, bind });
    }

    /// Verification aid: render the current planes offscreen at video size and write a PPM.
    fn dump_ppm(&self, path: &str, w: u32, h: u32) {
        let tex = self.device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: self.config.format, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });
        let view = tex.create_view(&Default::default());
        let bpr = (w * 4 + 255) / 256 * 256;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (bpr * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor { label: None, color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
            rp.set_pipeline(&self.pipe);
            rp.set_bind_group(0, &self.planes.as_ref().unwrap().bind, &[]);
            rp.draw(0..6, 0..1);
        }
        enc.copy_texture_to_buffer(tex.as_image_copy(), wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(h) } }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
        self.queue.submit([enc.finish()]);
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let data = buf.slice(..).get_mapped_range().unwrap();
        let bgra = matches!(self.config.format, wgpu::TextureFormat::Bgra8Unorm);
        let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
        for y in 0..h as usize {
            for x in 0..w as usize {
                let p = &data[y * bpr as usize + x * 4..][..4];
                if bgra { out.extend_from_slice(&[p[2], p[1], p[0]]) } else { out.extend_from_slice(&p[..3]) }
            }
        }
        std::fs::write(path, out).unwrap();
        eprintln!("dumped {path}");
    }

    /// Upload the frame's planes, convert in the shader, present. Returns after `present()`.
    fn show(&mut self, f: &ff::frame::Video, l: Layout) -> bool {
        let (w, h) = (f.width(), f.height());
        self.ensure_planes(l, w, h);
        // uniforms
        let maxv = ((1u32 << l.bits) - 1) as f32;
        let full = l.force_full || f.color_range() == ff::color::Range::JPEG;
        let sh = l.bits - 8;
        let (y_off, y_scale, c_scale) = if full { (0., 1., 1.) } else { ((16u32 << sh) as f32 / maxv, maxv / (219u32 << sh) as f32, maxv / (224u32 << sh) as f32) };
        let c_off = (128u32 << sh) as f32 / maxv;
        let (kr, kb) = match f.color_space() {
            ff::color::Space::BT709 => (0.2126, 0.0722),
            ff::color::Space::BT2020NCL | ff::color::Space::BT2020CL => (0.2627, 0.0593),
            ff::color::Space::BT470BG | ff::color::Space::SMPTE170M => (0.299, 0.114),
            _ => if h >= 720 { (0.2126, 0.0722) } else { (0.299, 0.114) },
        };
        let sample_scale = if l.msb_aligned { 65535. / 65472. } else if l.wide { 65535. / maxv } else { 1. };
        // letterbox
        let (sw, shh) = (self.config.width as f32, self.config.height as f32);
        let (va, wa) = (w as f32 / h as f32, sw / shh);
        let (rx, ry) = if va > wa { (1., wa / va) } else { (va / wa, 1.) };
        let u: [f32; 12] = [y_off, y_scale, c_off, c_scale, kr, kb, sample_scale, if l.semi { 1. } else { 0. }, -rx, ry, rx, -ry];
        self.queue.write_buffer(&self.ubuf, 0, bytemuck::cast_slice(&u));
        let pl = self.planes.as_ref().unwrap();
        for (i, t) in pl.tex.iter().enumerate() {
            let (pw, ph) = if i == 0 { (w, h) } else { ((w + (1 << l.csx) - 1) >> l.csx, (h + (1 << l.csy) - 1) >> l.csy) };
            let bpt = (if l.semi && i == 1 { 2 } else { 1 }) * if l.wide { 2 } else { 1 };
            self.queue.write_texture(
                t.as_image_copy(),
                f.data(i),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(f.stride(i) as u32), rows_per_image: Some(ph) },
                wgpu::Extent3d { width: pw, height: ph, depth_or_array_layers: 1 },
            );
            let _ = bpt;
        }
        self.nframes += 1;
        if self.nframes == 120 {
            if let Some(path) = self.dump.take() {
                let mut u2 = u;
                u2[8..12].copy_from_slice(&[-1., 1., 1., -1.]);
                self.queue.write_buffer(&self.ubuf, 0, bytemuck::cast_slice(&u2));
                self.dump_ppm(&path, w, h);
                self.queue.write_buffer(&self.ubuf, 0, bytemuck::cast_slice(&u));
            }
        }
        let (out, ok) = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => (Some(t), true),
            // Window occluded (another window on top): still do the full upload+convert+draw into an offscreen
            // target so the CPU/GPU cost stays faithful, and report the frame as not presented.
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => (None, false),
            _ => { self.surface.configure(&self.device, &self.config); return false; }
        };
        if out.is_none() && self.spare.is_none() {
            self.spare = Some(self.device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: self.config.width, height: self.config.height, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: self.config.format, usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[] }));
        }
        let view = match &out { Some(t) => t.texture.create_view(&Default::default()), None => self.spare.as_ref().unwrap().create_view(&Default::default()) };
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor { label: None, color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
            rp.set_pipeline(&self.pipe);
            rp.set_bind_group(0, &pl.bind, &[]);
            rp.draw(0..6, 0..1);
        }
        self.queue.submit([enc.finish()]);
        if let Some(t) = out {
            self.window.pre_present_notify();
            self.queue.present(t);
        }
        ok
    }
}

// ------------------------------------------------------------------ measurement

fn rusage_cpu() -> f64 {
    unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut ru);
        let t = |tv: libc::timeval| tv.tv_sec as f64 + tv.tv_usec as f64 * 1e-6;
        t(ru.ru_utime) + t(ru.ru_stime)
    }
}

#[derive(Default, Clone, Copy)]
struct Snap { t: Option<Instant>, cpu: f64, shown: u64, skipped: u64 }

struct Stats {
    args: Args,
    info: Arc<DecInfo>,
    nominal_fps: f64,
    start: Option<Snap>,
    shown: u64,
    skipped: u64,
    occluded: u64,
    last_shown: Option<Instant>,
    intervals: Vec<f64>,
    fmt: String,
    dims: (u32, u32),
    unsupported: HashSet<String>,
}

impl Stats {
    fn report(&self, end: Snap) {
        let s = self.start.unwrap();
        let wall = end.t.unwrap().duration_since(s.t.unwrap()).as_secs_f64();
        let shown = end.shown - s.shown;
        let skipped = end.skipped - s.skipped;
        let cores = (end.cpu - s.cpu) / wall;
        let expected = (self.args.secs * self.nominal_fps).round();
        let mut iv = self.intervals.clone();
        iv.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let pct = |p: f64| if iv.is_empty() { 0. } else { iv[((iv.len() - 1) as f64 * p) as usize] };
        let late = iv.iter().filter(|&&x| x > 1.5 * 1000. / self.nominal_fps).count();
        println!("{{\"label\":\"{}\",\"file\":\"{}\",\"mode\":\"{}\",\"fmt\":\"{}\",\"dims\":\"{}x{}\",\"hw\":{},\"nominal_fps\":{:.3},\"wall_s\":{:.2},\"shown\":{},\"fps\":{:.2},\"expected\":{},\"drops\":{},\"skipped_late\":{},\"not_presented\":{},\"cores\":{:.3},\"iv_p50_ms\":{:.2},\"iv_p99_ms\":{:.2},\"iv_max_ms\":{:.2},\"iv_gt1.5x\":{}}}",
            self.args.label, self.args.file.rsplit('/').next().unwrap(), self.args.mode, self.fmt, self.dims.0, self.dims.1,
            self.info.hw_frames.load(Ordering::Relaxed), self.nominal_fps, wall, shown, shown as f64 / wall, expected, (expected - shown as f64).max(0.), skipped, self.occluded,
            cores, pct(0.5), pct(0.99), pct(1.0), late);
    }
}

// ------------------------------------------------------------------ app

struct App {
    a: Args,
    rx: Receiver<Item>,
    stats: Stats,
    gpu: Option<Gpu>,
    next: Option<Item>,
    started: bool,
    t0: Instant,
    pts0: f64,
    wait_since: Instant,
}

impl App {
    fn pull(&mut self) {
        if self.next.is_none() {
            if let Ok(it) = self.rx.try_recv() {
                self.stats.info.buffered.fetch_sub(1, Ordering::AcqRel);
                self.next = Some(it);
            }
        }
    }

    fn step(&mut self, el: &ActiveEventLoop) {
        let now = Instant::now();
        if !self.started {
            let ok = self.stats.info.buffered.load(Ordering::Acquire) >= self.a.prebuf || self.stats.info.done.load(Ordering::Acquire) || now.duration_since(self.wait_since) > Duration::from_secs(3);
            if !ok { el.set_control_flow(ControlFlow::WaitUntil(now + Duration::from_millis(2))); return; }
            self.pull();
            let Some(n) = &self.next else { el.set_control_flow(ControlFlow::WaitUntil(now + Duration::from_millis(2))); return; };
            self.pts0 = n.pts;
            self.t0 = now;
            self.started = true;
            eprintln!("clock started");
        }
        // collect every frame that is due; only the newest is shown
        let mut cand: Option<Item> = None;
        loop {
            self.pull();
            match &self.next {
                Some(n) if self.t0 + Duration::from_secs_f64((n.pts - self.pts0).max(0.)) <= now => {
                    if cand.is_some() { self.stats.skipped += 1; }
                    cand = self.next.take();
                }
                _ => break,
            }
        }
        if let Some(c) = cand {
            let f = &c.frame;
            match layout_of(f.format()) {
                Some(l) => {
                    if self.stats.fmt.is_empty() { self.stats.fmt = format!("{:?}", f.format()); self.stats.dims = (f.width(), f.height()); eprintln!("first frame {} {}x{}", self.stats.fmt, f.width(), f.height()); }
                    let ok = self.gpu.as_mut().unwrap().show(f, l);
                    if !ok { self.stats.occluded += 1; }
                    let t = Instant::now();
                    self.stats.shown += 1;
                    if let Some(prev) = self.stats.last_shown { if self.stats.start.is_some() { self.stats.intervals.push(t.duration_since(prev).as_secs_f64() * 1000.); } }
                    self.stats.last_shown = Some(t);
                }
                None => { eprintln!("unsupported pixel format {:?}", f.format()); el.exit(); return; }
            }
        }
        let now = Instant::now();
        let el_s = now.duration_since(self.t0).as_secs_f64();
        if self.stats.start.is_none() && el_s >= self.a.warmup {
            self.stats.start = Some(Snap { t: Some(now), cpu: rusage_cpu(), shown: self.stats.shown, skipped: self.stats.skipped });
            self.stats.intervals.clear();
        }
        if self.stats.start.is_some() && el_s >= self.a.warmup + self.a.secs {
            self.stats.report(Snap { t: Some(now), cpu: rusage_cpu(), shown: self.stats.shown, skipped: self.stats.skipped });
            el.exit();
            return;
        }
        if self.stats.info.done.load(Ordering::Acquire) && self.next.is_none() && self.rx.try_recv().is_err() { eprintln!("decoder finished early"); el.exit(); return; }
        let wake = match &self.next { Some(n) => self.t0 + Duration::from_secs_f64((n.pts - self.pts0).max(0.)), None => now + Duration::from_millis(1) };
        el.set_control_flow(ControlFlow::WaitUntil(wake));
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gpu.is_some() { return; }
        let attrs = Window::default_attributes().with_title("video-proto (throwaway)").with_inner_size(winit::dpi::LogicalSize::new(self.a.size.0, self.a.size.1)).with_window_level(winit::window::WindowLevel::AlwaysOnTop);
        let w = Arc::new(el.create_window(attrs).unwrap());
        w.focus_window();
        let mut g = Gpu::new(w);
        g.dump = self.a.dump.clone();
        self.gpu = Some(g);
        self.wait_since = Instant::now();
    }
    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, ev: WindowEvent) {
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => if let Some(g) = &mut self.gpu { g.config.width = s.width.max(1); g.config.height = s.height.max(1); g.surface.configure(&g.device, &g.config); },
            _ => {}
        }
    }
    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if self.gpu.is_some() { self.step(el); }
    }
}

fn nominal_fps(file: &str) -> f64 {
    let i = ff::format::input(&file).unwrap();
    let s = i.streams().best(ff::media::Type::Video).unwrap();
    let r = s.avg_frame_rate();
    r.numerator() as f64 / r.denominator() as f64
}

fn main() {
    let a = parse_args();
    ff::init().unwrap();
    unsafe { ffi::av_log_set_level(ffi::AV_LOG_ERROR as i32); }
    let fps = nominal_fps(&a.file);
    let info = Arc::new(DecInfo::default());
    let (tx, rx) = sync_channel::<Item>(a.queue);
    {
        let (a, info) = (a.clone(), info.clone());
        std::thread::spawn(move || decode_thread(a, tx, info));
    }
    let stats = Stats { args: a.clone(), info: info.clone(), nominal_fps: fps, start: None, shown: 0, skipped: 0, occluded: 0, last_shown: None, intervals: vec![], fmt: String::new(), dims: (0, 0), unsupported: HashSet::new() };

    if a.mode == "decode" {
        // decode-only throughput: no window, no clock
        let (mut n, mut fmt, mut dims) = (0u64, String::new(), (0, 0));
        let t_start = Instant::now();
        let mut snap: Option<(Instant, f64, u64)> = None;
        loop {
            match rx.recv_timeout(Duration::from_secs(5)) {
                Ok(it) => {
                    info.buffered.fetch_sub(1, Ordering::AcqRel);
                    n += 1;
                    if fmt.is_empty() { fmt = format!("{:?}", it.frame.format()); dims = (it.frame.width(), it.frame.height()); }
                }
                Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => { eprintln!("decoder stalled/ended"); break; }
            }
            let e = t_start.elapsed().as_secs_f64();
            if snap.is_none() && e >= a.warmup { snap = Some((Instant::now(), rusage_cpu(), n)); }
            if let Some((t, c, n0)) = snap { if t.elapsed().as_secs_f64() >= a.secs {
                let wall = t.elapsed().as_secs_f64();
                println!("{{\"label\":\"{}\",\"file\":\"{}\",\"mode\":\"decode\",\"fmt\":\"{}\",\"dims\":\"{}x{}\",\"hw\":{},\"nominal_fps\":{:.3},\"wall_s\":{:.2},\"fps\":{:.2},\"cores\":{:.3}}}",
                    a.label, a.file.rsplit('/').next().unwrap(), fmt, dims.0, dims.1, info.hw_frames.load(Ordering::Relaxed), fps, wall, (n - n0) as f64 / wall, (rusage_cpu() - c) / wall);
                break;
            } }
        }
        std::process::exit(0);
    }

    let el = EventLoop::new().unwrap();
    el.set_control_flow(ControlFlow::Wait);
    let mut app = App { a, rx, stats, gpu: None, next: None, started: false, t0: Instant::now(), pts0: 0., wait_since: Instant::now() };
    el.run_app(&mut app).unwrap();
    std::process::exit(0);
}
