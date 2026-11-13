#version 120
uniform sampler2D tex0;
uniform sampler2D tex1;
uniform float u_lod_bias;
uniform float u_renpy_dissolve;
varying vec2 v_tex_coord;

void main() {

        vec4 color0 = texture2D(tex0, v_tex_coord.st, u_lod_bias);
        vec4 color1 = texture2D(tex1, v_tex_coord.st, u_lod_bias);

        gl_FragColor = mix(color0, color1, u_renpy_dissolve);
    }
