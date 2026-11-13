#version 450
layout(std140, set = 0, binding = 0) uniform RenpyUniforms {
    float u_lod_bias;
    float u_text_slow_duration;
    float u_text_slow_time;
    float u_textshader_zoom_duration;
    float u_textshader_zoom_zoom;
    mat4 u_transform;
    float renpy_flip_y;
    vec2 renpy_target_size;
};
layout(set = 1, binding = 0) uniform texture2D tex0_t;
layout(set = 1, binding = 1) uniform sampler tex0_s;
layout(location = 0) in vec4 a_position;
layout(location = 1) in vec2 a_tex_coord;
layout(location = 2) in vec2 a_text_center;
layout(location = 3) in float a_text_min_time;
layout(location = 0) out vec2 v_tex_coord;












void renpy_main ( ) { 

gl_Position = a_position ; 

float l_textshader_zoom_duration = u_textshader_zoom_duration * u_text_slow_duration ; 

if ( l_textshader_zoom_duration > 0.0 ) { 
float l_textshader_zoom_done = clamp ( ( u_text_slow_time - a_text_min_time ) / l_textshader_zoom_duration , 0.0 , 1.0 ) ; 
gl_Position . xy = mix ( a_text_center + ( gl_Position . xy - a_text_center ) * u_textshader_zoom_zoom , gl_Position . xy , l_textshader_zoom_done ) ; 
} 

gl_Position = u_transform * gl_Position ; 

v_tex_coord = a_tex_coord ; 
} 
void main() {
    renpy_main();
    gl_Position.z = (gl_Position.z + gl_Position.w) * 0.5;
    gl_Position.y = gl_Position.y * renpy_flip_y;
}
