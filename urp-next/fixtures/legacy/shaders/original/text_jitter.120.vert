#version 120
attribute vec4 a_position;
attribute vec2 a_tex_coord;
uniform vec4 u_random;
uniform float u_text_to_drawable;
uniform vec2 u_textshader_jitter_jitter;
uniform mat4 u_transform;
varying vec2 v_tex_coord;

void main() {

        gl_Position = a_position;
    
    vec2 l_textshader_jitter_jitter = u_textshader_jitter_jitter * u_text_to_drawable;
    gl_Position.xy += l_textshader_jitter_jitter * u_random.xy - l_textshader_jitter_jitter / 2.0;
    
        gl_Position = u_transform * gl_Position;
    
        v_tex_coord = a_tex_coord;
    }
