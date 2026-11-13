#version 120
uniform sampler2D tex0;
uniform sampler2D tex1;
varying vec2 v_mask_coord;
varying vec2 v_tex_coord;

void main() {

        vec4 color = texture2D(tex0, v_tex_coord);
        vec4 mask = texture2D(tex1, v_mask_coord);
        gl_FragColor = color * (1.0 - mask.a);
    }
