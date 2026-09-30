# Stand-in for gl2shader.pyx of stock Ren'Py (Ren'Py 8.5.3, MIT licence, Copyright 2004-2026 Tom Rothamel).
# `Variable` and `ShaderError` are the stock classes. `Program` keeps the interface that gl2shadercache uses
# (`Program(name, vertex, fragment)` then `load()`), and hands the source to the Rust translator of renpy.gl2.wgpudraw
# instead of an OpenGL driver.

import re

import renpy


class ShaderError(Exception):
    pass


GLSL_PRECISIONS = {
    "highp",
    "mediump",
    "lowp",
    }

ATTRIBUTE_TYPES = {
    "float": 1,
    "vec2": 2,
    "vec3": 3,
    "vec4": 4,
}

UNIFORM_TYPES = {
    "float",
    "vec2",
    "vec3",
    "vec4",
    "int",
    "ivec2",
    "ivec3",
    "ivec4",
    "bool",
    "bvec2",
    "bvec3",
    "bvec4",
    "mat2",
    "mat3",
    "mat4",
    "sampler2D",
}

VARYING_TYPES = set(ATTRIBUTE_TYPES) | set(UNIFORM_TYPES)


class Variable:
    """
    Represents a variable parsed from a shader, as part of the parsing process.
    Returns an empty object if the line is not a variable.
    """

    storage: str | None = None
    "The storage class, one of uniform, attribute, or varying, or None if not a variable."

    type: str | None = None
    "The type of the variable, one of float, int, bool, vec<2-4>, ivec<2-4>, bvec<2-4>, mat<2-4>, or sampler2D."

    name: str | None = None
    "The name of the variable."

    array: int | None = None
    "The size of the array, or None if not an array."

    line: str
    "The line of source code that the variable was parsed from, including qualifiers and the trailing semicolon."

    def __init__(self, shader_name, line):

        l = line.strip().rstrip("; ")
        self.line = l

        def match_word():
            nonlocal l
            if m := re.match(r'\s*(\w+)', l):
                l = l[m.end():]
                return m.group(1)
            else:
                return None

        def match_array():
            nonlocal l
            if m := re.match(r'\s*\[\s*(\d+)\s*\]', l):
                l = l[m.end():]
                return int(m.group(1))
            else:
                return None

        token = match_word()

        if token == "invariant":
            token = match_word()

        if token == "uniform":
            self.storage = "uniform"
            types = UNIFORM_TYPES
        elif token == "attribute":
            self.storage = "attribute"
            types = ATTRIBUTE_TYPES
        elif token == "varying":
            self.storage = "varying"
            types = VARYING_TYPES
        else:
            self.storage = None
            return

        token = match_word()

        if token in ("highp", "mediump", "lowp"):
            token = match_word()

        if token not in types:
            raise ShaderError(f"In {shader_name}, Unsupported type {token} in '{line}'. Only float, int, bool, vec<2-4>, ivec<2-4>, bvec<2-4>, mat<2-4>, and sampler2D are supported.")

        self.type = token

        self.array = match_array()

        self.name = match_word()
        if self.name is None:
            raise ShaderError(f"In {shader_name}, couldn't find name in '{line}'.")

        if self.array is None:
            self.array = match_array()

        if l.rstrip():
            raise ShaderError("Spurious tokens after the name in '{}'.".format(line))

    def __hash__(self):
        return hash((self.storage, self.type, self.name, self.array))

    def __eq__(self, other):
        return (self.storage, self.type, self.name, self.array) == (other.storage, other.type, other.name, other.array)


class Program:
    """
    A shader program: the GLSL source that gl2shadercache assembled, and, after load(), the translated program.
    """

    def __init__(self, name, vertex, fragment):
        self.name = name
        self.vertex = vertex
        self.fragment = fragment

        # The renpy.gl2.wgpudraw.GpuProgram, set by load().
        self.handle = None

        # The renpy.gl2.gl2meshbridge.Plan that gets the uniform values, set by load().
        self.plan = None

    def load(self):
        """
        Translates the program to WGSL-ready modules and validates it. Raises ShaderError on failure, as a GL driver
        would when compiling or linking.
        """

        import renpy.gl2.wgpudraw as wgpudraw
        from renpy.gl2.gl2meshbridge import make_plan

        shader_name = "+".join(self.name)

        try:
            handle = wgpudraw.compile_program(shader_name, self.vertex, self.fragment)
        except ValueError as e:
            renpy.display.log.write("Error compiling shader %s: %s", shader_name, e)

            renpy.display.log.write("Vertex shader:")
            for i, l in enumerate(self.vertex.splitlines()):
                renpy.display.log.write("% 3d %s" % (i + 1, l))

            renpy.display.log.write("Fragment shader:")
            for i, l in enumerate(self.fragment.splitlines()):
                renpy.display.log.write("% 3d %s" % (i + 1, l))

            raise ShaderError(str(e))

        try:
            plan = make_plan(shader_name, handle.uniforms(), handle.samplers())
        except TypeError as e:
            raise ShaderError(str(e))

        self.handle = handle
        self.plan = plan

        wgpudraw.warm_program(handle)
