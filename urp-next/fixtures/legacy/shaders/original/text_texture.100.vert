#version 100
attribute vec4 a_position;
attribute vec2 a_tex_coord;
uniform float u_text_to_virtual;
uniform vec2 u_textshader_texture_texture_res;
uniform mat4 u_transform;
varying vec2 v_tex_coord;
varying vec2 v_textshader_texture_coord;

void main() {

        gl_Position = a_position;
    
    v_textshader_texture_coord = u_text_to_virtual * gl_Position.xy / u_textshader_texture_texture_res;
    
        gl_Position = u_transform * gl_Position;
    
        v_tex_coord = a_tex_coord;
    }
