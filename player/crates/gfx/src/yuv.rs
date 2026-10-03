//! GPU YUV to RGBA conversion for video frames (shader from prototype/video-proto).

use std::sync::Arc;

use media::{Matrix, PlaneLayout, VideoFrame};
use wgpu::*;

use crate::gpu::{COLOR_FORMAT, Shared, TexInner};

const SHADER: &str = include_str!("shader.wgsl");

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Layout {
    semi: bool,
    wide: bool,
    bits: u32,
    msb_aligned: bool,
    csx: u32,
    csy: u32,
    /// Planar RGB (G, B, R planes): no matrix, always full range.
    rgb: bool,
}

fn layout_of(l: PlaneLayout) -> Layout {
    let mk = |semi, wide, bits, msb_aligned, csx, csy| Layout {
        rgb: false,
        semi,
        wide,
        bits,
        msb_aligned,
        csx,
        csy,
    };
    match l {
        PlaneLayout::Nv12 => mk(true, false, 8, false, 1, 1),
        PlaneLayout::P010 => mk(true, true, 10, true, 1, 1),
        PlaneLayout::Yuv420p => mk(false, false, 8, false, 1, 1),
        PlaneLayout::Yuv422p => mk(false, false, 8, false, 1, 0),
        PlaneLayout::Yuv444p => mk(false, false, 8, false, 0, 0),
        PlaneLayout::Yuv420p10 => mk(false, true, 10, false, 1, 1),
        PlaneLayout::Yuv422p10 => mk(false, true, 10, false, 1, 0),
        PlaneLayout::Yuv444p10 => mk(false, true, 10, false, 0, 0),
        PlaneLayout::Gbrp => Layout {
            rgb: true,
            ..mk(false, false, 8, false, 0, 0)
        },
        PlaneLayout::Gbrp10 => Layout {
            rgb: true,
            ..mk(false, true, 10, false, 0, 0)
        },
        PlaneLayout::Gbrp12 => Layout {
            rgb: true,
            ..mk(false, true, 12, false, 0, 0)
        },
    }
}

struct Planes {
    key: (Layout, Vec<(u32, u32)>),
    tex: Vec<Texture>,
    bind: BindGroup,
}

pub struct Yuv {
    pipeline: RenderPipeline,
    bgl: BindGroupLayout,
    sampler: Sampler,
    ubuf: Buffer,
    dummy: TextureView,
    planes: Option<Planes>,
}

impl Yuv {
    pub fn new(sh: &Shared) -> Yuv {
        let dev = &sh.device;
        let module = dev.create_shader_module(ShaderModuleDescriptor {
            label: Some("yuv"),
            source: ShaderSource::Wgsl(SHADER.into()),
        });
        let tex_entry = |b| BindGroupLayoutEntry {
            binding: b,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: true },
                view_dimension: TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let bgl = dev.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                tex_entry(0),
                tex_entry(1),
                tex_entry(2),
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::VERTEX_FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pl = dev.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = dev.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("yuv"),
            layout: Some(&pl),
            vertex: VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = dev.create_sampler(&SamplerDescriptor {
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            ..Default::default()
        });
        let ubuf = dev.create_buffer(&BufferDescriptor {
            label: None,
            size: 64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let dummy = dev
            .create_texture(&TextureDescriptor {
                label: None,
                size: Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::R8Unorm,
                usage: TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&TextureViewDescriptor::default());
        Yuv {
            pipeline,
            bgl,
            sampler,
            ubuf,
            dummy,
            planes: None,
        }
    }

    /// Uploads the planes, converts to RGBA in a render pass, and builds mips. Submits its own command buffer.
    pub fn convert(
        &mut self,
        sh: &Arc<Shared>,
        f: &VideoFrame,
        mipmap: bool,
    ) -> Result<Arc<TexInner>, String> {
        let l = layout_of(f.layout);
        if l.wide && !sh.has_16bit {
            return Err(
                "this GPU cannot sample 16 bit normalized textures, needed for 10 bit video".into(),
            );
        }
        let want = if l.semi { 2 } else { 3 };
        if f.planes.len() != want {
            return Err(format!(
                "expected {want} planes for {:?}, got {}",
                f.layout,
                f.planes.len()
            ));
        }
        let dims: Vec<(u32, u32)> = f.planes.iter().map(|p| (p.width, p.height)).collect();
        let key = (l, dims);
        if self.planes.as_ref().map(|p| &p.key) != Some(&key) {
            let (r, rg) = if l.wide {
                (TextureFormat::R16Unorm, TextureFormat::Rg16Unorm)
            } else {
                (TextureFormat::R8Unorm, TextureFormat::Rg8Unorm)
            };
            let tex: Vec<Texture> = f
                .planes
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    sh.device.create_texture(&TextureDescriptor {
                        label: None,
                        size: Extent3d {
                            width: p.width,
                            height: p.height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: TextureDimension::D2,
                        format: if l.semi && i == 1 { rg } else { r },
                        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                        view_formats: &[],
                    })
                })
                .collect();
            let views: Vec<TextureView> = tex
                .iter()
                .map(|t| t.create_view(&TextureViewDescriptor::default()))
                .collect();
            let v2 = if l.semi { &self.dummy } else { &views[2] };
            let bind = sh.device.create_bind_group(&BindGroupDescriptor {
                label: None,
                layout: &self.bgl,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::TextureView(&views[0]),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::TextureView(&views[1]),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: BindingResource::TextureView(v2),
                    },
                    BindGroupEntry {
                        binding: 3,
                        resource: BindingResource::Sampler(&self.sampler),
                    },
                    BindGroupEntry {
                        binding: 4,
                        resource: self.ubuf.as_entire_binding(),
                    },
                ],
            });
            self.planes = Some(Planes { key, tex, bind });
        }
        let pl = self.planes.as_ref().unwrap();
        for (t, p) in pl.tex.iter().zip(&f.planes) {
            let need = p.stride * (p.height as usize - 1)
                + p.width as usize
                    * (if l.semi && std::ptr::eq(p, &f.planes[1]) {
                        2
                    } else {
                        1
                    })
                    * if l.wide { 2 } else { 1 };
            if p.data.len() < need {
                return Err("video plane data is shorter than its stride and height".into());
            }
            sh.queue.write_texture(
                TexelCopyTextureInfo {
                    texture: t,
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                &p.data,
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(p.stride as u32),
                    rows_per_image: Some(p.height),
                },
                Extent3d {
                    width: p.width,
                    height: p.height,
                    depth_or_array_layers: 1,
                },
            );
        }
        let maxv = ((1u32 << l.bits) - 1) as f32;
        let sh_bits = l.bits - 8;
        // Planar RGB is full range, as FFmpeg's own conversion treats it.
        let full = f.color.full_range || l.rgb;
        let (y_off, y_scale, c_scale) = if full {
            (0.0, 1.0, 1.0)
        } else {
            (
                (16u32 << sh_bits) as f32 / maxv,
                maxv / (219u32 << sh_bits) as f32,
                maxv / (224u32 << sh_bits) as f32,
            )
        };
        let c_off = (128u32 << sh_bits) as f32 / maxv;
        let (kr, kb) = match f.color.matrix {
            Matrix::Bt709 => (0.2126, 0.0722),
            Matrix::Bt2020 => (0.2627, 0.0593),
            Matrix::Bt601 => (0.299, 0.114),
        };
        let sample_scale = if l.msb_aligned {
            65535.0 / 65472.0
        } else if l.wide {
            65535.0 / maxv
        } else {
            1.0
        };
        let u: [f32; 12] = [
            y_off,
            y_scale,
            c_off,
            c_scale,
            kr,
            kb,
            sample_scale,
            if l.semi {
                1.0
            } else if l.rgb {
                2.0
            } else {
                0.0
            },
            -1.0,
            1.0,
            1.0,
            -1.0,
        ];
        sh.queue
            .write_buffer(&self.ubuf, 0, bytemuck::cast_slice(&u));
        let mips = if mipmap {
            crate::gpu::mip_count(f.width, f.height)
        } else {
            1
        };
        let out = sh.new_texture(f.width, f.height, mips)?;
        let mut enc = sh.device.create_command_encoder(&Default::default());
        {
            let mut rp = enc.begin_render_pass(&RenderPassDescriptor {
                label: Some("yuv"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &out.rt_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLACK),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            rp.set_pipeline(&self.pipeline);
            rp.set_bind_group(0, &pl.bind, &[]);
            rp.draw(0..6, 0..1);
        }
        sh.encode_mips(&mut enc, &out);
        sh.queue.submit([enc.finish()]);
        Ok(out)
    }
}
