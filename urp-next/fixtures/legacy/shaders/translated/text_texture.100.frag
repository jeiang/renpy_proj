#version 450
layout(std140, set = 0, binding = 0) uniform RenpyUniforms {
    float u_lod_bias;
    float u_text_main;
    float u_text_to_virtual;
    vec2 u_textshader_texture_texture_res;
    mat4 u_transform;
    float renpy_flip_y;
    vec2 renpy_target_size;
};
layout(set = 1, binding = 0) uniform texture2D tex0_t;
layout(set = 1, binding = 1) uniform sampler tex0_s;
layout(set = 1, binding = 2) uniform texture2D u_textshader_texture_texture_t;
layout(set = 1, binding = 3) uniform sampler u_textshader_texture_texture_s;
layout(location = 0) in vec2 v_tex_coord;
layout(location = 1) in vec2 v_textshader_texture_coord;
layout(location = 0) out vec4 renpy_FragColor;
vec4 renpy_FragCoord;

#ifdef GL_FRAGMENT_PRECISION_HIGH


#else


#endif







void renpy_main ( ) { 

renpy_FragColor = texture ( sampler2D ( tex0_t , tex0_s ) , v_tex_coord . xy , u_lod_bias ) ; 

if ( u_text_main == 1.0 ) { 
renpy_FragColor = texture ( sampler2D ( u_textshader_texture_texture_t , u_textshader_texture_texture_s ) , v_textshader_texture_coord ) * renpy_FragColor ; 
} 
} 
void main() {
    renpy_FragCoord = vec4(gl_FragCoord.x, renpy_flip_y > 0.0 ? renpy_target_size.y - gl_FragCoord.y : gl_FragCoord.y, gl_FragCoord.z, gl_FragCoord.w);
    renpy_main();
}
