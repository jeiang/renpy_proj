#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
    precision highp float;
    precision highp int;
#else
    precision mediump float;
    precision mediump int;
#endif
uniform vec4 u_multiply;
uniform vec4 u_screen;

void main() {

        gl_FragColor.rgb = gl_FragColor.rgb * u_multiply.rgb;
        gl_FragColor.rgb = (gl_FragColor.rgb + u_screen.rgb * gl_FragColor.a) - (gl_FragColor.rgb * u_screen.rgb);
    }
