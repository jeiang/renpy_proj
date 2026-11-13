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
uniform float u_renpy_alpha;
uniform mat4 u_renpy_matrixcolor;
uniform float u_renpy_over;
varying vec2 v_tex_coord;

void main() {

        gl_FragColor = texture2D(tex0, v_tex_coord.xy, u_lod_bias);
    
        gl_FragColor = u_renpy_matrixcolor * gl_FragColor;
    
        gl_FragColor = gl_FragColor * vec4(u_renpy_alpha, u_renpy_alpha, u_renpy_alpha, u_renpy_alpha * u_renpy_over);
    }
