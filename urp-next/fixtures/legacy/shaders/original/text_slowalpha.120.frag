#version 120
uniform sampler2D tex0;
uniform float u_lod_bias;
uniform float u_textshader_slowalpha_alpha;
varying vec2 v_tex_coord;

void main() {

        gl_FragColor = texture2D(tex0, v_tex_coord.xy, u_lod_bias);
    
vec4 l_textshader_slowalpha_color = gl_FragColor;

    gl_FragColor = mix(gl_FragColor, l_textshader_slowalpha_color, u_textshader_slowalpha_alpha);
    }
