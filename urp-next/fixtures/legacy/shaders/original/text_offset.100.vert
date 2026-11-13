#version 100
attribute vec4 a_position;
attribute vec2 a_tex_coord;
uniform float u_text_to_drawable;
uniform vec2 u_textshader_offset_offset;
uniform mat4 u_transform;
varying vec2 v_tex_coord;

void main() {

        gl_Position = a_position;
    
    gl_Position.xy += u_textshader_offset_offset * u_text_to_drawable;
    
        gl_Position = u_transform * gl_Position;
    
        v_tex_coord = a_tex_coord;
    }
