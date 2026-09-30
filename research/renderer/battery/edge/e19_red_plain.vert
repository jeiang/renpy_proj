#version 120
uniform mat4 u_transform;
attribute vec4 a_position;
void main() {
    gl_Position = u_transform * a_position;
}
