//! Headless wgpu probe for ticket #22: run translated Ren'Py shaders on a real device and check the
//! coordinate-space / mipmap / sampler / readback assumptions in ../README.md.
#[path = "../../translator-probe/src/translate.rs"]
#[allow(dead_code)]
mod translate;

use std::{collections::HashMap, fs, time::Instant};
use wgpu::*;

fn f32s(v: &[f32]) -> Vec<u8> { v.iter().flat_map(|x| x.to_ne_bytes()).collect() }

struct Gpu { dev: Device, q: Queue }

fn uniform_layout(m: &naga::Module) -> (HashMap<String, u32>, u32) {
    for (_, ty) in m.types.iter() {
        if ty.name.as_deref() == Some("RenpyUniforms") {
            if let naga::TypeInner::Struct { members, span } = &ty.inner {
                return (members.iter().map(|x| (x.name.clone().unwrap(), x.offset)).collect(), *span);
            }
        }
    }
    panic!("no RenpyUniforms");
}

fn parse(src: &str, st: naga::ShaderStage) -> naga::Module {
    naga::front::glsl::Frontend::default().parse(&naga::front::glsl::Options::from(st), src).map_err(|e| format!("{:?}", e.errors.iter().map(|x| x.kind.to_string()).collect::<Vec<_>>())).unwrap()
}

struct Built { pipe: RenderPipeline, bgl0: BindGroupLayout, bgl1: BindGroupLayout, offs: HashMap<String, u32>, size: u32 }

fn build(g: &Gpu, p: &translate::Program, fmt: TextureFormat, blend: Option<BlendState>, via_wgsl: bool) -> Result<Built, String> {
    let vm = parse(&p.vs, naga::ShaderStage::Vertex);
    let fm = parse(&p.fs, naga::ShaderStage::Fragment);
    let (offs, size) = uniform_layout(&vm);
    let (vsrc, fsrc) = if via_wgsl {
        let w = |m: &naga::Module| { let i = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all()).validate(m).unwrap(); naga::back::wgsl::write_string(m, &i, naga::back::wgsl::WriterFlags::empty()).unwrap() };
        (ShaderSource::Wgsl(w(&vm).into()), ShaderSource::Wgsl(w(&fm).into()))
    } else { (ShaderSource::Naga(std::borrow::Cow::Owned(vm)), ShaderSource::Naga(std::borrow::Cow::Owned(fm))) };
    let guard = g.dev.push_error_scope(ErrorFilter::Validation);
    let vs = g.dev.create_shader_module(ShaderModuleDescriptor { label: Some("vs"), source: vsrc });
    let fs = g.dev.create_shader_module(ShaderModuleDescriptor { label: Some("fs"), source: fsrc });
    let bgl0 = g.dev.create_bind_group_layout(&BindGroupLayoutDescriptor { label: None, entries: &[BindGroupLayoutEntry { binding: 0, visibility: ShaderStages::VERTEX_FRAGMENT, ty: BindingType::Buffer { ty: BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }] });
    let mut e1 = vec![];
    for i in 0..p.samplers.len() as u32 {
        e1.push(BindGroupLayoutEntry { binding: 2 * i, visibility: ShaderStages::VERTEX_FRAGMENT, ty: BindingType::Texture { sample_type: TextureSampleType::Float { filterable: true }, view_dimension: TextureViewDimension::D2, multisampled: false }, count: None });
        e1.push(BindGroupLayoutEntry { binding: 2 * i + 1, visibility: ShaderStages::VERTEX_FRAGMENT, ty: BindingType::Sampler(SamplerBindingType::Filtering), count: None });
    }
    let bgl1 = g.dev.create_bind_group_layout(&BindGroupLayoutDescriptor { label: None, entries: &e1 });
    let pl = g.dev.create_pipeline_layout(&PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&bgl0), Some(&bgl1)], immediate_size: 0 });
    // a_position: buffer 0 (Float32x2, stride 8, like Mesh2.point_data); everything else interleaved in buffer 1 (Mesh.attribute).
    let mut attrs0 = vec![]; let mut attrs1 = vec![]; let mut off = 0u64;
    for (n, d, loc) in &p.attributes {
        let (fmt_, n_f) = match d.ty.as_str() { "float" => (VertexFormat::Float32, 1), "vec2" => (VertexFormat::Float32x2, 2), "vec3" => (VertexFormat::Float32x3, 3), _ => (VertexFormat::Float32x4, 4) };
        if n == "a_position" { attrs0.push(VertexAttribute { format: VertexFormat::Float32x2, offset: 0, shader_location: *loc }); }
        else { attrs1.push(VertexAttribute { format: fmt_, offset: off, shader_location: *loc }); off += 4 * n_f; }
    }
    let mut bufs = vec![Some(VertexBufferLayout { array_stride: 8, step_mode: VertexStepMode::Vertex, attributes: &attrs0 })];
    if !attrs1.is_empty() { bufs.push(Some(VertexBufferLayout { array_stride: off, step_mode: VertexStepMode::Vertex, attributes: &attrs1 })); }
    let pipe = g.dev.create_render_pipeline(&RenderPipelineDescriptor {
        label: None, layout: Some(&pl),
        vertex: VertexState { module: &vs, entry_point: Some("main"), compilation_options: Default::default(), buffers: &bufs },
        fragment: Some(FragmentState { module: &fs, entry_point: Some("main"), compilation_options: Default::default(), targets: &[Some(ColorTargetState { format: fmt, blend, write_mask: ColorWrites::ALL })] }),
        primitive: PrimitiveState::default(), depth_stencil: None, multisample: MultisampleState::default(), multiview_mask: None, cache: None,
    });
    if let Some(e) = pollster::block_on(guard.pop()) { return Err(e.to_string().lines().next().unwrap_or("").to_string()); }
    Ok(Built { pipe, bgl0, bgl1, offs, size })
}

fn tex(g: &Gpu, w: u32, h: u32, mips: u32, usage: TextureUsages) -> Texture {
    g.dev.create_texture(&TextureDescriptor { label: None, size: Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: mips, sample_count: 1, dimension: TextureDimension::D2, format: TextureFormat::Rgba8Unorm, usage, view_formats: &[] })
}

fn readback(g: &Gpu, t: &Texture, w: u32, h: u32) -> Vec<u8> {
    let bpr = (w * 4 + 255) / 256 * 256;
    let buf = g.dev.create_buffer(&BufferDescriptor { label: None, size: (bpr * h) as u64, usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut enc = g.dev.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(TexelCopyTextureInfo { texture: t, mip_level: 0, origin: Origin3d::ZERO, aspect: TextureAspect::All }, TexelCopyBufferInfo { buffer: &buf, layout: TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(h) } }, Extent3d { width: w, height: h, depth_or_array_layers: 1 });
    g.q.submit([enc.finish()]);
    let sl = buf.slice(..);
    sl.map_async(MapMode::Read, |_| {});
    g.dev.poll(PollType::wait_indefinitely()).unwrap();
    let d = sl.get_mapped_range().unwrap();
    let mut out = vec![];
    for r in 0..h { out.extend_from_slice(&d[(r * bpr) as usize..(r * bpr + w * 4) as usize]); }
    out
}

fn proj(w: f32, h: f32, rtt: bool) -> [f32; 16] {
    // column-major; Ren'Py screen_projection / texture_projection (see matrix.pyx doc strings)
    let sy = if rtt { 2.0 / h } else { -2.0 / h };
    let ty = if rtt { -1.0 } else { 1.0 };
    [2.0 / w, 0., 0., 0.,  0., sy, 0., 0.,  0., 0., 1., 0.,  -1., ty, 0., 1.]
}

fn main() {
    let inst = Instance::new(InstanceDescriptor::new_without_display_handle());
    let ad = pollster::block_on(inst.request_adapter(&RequestAdapterOptions { power_preference: PowerPreference::HighPerformance, force_fallback_adapter: false, compatible_surface: None, apply_limit_buckets: false })).expect("adapter");
    let info = ad.get_info();
    println!("adapter: {} / {:?} / {:?}", info.name, info.backend, info.device_type);
    let lim = ad.limits();
    println!("limits: max_texture_2d={} max_bind_groups={} max_uniform_buffer={} ", lim.max_texture_dimension_2d, lim.max_bind_groups, lim.max_uniform_buffer_binding_size);
    let (dev, q) = pollster::block_on(ad.request_device(&DeviceDescriptor::default())).expect("device");
    let g = Gpu { dev, q };

    // ---- T1: translate + build renpy.texture, draw with sampled texture into screen-like and RTT-like targets.
    let dir = "../battery/generated";
    let vs = fs::read_to_string(format!("{dir}/texture.120.vert")).expect("run ../extract_shaders.py first");
    let fs_ = fs::read_to_string(format!("{dir}/texture.120.frag")).unwrap();
    let prog = translate::translate(&vs, &fs_).unwrap();
    let b = build(&g, &prog, TextureFormat::Rgba8Unorm, Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING), false).expect("build");
    println!("T1 pipeline(renpy.geometry+renpy.texture, naga IR): ok; uniform offsets {:?} size {}", { let mut v: Vec<_> = b.offs.iter().collect(); v.sort(); v }, b.size);

    let n = 8u32;
    let src = tex(&g, n, n, 1, TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST);
    let mut px = vec![];
    for y in 0..n { for x in 0..n { px.extend_from_slice(&[(x * 32) as u8, (y * 32) as u8, 0, 255]); } }
    g.q.write_texture(src.as_image_copy(), &px, TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(n * 4), rows_per_image: Some(n) }, Extent3d { width: n, height: n, depth_or_array_layers: 1 });
    let smp = g.dev.create_sampler(&SamplerDescriptor { mag_filter: FilterMode::Nearest, min_filter: FilterMode::Nearest, mipmap_filter: MipmapFilterMode::Nearest, ..Default::default() });
    let wf = n as f32;
    let pos = f32s(&[0., 0., wf, 0., 0., wf, wf, wf]);
    let attr = f32s(&[0., 0., 1., 0., 0., 1., 1., 1.]);
    let idx: Vec<u8> = [0u32, 1, 2, 2, 1, 3].iter().flat_map(|x| x.to_ne_bytes()).collect();
    let mk = |data: &[u8], u: BufferUsages| { let b = g.dev.create_buffer(&BufferDescriptor { label: None, size: data.len() as u64, usage: u | BufferUsages::COPY_DST, mapped_at_creation: false }); g.q.write_buffer(&b, 0, data); b };
    let (pb, ab, ib) = (mk(&pos, BufferUsages::VERTEX), mk(&attr, BufferUsages::VERTEX), mk(&idx, BufferUsages::INDEX));
    let draw = |b: &Built, target: &Texture, m: [f32; 16], flip: f32, bias: f32, smp: &Sampler, srcview: &TextureView| {
        let mut u = vec![0u8; b.size as usize];
        let put = |u: &mut Vec<u8>, name: &str, v: &[f32]| { let o = b.offs[name] as usize; for (i, x) in v.iter().enumerate() { u[o + 4 * i..o + 4 * i + 4].copy_from_slice(&x.to_ne_bytes()); } };
        put(&mut u, "u_transform", &m); put(&mut u, "renpy_flip_y", &[flip]); put(&mut u, "u_lod_bias", &[bias]); put(&mut u, "renpy_target_size", &[target.width() as f32, target.height() as f32]);
        let ub = mk(&u, BufferUsages::UNIFORM);
        let bg0 = g.dev.create_bind_group(&BindGroupDescriptor { label: None, layout: &b.bgl0, entries: &[BindGroupEntry { binding: 0, resource: ub.as_entire_binding() }] });
        let bg1 = g.dev.create_bind_group(&BindGroupDescriptor { label: None, layout: &b.bgl1, entries: &[BindGroupEntry { binding: 0, resource: BindingResource::TextureView(srcview) }, BindGroupEntry { binding: 1, resource: BindingResource::Sampler(smp) }] });
        let tv = target.create_view(&Default::default());
        let mut enc = g.dev.create_command_encoder(&Default::default());
        { let mut rp = enc.begin_render_pass(&RenderPassDescriptor { label: None, color_attachments: &[Some(RenderPassColorAttachment { view: &tv, depth_slice: None, resolve_target: None, ops: Operations { load: LoadOp::Clear(Color::TRANSPARENT), store: StoreOp::Store } })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
          rp.set_pipeline(&b.pipe); rp.set_bind_group(0, &bg0, &[]); rp.set_bind_group(1, &bg1, &[]);
          rp.set_vertex_buffer(0, pb.slice(..)); rp.set_vertex_buffer(1, ab.slice(..)); rp.set_index_buffer(ib.slice(..), IndexFormat::Uint32); rp.draw_indexed(0..6, 0, 0..1); }
        g.q.submit([enc.finish()]);
    };
    let sv = src.create_view(&Default::default());
    let target = tex(&g, n, n, 1, TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC);
    let check = |label: &str, out: &[u8], flipped: bool| {
        let mut bad = 0;
        for y in 0..n { for x in 0..n { let sy = if flipped { n - 1 - y } else { y }; let o = ((y * n + x) * 4) as usize; if out[o..o + 4] != px[((sy * n + x) * 4) as usize..((sy * n + x) * 4 + 4) as usize] { bad += 1; } } }
        println!("{label}: {} of {} texels differ from {}", bad, n * n, if flipped { "vertically mirrored source" } else { "source" });
    };
    draw(&b, &target, proj(wf, wf, false), 1.0, 0.0, &smp, &sv);
    check("T2 screen pass (screen_projection, flip_y=+1)          ", &readback(&g, &target, n, n), false);
    draw(&b, &target, proj(wf, wf, true), -1.0, 0.0, &smp, &sv);
    check("T3 RTT pass (texture_projection, translator flip_y=-1) ", &readback(&g, &target, n, n), false);
    draw(&b, &target, proj(wf, wf, true), 1.0, 0.0, &smp, &sv);
    check("T3b RTT pass without the flip epilogue (flip_y=+1)     ", &readback(&g, &target, n, n), true);

    // ---- T4: mip chain by render-pass blit, then textureSampleBias with lod bias == mip selection (Ren'Py blur semantics).
    let m = 32u32; let levels = 6u32;
    let mt = tex(&g, m, m, levels, TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_DST | TextureUsages::COPY_SRC);
    let mut mpx = vec![]; let mut seed = 12345u32;
    for _ in 0..m * m { seed = seed.wrapping_mul(1664525).wrapping_add(1013904223); let v = seed.to_ne_bytes(); mpx.extend_from_slice(&[v[3], v[2], v[1], 255]); }
    g.q.write_texture(mt.as_image_copy(), &mpx, TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(m * 4), rows_per_image: Some(m) }, Extent3d { width: m, height: m, depth_or_array_layers: 1 });
    let blit = g.dev.create_shader_module(ShaderModuleDescriptor { label: None, source: ShaderSource::Wgsl(r#"
@group(0) @binding(0) var t: texture_2d<f32>; @group(0) @binding(1) var s: sampler;
struct O { @builtin(position) p: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@builtin(vertex_index) i: u32) -> O { var o: O; let x = f32((i << 1u) & 2u); let y = f32(i & 2u); o.p = vec4(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0); o.uv = vec2(x, y); return o; }
@fragment fn fs(o: O) -> @location(0) vec4<f32> { return textureSample(t, s, o.uv); }"#.into()) });
    let bp = g.dev.create_render_pipeline(&RenderPipelineDescriptor { label: None, layout: None, vertex: VertexState { module: &blit, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] }, fragment: Some(FragmentState { module: &blit, entry_point: Some("fs"), compilation_options: Default::default(), targets: &[Some(TextureFormat::Rgba8Unorm.into())] }), primitive: PrimitiveState::default(), depth_stencil: None, multisample: MultisampleState::default(), multiview_mask: None, cache: None });
    let lin = g.dev.create_sampler(&SamplerDescriptor { mag_filter: FilterMode::Linear, min_filter: FilterMode::Linear, ..Default::default() });
    let t0 = Instant::now();
    let mut enc = g.dev.create_command_encoder(&Default::default());
    for l in 1..levels {
        let sv = mt.create_view(&TextureViewDescriptor { base_mip_level: l - 1, mip_level_count: Some(1), ..Default::default() });
        let dv = mt.create_view(&TextureViewDescriptor { base_mip_level: l, mip_level_count: Some(1), ..Default::default() });
        let bg = g.dev.create_bind_group(&BindGroupDescriptor { label: None, layout: &bp.get_bind_group_layout(0), entries: &[BindGroupEntry { binding: 0, resource: BindingResource::TextureView(&sv) }, BindGroupEntry { binding: 1, resource: BindingResource::Sampler(&lin) }] });
        let mut rp = enc.begin_render_pass(&RenderPassDescriptor { label: None, color_attachments: &[Some(RenderPassColorAttachment { view: &dv, depth_slice: None, resolve_target: None, ops: Operations { load: LoadOp::Clear(Color::TRANSPARENT), store: StoreOp::Store } })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
        rp.set_pipeline(&bp); rp.set_bind_group(0, &bg, &[]); rp.draw(0..3, 0..1);
    }
    g.q.submit([enc.finish()]);
    g.dev.poll(PollType::wait_indefinitely()).unwrap();
    println!("T4 mip chain {m}x{m} -> {levels} levels by blit passes: {:?}", t0.elapsed());
    // level-1 texel 0 vs CPU box average of the 2x2 block
    let l1 = { let bpr = 256u32; let buf = g.dev.create_buffer(&BufferDescriptor { label: None, size: 256 * 16, usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ, mapped_at_creation: false }); let mut e = g.dev.create_command_encoder(&Default::default()); e.copy_texture_to_buffer(TexelCopyTextureInfo { texture: &mt, mip_level: 1, origin: Origin3d::ZERO, aspect: TextureAspect::All }, TexelCopyBufferInfo { buffer: &buf, layout: TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(16) } }, Extent3d { width: 16, height: 16, depth_or_array_layers: 1 }); g.q.submit([e.finish()]); let s = buf.slice(..); s.map_async(MapMode::Read, |_| {}); g.dev.poll(PollType::wait_indefinitely()).unwrap(); let d = s.get_mapped_range().unwrap(); let mut v = vec![]; for r in 0..16 { v.extend_from_slice(&d[(r * 256) as usize..(r * 256 + 64) as usize]); } v };
    let mut maxd = 0i32;
    for y in 0..16usize { for x in 0..16usize { for c in 0..4 { let mut s = 0u32; for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] { s += mpx[(((2 * y + dy) * 32 + 2 * x + dx) * 4 + c) as usize] as u32; } let avg = (s as f32 / 4.0).round() as i32; maxd = maxd.max((avg - l1[(y * 16 + x) * 4 + c] as i32).abs()); } } }
    println!("T4 blit-generated level 1 vs CPU 2x2 box average, max abs diff over 16x16x4: {maxd}");
    // sample with bias 1.0 at 1:1, Nearest min filter, mip Nearest -> must equal level 1 texel (x/2,y/2)
    let mv = mt.create_view(&Default::default());
    let smp2 = g.dev.create_sampler(&SamplerDescriptor { mag_filter: FilterMode::Nearest, min_filter: FilterMode::Nearest, mipmap_filter: MipmapFilterMode::Nearest, ..Default::default() });
    let mtarget = tex(&g, m, m, 1, TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC);
    let mw = m as f32;
    // reuse the 8x8 geometry buffers scaled: build new position buffer
    let pos2 = mk(&f32s(&[0., 0., mw, 0., 0., mw, mw, mw]), BufferUsages::VERTEX);
    let draw2 = |bias: f32| {
        let mut u = vec![0u8; b.size as usize];
        let put = |u: &mut Vec<u8>, name: &str, v: &[f32]| { let o = b.offs[name] as usize; for (i, x) in v.iter().enumerate() { u[o + 4 * i..o + 4 * i + 4].copy_from_slice(&x.to_ne_bytes()); } };
        put(&mut u, "u_transform", &proj(mw, mw, false)); put(&mut u, "renpy_flip_y", &[1.0]); put(&mut u, "u_lod_bias", &[bias]); put(&mut u, "renpy_target_size", &[mw, mw]);
        let ub = mk(&u, BufferUsages::UNIFORM);
        let bg0 = g.dev.create_bind_group(&BindGroupDescriptor { label: None, layout: &b.bgl0, entries: &[BindGroupEntry { binding: 0, resource: ub.as_entire_binding() }] });
        let bg1 = g.dev.create_bind_group(&BindGroupDescriptor { label: None, layout: &b.bgl1, entries: &[BindGroupEntry { binding: 0, resource: BindingResource::TextureView(&mv) }, BindGroupEntry { binding: 1, resource: BindingResource::Sampler(&smp2) }] });
        let tv = mtarget.create_view(&Default::default());
        let mut enc = g.dev.create_command_encoder(&Default::default());
        { let mut rp = enc.begin_render_pass(&RenderPassDescriptor { label: None, color_attachments: &[Some(RenderPassColorAttachment { view: &tv, depth_slice: None, resolve_target: None, ops: Operations { load: LoadOp::Clear(Color::TRANSPARENT), store: StoreOp::Store } })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
          rp.set_pipeline(&b.pipe); rp.set_bind_group(0, &bg0, &[]); rp.set_bind_group(1, &bg1, &[]);
          rp.set_vertex_buffer(0, pos2.slice(..)); rp.set_vertex_buffer(1, ab.slice(..)); rp.set_index_buffer(ib.slice(..), IndexFormat::Uint32); rp.draw_indexed(0..6, 0, 0..1); }
        g.q.submit([enc.finish()]);
        readback(&g, &mtarget, m, m)
    };
    for (bias, lvl) in [(0.0f32, 0u32), (1.0, 1), (2.0, 2), (3.0, 3)] {
        let out = draw2(bias);
        let lv = if lvl == 0 { mpx.clone() } else { let sz = m >> lvl; let bpr = 256u32; let buf = g.dev.create_buffer(&BufferDescriptor { label: None, size: 256 * sz as u64, usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ, mapped_at_creation: false }); let mut e = g.dev.create_command_encoder(&Default::default()); e.copy_texture_to_buffer(TexelCopyTextureInfo { texture: &mt, mip_level: lvl, origin: Origin3d::ZERO, aspect: TextureAspect::All }, TexelCopyBufferInfo { buffer: &buf, layout: TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(sz) } }, Extent3d { width: sz, height: sz, depth_or_array_layers: 1 }); g.q.submit([e.finish()]); let s = buf.slice(..); s.map_async(MapMode::Read, |_| {}); g.dev.poll(PollType::wait_indefinitely()).unwrap(); let d = s.get_mapped_range().unwrap(); let mut v = vec![]; for r in 0..sz { v.extend_from_slice(&d[(r * 256) as usize..(r * 256 + sz * 4) as usize]); } v };
        let sz = m >> lvl; let mut bad = 0;
        for y in 0..m { for x in 0..m { let o = ((y * m + x) * 4) as usize; let lo = (((y >> lvl) * sz + (x >> lvl)) * 4) as usize; if out[o..o + 4] != lv[lo..lo + 4] { bad += 1; } } }
        println!("T5 draw at 1:1 with u_lod_bias={bias}: {bad} of {} pixels differ from mip level {lvl}", m * m);
    }

    // ---- T6: sampler validation / blend validation.
    for (label, d) in [
        ("aniso16 + min/mag Linear + mipmap Nearest (Ren'Py LINEAR_MIPMAP_NEAREST + anisotropy)", SamplerDescriptor { anisotropy_clamp: 16, min_filter: FilterMode::Linear, mag_filter: FilterMode::Linear, mipmap_filter: MipmapFilterMode::Nearest, ..Default::default() }),
        ("aniso16 + min/mag/mipmap Linear", SamplerDescriptor { anisotropy_clamp: 16, min_filter: FilterMode::Linear, mag_filter: FilterMode::Linear, mipmap_filter: MipmapFilterMode::Linear, ..Default::default() }),
        ("lod_max_clamp = 0 (Ren'Py max_mipmap_level=0)", SamplerDescriptor { lod_max_clamp: 0.0, ..Default::default() }),
    ] {
        let guard = g.dev.push_error_scope(ErrorFilter::Validation);
        let _ = g.dev.create_sampler(&d);
        let e = pollster::block_on(guard.pop());
        println!("T6 sampler {label}: {}", e.map(|e| format!("REJECTED: {}", e.to_string().lines().next().unwrap_or(""))).unwrap_or("accepted".into()));
    }
    for (label, bl) in [
        ("GL_MIN (min, One, One)", BlendState { color: BlendComponent { src_factor: BlendFactor::One, dst_factor: BlendFactor::One, operation: BlendOperation::Min }, alpha: BlendComponent { src_factor: BlendFactor::One, dst_factor: BlendFactor::One, operation: BlendOperation::Min } }),
        ("GL_MAX with non-One factors", BlendState { color: BlendComponent { src_factor: BlendFactor::SrcAlpha, dst_factor: BlendFactor::One, operation: BlendOperation::Max }, alpha: BlendComponent::OVER }),
        ("multiply: (DST_COLOR, ONE_MINUS_SRC_ALPHA / ZERO, ONE)", BlendState { color: BlendComponent { src_factor: BlendFactor::Dst, dst_factor: BlendFactor::OneMinusSrcAlpha, operation: BlendOperation::Add }, alpha: BlendComponent { src_factor: BlendFactor::Zero, dst_factor: BlendFactor::One, operation: BlendOperation::Add } }),
    ] {
        match build(&g, &prog, TextureFormat::Rgba8Unorm, Some(bl), false) { Ok(_) => println!("T7 blend {label}: accepted"), Err(e) => println!("T7 blend {label}: REJECTED: {e}") }
    }

    // ---- T8: edge-case shaders through real pipeline creation (naga IR route and WGSL-text route).
    for name in ["e01_mod_negative", "e02_sampler_param", "e03_nonuniform_sampling", "e04_reserved_names", "e05_uniform_types", "e08_vertex_texture", "e12_bias_uniform_branch", "e14_int_loop_matrix", "e15_array_sum"] {
        let v = fs::read_to_string(format!("../battery/edge/{name}.vert")).unwrap();
        let f = fs::read_to_string(format!("../battery/edge/{name}.frag")).unwrap();
        let p = translate::translate(&v, &f).unwrap();
        let r1 = build(&g, &p, TextureFormat::Rgba8Unorm, None, false).map(|_| "ok".to_string());
        let r2 = build(&g, &p, TextureFormat::Rgba8Unorm, None, true).map(|_| "ok".to_string());
        println!("T8 {name:<28} naga-IR: {:<8} WGSL text: {}", r1.unwrap_or_else(|e| format!("ERR {e}")), r2.unwrap_or_else(|e| format!("ERR {e}")));
    }


    // ---- T10: uniform packing of arrays / mat2 / bool through the naga-IR route on the real device.
    {
        let v = fs::read_to_string(&format!("../battery/edge/{}.vert", std::env::var("T10").unwrap_or("e15_array_sum".into()))).unwrap();
        let f = fs::read_to_string(&format!("../battery/edge/{}.frag", std::env::var("T10").unwrap_or("e15_array_sum".into()))).unwrap();
        let p = translate::translate(&v, &f).unwrap();
        let fm = parse(&p.fs, naga::ShaderStage::Fragment);
        // find array strides in the IR
        let mut strides = vec![];
        for (_, ty) in fm.types.iter() { if let naga::TypeInner::Array { stride, size, .. } = &ty.inner { strides.push((format!("{:?}", size), *stride)); } }
        println!("T10 array types (size, IR stride bytes) in the fragment module: {:?}", strides);
        let b2 = build(&g, &p, TextureFormat::Rgba8Unorm, Some(BlendState::REPLACE), false).unwrap();
        println!("T10 offsets {:?} size {}", { let mut v: Vec<_> = b2.offs.iter().collect(); v.sort_by_key(|x| x.1); v }, b2.size);
        let mut u = vec![0u8; b2.size as usize];
        let stride = |n: usize| -> usize { strides.iter().filter(|(s, _)| s.contains(&format!("({n})"))).map(|x| x.1 as usize).max().unwrap() };
        let put = |u: &mut Vec<u8>, at: usize, v: &[f32]| { for (i, x) in v.iter().enumerate() { u[at + 4 * i..at + 4 * i + 4].copy_from_slice(&x.to_ne_bytes()); } };
                let has = |n: &str| b2.offs.contains_key(n);
        if has("u_weights") { let ow = b2.offs["u_weights"] as usize; let sw = stride(5); for (i, x) in [0.1f32, 0.2, 0.3, 0.05, 0.05].iter().enumerate() { put(&mut u, ow + i * sw, &[*x]); } }
        if has("u_colors") { let oc = b2.offs["u_colors"] as usize; let sc = stride(3); for i in 0..3 { put(&mut u, oc + i * sc, &[0.0, 0.25 * (i as f32 + 1.0), 0.0]); } }
        if has("u_m2") { put(&mut u, b2.offs["u_m2"] as usize, &[0.0, 0.5, 0.0, 0.0]); } // mat2 packed as vec4 (xy = column 0, zw = column 1)
        if has("u_flag") { let o = b2.offs["u_flag"] as usize; u[o..o + 4].copy_from_slice(&1i32.to_ne_bytes()); }
        put(&mut u, b2.offs["renpy_flip_y"] as usize, &[1.0]); put(&mut u, b2.offs["renpy_target_size"] as usize, &[4.0, 4.0]);
        put(&mut u, b2.offs["u_transform"] as usize, &proj(4.0, 4.0, false));
        let ub = mk(&u, BufferUsages::UNIFORM);
        let bg0 = g.dev.create_bind_group(&BindGroupDescriptor { label: None, layout: &b2.bgl0, entries: &[BindGroupEntry { binding: 0, resource: ub.as_entire_binding() }] });
        let bg1 = g.dev.create_bind_group(&BindGroupDescriptor { label: None, layout: &b2.bgl1, entries: &(0..p.samplers.len() as u32).flat_map(|i| [BindGroupEntry { binding: 2 * i, resource: BindingResource::TextureView(&sv) }, BindGroupEntry { binding: 2 * i + 1, resource: BindingResource::Sampler(&smp) }]).collect::<Vec<_>>() });
        let t1 = tex(&g, 4, 4, 1, TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC);
        let tv = t1.create_view(&Default::default());
        let quad = mk(&f32s(&[0., 0., 4., 0., 0., 4., 4., 4.]), BufferUsages::VERTEX);
        let mut enc = g.dev.create_command_encoder(&Default::default());
        { let mut rp = enc.begin_render_pass(&RenderPassDescriptor { label: None, color_attachments: &[Some(RenderPassColorAttachment { view: &tv, depth_slice: None, resolve_target: None, ops: Operations { load: LoadOp::Clear(Color { r: 0.0, g: 0.0, b: 1.0, a: 1.0 }), store: StoreOp::Store } })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
          rp.set_pipeline(&b2.pipe); rp.set_bind_group(0, &bg0, &[]); rp.set_bind_group(1, &bg1, &[]); rp.set_vertex_buffer(0, quad.slice(..)); if p.attributes.len() > 1 { rp.set_vertex_buffer(1, ab.slice(..)); } rp.set_index_buffer(ib.slice(..), IndexFormat::Uint32); rp.draw_indexed(0..6, 0, 0..1); }
        g.q.submit([enc.finish()]);
        let px = { let all = readback(&g, &t1, 4, 4); println!("T10 all px {:?}", all); all[..4].to_vec() };
        println!("T10 rendered RGBA8 = {:?}  (expected r=0.7*255=178.5, g=0.75*255=191, b=0.5*255=127.5, a=255)", px);
    }


    // ---- T11: cold-compile cost per shader combination: translate -> naga -> wgpu pipeline (Metal), all Ren'Py built-in combos.
    {
        let mut names: Vec<String> = fs::read_dir("../battery/generated").unwrap().filter_map(|e| e.ok()?.file_name().to_string_lossy().strip_suffix(".120.vert").map(|s| s.to_string())).collect();
        names.sort();
        let mut ms = vec![]; let mut tr = vec![]; let mut fails = 0;
        for n in &names {
            let v = fs::read_to_string(format!("../battery/generated/{n}.120.vert")).unwrap();
            let f = fs::read_to_string(format!("../battery/generated/{n}.120.frag")).unwrap();
            let t0 = Instant::now();
            let p = translate::translate(&v, &f).unwrap();
            tr.push(t0.elapsed().as_secs_f64() * 1e3);
            match build(&g, &p, TextureFormat::Rgba8Unorm, Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING), false) { Ok(_) => ms.push(t0.elapsed().as_secs_f64() * 1e3), Err(e) => { fails += 1; println!("T11 {n}: {e}"); } }
        }
        ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!("T11 {} built-in combos, {} pipeline failures; translate-only mean {:.2} ms; translate+naga+pipeline: min {:.1} / median {:.1} / max {:.1} ms", names.len(), fails, tr.iter().sum::<f64>() / tr.len() as f64, ms[0], ms[ms.len() / 2], ms[ms.len() - 1]);
    }

    // ---- T9: headless frame readback throughput at 1080p (RTT target -> buffer -> map), Rgba8, ~8 MB per frame.
    let (w9, h9) = (1920u32, 1080u32);
    let t9 = tex(&g, w9, h9, 1, TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC);
    let bpr = w9 * 4; // 7680 = 30 * 256
    let rb = g.dev.create_buffer(&BufferDescriptor { label: None, size: (bpr * h9) as u64, usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ, mapped_at_creation: false });
    let frames = 120;
    let t0 = Instant::now();
    let mut copy_ms = 0.0;
    for _ in 0..frames {
        let mut enc = g.dev.create_command_encoder(&Default::default());
        { let tv = t9.create_view(&Default::default()); let _rp = enc.begin_render_pass(&RenderPassDescriptor { label: None, color_attachments: &[Some(RenderPassColorAttachment { view: &tv, depth_slice: None, resolve_target: None, ops: Operations { load: LoadOp::Clear(Color { r: 0.2, g: 0.4, b: 0.6, a: 1.0 }), store: StoreOp::Store } })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None }); }
        enc.copy_texture_to_buffer(TexelCopyTextureInfo { texture: &t9, mip_level: 0, origin: Origin3d::ZERO, aspect: TextureAspect::All }, TexelCopyBufferInfo { buffer: &rb, layout: TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(h9) } }, Extent3d { width: w9, height: h9, depth_or_array_layers: 1 });
        g.q.submit([enc.finish()]);
        let s = rb.slice(..); s.map_async(MapMode::Read, |_| {});
        g.dev.poll(PollType::wait_indefinitely()).unwrap();
        let c0 = Instant::now();
        { let d = s.get_mapped_range().unwrap(); let mut acc = 0u8; for i in (0..d.len()).step_by(4096) { acc ^= d[i]; } std::hint::black_box(acc); }
        copy_ms += c0.elapsed().as_secs_f64() * 1e3;
        rb.unmap();
    }
    let el = t0.elapsed().as_secs_f64();
    println!("T9 1080p clear+copy_texture_to_buffer+map, {frames} frames: {:.2} ms/frame ({:.0} fps) sequential, no pipelining", el * 1e3 / frames as f64, frames as f64 / el);
    let _ = copy_ms;
}
