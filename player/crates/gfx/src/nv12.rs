//! Screen capture to NV12 on the GPU. One render pass reads the captured texture and writes
//! Y and interleaved UV into an `R8Unorm` target laid out as NV12. The readback then moves
//! 1.5 bytes per pixel, and the encoder gets NV12 without a swscale pass.

use wgpu::*;

const SHADER: &str = include_str!("nv12.wgsl");

/// The even size `(w & !1, h & !1)` that an NV12 frame covers. An odd last column or row is cropped.
pub(crate) fn even_size(w: u32, h: u32) -> (u32, u32) {
    (w & !1, h & !1)
}

/// Texel rows of an NV12 frame of even height `h`: `h` Y rows, then `h / 2` UV rows.
pub(crate) fn nv12_rows(h: u32) -> u32 {
    h + h / 2
}

/// Pipeline for one device, and the target for the current size.
pub(crate) struct Nv12 {
    pub(crate) device_id: usize,
    pipeline: RenderPipeline,
    bgl: BindGroupLayout,
    /// `((width, rows), texture)`, rebuilt when the size changes.
    target: Option<((u32, u32), Texture)>,
}

impl Nv12 {
    pub(crate) fn new(device: &Device, device_id: usize) -> Nv12 {
        let module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("nv12"),
            source: ShaderSource::Wgsl(SHADER.into()),
        });
        let bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: false },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let pl = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("nv12"),
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
                    format: TextureFormat::R8Unorm,
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
        Nv12 {
            device_id,
            pipeline,
            bgl,
            target: None,
        }
    }

    /// Records the conversion of `src` into the NV12 target and returns the target. `src` must
    /// have `TEXTURE_BINDING` and be at least 2 x 2. The target is `even_size(src)` wide and
    /// `nv12_rows(even height)` tall.
    pub(crate) fn record(
        &mut self,
        device: &Device,
        enc: &mut CommandEncoder,
        src: &Texture,
    ) -> Texture {
        let (w, h) = even_size(src.width(), src.height());
        let rows = nv12_rows(h);
        if self
            .target
            .as_ref()
            .is_none_or(|(size, _)| *size != (w, rows))
        {
            let tex = device.create_texture(&TextureDescriptor {
                label: Some("nv12 target"),
                size: Extent3d {
                    width: w,
                    height: rows,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::R8Unorm,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            self.target = Some(((w, rows), tex));
        }
        let Some((_, target)) = self.target.as_ref() else {
            unreachable!("target was set above");
        };
        let src_view = src.create_view(&TextureViewDescriptor::default());
        let bind = device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &self.bgl,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(&src_view),
            }],
        });
        let dst_view = target.create_view(&TextureViewDescriptor::default());
        {
            let mut rp = enc.begin_render_pass(&RenderPassDescriptor {
                label: Some("nv12"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &dst_view,
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
            rp.set_bind_group(0, &bind, &[]);
            rp.draw(0..3, 0..1);
        }
        target.clone()
    }
}

#[cfg(all(test, target_vendor = "apple"))]
mod tests {
    use super::*;
    use crate::capture::{CaptureFormat, CapturedFrame, capture, set_frame_sink};
    use crate::gpu::Shared;
    use std::sync::mpsc;
    use std::time::Duration;

    /// Opaque gradient card. Every channel varies, so chroma errors show up.
    fn card(w: u32, h: u32) -> Vec<u8> {
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let r = x * 255 / (w - 1);
                let g = y * 255 / (h - 1);
                let b = (x ^ y) & 255;
                v.extend_from_slice(&[r as u8, g as u8, b as u8, 255]);
            }
        }
        v
    }

    /// Uploads `rgba` to a sampleable texture and captures it in `format` through the real
    /// capture path.
    fn capture_card(
        sh: &Shared,
        rgba: &[u8],
        w: u32,
        h: u32,
        format: CaptureFormat,
    ) -> CapturedFrame {
        let extent = Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };
        let tex = sh.device.create_texture(&TextureDescriptor {
            label: Some("card"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_DST
                | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        sh.queue.write_texture(
            TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            rgba,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            extent,
        );
        let (tx, rx) = mpsc::channel();
        set_frame_sink(
            format,
            Some(Box::new(move |f| {
                let _ = tx.send(f);
            })),
        );
        capture(&sh.device, &sh.queue, 1, &tex, false, 1);
        let frame = rx
            .recv_timeout(Duration::from_secs(20))
            .expect("captured frame");
        set_frame_sink(format, None);
        frame
    }

    /// NV12 from swscale, the encoder's existing RGBA path, as tight rows.
    fn swscale_nv12(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
        let mut conv = encode::Converter::new(ffmpeg_next::format::Pixel::NV12);
        let raw = encode::RawFrame {
            width: w,
            height: h,
            format: encode::PixelFormat::Rgba,
            data: rgba.to_vec(),
            capture_ms: 0,
        };
        let out = conv.convert(&raw).expect("swscale NV12");
        let (w, h) = (w as usize, h as usize);
        let mut tight = Vec::with_capacity(w * h * 3 / 2);
        for (plane, rows) in [(0usize, h), (1, h / 2)] {
            let stride = out.stride(plane);
            for row in 0..rows {
                tight.extend_from_slice(&out.data(plane)[row * stride..row * stride + w]);
            }
        }
        tight
    }

    /// `(max abs diff, mean abs diff, PSNR in dB)` between two equal-length byte planes.
    fn diff(a: &[u8], b: &[u8]) -> (u8, f64, f64) {
        let (mut max, mut sum, mut sq) = (0u8, 0f64, 0f64);
        for (&x, &y) in a.iter().zip(b) {
            let d = x.abs_diff(y);
            max = max.max(d);
            sum += f64::from(d);
            sq += f64::from(d) * f64::from(d);
        }
        let n = a.len() as f64;
        let mse = sq / n;
        let psnr = if mse == 0.0 {
            f64::INFINITY
        } else {
            10.0 * (255.0 * 255.0 / mse).log10()
        };
        (max, sum / n, psnr)
    }

    #[test]
    fn gpu_nv12_matches_swscale() {
        let instance = Instance::new(InstanceDescriptor::new_without_display_handle());
        let sh = Shared::new(instance, None).expect("a GPU adapter");
        for (w, h) in [(320u32, 240u32), (1920, 1080)] {
            let rgba = card(w, h);
            let exact = capture_card(&sh, &rgba, w, h, CaptureFormat::Rgba);
            assert_eq!(
                exact.data, rgba,
                "RGBA capture must be exact for an opaque card"
            );

            let frame = capture_card(&sh, &rgba, w, h, CaptureFormat::Nv12);
            assert_eq!(frame.format, CaptureFormat::Nv12);
            assert_eq!((frame.width, frame.height), (w, h));
            let reference = swscale_nv12(&rgba, w, h);
            let luma = (w * h) as usize;
            assert_eq!(frame.data.len(), reference.len());
            let (ymax, ymean, ypsnr) = diff(&frame.data[..luma], &reference[..luma]);
            let (cmax, cmean, cpsnr) = diff(&frame.data[luma..], &reference[luma..]);
            eprintln!(
                "{w}x{h}: Y max {ymax} mean {ymean:.3} PSNR {ypsnr:.2} dB; \
                 UV max {cmax} mean {cmean:.3} PSNR {cpsnr:.2} dB"
            );
            assert!(
                ymax <= 3 && ymean <= 1.0,
                "luma drift: max {ymax}, mean {ymean}"
            );
            assert!(
                cmax <= 4 && cmean <= 1.5,
                "chroma drift: max {cmax}, mean {cmean}"
            );
        }
    }
}
