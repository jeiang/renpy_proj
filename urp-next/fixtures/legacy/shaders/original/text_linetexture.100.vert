#version 100
attribute vec4 a_position;
attribute vec2 a_tex_coord;
attribute vec2 a_text_center;
uniform float u_text_to_virtual;
uniform vec2 u_textshader_linetexture_scale;
uniform vec2 u_textshader_linetexture_texture_res;
uniform mat4 u_transform;
varying vec2 v_tex_coord;
varying vec2 v_textshader_linetexture_coord;

void main() {

        gl_Position = a_position;
    

    v_textshader_linetexture_coord = vec2( gl_Position.x, (gl_Position.y - a_text_center.y)) / u_textshader_linetexture_scale * u_text_to_virtual / u_textshader_linetexture_texture_res;
    v_textshader_linetexture_coord.y += 0.5;
    
        gl_Position = u_transform * gl_Position;
    
        v_tex_coord = a_tex_coord;
    }
