#version 450
layout(std140, set = 0, binding = 0) uniform RenpyUniforms {
    float u_lod_bias;
    vec4 u_random;
    float u_text_to_drawable;
    vec2 u_textshader_jitter_jitter;
    mat4 u_transform;
    float renpy_flip_y;
    vec2 renpy_target_size;
};
layout(set = 1, binding = 0) uniform texture2D tex0_t;
layout(set = 1, binding = 1) uniform sampler tex0_s;
layout(location = 0) in vec4 a_position;
layout(location = 1) in vec2 a_tex_coord;
layout(location = 0) out vec2 v_tex_coord;









void renpy_main ( ) { 

gl_Position = a_position ; 

vec2 l_textshader_jitter_jitter = u_textshader_jitter_jitter * u_text_to_drawable ; 
gl_Position . xy += l_textshader_jitter_jitter * u_random . xy - l_textshader_jitter_jitter / 2.0 ; 

gl_Position = u_transform * gl_Position ; 

v_tex_coord = a_tex_coord ; 
} 
void main() {
    renpy_main();
    gl_Position.z = (gl_Position.z + gl_Position.w) * 0.5;
    gl_Position.y = gl_Position.y * renpy_flip_y;
}
