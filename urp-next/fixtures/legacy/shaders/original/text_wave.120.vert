#version 120
attribute vec4 a_position;
attribute vec2 a_tex_coord;
attribute float a_text_index;
uniform float u_text_to_drawable;
uniform float u_textshader_wave_amplitude;
uniform float u_textshader_wave_frequency;
uniform float u_textshader_wave_wavelength;
uniform float u_time;
uniform mat4 u_transform;
varying vec2 v_tex_coord;

void main() {

        gl_Position = a_position;
    
    gl_Position.y += cos(2.0 * 3.14159265359 * (a_text_index / u_textshader_wave_wavelength + u_time * u_textshader_wave_frequency)) * u_textshader_wave_amplitude * u_text_to_drawable;
    
        gl_Position = u_transform * gl_Position;
    
        v_tex_coord = a_tex_coord;
    }
