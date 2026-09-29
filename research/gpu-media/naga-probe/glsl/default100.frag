#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
    precision highp float;
    precision highp int;
#else
    precision mediump float;
    precision mediump int;
#endif
uniform float u_lod_bias;
uniform sampler2D tex0;
varying vec2 v_tex_coord;

void main() {
gl_FragColor = texture2D(tex0, v_tex_coord.xy, u_lod_bias);
}
