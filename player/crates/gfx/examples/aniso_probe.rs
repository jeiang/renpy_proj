//! Raw wgpu probe for the anisotropy investigation (harness/testgames/aniso/README.md, "Why Linux differed").
//!
//! Draws one 72 degree tilted quad (perspective, 2048 px texture shown at about 614 px) with different sampler and
//! view states and prints the mean absolute difference between pairs of pictures. It uses no engine code, so any
//! difference comes from wgpu and the driver. `cargo run --release -p gfx --example aniso_probe -- <out dir>` also
//! writes each picture as a PPM file into `<out dir>`. `harness/testgames/aniso-probe/gl_probe.c` draws the same
//! picture with OpenGL, and `harness/testgames/aniso-probe/compare_ppm.py` compares the two sets of files.

use std::io::Write;
use wgpu::*;

const TEX: u32 = 2048;
const W: u32 = 1280;
const H: u32 = 720;

const SHADER: &str = r#"
struct V { @builtin(position) p: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@builtin(vertex_index) i: u32) -> V {
    // Quad corners (x, y in px about the centre), rotated 72 degrees about X, pinhole camera at distance f.
    var c = array<vec2<f32>, 6>(vec2(-1.,-1.), vec2(1.,-1.), vec2(-1.,1.), vec2(-1.,1.), vec2(1.,-1.), vec2(1.,1.));
    let q = c[i] * 307.;
    let a = radians(72.);
    let y = q.y * cos(a);
    let z = q.y * sin(a);
    let f = 600.;
    var o: V;
    let w = (f + z) / f;
    o.p = vec4(q.x / 640. , -y / 360., 0.5 * w, w);
    o.uv = c[i] * 0.5 + vec2(0.5, 0.5);
    return o;
}
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@fragment fn fs(v: V) -> @location(0) vec4<f32> { return textureSample(t, s, v.uv); }
"#;

/// Fragment shader that does the anisotropic filtering itself: `N` bilinear taps of level 0, evenly spread over the
/// longer axis of the pixel footprint (N from the axis ratio, at most 16). `__N__` is replaced per mode.
const EMU: &str = r#"
@fragment fn fs_emu(v: V) -> @location(0) vec4<f32> {
    let sz = vec2<f32>(2048., 2048.);
    let dx = dpdx(v.uv) * sz;
    let dy = dpdy(v.uv) * sz;
    let lx = length(dx);
    let ly = length(dy);
    var major = dx;
    var major_len = lx;
    var minor_len = ly;
    if (ly > lx) { major = dy; major_len = ly; minor_len = lx; }
    let ratio = min(major_len / max(minor_len, 1e-6), 16.);
    let n = clamp(__N__, 1., 16.);
    var acc = vec4<f32>(0.);
    let nn = i32(n);
    for (var i = 0; i < nn; i = i + 1) {
        let u = (f32(i) + 0.5) / n - 0.5;
        acc = acc + textureSampleLevel(t, s, v.uv + major * u * (min(n, ratio) / ratio) / sz, 0.);
    }
    return acc / n;
}
"#;

fn checker(x: u32, y: u32) -> [u8; 4] {
    if x.is_multiple_of(256) || y.is_multiple_of(256) {
        return [255, 0, 0, 255];
    }
    let v = if (x / 8 + y / 8).is_multiple_of(2) {
        230
    } else {
        25
    };
    [v, v, v, 255]
}

fn grating(x: u32, y: u32) -> [u8; 4] {
    let (qx, qy) = (x % (TEX / 2), y % (TEX / 2));
    let on = match (x < TEX / 2, y < TEX / 2) {
        (true, true) => qx % 4 < 2,
        (false, true) => qy % 4 < 2,
        (true, false) => (qx + qy) % 6 < 3,
        (false, false) => {
            let (dx, dy) = (qx as f32 - 512., qy as f32 - 512.);
            ((dx * dx + dy * dy) * 0.0016).sin() > 0.
        }
    };
    let v = if on { 235 } else { 20 };
    [v, v, v, 255]
}

fn mips(level0: Vec<u8>) -> Vec<(u32, Vec<u8>)> {
    let mut out = vec![(TEX, level0)];
    let mut s = TEX;
    while s > 1 {
        let n = s / 2;
        let prev = &out.last().unwrap().1;
        let mut next = vec![0u8; (n * n * 4) as usize];
        for y in 0..n {
            for x in 0..n {
                for c in 0..4 {
                    let at = |dx: u32, dy: u32| {
                        prev[(((2 * y + dy) * s + 2 * x + dx) * 4 + c) as usize] as u32
                    };
                    next[((y * n + x) * 4 + c) as usize] =
                        ((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) + 2) / 4) as u8;
                }
            }
        }
        out.push((n, next));
        s = n;
    }
    out
}

struct Cfg {
    name: &'static str,
    /// 0 single level texture, 1 all levels, 2 all levels but a view of level 0 only
    tex: u8,
    mip_linear: bool,
    lod_max: f32,
    aniso: u16,
    /// 0: the sampler does the filtering; 1 to 3: the fragment shader takes the anisotropic taps itself (see `EMU`)
    emu: u8,
}

fn main() {
    let dir = std::env::args().nth(1);
    let instance = Instance::new(InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
        power_preference: PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .expect("adapter");
    println!("adapter: {:?}", adapter.get_info());
    let (device, queue) =
        pollster::block_on(adapter.request_device(&DeviceDescriptor::default())).expect("device");

    let cfgs = [
        Cfg {
            name: "tri_a1",
            tex: 1,
            mip_linear: true,
            lod_max: 32.,
            aniso: 1,
            emu: 0,
        },
        Cfg {
            name: "tri_a16",
            tex: 1,
            mip_linear: true,
            lod_max: 32.,
            aniso: 16,
            emu: 0,
        },
        Cfg {
            name: "base_nomip",
            tex: 1,
            mip_linear: false,
            lod_max: 0.,
            aniso: 1,
            emu: 0,
        },
        Cfg {
            name: "base_lin_a1",
            tex: 1,
            mip_linear: true,
            lod_max: 0.,
            aniso: 1,
            emu: 0,
        },
        Cfg {
            name: "base_lin_a16",
            tex: 1,
            mip_linear: true,
            lod_max: 0.,
            aniso: 16,
            emu: 0,
        },
        Cfg {
            name: "single_a1",
            tex: 0,
            mip_linear: true,
            lod_max: 0.,
            aniso: 1,
            emu: 0,
        },
        Cfg {
            name: "single_a16",
            tex: 0,
            mip_linear: true,
            lod_max: 0.,
            aniso: 16,
            emu: 0,
        },
        Cfg {
            name: "view1_a1",
            tex: 2,
            mip_linear: true,
            lod_max: 0.,
            aniso: 1,
            emu: 0,
        },
        Cfg {
            name: "view1_a16",
            tex: 2,
            mip_linear: true,
            lod_max: 0.,
            aniso: 16,
            emu: 0,
        },
        Cfg {
            name: "view1_a16_lod32",
            tex: 2,
            mip_linear: true,
            lod_max: 32.,
            aniso: 16,
            emu: 0,
        },
        Cfg {
            name: "single_a16_lod32",
            tex: 0,
            mip_linear: true,
            lod_max: 32.,
            aniso: 16,
            emu: 0,
        },
        Cfg {
            name: "base_a16_lod0p5",
            tex: 1,
            mip_linear: true,
            lod_max: 0.5,
            aniso: 16,
            emu: 0,
        },
        Cfg {
            name: "base_a16_lod1",
            tex: 1,
            mip_linear: true,
            lod_max: 1.,
            aniso: 16,
            emu: 0,
        },
        Cfg {
            name: "base_a8",
            tex: 1,
            mip_linear: true,
            lod_max: 0.,
            aniso: 8,
            emu: 0,
        },
        Cfg {
            name: "base_a4",
            tex: 1,
            mip_linear: true,
            lod_max: 0.,
            aniso: 4,
            emu: 0,
        },
        Cfg {
            name: "base_a2",
            tex: 1,
            mip_linear: true,
            lod_max: 0.,
            aniso: 2,
            emu: 0,
        },
        Cfg {
            name: "emu_ceil",
            tex: 1,
            mip_linear: false,
            lod_max: 0.,
            aniso: 1,
            emu: 1,
        },
        Cfg {
            name: "emu_round",
            tex: 1,
            mip_linear: false,
            lod_max: 0.,
            aniso: 1,
            emu: 2,
        },
        Cfg {
            name: "emu_floor1",
            tex: 1,
            mip_linear: false,
            lod_max: 0.,
            aniso: 1,
            emu: 3,
        },
    ];

    let module = device.create_shader_module(ShaderModuleDescriptor {
        label: None,
        source: ShaderSource::Wgsl(SHADER.into()),
    });
    let bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let pl = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let make_pipeline = |module: &ShaderModule, fs: &str| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: None,
            layout: Some(&pl),
            vertex: VertexState {
                module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(FragmentState {
                module,
                entry_point: Some(fs),
                compilation_options: Default::default(),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    };
    let mut pipelines = vec![make_pipeline(&module, "fs")];
    for n in ["ceil(ratio)", "round(ratio)", "floor(ratio) + 1."] {
        let src = format!("{SHADER}{}", EMU.replace("__N__", n));
        let m = device.create_shader_module(ShaderModuleDescriptor {
            label: None,
            source: ShaderSource::Wgsl(src.into()),
        });
        pipelines.push(make_pipeline(&m, "fs_emu"));
    }

    let make_tex = |levels: &[(u32, Vec<u8>)], count: u32| {
        let t = device.create_texture(&TextureDescriptor {
            label: None,
            size: Extent3d {
                width: TEX,
                height: TEX,
                depth_or_array_layers: 1,
            },
            mip_level_count: count,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (i, (s, data)) in levels.iter().take(count as usize).enumerate() {
            queue.write_texture(
                TexelCopyTextureInfo {
                    texture: &t,
                    mip_level: i as u32,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                data,
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(s * 4),
                    rows_per_image: Some(*s),
                },
                Extent3d {
                    width: *s,
                    height: *s,
                    depth_or_array_layers: 1,
                },
            );
        }
        t
    };

    for (tname, f) in [
        ("checker", checker as fn(u32, u32) -> [u8; 4]),
        ("grating", grating),
    ] {
        let mut l0 = Vec::with_capacity((TEX * TEX * 4) as usize);
        for y in 0..TEX {
            for x in 0..TEX {
                l0.extend_from_slice(&f(x, y));
            }
        }
        let levels = mips(l0);
        let multi = make_tex(&levels, levels.len() as u32);
        let single = make_tex(&levels, 1);
        let mut pics: Vec<(&str, Vec<u8>)> = Vec::new();
        for c in &cfgs {
            let sampler = device.create_sampler(&SamplerDescriptor {
                address_mode_u: AddressMode::ClampToEdge,
                address_mode_v: AddressMode::ClampToEdge,
                mag_filter: FilterMode::Linear,
                min_filter: FilterMode::Linear,
                mipmap_filter: if c.mip_linear {
                    MipmapFilterMode::Linear
                } else {
                    MipmapFilterMode::Nearest
                },
                lod_min_clamp: 0.,
                lod_max_clamp: c.lod_max,
                anisotropy_clamp: c.aniso,
                ..Default::default()
            });
            let view = match c.tex {
                0 => single.create_view(&Default::default()),
                1 => multi.create_view(&Default::default()),
                _ => multi.create_view(&TextureViewDescriptor {
                    base_mip_level: 0,
                    mip_level_count: Some(1),
                    ..Default::default()
                }),
            };
            let bg = device.create_bind_group(&BindGroupDescriptor {
                label: None,
                layout: &bgl,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::TextureView(&view),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::Sampler(&sampler),
                    },
                ],
            });
            let target = device.create_texture(&TextureDescriptor {
                label: None,
                size: Extent3d {
                    width: W,
                    height: H,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rgba8Unorm,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let tv = target.create_view(&Default::default());
            let bpr = (W * 4).div_ceil(256) * 256;
            let buf = device.create_buffer(&BufferDescriptor {
                label: None,
                size: (bpr * H) as u64,
                usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut enc = device.create_command_encoder(&Default::default());
            {
                let mut pass = enc.begin_render_pass(&RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view: &tv,
                        depth_slice: None,
                        resolve_target: None,
                        ops: Operations {
                            load: LoadOp::Clear(Color {
                                r: 0.44,
                                g: 0.44,
                                b: 0.44,
                                a: 1.,
                            }),
                            store: StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&pipelines[c.emu as usize]);
                pass.set_bind_group(0, &bg, &[]);
                pass.draw(0..6, 0..1);
            }
            enc.copy_texture_to_buffer(
                TexelCopyTextureInfo {
                    texture: &target,
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                TexelCopyBufferInfo {
                    buffer: &buf,
                    layout: TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(bpr),
                        rows_per_image: Some(H),
                    },
                },
                Extent3d {
                    width: W,
                    height: H,
                    depth_or_array_layers: 1,
                },
            );
            queue.submit([enc.finish()]);
            let slice = buf.slice(..);
            slice.map_async(MapMode::Read, |_| {});
            device.poll(PollType::wait_indefinitely()).unwrap();
            let data = slice.get_mapped_range().unwrap();
            let mut rgb = Vec::with_capacity((W * H * 3) as usize);
            for r in 0..H as usize {
                for p in 0..W as usize {
                    let o = r * bpr as usize + p * 4;
                    rgb.extend_from_slice(&data[o..o + 3]);
                }
            }
            drop(data);
            buf.unmap();
            if let Some(d) = &dir {
                std::fs::create_dir_all(d).unwrap();
                let mut f = std::fs::File::create(format!("{d}/{tname}_{}.ppm", c.name)).unwrap();
                write!(f, "P6\n{W} {H}\n255\n").unwrap();
                f.write_all(&rgb).unwrap();
            }
            pics.push((c.name, rgb));
        }
        let diff = |a: &str, b: &str| {
            let pa = &pics.iter().find(|p| p.0 == a).unwrap().1;
            let pb = &pics.iter().find(|p| p.0 == b).unwrap().1;
            let s: u64 = pa.iter().zip(pb).map(|(x, y)| x.abs_diff(*y) as u64).sum();
            s as f64 / pa.len() as f64
        };
        println!("== {tname} (mean abs difference, 0..255, whole {W}x{H} picture)");
        for (a, b) in [
            ("tri_a1", "tri_a16"),
            ("base_nomip", "base_lin_a1"),
            ("base_lin_a1", "base_lin_a16"),
            ("base_nomip", "base_lin_a16"),
            ("single_a1", "single_a16"),
            ("base_lin_a1", "single_a1"),
            ("view1_a1", "view1_a16"),
            ("single_a16", "view1_a16"),
            ("single_a16", "base_lin_a16"),
            ("base_nomip", "single_a16"),
            ("base_lin_a16", "view1_a16_lod32"),
            ("base_lin_a16", "single_a16_lod32"),
            ("view1_a16_lod32", "single_a16_lod32"),
            ("base_lin_a16", "base_a16_lod0p5"),
            ("base_lin_a16", "base_a16_lod1"),
            ("base_lin_a16", "base_a8"),
            ("base_lin_a16", "base_a4"),
            ("base_lin_a16", "base_a2"),
            ("base_lin_a1", "base_a2"),
            ("base_lin_a16", "emu_ceil"),
            ("base_lin_a16", "emu_round"),
            ("base_lin_a16", "emu_floor1"),
            ("base_nomip", "emu_ceil"),
        ] {
            println!("{a:>12} vs {b:<13} {:.3}", diff(a, b));
        }
    }
}
