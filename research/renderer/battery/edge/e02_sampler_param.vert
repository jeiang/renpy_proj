#version 120
uniform mat4 u_transform;
attribute vec4 a_position;
attribute vec2 a_tex_coord;
varying vec2 v_tex_coord;
void main() {
    gl_Position = a_position;
    gl_Position = u_transform * gl_Position;
    v_tex_coord = a_tex_coord;
}
