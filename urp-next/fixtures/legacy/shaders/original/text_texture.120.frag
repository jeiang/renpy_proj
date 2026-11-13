#version 120
uniform sampler2D tex0;
uniform float u_lod_bias;
uniform float u_text_main;
uniform sampler2D u_textshader_texture_texture;
varying vec2 v_tex_coord;
varying vec2 v_textshader_texture_coord;

void main() {

        gl_FragColor = texture2D(tex0, v_tex_coord.xy, u_lod_bias);
    
    if (u_text_main == 1.0) {
        gl_FragColor = texture2D(u_textshader_texture_texture, v_textshader_texture_coord) * gl_FragColor;
    }
    }
