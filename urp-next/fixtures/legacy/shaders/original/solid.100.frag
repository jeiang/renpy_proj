#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
    precision highp float;
    precision highp int;
#else
    precision mediump float;
    precision mediump int;
#endif
uniform vec4 u_renpy_solid_color;

void main() {

        gl_FragColor = u_renpy_solid_color;
    }
