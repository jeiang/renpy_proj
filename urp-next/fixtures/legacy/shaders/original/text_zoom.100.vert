#version 100
attribute vec4 a_position;
attribute vec2 a_tex_coord;
attribute vec2 a_text_center;
attribute float a_text_min_time;
uniform float u_text_slow_duration;
uniform float u_text_slow_time;
uniform float u_textshader_zoom_duration;
uniform float u_textshader_zoom_zoom;
uniform mat4 u_transform;
varying vec2 v_tex_coord;

void main() {

        gl_Position = a_position;
    
    float l_textshader_zoom_duration = u_textshader_zoom_duration * u_text_slow_duration;

    if (l_textshader_zoom_duration > 0.0) {
        float l_textshader_zoom_done = clamp((u_text_slow_time - a_text_min_time) / l_textshader_zoom_duration, 0.0, 1.0);
        gl_Position.xy = mix(a_text_center + (gl_Position.xy - a_text_center) * u_textshader_zoom_zoom, gl_Position.xy, l_textshader_zoom_done);
    }
    
        gl_Position = u_transform * gl_Position;
    
        v_tex_coord = a_tex_coord;
    }
