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
uniform sampler2D tex2;
uniform float u_lod_bias;
uniform float u_renpy_dissolve_multiplier;
uniform float u_renpy_dissolve_offset;
varying vec2 v_tex_coord;

void main() {

        vec4 color0 = texture2D(tex0, v_tex_coord.st, u_lod_bias);
        vec4 color1 = texture2D(tex1, v_tex_coord.st, u_lod_bias);
        vec4 color2 = texture2D(tex2, v_tex_coord.st, u_lod_bias);

        float a = clamp((color0.a + u_renpy_dissolve_offset) * u_renpy_dissolve_multiplier, 0.0, 1.0);
        gl_FragColor = mix(color1, color2, a);
    }
