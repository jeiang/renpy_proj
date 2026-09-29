#version 120
attribute vec2 a_tex_coord;
attribute vec4 a_position;
uniform mat4 u_transform;
varying vec2 v_tex_coord;

void main() {
gl_Position = a_position;
gl_Position = u_transform * gl_Position;
v_tex_coord = a_tex_coord;
}
