#version 450
layout(std140, set = 0, binding = 0) uniform RenpyUniforms {
    vec2 u_live2d_offset;
    float u_live2d_ppu;
    vec2 u_model_size;
    mat4 u_transform;
    float renpy_flip_y;
    vec2 renpy_target_size;
};
layout(set = 1, binding = 0) uniform texture2D tex0_t;
layout(set = 1, binding = 1) uniform sampler tex0_s;
layout(set = 1, binding = 2) uniform texture2D tex1_t;
layout(set = 1, binding = 3) uniform sampler tex1_s;
layout(location = 0) in vec2 v_mask_coord;
layout(location = 1) in vec2 v_tex_coord;
layout(location = 0) out vec4 renpy_FragColor;
vec4 renpy_FragCoord;

#ifdef GL_FRAGMENT_PRECISION_HIGH


#else


#endif





void renpy_main ( ) { 

vec4 color = texture ( sampler2D ( tex0_t , tex0_s ) , v_tex_coord ) ; 
vec4 mask = texture ( sampler2D ( tex1_t , tex1_s ) , v_mask_coord ) ; 
renpy_FragColor = color * mask . a ; 
} 
void main() {
    renpy_FragCoord = vec4(gl_FragCoord.x, renpy_flip_y > 0.0 ? renpy_target_size.y - gl_FragCoord.y : gl_FragCoord.y, gl_FragCoord.z, gl_FragCoord.w);
    renpy_main();
}
