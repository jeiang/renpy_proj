#version 100
attribute vec4 a_position;
attribute vec2 a_tex_coord;
attribute vec2 a_text_center;
attribute float a_text_min_time;
uniform float u_text_slow_duration;
uniform float u_text_slow_time;
uniform float u_textshader_flip_duration;
uniform mat4 u_transform;
varying vec2 v_tex_coord;

void main() {

        gl_Position = a_position;
    
    float l_textshader_flip_duration = u_textshader_flip_duration * u_text_slow_duration;
    float l_textshader_flip_done;

    if (l_textshader_flip_duration > 0.0) {
        l_textshader_flip_done = clamp((u_text_slow_time - a_text_min_time) / l_textshader_flip_duration, 0.0, 1.0);
    } else {
        l_textshader_flip_done = a_text_min_time <= u_text_slow_time ? 1.0 : 0.0;
    }

    gl_Position.x = mix(a_text_center.x - (gl_Position.x - a_text_center.x), gl_Position.x, l_textshader_flip_done);
    
        gl_Position = u_transform * gl_Position;
    
        v_tex_coord = a_tex_coord;
    }
