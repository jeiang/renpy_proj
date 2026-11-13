#version 450
layout(std140, set = 0, binding = 0) uniform RenpyUniforms {
    float u_lod_bias;
    float u_text_slow_duration;
    float u_text_slow_time;
    float u_textshader_dissolve_duration;
    mat4 u_transform;
    float renpy_flip_y;
    vec2 renpy_target_size;
};
layout(set = 1, binding = 0) uniform texture2D tex0_t;
layout(set = 1, binding = 1) uniform sampler tex0_s;
layout(location = 0) in vec2 v_tex_coord;
layout(location = 1) in float v_text_time;
layout(location = 0) out vec4 renpy_FragColor;
vec4 renpy_FragCoord;









void renpy_main ( ) { 

renpy_FragColor = texture ( sampler2D ( tex0_t , tex0_s ) , v_tex_coord . xy , u_lod_bias ) ; 

float l_textshader_dissolve_duration = u_textshader_dissolve_duration * u_text_slow_duration ; 
float l_textshader_dissolve_done ; 
if ( l_textshader_dissolve_duration > 0.0 ) { 
l_textshader_dissolve_done = clamp ( ( u_text_slow_time - v_text_time ) / l_textshader_dissolve_duration , 0.0 , 1.0 ) ; 
} else { 
l_textshader_dissolve_done = v_text_time <= u_text_slow_time ? 1.0 : 0.0 ; 
} 
renpy_FragColor = renpy_FragColor * l_textshader_dissolve_done ; 
} 
void main() {
    renpy_FragCoord = vec4(gl_FragCoord.x, renpy_flip_y > 0.0 ? renpy_target_size.y - gl_FragCoord.y : gl_FragCoord.y, gl_FragCoord.z, gl_FragCoord.w);
    renpy_main();
}
