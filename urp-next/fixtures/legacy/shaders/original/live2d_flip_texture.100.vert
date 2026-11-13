#version 100
attribute vec4 a_position;
attribute vec2 a_tex_coord;
uniform mat4 u_transform;
varying vec2 v_tex_coord;

void main() {

        gl_Position = a_position;
    
        gl_Position = u_transform * gl_Position;
    
        v_tex_coord = a_tex_coord;
    
        v_tex_coord.y = 1.0 - v_tex_coord.y;
    }
