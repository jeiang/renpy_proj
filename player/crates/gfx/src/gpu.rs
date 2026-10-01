//! wgpu device, textures, samplers, mip chains, pass recording and submission. No Python types here.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use parking_lot::Mutex;
use wgpu::*;

use crate::program::{PipeKey, ProgramInner, VLayout};

/// Every texture Ren'Py samples or renders to. Never an sRGB format: Ren'Py blends in gamma space.
pub const COLOR_FORMAT: TextureFormat = TextureFormat::Rgba8Unorm;
pub const DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SamplerKey {
    pub wrap_s: u32,
    pub wrap_t: u32,
    pub mag_linear: bool,
    pub min_linear: bool,
    /// 0 none, 1 nearest, 2 linear.
    pub mip: u8,
    pub aniso: u16,
}

const GL_REPEAT: u32 = 0x2901;
const GL_MIRRORED_REPEAT: u32 = 0x8370;
const GL_CLAMP_TO_BORDER: u32 = 0x812D;

fn address_mode(gl: u32) -> AddressMode {
    match gl {
        GL_REPEAT => AddressMode::Repeat,
        GL_MIRRORED_REPEAT => AddressMode::MirrorRepeat,
        GL_CLAMP_TO_BORDER => AddressMode::ClampToEdge,
        _ => AddressMode::ClampToEdge,
    }
}

/// The fullscreen-triangle blit that builds mip chains.
struct MipBlit {
    pipeline: RenderPipeline,
    bgl: BindGroupLayout,
    sampler: Sampler,
}

const BLIT_WGSL: &str = r#"
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
struct V { @builtin(position) p: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@builtin(vertex_index) i: u32) -> V {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var o: V;
    o.p = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    o.uv = vec2<f32>(x, y);
    return o;
}
@fragment fn fs(v: V) -> @location(0) vec4<f32> { return textureSampleLevel(t, s, v.uv, 0.0); }
"#;

/// GPU memory budget in bytes, set when a device opens. 0 means unknown.
static GPU_MEMORY: AtomicU64 = AtomicU64::new(0);

/// The best estimate of the memory the GPU can use, or `None` when the backend gives no figure.
/// Metal: `recommendedMaxWorkingSetSize` of the device.
pub fn gpu_memory_bytes() -> Option<u64> {
    match GPU_MEMORY.load(Ordering::Relaxed) {
        0 => default_gpu_memory(),
        n => Some(n),
    }
}

/// Before a device opens (Ren'Py sizes its image cache before display init): ask the system default Metal device.
#[cfg(target_vendor = "apple")]
fn default_gpu_memory() -> Option<u64> {
    use objc2_metal::{MTLCreateSystemDefaultDevice, MTLDevice};
    static DEFAULT: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    *DEFAULT
        .get_or_init(|| MTLCreateSystemDefaultDevice().map(|d| d.recommendedMaxWorkingSetSize()))
}

#[cfg(not(target_vendor = "apple"))]
fn default_gpu_memory() -> Option<u64> {
    None
}

#[cfg(target_vendor = "apple")]
fn query_gpu_memory(device: &Device) -> Option<u64> {
    use objc2_metal::MTLDevice;
    // SAFETY: the device is not used through the raw handle beyond this read-only query.
    let hal = unsafe { device.as_hal::<wgpu::hal::api::Metal>() }?;
    Some(hal.raw_device().recommendedMaxWorkingSetSize())
}

#[cfg(not(target_vendor = "apple"))]
fn query_gpu_memory(_device: &Device) -> Option<u64> {
    None
}

pub struct Shared {
    pub instance: Instance,
    pub adapter: Adapter,
    pub device: Device,
    pub queue: Queue,
    pub max_texture_size: u32,
    pub uniform_align: u32,
    pub has_16bit: bool,
    pub adapter_name: String,
    pub backend: String,
    samplers: Mutex<HashMap<SamplerKey, Sampler>>,
    mip: MipBlit,
    next_id: AtomicU64,
    pub tex_bytes: AtomicUsize,
    pub tex_count: AtomicUsize,
}

impl Shared {
    /// `surface` is created first so that the adapter is compatible with it.
    pub fn new(
        instance: Instance,
        surface: Option<&Surface<'static>>,
    ) -> Result<Arc<Shared>, String> {
        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: surface,
            apply_limit_buckets: false,
        }))
        .map_err(|e| format!("no suitable GPU adapter: {e}"))?;
        let info = adapter.get_info();
        let lim = adapter.limits();
        let has_16bit = adapter
            .features()
            .contains(Features::TEXTURE_FORMAT_16BIT_NORM);
        let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
            required_features: if has_16bit {
                Features::TEXTURE_FORMAT_16BIT_NORM
            } else {
                Features::empty()
            },
            required_limits: Limits {
                max_texture_dimension_2d: lim.max_texture_dimension_2d,
                ..Limits::default()
            },
            ..Default::default()
        }))
        .map_err(|e| format!("cannot open GPU device: {e}"))?;
        let mip = {
            let module = device.create_shader_module(ShaderModuleDescriptor {
                label: Some("mip blit"),
                source: ShaderSource::Wgsl(BLIT_WGSL.into()),
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
            let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
                label: Some("mip blit"),
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
            let sampler = device.create_sampler(&SamplerDescriptor {
                mag_filter: FilterMode::Linear,
                min_filter: FilterMode::Linear,
                ..Default::default()
            });
            MipBlit {
                pipeline,
                bgl,
                sampler,
            }
        };
        GPU_MEMORY.store(query_gpu_memory(&device).unwrap_or(0), Ordering::Relaxed);
        Ok(Arc::new(Shared {
            instance,
            adapter,
            uniform_align: device.limits().min_uniform_buffer_offset_alignment,
            max_texture_size: lim.max_texture_dimension_2d,
            has_16bit,
            adapter_name: info.name.clone(),
            backend: format!("{:?}", info.backend),
            device,
            queue,
            samplers: Mutex::new(HashMap::new()),
            mip,
            next_id: AtomicU64::new(1),
            tex_bytes: AtomicUsize::new(0),
            tex_count: AtomicUsize::new(0),
        }))
    }

    pub fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    pub fn sampler(&self, k: SamplerKey) -> Sampler {
        let mut m = self.samplers.lock();
        if let Some(s) = m.get(&k) {
            return s.clone();
        }
        let filt = |l: bool| {
            if l {
                FilterMode::Linear
            } else {
                FilterMode::Nearest
            }
        };
        // wgpu allows anisotropy only with all three filters linear. A sampler with no mip levels (`mip == 0`, GL_LINEAR)
        // samples level 0 only (lod_max_clamp 0), so a Linear mipmap filter changes nothing there and lets the clamp
        // through; GL applies anisotropy to such a texture too (Mesa radeonsi and Apple's GL do).
        // This is the closest Vulkan can get on AMD. Mesa radeonsi programs "no mip filter" (Z_FILTER_NONE) for
        // GL_LINEAR, and RADV has no such state (Vulkan has no "none" mipmap mode; wgpu needs Linear for anisotropy).
        // On RX 9070 XT the two give pictures that differ by 1 to 2.5 (mean, 0..255) in strongly aliased scenes.
        // Measured, none of the states below gets closer to radeonsi than this one: maxAnisotropy 8, 4 or 2, lod_max_clamp
        // 0.5 or 1, a one-level view, a one-level texture, a LOD bias. See harness/testgames/aniso/README.md.
        let all_linear = k.mag_linear && k.min_linear;
        let mipf = match k.mip {
            2 => MipmapFilterMode::Linear,
            0 if all_linear && k.aniso > 1 => MipmapFilterMode::Linear,
            _ => MipmapFilterMode::Nearest,
        };
        let aniso = if all_linear && matches!(mipf, MipmapFilterMode::Linear) {
            k.aniso.max(1)
        } else {
            1
        };
        let s = self.device.create_sampler(&SamplerDescriptor {
            address_mode_u: address_mode(k.wrap_s),
            address_mode_v: address_mode(k.wrap_t),
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: filt(k.mag_linear),
            min_filter: filt(k.min_linear),
            mipmap_filter: mipf,
            lod_min_clamp: 0.0,
            lod_max_clamp: if k.mip == 0 { 0.0 } else { 32.0 },
            anisotropy_clamp: aniso,
            ..Default::default()
        });
        m.insert(k, s.clone());
        s
    }

    pub fn new_texture(
        self: &Arc<Self>,
        width: u32,
        height: u32,
        mips: u32,
    ) -> Result<Arc<TexInner>, String> {
        if width == 0
            || height == 0
            || width > self.max_texture_size
            || height > self.max_texture_size
        {
            return Err(format!(
                "cannot allocate a {width}x{height} texture (limit {})",
                self.max_texture_size
            ));
        }
        let tex = self.device.create_texture(&TextureDescriptor {
            label: None,
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: mips,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_DST
                | TextureUsages::COPY_SRC
                | TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = tex.create_view(&TextureViewDescriptor::default());
        let rt_view = tex.create_view(&TextureViewDescriptor {
            mip_level_count: Some(1),
            ..Default::default()
        });
        let bytes = mem_size(width, height, mips);
        self.tex_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.tex_count.fetch_add(1, Ordering::Relaxed);
        Ok(Arc::new(TexInner {
            shared: self.clone(),
            tex,
            view,
            rt_view,
            width,
            height,
            mips,
            id: self.next_id(),
            bytes,
        }))
    }

    /// Appends the blits that fill mip levels 1.. from level 0.
    pub fn encode_mips(&self, enc: &mut CommandEncoder, t: &TexInner) {
        for level in 1..t.mips {
            let src = t.tex.create_view(&TextureViewDescriptor {
                base_mip_level: level - 1,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let dst = t.tex.create_view(&TextureViewDescriptor {
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let bg = self.device.create_bind_group(&BindGroupDescriptor {
                label: None,
                layout: &self.mip.bgl,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::TextureView(&src),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::Sampler(&self.mip.sampler),
                    },
                ],
            });
            let mut rp = enc.begin_render_pass(&RenderPassDescriptor {
                label: Some("mip"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &dst,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::TRANSPARENT),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            rp.set_pipeline(&self.mip.pipeline);
            rp.set_bind_group(0, &bg, &[]);
            rp.draw(0..3, 0..1);
        }
    }
}

pub fn mip_count(w: u32, h: u32) -> u32 {
    32 - w.max(h).max(1).leading_zeros()
}

fn mem_size(w: u32, h: u32, mips: u32) -> usize {
    let mut total = 0usize;
    let (mut w, mut h) = (w as usize, h as usize);
    for _ in 0..mips {
        total += w * h * 4;
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    total
}

pub struct TexInner {
    pub shared: Arc<Shared>,
    pub tex: Texture,
    /// All mip levels, for sampling.
    pub view: TextureView,
    /// Level 0 only, for render attachments.
    pub rt_view: TextureView,
    pub width: u32,
    pub height: u32,
    pub mips: u32,
    pub id: u64,
    bytes: usize,
}

impl Drop for TexInner {
    fn drop(&mut self) {
        self.shared
            .tex_bytes
            .fetch_sub(self.bytes, Ordering::Relaxed);
        self.shared.tex_count.fetch_sub(1, Ordering::Relaxed);
    }
}

impl TexInner {
    /// Uploads tightly or loosely packed RGBA8 rows into level 0.
    pub fn write_level0(&self, rgba: &[u8], pitch: usize, width: u32, height: u32) {
        self.shared.queue.write_texture(
            TexelCopyTextureInfo {
                texture: &self.tex,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            rgba,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pitch as u32),
                rows_per_image: Some(height),
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
}

/// One draw call recorded into a pass.
pub struct DrawRec {
    pub prog: Arc<ProgramInner>,
    pub pipe: Arc<RenderPipeline>,
    /// Vertex arena ranges: position, attributes (both optional), indices.
    pub pos: Option<(u64, u64)>,
    pub attr: Option<(u64, u64)>,
    pub idx: (u64, u64),
    pub index_count: u32,
    pub uniform_off: u32,
    pub textures: Vec<(Arc<TexInner>, SamplerKey)>,
    /// Clear the depth buffer before this draw.
    pub clear_depth: bool,
}

#[derive(Clone)]
pub enum Target {
    Screen,
    Tex(Arc<TexInner>),
}

pub struct PassRec {
    pub target: Target,
    pub width: u32,
    pub height: u32,
    /// x, y, w, h in target pixels, origin top-left.
    pub viewport: [f32; 4],
    pub clear: Option<[f64; 4]>,
    /// 1.0 for screen passes, -1.0 for render-to-texture passes.
    pub flip_y: f32,
    pub draws: Vec<DrawRec>,
}

pub enum Cmd {
    Pass(PassRec),
    Mips(Arc<TexInner>),
}

/// The window surface, or a stand-in texture when running headless.
// One Screen per Renderer, never in a collection: boxing the window variant gains nothing.
#[allow(clippy::large_enum_variant)]
enum Screen {
    Window {
        surface: Surface<'static>,
        config: SurfaceConfiguration,
        frame: Option<SurfaceTexture>,
        spare: Option<Texture>,
    },
    Headless {
        tex: Texture,
    },
}

pub struct Renderer {
    pub sh: Arc<Shared>,
    screen: Screen,
    pub screen_format: TextureFormat,
    pub screen_size: (u32, u32),
    cmds: Vec<Cmd>,
    stack: Vec<PassRec>,
    vertex_arena: Vec<u8>,
    uniform_arena: Vec<u8>,
    vbuf: Option<Buffer>,
    ubuf: Option<Buffer>,
    depth: HashMap<(u32, u32), Texture>,
    bytes_this_frame: usize,
    pub yuv: Option<crate::yuv::Yuv>,
    /// Frames drawn into the spare target because the window gave no texture (covered or minimized).
    pub skipped_frames: u64,
    /// Flips so far (the `seq` of captured frames).
    flips: u64,
}

fn pad4(v: &mut Vec<u8>) {
    while !v.len().is_multiple_of(4) {
        v.push(0);
    }
}

impl Renderer {
    pub fn new_window(window: Arc<winit::window::Window>) -> Result<Renderer, String> {
        let instance = Instance::new(InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| format!("cannot create surface: {e}"))?;
        let sh = Shared::new(instance, Some(&surface))?;
        let caps = surface.get_capabilities(&sh.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| {
                !f.is_srgb() && matches!(f, TextureFormat::Bgra8Unorm | TextureFormat::Rgba8Unorm)
            })
            .or_else(|| caps.formats.iter().copied().find(|f| !f.is_srgb()))
            .unwrap_or(caps.formats[0]);
        let size = window.inner_size();
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            color_space: SurfaceColorSpace::Auto,
            view_formats: vec![],
        };
        surface.configure(&sh.device, &config);
        let screen_size = (config.width, config.height);
        Ok(Renderer::with_screen(
            sh,
            Screen::Window {
                surface,
                config,
                frame: None,
                spare: None,
            },
            format,
            screen_size,
        ))
    }

    pub fn new_headless(width: u32, height: u32) -> Result<Renderer, String> {
        let instance = Instance::new(InstanceDescriptor::new_without_display_handle());
        let sh = Shared::new(instance, None)?;
        let tex = headless_tex(&sh, width, height);
        Ok(Renderer::with_screen(
            sh,
            Screen::Headless { tex },
            COLOR_FORMAT,
            (width, height),
        ))
    }

    fn with_screen(
        sh: Arc<Shared>,
        screen: Screen,
        screen_format: TextureFormat,
        screen_size: (u32, u32),
    ) -> Renderer {
        Renderer {
            sh,
            screen,
            screen_format,
            screen_size,
            cmds: vec![],
            stack: vec![],
            vertex_arena: vec![],
            uniform_arena: vec![],
            vbuf: None,
            ubuf: None,
            depth: HashMap::new(),
            bytes_this_frame: 0,
            yuv: None,
            skipped_frames: 0,
            flips: 0,
        }
    }

    /// True when the screen is an offscreen texture (no window).
    pub fn is_offscreen(&self) -> bool {
        matches!(self.screen, Screen::Headless { .. })
    }

    /// Selects the present mode. `sync` is Fifo (the display paces the frames); otherwise frames show at once.
    pub fn set_vsync(&mut self, sync: bool) {
        if let Screen::Window {
            surface,
            config,
            frame,
            ..
        } = &mut self.screen
        {
            let mode = if sync {
                PresentMode::Fifo
            } else {
                PresentMode::Immediate
            };
            if config.present_mode == mode {
                return;
            }
            *frame = None;
            config.present_mode = mode;
            surface.configure(&self.sh.device, config);
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let (w, h) = (width.max(1), height.max(1));
        self.screen_size = (w, h);
        match &mut self.screen {
            Screen::Window {
                surface,
                config,
                frame,
                ..
            } => {
                *frame = None;
                config.width = w;
                config.height = h;
                surface.configure(&self.sh.device, config);
            }
            Screen::Headless { tex } => *tex = headless_tex(&self.sh, w, h),
        }
    }

    /// Starts recording a pass. Passes nest: a pass started while another is open is finished first.
    pub fn begin_pass(&mut self, target: Target, viewport: [f32; 4], clear: Option<[f64; 4]>) {
        let (width, height, flip_y) = match &target {
            Target::Screen => (self.screen_size.0, self.screen_size.1, 1.0),
            Target::Tex(t) => (t.width, t.height, -1.0),
        };
        self.stack.push(PassRec {
            target,
            width,
            height,
            viewport,
            clear,
            flip_y,
            draws: vec![],
        });
    }

    /// Same as `begin_pass` but the pass renders like the screen (no Y flip). Used for screenshots.
    pub fn begin_screen_like_pass(
        &mut self,
        target: Target,
        viewport: [f32; 4],
        clear: Option<[f64; 4]>,
    ) {
        self.begin_pass(target, viewport, clear);
        self.stack.last_mut().unwrap().flip_y = 1.0;
    }

    pub fn end_pass(&mut self) -> Result<(), String> {
        let p = self.stack.pop().ok_or("end_pass without begin_pass")?;
        self.cmds.push(Cmd::Pass(p));
        Ok(())
    }

    pub fn queue_mips(&mut self, t: &Arc<TexInner>) {
        if t.mips > 1 {
            self.cmds.push(Cmd::Mips(t.clone()));
        }
    }

    pub fn current_target_is_screen(&self) -> bool {
        matches!(self.stack.last().map(|p| &p.target), Some(Target::Screen))
    }

    pub fn current_viewport(&self) -> Option<([f32; 4], u32, u32)> {
        self.stack.last().map(|p| (p.viewport, p.width, p.height))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        prog: &Arc<ProgramInner>,
        vl: &VLayout,
        key: PipeKey,
        pos: &[u8],
        attr: &[u8],
        idx: &[u8],
        mut uniforms: Vec<u8>,
        textures: Vec<(Arc<TexInner>, SamplerKey)>,
        clear_depth: bool,
    ) -> Result<(), String> {
        let pipe = prog.pipeline(&self.sh, &key)?;
        let pass = self.stack.last_mut().ok_or("draw outside of a pass")?;
        let (flip, size, vp) = (
            pass.flip_y,
            [pass.width as f32, pass.height as f32],
            pass.viewport,
        );
        // Translator-owned members of the uniform block.
        uniforms[prog.flip_offset as usize..prog.flip_offset as usize + 4]
            .copy_from_slice(&flip.to_ne_bytes());
        for (i, f) in size.iter().enumerate() {
            let o = prog.size_offset as usize + 4 * i;
            uniforms[o..o + 4].copy_from_slice(&f.to_ne_bytes());
        }
        // u_viewport reads the GL viewport: origin at the bottom left.
        if let Some(pos_) = prog.slots.iter().position(|s| s.name == "u_viewport") {
            let o = prog.slots[pos_].offset as usize;
            let v = [vp[0], size[1] - vp[1] - vp[3], vp[2], vp[3]];
            for (i, f) in v.iter().enumerate() {
                uniforms[o + 4 * i..o + 4 * i + 4].copy_from_slice(&f.to_ne_bytes());
            }
        }
        let va = &mut self.vertex_arena;
        let mut rng = |bytes: &[u8]| -> (u64, u64) {
            pad4(va);
            let s = va.len() as u64;
            va.extend_from_slice(bytes);
            (s, va.len() as u64)
        };
        let pos_r = if vl.has_position {
            Some(rng(pos))
        } else {
            None
        };
        let attr_r = if vl.stride > 0 { Some(rng(attr)) } else { None };
        let idx_r = rng(idx);
        let ua = &mut self.uniform_arena;
        let align = self.sh.uniform_align as usize;
        while !ua.len().is_multiple_of(align) {
            ua.push(0);
        }
        let uoff = ua.len() as u32;
        ua.extend_from_slice(&uniforms);
        self.bytes_this_frame += uniforms.len() + pos.len() + attr.len() + idx.len();
        pass.draws.push(DrawRec {
            prog: prog.clone(),
            pipe,
            pos: pos_r,
            attr: attr_r,
            idx: idx_r,
            index_count: (idx.len() / 4) as u32,
            uniform_off: uoff,
            textures,
            clear_depth,
        });
        Ok(())
    }

    fn depth_view(&mut self, w: u32, h: u32) -> TextureView {
        let dev = &self.sh.device;
        self.depth
            .entry((w, h))
            .or_insert_with(|| {
                dev.create_texture(&TextureDescriptor {
                    label: Some("depth"),
                    size: Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: DEPTH_FORMAT,
                    usage: TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
            })
            .create_view(&TextureViewDescriptor::default())
    }

    fn screen_view(&mut self) -> Result<TextureView, String> {
        match &mut self.screen {
            Screen::Headless { tex } => Ok(tex.create_view(&TextureViewDescriptor::default())),
            Screen::Window {
                surface,
                config,
                frame,
                spare,
            } => {
                if frame.is_none() {
                    for _ in 0..3 {
                        match surface.get_current_texture() {
                            CurrentSurfaceTexture::Success(t)
                            | CurrentSurfaceTexture::Suboptimal(t) => {
                                *frame = Some(t);
                                break;
                            }
                            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                                surface.configure(&self.sh.device, config);
                            }
                            // The window is covered or minimized: draw into a spare target and do not present.
                            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => {
                                break;
                            }
                            other => {
                                return Err(format!(
                                    "cannot acquire the window surface: {other:?}"
                                ));
                            }
                        }
                    }
                }
                if let Some(f) = frame.as_ref() {
                    return Ok(f.texture.create_view(&TextureViewDescriptor::default()));
                }
                self.skipped_frames += 1;
                let (w, h, fmt) = (config.width, config.height, config.format);
                if spare.as_ref().map(|t| (t.width(), t.height())) != Some((w, h)) {
                    *spare = Some(self.sh.device.create_texture(&TextureDescriptor {
                        label: Some("spare screen"),
                        size: Extent3d {
                            width: w,
                            height: h,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: TextureDimension::D2,
                        format: fmt,
                        usage: TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    }));
                }
                Ok(spare
                    .as_ref()
                    .unwrap()
                    .create_view(&TextureViewDescriptor::default()))
            }
        }
    }

    fn grow(dev: &Device, slot: &mut Option<Buffer>, need: u64, usage: BufferUsages) -> Buffer {
        if let Some(b) = slot
            && b.size() >= need
        {
            return b.clone();
        }
        let size = need.max(1 << 20).next_power_of_two();
        let b = dev.create_buffer(&BufferDescriptor {
            label: None,
            size,
            usage: usage | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        *slot = Some(b.clone());
        b
    }

    /// Encodes and submits everything recorded so far.
    pub fn flush(&mut self) -> Result<(), String> {
        while !self.stack.is_empty() {
            self.end_pass()?;
        }
        if self.cmds.is_empty() {
            return Ok(());
        }
        let cmds = std::mem::take(&mut self.cmds);
        let sh = self.sh.clone();
        let vbuf = Self::grow(
            &sh.device,
            &mut self.vbuf,
            self.vertex_arena.len() as u64,
            BufferUsages::VERTEX | BufferUsages::INDEX,
        );
        let ubuf = Self::grow(
            &sh.device,
            &mut self.ubuf,
            self.uniform_arena.len() as u64,
            BufferUsages::UNIFORM,
        );
        if !self.vertex_arena.is_empty() {
            pad4(&mut self.vertex_arena);
            sh.queue.write_buffer(&vbuf, 0, &self.vertex_arena);
        }
        if !self.uniform_arena.is_empty() {
            sh.queue.write_buffer(&ubuf, 0, &self.uniform_arena);
        }
        let mut enc = sh.device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("renpy frame"),
        });
        let mut bg0s: HashMap<u64, BindGroup> = HashMap::new();
        let mut bg1s: HashMap<Vec<(u64, SamplerKey)>, BindGroup> = HashMap::new();
        for cmd in &cmds {
            match cmd {
                Cmd::Mips(t) => sh.encode_mips(&mut enc, t),
                Cmd::Pass(p) => {
                    let view = match &p.target {
                        Target::Screen => self.screen_view()?,
                        Target::Tex(t) => t.rt_view.clone(),
                    };
                    let dview = self.depth_view(p.width, p.height);
                    let mut rp = None;
                    let mut first = true;
                    let empty = p.draws.is_empty();
                    let start = |enc: &mut CommandEncoder,
                                 clear_color: bool,
                                 first: bool|
                     -> RenderPass<'static> {
                        let load = match (&p.clear, clear_color) {
                            (Some(c), true) => LoadOp::Clear(Color {
                                r: c[0],
                                g: c[1],
                                b: c[2],
                                a: c[3],
                            }),
                            _ => LoadOp::Load,
                        };
                        let _ = first;
                        let mut pass = enc
                            .begin_render_pass(&RenderPassDescriptor {
                                label: None,
                                color_attachments: &[Some(RenderPassColorAttachment {
                                    view: &view,
                                    depth_slice: None,
                                    resolve_target: None,
                                    ops: Operations {
                                        load,
                                        store: StoreOp::Store,
                                    },
                                })],
                                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                                    view: &dview,
                                    depth_ops: Some(Operations {
                                        load: LoadOp::Clear(1.0),
                                        store: StoreOp::Discard,
                                    }),
                                    stencil_ops: None,
                                }),
                                timestamp_writes: None,
                                occlusion_query_set: None,
                                multiview_mask: None,
                            })
                            .forget_lifetime();
                        let vp = p.viewport;
                        let w = p.width as f32;
                        let h = p.height as f32;
                        let x = vp[0].clamp(0.0, w);
                        let y = vp[1].clamp(0.0, h);
                        let vw = vp[2].min(w - x).max(0.0);
                        let vh = vp[3].min(h - y).max(0.0);
                        if vw > 0.0 && vh > 0.0 {
                            pass.set_viewport(x, y, vw, vh, 0.0, 1.0);
                        }
                        pass
                    };
                    if empty {
                        // Still clear the target.
                        let _ = start(&mut enc, true, true);
                        continue;
                    }
                    for d in &p.draws {
                        if rp.is_none() {
                            rp = Some(start(&mut enc, first, first));
                            first = false;
                        } else if d.clear_depth {
                            drop(rp.take());
                            rp = Some(start(&mut enc, false, false));
                        }
                        let pass = rp.as_mut().unwrap();
                        let bg0 = bg0s
                            .entry(d.prog.id)
                            .or_insert_with(|| {
                                sh.device.create_bind_group(&BindGroupDescriptor {
                                    label: None,
                                    layout: &d.prog.bgl0(&sh),
                                    entries: &[BindGroupEntry {
                                        binding: 0,
                                        resource: BindingResource::Buffer(BufferBinding {
                                            buffer: &ubuf,
                                            offset: 0,
                                            size: std::num::NonZeroU64::new(
                                                d.prog.block_size as u64,
                                            ),
                                        }),
                                    }],
                                })
                            })
                            .clone();
                        let key: Vec<(u64, SamplerKey)> = std::iter::once((
                            d.prog.id,
                            SamplerKey {
                                wrap_s: 0,
                                wrap_t: 0,
                                mag_linear: false,
                                min_linear: false,
                                mip: 9,
                                aniso: 0,
                            },
                        ))
                        .chain(d.textures.iter().map(|(t, k)| (t.id, *k)))
                        .collect();
                        let bg1 = bg1s
                            .entry(key)
                            .or_insert_with(|| {
                                let samplers: Vec<Sampler> =
                                    d.textures.iter().map(|(_, k)| sh.sampler(*k)).collect();
                                let mut entries = vec![];
                                for (i, (t, _)) in d.textures.iter().enumerate() {
                                    entries.push(BindGroupEntry {
                                        binding: 2 * i as u32,
                                        resource: BindingResource::TextureView(&t.view),
                                    });
                                    entries.push(BindGroupEntry {
                                        binding: 2 * i as u32 + 1,
                                        resource: BindingResource::Sampler(&samplers[i]),
                                    });
                                }
                                sh.device.create_bind_group(&BindGroupDescriptor {
                                    label: None,
                                    layout: &d.prog.bgl1(&sh),
                                    entries: &entries,
                                })
                            })
                            .clone();
                        pass.set_pipeline(&d.pipe);
                        pass.set_bind_group(0, &bg0, &[d.uniform_off]);
                        pass.set_bind_group(1, &bg1, &[]);
                        let mut slot = 0;
                        if let Some((a, b)) = d.pos {
                            pass.set_vertex_buffer(slot, vbuf.slice(a..b));
                            slot += 1;
                        }
                        if let Some((a, b)) = d.attr {
                            pass.set_vertex_buffer(slot, vbuf.slice(a..b));
                        }
                        pass.set_index_buffer(vbuf.slice(d.idx.0..d.idx.1), IndexFormat::Uint32);
                        pass.draw_indexed(0..d.index_count, 0, 0..1);
                    }
                }
            }
        }
        sh.queue.submit([enc.finish()]);
        self.vertex_arena.clear();
        self.uniform_arena.clear();
        self.bytes_this_frame = 0;
        Ok(())
    }

    /// Presents the window frame, if one was drawn.
    pub fn present(&mut self) -> Result<(), String> {
        self.flush()?;
        self.flips += 1;
        let capture = crate::capture::sink_active();
        let (tex, frame) = match &mut self.screen {
            Screen::Window { frame, .. } => {
                let f = frame.take();
                (f.as_ref().filter(|_| capture).map(|f| f.texture.clone()), f)
            }
            Screen::Headless { tex } => (capture.then(|| tex.clone()), None),
        };
        if let Some(t) = &tex {
            self.capture_tex(t);
        }
        if let Some(f) = frame {
            self.sh.queue.present(f);
        }
        // Depth targets are cheap to rebuild and can be resized away.
        if self.depth.len() > 8 {
            self.depth.clear();
        }
        Ok(())
    }

    fn capture_tex(&self, tex: &Texture) {
        crate::capture::capture(
            &self.sh.device,
            &self.sh.queue,
            Arc::as_ptr(&self.sh) as usize,
            tex,
            self.screen_format == TextureFormat::Bgra8Unorm,
            self.flips,
        );
    }

    /// Flushes, then reads level 0 of `t` (or the screen when `None`) as tightly packed RGBA bytes in the target's own
    /// channel order. Returns (width, height, bytes, is_bgra).
    pub fn read_pixels(
        &mut self,
        t: Option<&Arc<TexInner>>,
        rect: Option<[u32; 4]>,
    ) -> Result<(u32, u32, Vec<u8>, bool), String> {
        self.flush()?;
        let (tex, bgra, w, h) = match t {
            Some(t) => (t.tex.clone(), false, t.width, t.height),
            None => {
                let tex = match &self.screen {
                    Screen::Headless { tex } => tex.clone(),
                    Screen::Window { .. } => {
                        return Err("cannot read back the window surface".into());
                    }
                };
                (tex, false, self.screen_size.0, self.screen_size.1)
            }
        };
        let [rx, ry, rw, rh] = rect.unwrap_or([0, 0, w, h]);
        let bpr = (rw * 4).div_ceil(256) * 256;
        let buf = self.sh.device.create_buffer(&BufferDescriptor {
            label: None,
            size: (bpr * rh) as u64,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.sh.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: Origin3d { x: rx, y: ry, z: 0 },
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: &buf,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bpr),
                    rows_per_image: Some(rh),
                },
            },
            Extent3d {
                width: rw,
                height: rh,
                depth_or_array_layers: 1,
            },
        );
        self.sh.queue.submit([enc.finish()]);
        let slice = buf.slice(..);
        slice.map_async(MapMode::Read, |_| {});
        self.sh
            .device
            .poll(PollType::wait_indefinitely())
            .map_err(|e| format!("GPU readback failed: {e:?}"))?;
        let data = slice
            .get_mapped_range()
            .map_err(|e| format!("GPU readback failed: {e:?}"))?;
        let mut out = Vec::with_capacity((rw * rh * 4) as usize);
        for r in 0..rh as usize {
            out.extend_from_slice(&data[r * bpr as usize..r * bpr as usize + rw as usize * 4]);
        }
        drop(data);
        buf.unmap();
        Ok((rw, rh, out, bgra))
    }
}

/// One standard layout: stride in floats, attribute offsets, is the text layout.
pub type StdLayout = (u32, Vec<(&'static str, u32)>, bool);

/// The mesh attribute layouts of `renpy.gl2.gl2mesh` (`TEXTURE_LAYOUT`, `TEXT_LAYOUT`) used to warm pipelines:
/// (stride in floats, attribute offsets, is the text layout).
pub fn standard_layouts() -> Vec<StdLayout> {
    vec![
        (0, vec![], false),
        (2, vec![("a_tex_coord", 0)], false),
        (
            15,
            vec![
                ("a_tex_coord", 0),
                ("a_text_center", 2),
                ("a_text_time", 4),
                ("a_text_min_time", 5),
                ("a_text_max_time", 6),
                ("a_text_index", 7),
                ("a_text_pos_rect", 8),
                ("a_text_ascent", 12),
                ("a_text_descent", 13),
                ("a_text_pseudo_glyph", 14),
            ],
            true,
        ),
    ]
}

fn headless_tex(sh: &Shared, w: u32, h: u32) -> Texture {
    sh.device.create_texture(&TextureDescriptor {
        label: Some("headless screen"),
        size: Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: COLOR_FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::COPY_SRC
            | TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}
