#version 450
layout(std140, set = 0, binding = 0) uniform RenpyUniforms {
    float u_lod_bias;
    float u_text_to_drawable;
    float u_textshader_wave_amplitude;
    float u_textshader_wave_frequency;
    float u_textshader_wave_wavelength;
    float u_time;
    mat4 u_transform;
    float renpy_flip_y;
    vec2 renpy_target_size;
};
layout(set = 1, binding = 0) uniform texture2D tex0_t;
layout(set = 1, binding = 1) uniform sampler tex0_s;
layout(location = 0) in vec4 a_position;
layout(location = 1) in vec2 a_tex_coord;
layout(location = 2) in float a_text_index;
layout(location = 0) out vec2 v_tex_coord;












void renpy_main ( ) { 

gl_Position = a_position ; 

gl_Position . y += cos ( 2.0 * 3.14159265359 * ( a_text_index / u_textshader_wave_wavelength + u_time * u_textshader_wave_frequency ) ) * u_textshader_wave_amplitude * u_text_to_drawable ; 

gl_Position = u_transform * gl_Position ; 

v_tex_coord = a_tex_coord ; 
} 
void main() {
    renpy_main();
    gl_Position.z = (gl_Position.z + gl_Position.w) * 0.5;
    gl_Position.y = gl_Position.y * renpy_flip_y;
}
