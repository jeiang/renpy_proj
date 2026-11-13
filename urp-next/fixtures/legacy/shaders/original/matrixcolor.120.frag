#version 120
uniform sampler2D tex0;
uniform float u_lod_bias;
uniform mat4 u_renpy_matrixcolor;
varying vec2 v_tex_coord;

void main() {

        gl_FragColor = texture2D(tex0, v_tex_coord.xy, u_lod_bias);
    
        gl_FragColor = u_renpy_matrixcolor * gl_FragColor;
    }
