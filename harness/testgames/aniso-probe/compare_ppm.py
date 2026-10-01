#!/usr/bin/env python3
"""Compare the pictures of gl_probe.c (OpenGL) with those of aniso_probe.rs (wgpu): mean absolute difference, 0..255.

    compare_ppm.py <gl dir> <wgpu dir>
"""
import sys

GL = ["gl_linear_a16", "gl_linear_a8", "gl_linear_a1", "gl_lml_lod0_a16", "gl_lml_a16"]
VK = ["base_lin_a16", "base_a8", "base_lin_a1", "tri_a16", "emu_ceil"]


def ppm(path):
    return open(path, "rb").read().split(b"\n", 3)[3]


def mad(a, b):
    return sum(abs(x - y) for x, y in zip(a, b)) / len(a)


def main(gl_dir, vk_dir):
    for tex in ("checker", "grating"):
        vk = {v: ppm("%s/%s_%s.ppm" % (vk_dir, tex, v)) for v in VK}
        print("==", tex)
        print("%20s" % "", *["%13s" % v for v in VK])
        for g in GL:
            a = ppm("%s/%s_%s.ppm" % (gl_dir, tex, g))
            print("%20s" % g, *["%13.3f" % mad(a, vk[v]) for v in VK])


if __name__ == "__main__":
    main(*sys.argv[1:3])
