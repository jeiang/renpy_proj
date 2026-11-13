#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
    precision highp float;
    precision highp int;
#else
    precision mediump float;
    precision mediump int;
#endif
uniform sampler2D tex0;
uniform sampler2D tex1;
varying vec2 v_tex_coord;

void main() {

        vec4 src  = texture2D(tex0, v_tex_coord.xy);
        vec4 mask = texture2D(tex1, v_tex_coord.xy);

        gl_FragColor = vec4(src.r * mask.r, src.g * mask.r, src.b * mask.r, mask.r);
    }
