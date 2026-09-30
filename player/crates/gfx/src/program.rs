//! A translated shader program: naga modules, the std140 uniform layout, and the render pipelines built from it.

use std::collections::HashMap;
use std::num::NonZeroU64;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;
use wgpu::*;

use crate::gpu::Shared;
use crate::translate;

static NEXT_PROGRAM_ID: AtomicU64 = AtomicU64::new(1);

/// Where one uniform lives in the std140 block and how many values it takes.
#[derive(Clone, Debug)]
pub struct UniformSlot {
    pub name: String,
    pub ty: String,
    pub array: Option<u32>,
    pub offset: u32,
    /// Number of values in one element (matrices count all their floats).
    pub comps: usize,
}

/// The vertex-buffer layout of one draw, resolved against a program's attributes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VLayout {
    /// Floats per position (2 for `Mesh2`, 3 for `Mesh3`).
    pub point_size: u32,
    /// Floats per vertex in the interleaved attribute buffer (0 when the program needs none).
    pub stride: u32,
    /// (shader location, float count, offset in floats).
    pub attrs: Vec<(u32, u32, u32)>,
    pub has_position: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PipeKey {
    pub format: TextureFormat,
    pub blend: Option<[i32; 6]>,
    pub mask: u8,
    /// 0 none, 1 Ren'Py "cw", 2 Ren'Py "ccw".
    pub cull: u8,
    pub depth: bool,
    pub vlayout: VLayout,
}

struct GpuObjects {
    vs: ShaderModule,
    fs: ShaderModule,
    pub bgl0: BindGroupLayout,
    pub bgl1: BindGroupLayout,
    layout: PipelineLayout,
}

pub struct ProgramInner {
    pub id: u64,
    pub name: String,
    pub tr: translate::Program,
    vm: naga::Module,
    fm: naga::Module,
    pub slots: Vec<UniformSlot>,
    pub flip_offset: u32,
    pub size_offset: u32,
    pub block_size: u32,
    objs: Mutex<Option<Arc<GpuObjects>>>,
    pipelines: Mutex<HashMap<PipeKey, Arc<RenderPipeline>>>,
    /// Held while a pipeline builds, so a draw that needs a pipeline the warm-up thread is building waits for it.
    build_lock: Mutex<()>,
}

fn comps_of(ty: &str) -> Option<usize> {
    Some(match ty {
        "float" | "int" | "bool" => 1,
        "vec2" | "ivec2" | "bvec2" => 2,
        "vec3" | "ivec3" | "bvec3" => 3,
        "vec4" | "ivec4" | "bvec4" | "mat2" => 4,
        "mat3" => 9,
        "mat4" => 16,
        _ => return None,
    })
}

fn parse(src: &str, stage: naga::ShaderStage) -> Result<naga::Module, String> {
    naga::front::glsl::Frontend::default()
        .parse(&naga::front::glsl::Options::from(stage), src)
        .map_err(|e| {
            e.errors
                .iter()
                .map(|x| x.kind.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        })
}

fn validate(m: &naga::Module) -> Result<(), String> {
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(m)
    .map(|_| ())
    .map_err(|e| format!("validate: {}", e.as_inner()))
}

fn uniform_block(m: &naga::Module) -> Result<(HashMap<String, u32>, u32), String> {
    for (_, ty) in m.types.iter() {
        if ty.name.as_deref() == Some("RenpyUniforms")
            && let naga::TypeInner::Struct { members, span } = &ty.inner {
                return Ok((
                    members
                        .iter()
                        .filter_map(|x| Some((x.name.clone()?, x.offset)))
                        .collect(),
                    *span,
                ));
            }
    }
    Err("internal error: no RenpyUniforms block".into())
}

impl ProgramInner {
    /// Translates and validates a program. Fails the way `glCompileShader` / `glLinkProgram` would.
    pub fn compile(name: &str, vertex: &str, fragment: &str) -> Result<ProgramInner, String> {
        let tr = translate::translate(vertex, fragment)?;
        let vm =
            parse(&tr.vs, naga::ShaderStage::Vertex).map_err(|e| format!("vertex shader: {e}"))?;
        let fm = parse(&tr.fs, naga::ShaderStage::Fragment)
            .map_err(|e| format!("fragment shader: {e}"))?;
        validate(&vm).map_err(|e| format!("vertex shader: {e}"))?;
        validate(&fm).map_err(|e| format!("fragment shader: {e}"))?;
        let (offs, span) = uniform_block(&vm)?;
        let mut slots = vec![];
        for d in &tr.uniforms {
            let comps = comps_of(&d.ty)
                .ok_or_else(|| format!("unsupported uniform type {} for {}", d.ty, d.name))?;
            let offset = *offs
                .get(&d.name)
                .ok_or_else(|| format!("uniform {} missing from block", d.name))?;
            slots.push(UniformSlot {
                name: d.name.clone(),
                ty: d.ty.clone(),
                array: d.array,
                offset,
                comps,
            });
        }
        let flip_offset = *offs.get("renpy_flip_y").ok_or("no renpy_flip_y")?;
        let size_offset = *offs
            .get("renpy_target_size")
            .ok_or("no renpy_target_size")?;
        Ok(ProgramInner {
            id: NEXT_PROGRAM_ID.fetch_add(1, Ordering::Relaxed),
            name: name.to_string(),
            tr,
            vm,
            fm,
            slots,
            flip_offset,
            size_offset,
            block_size: span.max(16),
            objs: Mutex::new(None),
            pipelines: Mutex::new(HashMap::new()),
            build_lock: Mutex::new(()),
        })
    }

    /// Resolves the program's attributes against a mesh layout. `offsets` maps attribute names to float offsets.
    pub fn resolve(
        &self,
        point_size: u32,
        stride: u32,
        offsets: &HashMap<String, u32>,
    ) -> Result<VLayout, String> {
        let mut attrs = vec![];
        let mut has_position = false;
        for (n, d, loc) in &self.tr.attributes {
            let count = match d.ty.as_str() {
                "float" => 1,
                "vec2" => 2,
                "vec3" => 3,
                _ => 4,
            } * d.array.unwrap_or(1);
            if n == "a_position" {
                has_position = true;
                continue;
            }
            match offsets.get(n) {
                Some(o) => attrs.push((*loc, count, *o)),
                None => {
                    return Err(format!(
                        "Shader {} requires attribute {}, but it is not in the mesh.",
                        self.name, n
                    ));
                }
            }
        }
        let stride = if attrs.is_empty() { 0 } else { stride };
        Ok(VLayout {
            point_size,
            stride,
            attrs,
            has_position,
        })
    }

    /// The location of `a_position`, if the program reads it.
    fn position_location(&self) -> Option<u32> {
        self.tr
            .attributes
            .iter()
            .find(|(n, _, _)| n == "a_position")
            .map(|(_, _, l)| *l)
    }

    fn objects(&self, sh: &Shared) -> Arc<GpuObjects> {
        let mut g = self.objs.lock();
        if let Some(o) = g.as_ref() {
            return o.clone();
        }
        let dev = &sh.device;
        let vs = dev.create_shader_module(ShaderModuleDescriptor {
            label: Some("renpy vs"),
            source: ShaderSource::Naga(std::borrow::Cow::Owned(self.vm.clone())),
        });
        let fs = dev.create_shader_module(ShaderModuleDescriptor {
            label: Some("renpy fs"),
            source: ShaderSource::Naga(std::borrow::Cow::Owned(self.fm.clone())),
        });
        let bgl0 = dev.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX_FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(self.block_size as u64),
                },
                count: None,
            }],
        });
        let mut e1 = vec![];
        for i in 0..self.tr.samplers.len() as u32 {
            e1.push(BindGroupLayoutEntry {
                binding: 2 * i,
                visibility: ShaderStages::VERTEX_FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
            e1.push(BindGroupLayoutEntry {
                binding: 2 * i + 1,
                visibility: ShaderStages::VERTEX_FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            });
        }
        let bgl1 = dev.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &e1,
        });
        let layout = dev.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bgl0), Some(&bgl1)],
            immediate_size: 0,
        });
        let o = Arc::new(GpuObjects {
            vs,
            fs,
            bgl0,
            bgl1,
            layout,
        });
        *g = Some(o.clone());
        o
    }

    pub fn bgl0(&self, sh: &Shared) -> BindGroupLayout {
        self.objects(sh).bgl0.clone()
    }

    pub fn bgl1(&self, sh: &Shared) -> BindGroupLayout {
        self.objects(sh).bgl1.clone()
    }

    /// Gets or builds the pipeline for `key`.
    pub fn pipeline(&self, sh: &Shared, key: &PipeKey) -> Result<Arc<RenderPipeline>, String> {
        if let Some(p) = self.pipelines.lock().get(key) {
            return Ok(p.clone());
        }
        let _building = self.build_lock.lock();
        if let Some(p) = self.pipelines.lock().get(key) {
            return Ok(p.clone());
        }
        let o = self.objects(sh);
        let v = &key.vlayout;
        let pos_fmt = match v.point_size {
            2 => VertexFormat::Float32x2,
            3 => VertexFormat::Float32x3,
            n => return Err(format!("unsupported mesh point size {n}")),
        };
        let pos_attr;
        let mut attrs1 = vec![];
        for (loc, count, off) in &v.attrs {
            let f = match count {
                1 => VertexFormat::Float32,
                2 => VertexFormat::Float32x2,
                3 => VertexFormat::Float32x3,
                4 => VertexFormat::Float32x4,
                n => return Err(format!("unsupported attribute width {n}")),
            };
            attrs1.push(VertexAttribute {
                format: f,
                offset: *off as u64 * 4,
                shader_location: *loc,
            });
        }
        let mut bufs = vec![];
        if v.has_position {
            pos_attr = [VertexAttribute {
                format: pos_fmt,
                offset: 0,
                shader_location: self.position_location().unwrap_or(0),
            }];
            bufs.push(Some(VertexBufferLayout {
                array_stride: v.point_size as u64 * 4,
                step_mode: VertexStepMode::Vertex,
                attributes: &pos_attr,
            }));
        }
        if !attrs1.is_empty() {
            bufs.push(Some(VertexBufferLayout {
                array_stride: v.stride as u64 * 4,
                step_mode: VertexStepMode::Vertex,
                attributes: &attrs1,
            }));
        }
        let blend = match key.blend {
            None => Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            Some(b) => Some(blend_state(&b)?),
        };
        let mut mask = ColorWrites::empty();
        if key.mask & 1 != 0 {
            mask |= ColorWrites::RED;
        }
        if key.mask & 2 != 0 {
            mask |= ColorWrites::GREEN;
        }
        if key.mask & 4 != 0 {
            mask |= ColorWrites::BLUE;
        }
        if key.mask & 8 != 0 {
            mask |= ColorWrites::ALPHA;
        }
        let (cull_mode, front_face) = match key.cull {
            1 => (Some(Face::Back), FrontFace::Ccw),
            2 => (Some(Face::Back), FrontFace::Cw),
            _ => (None, FrontFace::Ccw),
        };
        let depth_stencil = Some(DepthStencilState {
            format: crate::gpu::DEPTH_FORMAT,
            depth_write_enabled: Some(key.depth),
            depth_compare: Some(if key.depth {
                CompareFunction::LessEqual
            } else {
                CompareFunction::Always
            }),
            stencil: StencilState::default(),
            bias: DepthBiasState::default(),
        });
        let guard = sh.device.push_error_scope(ErrorFilter::Validation);
        let pipe = sh.device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some(&self.name),
            layout: Some(&o.layout),
            vertex: VertexState {
                module: &o.vs,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                buffers: &bufs,
            },
            fragment: Some(FragmentState {
                module: &o.fs,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                targets: &[Some(ColorTargetState {
                    format: key.format,
                    blend,
                    write_mask: mask,
                })],
            }),
            primitive: PrimitiveState {
                cull_mode,
                front_face,
                ..Default::default()
            },
            depth_stencil,
            multisample: MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        if let Some(e) = pollster::block_on(guard.pop()) {
            return Err(format!("pipeline for {}: {e}", self.name));
        }
        let pipe = Arc::new(pipe);
        self.pipelines.lock().insert(key.clone(), pipe.clone());
        Ok(pipe)
    }

    /// Packs the flattened values of every uniform into `out` (`block_size` bytes). `values[i]` belongs to `slots[i]`.
    pub fn pack(&self, values: &[Vec<f64>], out: &mut [u8]) -> Result<(), String> {
        for (slot, v) in self.slots.iter().zip(values) {
            pack_slot(slot, v, out)
                .map_err(|e| format!("uniform {} in shader {}: {e}", slot.name, self.name))?;
        }
        Ok(())
    }
}

fn put_f(out: &mut [u8], off: usize, x: f64) {
    out[off..off + 4].copy_from_slice(&(x as f32).to_ne_bytes());
}

fn put_i(out: &mut [u8], off: usize, x: f64) {
    out[off..off + 4].copy_from_slice(&(x as i32).to_ne_bytes());
}

fn pack_slot(slot: &UniformSlot, v: &[f64], out: &mut [u8]) -> Result<(), String> {
    let is_int = slot.ty.starts_with("int")
        || slot.ty.starts_with("ivec")
        || slot.ty.starts_with("bool")
        || slot.ty.starts_with("bvec");
    let put = |out: &mut [u8], off: usize, x: f64| {
        if is_int {
            put_i(out, off, x)
        } else {
            put_f(out, off, x)
        }
    };
    let base = slot.offset as usize;
    let n = slot.array.map(|a| a as usize);
    let elems = n.unwrap_or(1);
    let need = elems * slot.comps;
    if v.len() != need {
        return Err(format!("expected {need} values, got {}", v.len()));
    }
    if n.is_some() && matches!(slot.ty.as_str(), "mat2" | "mat3" | "mat4") {
        return Err("arrays of matrices are not supported".into());
    }
    for e in 0..elems {
        let vals = &v[e * slot.comps..(e + 1) * slot.comps];
        // Array elements each take one 16 byte slot (the translator widens them to vec4).
        let eb = base + if n.is_some() { e * 16 } else { 0 };
        match slot.ty.as_str() {
            "mat3" => {
                for c in 0..3 {
                    for r in 0..3 {
                        put(out, eb + c * 16 + r * 4, vals[c * 3 + r]);
                    }
                }
            }
            _ => {
                for (i, x) in vals.iter().enumerate() {
                    put(out, eb + i * 4, *x);
                }
            }
        }
    }
    Ok(())
}

/// GL blend enums to a wgpu blend state. `MIN` and `MAX` ignore their factors in GL, wgpu requires `One`.
pub fn blend_state(b: &[i32; 6]) -> Result<BlendState, String> {
    fn op(e: i32) -> Result<BlendOperation, String> {
        Ok(match e {
            0x8006 => BlendOperation::Add,
            0x800A => BlendOperation::Subtract,
            0x800B => BlendOperation::ReverseSubtract,
            0x8007 => BlendOperation::Min,
            0x8008 => BlendOperation::Max,
            _ => return Err(format!("unsupported blend equation 0x{e:x}")),
        })
    }
    fn factor(e: i32) -> Result<BlendFactor, String> {
        Ok(match e {
            0 => BlendFactor::Zero,
            1 => BlendFactor::One,
            0x300 => BlendFactor::Src,
            0x301 => BlendFactor::OneMinusSrc,
            0x302 => BlendFactor::SrcAlpha,
            0x303 => BlendFactor::OneMinusSrcAlpha,
            0x304 => BlendFactor::DstAlpha,
            0x305 => BlendFactor::OneMinusDstAlpha,
            0x306 => BlendFactor::Dst,
            0x307 => BlendFactor::OneMinusDst,
            0x308 => BlendFactor::SrcAlphaSaturated,
            _ => return Err(format!("unsupported blend factor 0x{e:x}")),
        })
    }
    let comp = |eq: i32, s: i32, d: i32| -> Result<BlendComponent, String> {
        let operation = op(eq)?;
        if matches!(operation, BlendOperation::Min | BlendOperation::Max) {
            Ok(BlendComponent {
                src_factor: BlendFactor::One,
                dst_factor: BlendFactor::One,
                operation,
            })
        } else {
            Ok(BlendComponent {
                src_factor: factor(s)?,
                dst_factor: factor(d)?,
                operation,
            })
        }
    };
    Ok(BlendState {
        color: comp(b[0], b[1], b[2])?,
        alpha: comp(b[3], b[4], b[5])?,
    })
}
