# cython: language_level=3
# Additive module for the wgpu renderer (player/crates/gfx). It does the parts of the GL2 drawing code that need
# `cdef` access to Ren'Py's Cython classes: the Render tree walk (a port of GL2DrawingContext in gl2draw.pyx), the
# uniform getters (gl2uniform.pyx), and raw access to Mesh buffers. The GPU work stays in Rust: every model becomes
# one call to `Gpu.draw`.
#
# Derived from Ren'Py (MIT licence), gl2draw.pyx, gl2uniform.pyx, gl2shader.pyx. Copyright 2004-2026 Tom Rothamel.

from libc.math cimport roundf, hypot
from cpython.bytes cimport PyBytes_FromStringAndSize

from renpy.display.render cimport Render, MATRIX_PROJECTION, MATRIX_VIEW, MATRIX_MODEL
from renpy.display.matrix cimport Matrix
from renpy.gl2.gl2mesh cimport Mesh
from renpy.gl2.gl2polygon cimport Polygon
from renpy.gl2.gl2model cimport GL2Model

import random

import renpy
import renpy.display.matrix
import renpy.display.render

cdef Matrix IDENTITY = renpy.display.matrix.IDENTITY

# Chosen to be bigger than any reasonable screen size, to limit clipping to one axis.
cdef float BIG_PIXELS = 65536

# GL constants used by sampler state.
cdef int GL_NEAREST = 0x2600
cdef int GL_LINEAR = 0x2601
cdef int GL_NEAREST_MIPMAP_NEAREST = 0x2700
cdef int GL_LINEAR_MIPMAP_NEAREST = 0x2701
cdef int GL_NEAREST_MIPMAP_LINEAR = 0x2702
cdef int GL_LINEAR_MIPMAP_LINEAR = 0x2703
cdef int GL_CLAMP_TO_EDGE = 0x812F

# Filters as (mag, min) GL enums, from gl2uniform.TEXTURE_SCALING.
TEXTURE_SCALING = {
    "nearest": (GL_NEAREST, GL_NEAREST),
    "linear": (GL_LINEAR, GL_LINEAR),
    "nearest_mipmap_nearest": (GL_NEAREST, GL_NEAREST_MIPMAP_NEAREST),
    "linear_mipmap_nearest": (GL_LINEAR, GL_LINEAR_MIPMAP_NEAREST),
    "nearest_mipmap_linear": (GL_NEAREST, GL_NEAREST_MIPMAP_LINEAR),
    "linear_mipmap_linear": (GL_LINEAR, GL_LINEAR_MIPMAP_LINEAR),
}

# Getter kinds.
DEF G_CONTEXT = 0
DEF G_PROJECTION = 1
DEF G_VIEW = 2
DEF G_MODEL = 3
DEF G_PROJECTIONVIEW = 4
DEF G_TRANSFORM = 5
DEF G_MODEL_SIZE = 6
DEF G_LOD_BIAS = 7
DEF G_TIME = 8
DEF G_RANDOM = 9
DEF G_VIEWPORT = 10
DEF G_DRAWABLE_SIZE = 11
DEF G_VIRTUAL_SIZE = 12
DEF G_TEX = 20  # + index
DEF G_RES = 30  # + index

# Operations on a getter, from the `_OP_` suffix.
DEF OP_NONE = 0
DEF OP_PREMUL = 1
DEF OP_INVERSE = 2
DEF OP_TRANSPOSE = 3
DEF OP_INVERSE_TRANSPOSE = 4
DEF OP_RES = 5

# name -> (getter kind, required type)
UNIFORM_GETTERS = {
    "u_projection": (G_PROJECTION, "mat4"),
    "u_view": (G_VIEW, "mat4"),
    "u_model": (G_MODEL, "mat4"),
    "u_projectionview": (G_PROJECTIONVIEW, "mat4"),
    "u_transform": (G_TRANSFORM, "mat4"),
    "u_model_size": (G_MODEL_SIZE, "vec2"),
    "u_lod_bias": (G_LOD_BIAS, "float"),
    "u_time": (G_TIME, "float"),
    "u_random": (G_RANDOM, "vec4"),
    "u_viewport": (G_VIEWPORT, "vec4"),
    "u_drawable_size": (G_DRAWABLE_SIZE, "vec2"),
    "u_virtual_size": (G_VIRTUAL_SIZE, "vec2"),
    "tex0": (G_TEX + 0, "sampler2D"),
    "tex1": (G_TEX + 1, "sampler2D"),
    "tex2": (G_TEX + 2, "sampler2D"),
    "tex3": (G_TEX + 3, "sampler2D"),
    "res0": (G_RES + 0, "vec2"),
    "res1": (G_RES + 1, "vec2"),
    "res2": (G_RES + 2, "vec2"),
    "res3": (G_RES + 3, "vec2"),
}

OPERATIONS = {
    "premul": (OP_PREMUL, "vec4"),
    "inverse": (OP_INVERSE, "mat4"),
    "transpose": (OP_TRANSPOSE, "mat4"),
    "inverse_transpose": (OP_INVERSE_TRANSPOSE, "mat4"),
    "res": (OP_RES, "vec2"),
}

ARRAY_TYPES = {"float", "vec2", "vec3", "vec4", "int", "ivec2", "ivec3", "ivec4", "bool", "bvec2", "bvec3", "bvec4"}
SCALAR_TYPES = ARRAY_TYPES | {"mat2", "mat3", "mat4"}


cdef class UniformEntry:
    cdef public str name
    cdef public int kind
    cdef public int op
    cdef public str type  # the type after the operation
    cdef public object array


cdef class Plan:
    """
    How to get every live uniform of one program: the port of `generate_uniform_setter` for the getters. Sampler
    entries are kept apart because the renderer binds them as textures.
    """

    cdef public str shader_name
    cdef public list uniforms
    cdef public list samplers


def make_plan(shader_name, uniforms, samplers):
    """
    `uniforms` is `GpuProgram.uniforms()`: (name, type, array) tuples. `samplers` is `GpuProgram.samplers()`: names.
    """

    rv = Plan()
    rv.shader_name = shader_name
    rv.uniforms = [ _entry(shader_name, n, t, a) for n, t, a in uniforms ]
    rv.samplers = [ _entry(shader_name, n, "sampler2D", None) for n in samplers ]
    return rv


def _entry(shader_name, uniform_name, uniform_type, array):
    cdef UniformEntry e = UniformEntry()

    if "_OP_" in uniform_name:
        base_name, _, operation = uniform_name.rpartition("_OP_")
    else:
        base_name = uniform_name
        operation = None

    if base_name in UNIFORM_GETTERS:
        kind, require_type = UNIFORM_GETTERS[base_name]
    else:
        kind = G_CONTEXT
        require_type = None

    if require_type is not None and uniform_type != require_type:
        raise TypeError(
            f"Uniform {uniform_name} in shader {shader_name} has type {uniform_type}, "
            f"but requires type {require_type}."
        )

    op = OP_NONE

    if operation is not None:
        if operation not in OPERATIONS:
            raise TypeError(
                f"Uniform {uniform_name} in shader {shader_name} has unknown operation "
                f"{operation}."
            )

        op, uniform_type = OPERATIONS[operation]

    if uniform_type != "sampler2D":
        if array is None:
            if uniform_type not in SCALAR_TYPES:
                raise TypeError(f"Uniform {uniform_name} in shader {shader_name} has unknown type {uniform_type}.")
        elif uniform_type not in ARRAY_TYPES:
            raise TypeError(f"Uniform {uniform_name} in shader {shader_name} has unknown type {uniform_type}.")

    e.name = uniform_name
    e.kind = kind
    e.op = op
    e.type = uniform_type
    e.array = array
    return e


def mesh_buffers(Mesh mesh):
    """
    Returns (points, attributes, triangles) memoryviews of the live parts of a mesh: float32 positions
    (`points * point_size`), float32 attributes (`points * layout.stride`) and uint32 indices (`triangles * 3`).
    They alias the mesh memory and must not outlive it.
    """

    cdef Py_ssize_t np = mesh.points * mesh.point_size
    cdef Py_ssize_t na = mesh.points * mesh.layout.stride
    cdef Py_ssize_t nt = mesh.triangles * 3

    cdef float[:] points = <float[:np]> mesh.point_data if np else memoryview(b"").cast("f")
    cdef float[:] attributes = <float[:na]> mesh.attribute if na else memoryview(b"").cast("f")
    cdef unsigned int[:] triangles = <unsigned int[:nt]> mesh.triangle if nt else memoryview(b"").cast("I")

    return points, attributes, triangles


cdef inline object matrix_bytes(Matrix m):
    return PyBytes_FromStringAndSize(<char *> m.m, 16 * sizeof(float))


cdef object convert_matrix(Matrix m, str uniform_type):
    """
    Lays a matrix out the way the Mat2/3/4 setters of gl2uniform.pyx do (column major).
    """

    if uniform_type == "mat4":
        return matrix_bytes(m)
    elif uniform_type == "mat3":
        return (m.xdx, m.ydx, m.zdx, m.xdy, m.ydy, m.zdy, m.xdz, m.ydz, m.zdz)
    elif uniform_type == "mat2":
        return (m.xdx, m.ydx, m.xdy, m.ydy)
    else:
        raise TypeError("A matrix cannot be used as a " + uniform_type)


cdef class Sink:
    """
    State shared by every context of one draw_render call.
    """

    cdef object gpu
    cdef object draw
    cdef bint depth_on
    cdef bint clear_depth_pending
    cdef str drawable_note


cdef class DrawContext:
    """
    The state of the drawing walk: a port of GL2DrawingContext.
    """

    cdef int width
    cdef int height
    cdef bint debug
    cdef DrawContext _child_context

    cdef Matrix projection_matrix
    cdef Matrix view_matrix
    cdef Matrix projectionview_matrix
    cdef Matrix model_matrix
    cdef Matrix scratch

    cdef Polygon clip_polygon
    cdef tuple shaders
    cdef dict uniforms
    cdef dict properties
    cdef bint pixel_perfect
    cdef bint has_depth
    cdef object cull_face
    cdef Sink sink

    def __init__(self):
        self.projection_matrix = Matrix(None)
        self.view_matrix = Matrix(None)
        self.projectionview_matrix = Matrix(None)
        self.model_matrix = Matrix(None)
        self.scratch = Matrix(None)

    cdef DrawContext child_context(self):
        cdef DrawContext rv

        rv = self._child_context
        if rv is None:
            rv = DrawContext()
            self._child_context = rv

        rv.width = self.width
        rv.height = self.height
        rv.debug = self.debug
        rv.sink = self.sink

        rv.projection_matrix.ctake(self.projection_matrix)
        rv.view_matrix.ctake(self.view_matrix)
        rv.projectionview_matrix.ctake(self.projectionview_matrix)
        rv.model_matrix.ctake(self.model_matrix)

        rv.clip_polygon = self.clip_polygon

        rv.shaders = self.shaders
        rv.uniforms = self.uniforms
        rv.properties = self.properties

        rv.pixel_perfect = self.pixel_perfect
        rv.has_depth = self.has_depth
        rv.cull_face = self.cull_face

        return rv

    cdef dict merge_properties(self, dict old, dict child):
        rv = dict(old)

        if not child:
            return rv

        rv.update(child)

        rv.pop("depth", None)
        rv.pop("pixel_perfect", None)
        return rv

    def merge_uniforms(self, dict uniforms):
        if not self.uniforms:
            self.uniforms = uniforms
            return

        self.uniforms = dict(self.uniforms)

        for k, v in uniforms.items():
            if (k in self.uniforms) and (k in renpy.config.merge_uniforms):
                self.uniforms[k] = renpy.config.merge_uniforms[k](self.uniforms[k], v)
            else:
                self.uniforms[k] = v

    cdef void correct_pixel_perfect(self):
        cdef float halfwidth
        cdef float halfheight
        cdef float sx, sy, sz, sw

        halfwidth = self.width / 2.0
        halfheight = self.height / 2.0

        sx = 0
        sy = 0
        sz = 0
        sw = 1

        self.model_matrix.transform4(&sx, &sy, &sz, &sw, sx, sy, sz, sw)
        self.view_matrix.transform4(&sx, &sy, &sz, &sw, sx, sy, sz, sw)
        self.projection_matrix.transform4(&sx, &sy, &sz, &sw, sx, sy, sz, sw)

        sx = roundf(sx * 10000) / 10000
        sy = roundf(sy * 10000) / 10000

        sx = sx * halfwidth + halfwidth
        sy = sy * halfheight + halfheight

        cdef float xoff = roundf(sx) - sx
        cdef float yoff = roundf(sy) - sy

        self.projection_matrix.inplace_reverse_offset(xoff / halfwidth, yoff / halfheight)
        self.projectionview_matrix.ctake(self.projection_matrix)
        self.projectionview_matrix.inplace_multiply(self.view_matrix)

    cdef object draw_model(self, GL2Model model):
        cdef Mesh mesh = model.mesh

        # If a clip polygon is in place, clip the mesh with it.
        if self.clip_polygon is not None:

            if model.reverse is not IDENTITY:
                self.clip_polygon = self.clip_polygon.multiply_matrix(model.forward)

            mesh = mesh.crop(self.clip_polygon)

        if not mesh.triangles:
            return

        if model.properties:
            self.properties = self.merge_properties(self.properties, model.properties)

        if model.reverse is not IDENTITY:
            self.model_matrix.inplace_multiply(model.reverse)

        if model.shaders:
            self.shaders = self.shaders + model.shaders

        if model.uniforms:
            self.merge_uniforms(model.uniforms)

        program = self.sink.draw.shader_cache.get(self.shaders)

        self.submit(program, model, mesh)

    cdef object get_value(self, UniformEntry e, GL2Model model):
        """
        The port of the Getter classes of gl2uniform.pyx. Returns the raw value, before the operation is applied.
        """

        cdef int kind = e.kind
        cdef Matrix m

        if kind == G_CONTEXT:
            return self.uniforms[e.name]
        elif kind == G_PROJECTION:
            return self.projection_matrix
        elif kind == G_VIEW:
            return self.view_matrix
        elif kind == G_MODEL:
            return self.model_matrix
        elif kind == G_PROJECTIONVIEW:
            return self.projectionview_matrix
        elif kind == G_TRANSFORM:
            m = self.scratch
            m.ctake(self.projectionview_matrix)
            m.inplace_multiply(self.model_matrix)
            return m
        elif kind == G_MODEL_SIZE:
            return (model.width, model.height)
        elif kind == G_LOD_BIAS:
            return self.uniforms.get(e.name, float(renpy.config.gl_lod_bias))
        elif kind == G_TIME:
            return (renpy.display.interface.frame_time - renpy.display.interface.init_time) % 86400
        elif kind == G_RANDOM:
            return (random.random(), random.random(), random.random(), random.random())
        elif kind == G_VIEWPORT:
            return (0.0, 0.0, 0.0, 0.0) # Filled in by the renderer from the pass.
        elif kind == G_DRAWABLE_SIZE:
            return self.sink.draw.drawable_viewport[2:]
        elif kind == G_VIRTUAL_SIZE:
            return self.sink.draw.virtual_size
        elif G_TEX <= kind < G_TEX + 4:
            return model.get_texture(kind - G_TEX)
        elif G_RES <= kind < G_RES + 4:
            tex = model.get_texture(kind - G_RES)
            return (tex.texture_width, tex.texture_height)

        raise Exception("Unknown getter kind %d" % kind)

    cdef object apply_op(self, UniformEntry e, value):
        cdef int op = e.op
        cdef Matrix m

        if op == OP_NONE:
            return value

        if op == OP_PREMUL:
            if type(value) is tuple and len(value) == 4:
                return (value[0] * value[3], value[1] * value[3], value[2] * value[3], value[3])
            raise TypeError("PremultiplyGetter only works with vec4 values.")

        if op == OP_RES:
            if type(value) is Render:
                value = value.cached_texture

            if hasattr(value, "texture_width"):
                return (value.texture_width, value.texture_height)

            raise TypeError("ResGetter only works with texture values.")

        if type(value) is not Matrix:
            raise TypeError("This operation only works with Matrix values.")

        m = Matrix(None)
        m.ctake(value)

        if op == OP_INVERSE:
            m.inplace_inverse()
        elif op == OP_TRANSPOSE:
            m.inplace_transpose()
        elif op == OP_INVERSE_TRANSPOSE:
            m.inplace_inverse()
            m.inplace_transpose()

        return m

    cdef object sampler_tuple(self, UniformEntry e, value):
        """
        Returns (texture handle, wrap_s, wrap_t, mag_linear, min_linear, mip, anisotropy) for one sampler, the port
        of Sampler2DSetter.set.
        """

        if value is None:
            shader_name = "+".join(sorted(self.shaders))
            raise Exception(f"Uniform {e.name} in shader {shader_name} given None.")

        if type(value) is Render:
            value = value.cached_texture

        handle = value.handle

        wrap_s = GL_CLAMP_TO_EDGE
        wrap_t = GL_CLAMP_TO_EDGE
        anisotropy = 16
        mag_filter = value.default_mag_filter
        min_filter = value.default_min_filter

        props = self.properties

        if props:
            key = "texture_wrap_" + e.name

            if key in props:
                wrap_s, wrap_t = props[key]
            elif "texture_wrap" in props:
                wrap_s, wrap_t = props["texture_wrap"]

            if not props.get("anisotropic", True):
                anisotropy = 1

            if "texture_scaling" in props:
                mag_filter, min_filter = TEXTURE_SCALING[props["texture_scaling"]]

        mag_linear = mag_filter != GL_NEAREST

        if min_filter == GL_NEAREST:
            return (handle, wrap_s, wrap_t, mag_linear, False, 0, 1)
        elif min_filter == GL_LINEAR:
            return (handle, wrap_s, wrap_t, mag_linear, True, 0, anisotropy)
        elif min_filter == GL_NEAREST_MIPMAP_NEAREST:
            return (handle, wrap_s, wrap_t, mag_linear, False, 1, 1)
        elif min_filter == GL_LINEAR_MIPMAP_NEAREST:
            return (handle, wrap_s, wrap_t, mag_linear, True, 1, 1)
        elif min_filter == GL_NEAREST_MIPMAP_LINEAR:
            return (handle, wrap_s, wrap_t, mag_linear, False, 2, 1)
        else:
            return (handle, wrap_s, wrap_t, mag_linear, True, 2, anisotropy)

    cdef object submit(self, program, GL2Model model, Mesh mesh):
        """
        Collects the uniforms and textures of `program` for this model and hands the draw to the GPU.
        """

        cdef UniformEntry e
        cdef Plan plan = program.plan
        cdef Sink sink = self.sink
        cdef dict properties = self.properties

        values = [ ]

        for e in plan.uniforms:
            try:
                value = self.apply_op(e, self.get_value(e, model))

                if isinstance(value, Matrix):
                    value = convert_matrix(value, e.type)

            except Exception as ex:
                from renpy.gl2.gl2shader import ShaderError
                raise ShaderError(f"Could not get value for uniform {e.name} in shader {plan.shader_name}: {ex!r}")

            values.append(value)

        textures = [ ]

        for e in plan.samplers:
            try:
                textures.append(self.sampler_tuple(e, self.apply_op(e, self.get_value(e, model))))
            except Exception as ex:
                from renpy.gl2.gl2shader import ShaderError
                raise ShaderError(f"Could not get texture {e.name} in shader {plan.shader_name}: {ex!r}")

        blend = None
        mask = None

        if properties:
            blend = properties.get("blend_func", None)
            mask = properties.get("color_mask", None)

        cull = 0
        if self.cull_face == "cw":
            cull = 1
        elif self.cull_face == "ccw":
            cull = 2

        clear = sink.clear_depth_pending
        sink.clear_depth_pending = False

        cdef int stride = mesh.layout.stride

        try:
            sink.gpu.draw(
                program.handle,
                mesh.point_size,
                <size_t> mesh.point_data,
                mesh.points * mesh.point_size,
                <size_t> mesh.attribute,
                mesh.points * stride,
                stride,
                mesh.layout.offset,
                <size_t> mesh.triangle,
                mesh.triangles * 3,
                values,
                textures,
                blend,
                mask,
                cull,
                sink.depth_on,
                clear,
            )
        except ValueError as ex:
            from renpy.gl2.gl2shader import ShaderError
            raise ShaderError(str(ex))

    cdef void set_text_rect(self, Render r):
        cdef int wvirt
        cdef int hvirt

        cdef float x0
        cdef float y0
        cdef float x1
        cdef float y1

        cdef float xmin
        cdef float xmax
        cdef float ymin
        cdef float ymax

        cdef Matrix tovirt

        wvirt, hvirt = self.sink.draw.virtual_size

        tovirt = Matrix.cscreen_projection(wvirt, hvirt).inverse() * self.projection_matrix * self.view_matrix * self.model_matrix

        x0, y0 = tovirt.transform(0, 0)
        x1, y1 = tovirt.transform(r.width, r.height)

        xmin = min(x0, x1)
        xmax = max(x0, x1)
        ymin = min(y0, y1)
        ymax = max(y0, y1)

        renpy.display.interface.text_rect = (xmin, ymin, xmax - xmin, ymax - ymin)

    cdef object draw_one(self, what):
        cdef DrawContext ctx
        cdef Polygon new_clip_polygon
        cdef bint has_reverse = False
        cdef bint has_depth = False

        if what.__class__ is not Render:

            if isinstance(what, GL2Model):
                ctx = self.child_context()
                ctx.draw_model(what)
                return

            # A Surface.
            what = self.sink.draw.load_texture(what)

            if isinstance(what, GL2Model):
                ctx = self.child_context()
                ctx.draw_model(what)
                return

        cdef Render r = what

        if r.text_input:
            self.set_text_rect(r)

        # Handle clipping.
        if (r.xclipping or r.yclipping):
            new_clip_polygon = Polygon.rectangle(
                0 if r.xclipping else -BIG_PIXELS,
                0 if r.yclipping else -BIG_PIXELS,
                r.width if r.xclipping else BIG_PIXELS,
                r.height if r.yclipping else BIG_PIXELS)

            if self.clip_polygon is not None:
                self.clip_polygon = new_clip_polygon.intersect(self.clip_polygon)
                if self.clip_polygon is None:
                    return
            else:
                self.clip_polygon = new_clip_polygon

        has_reverse = (r.reverse is not None) and (r.reverse is not IDENTITY)
        has_depth = False

        if r.properties:

            self.properties = self.merge_properties(self.properties, r.properties)

            if r.properties.get("pixel_perfect", False) and self.pixel_perfect:
                self.correct_pixel_perfect()
                self.pixel_perfect = False

            has_depth = not self.has_depth and r.properties.get("depth", False)

            if has_depth:
                self.sink.clear_depth_pending = True
                self.sink.depth_on = True
                self.has_depth = True

            cull_face = r.properties.get("cull_face", False)
            if cull_face is not False:
                self.cull_face = cull_face

        if has_reverse:
            self.pixel_perfect = False

        if r.shaders is not None:
            self.shaders = self.shaders + r.shaders

        children = r.children

        if r.cached_model is not None:
            children = [ (r.cached_model, 0, 0, False, False) ]
        else:
            if r.uniforms:
                self.merge_uniforms(r.uniforms)

        for child, cx, cy, focus, main in children:

            ctx = self.child_context()

            if (cx or cy):
                if type(cx) is not int:
                    ctx.pixel_perfect = False

                ctx.model_matrix.inplace_offset(cx, cy)

                if ctx.clip_polygon is not None:
                    ctx.clip_polygon = ctx.clip_polygon.offset(-cx, -cy)

            if has_reverse:
                ctx.model_matrix.inplace_multiply(r.reverse)

                if r.matrix_kind == MATRIX_PROJECTION:
                    ctx.projection_matrix.inplace_multiply(ctx.view_matrix)
                    ctx.projection_matrix.inplace_multiply(ctx.model_matrix)

                    ctx.view_matrix.ctake(IDENTITY)
                    ctx.model_matrix.ctake(IDENTITY)

                    ctx.projectionview_matrix.ctake(ctx.projection_matrix)

                elif r.matrix_kind == MATRIX_VIEW:
                    ctx.view_matrix.inplace_multiply(ctx.model_matrix)
                    ctx.model_matrix.ctake(IDENTITY)

                    ctx.projectionview_matrix.ctake(ctx.projection_matrix)
                    ctx.projectionview_matrix.inplace_multiply(ctx.view_matrix)

                if ctx.clip_polygon is not None:
                    ctx.clip_polygon = ctx.clip_polygon.multiply_matrix(r.forward)

            ctx.draw_one(child)

        if has_depth:
            self.sink.depth_on = False

        return 0


cdef DrawContext root_context = DrawContext()


def draw_render(gpu, draw, what, int drawable_width, int drawable_height, Matrix projection):
    """
    Records the draws for `what` (a Render, GL2Model or Surface) into the open pass of `gpu`.

    `gpu`
        The `wgpudraw.Gpu` with a pass begun.

    `draw`
        The WgpuDraw, for the shader cache, texture loading and the virtual and drawable sizes.

    `drawable_width`, `drawable_height`
        The size of the drawable area, in pixels.

    `projection`
        The matrix that projects from view space to the viewport.
    """

    cdef DrawContext ctx = root_context
    cdef Sink sink = Sink()

    sink.gpu = gpu
    sink.draw = draw
    sink.depth_on = False
    sink.clear_depth_pending = False

    ctx.sink = sink
    ctx.width = drawable_width
    ctx.height = drawable_height
    ctx.debug = False

    ctx.projection_matrix.ctake(projection)
    ctx.view_matrix.ctake(IDENTITY)
    ctx.projectionview_matrix.ctake(projection)
    ctx.model_matrix.ctake(IDENTITY)

    ctx.shaders = ()
    ctx.uniforms = { }
    ctx.properties = { }

    ctx.clip_polygon = None
    ctx.pixel_perfect = True
    ctx.has_depth = False
    ctx.cull_face = None

    if renpy.config.nearest_neighbor:
        ctx.properties["texture_scaling"] = "nearest"

    try:
        ctx.draw_one(what)
    finally:
        while ctx is not None:
            ctx.uniforms = None
            ctx.properties = None
            ctx.sink = None
            ctx.clip_polygon = None

            ctx = ctx._child_context
