#version 120
uniform float u_lod_bias;
uniform sampler2D tex0;
varying vec2 v_tex_coord;

void main() {
gl_FragColor = texture2D(tex0, v_tex_coord.xy, u_lod_bias);
}
