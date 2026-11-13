#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
    precision highp float;
    precision highp int;
#else
    precision mediump float;
    precision mediump int;
#endif
uniform sampler2D tex0;
uniform float u_lod_bias;
uniform float u_text_slow_duration;
uniform float u_text_slow_time;
uniform float u_textshader_dissolve_duration;
varying vec2 v_tex_coord;
varying float v_text_time;

void main() {

        gl_FragColor = texture2D(tex0, v_tex_coord.xy, u_lod_bias);
    
    float l_textshader_dissolve_duration = u_textshader_dissolve_duration * u_text_slow_duration;
    float l_textshader_dissolve_done;
    if (l_textshader_dissolve_duration > 0.0) {
        l_textshader_dissolve_done = clamp((u_text_slow_time - v_text_time) / l_textshader_dissolve_duration, 0.0, 1.0);
    } else {
        l_textshader_dissolve_done = v_text_time <= u_text_slow_time ? 1.0 : 0.0;
    }
    gl_FragColor = gl_FragColor * l_textshader_dissolve_done;
    }
