#version 120
uniform sampler2D tex0;
uniform mat4 u_transform;
attribute vec4 a_position;
attribute vec2 a_tex_coord;
varying vec2 v_tex_coord;
varying vec4 v_color;
void main() {
    gl_Position = u_transform * a_position;
    v_tex_coord = a_tex_coord;
    v_color = texture2D(tex0, a_tex_coord);
}
