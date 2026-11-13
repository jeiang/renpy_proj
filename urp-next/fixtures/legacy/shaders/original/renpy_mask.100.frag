#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
    precision highp float;
    precision highp int;
#else
    precision mediump float;
    precision mediump int;
#endif
uniform sampler2D tex0;
uniform sampler2D tex1;
uniform float u_lod_bias;
uniform float u_renpy_mask_multiplier;
uniform float u_renpy_mask_offset;
varying vec2 v_tex_coord;

void main() {

        vec4 src = texture2D(tex0, v_tex_coord.st, u_lod_bias);
        vec4 mask = texture2D(tex1, v_tex_coord.st, u_lod_bias);

        gl_FragColor = src * (mask.a * u_renpy_mask_multiplier + u_renpy_mask_offset);
    }
