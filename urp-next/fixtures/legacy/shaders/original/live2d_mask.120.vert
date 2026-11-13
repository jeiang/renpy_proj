#version 120
attribute vec4 a_position;
attribute vec2 a_tex_coord;
uniform vec2 u_live2d_offset;
uniform float u_live2d_ppu;
uniform vec2 u_model_size;
uniform mat4 u_transform;
varying vec2 v_mask_coord;
varying vec2 v_tex_coord;

void main() {

        gl_Position = a_position;
    
        gl_Position = u_transform * gl_Position;
    
        v_tex_coord = a_tex_coord;
        v_mask_coord = (a_position.xy * u_live2d_ppu + u_live2d_offset) / u_model_size;
        v_mask_coord.y = 1.0 - v_mask_coord.y;
    }
