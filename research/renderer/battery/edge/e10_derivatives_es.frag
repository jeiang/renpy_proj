#version 100
#extension GL_OES_standard_derivatives : enable
precision mediump float;
uniform sampler2D tex0;
varying vec2 v_tex_coord;
void main() {
    float w = fwidth(v_tex_coord.x);
    gl_FragColor = texture2D(tex0, v_tex_coord) * (1.0 - w);
}
