#version 120
uniform sampler2D tex0;
uniform float u_lod_bias;
uniform float u_mode;
varying vec2 v_tex_coord;
void main() {
    if (u_mode > 0.5) { gl_FragColor = texture2D(tex0, v_tex_coord, u_lod_bias); }
    else { gl_FragColor = texture2D(tex0, v_tex_coord.yx, u_lod_bias); }
}
