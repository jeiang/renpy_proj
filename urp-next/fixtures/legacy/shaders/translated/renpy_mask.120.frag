#version 450
layout(std140, set = 0, binding = 0) uniform RenpyUniforms {
    float u_lod_bias;
    float u_renpy_mask_multiplier;
    float u_renpy_mask_offset;
    mat4 u_transform;
    float renpy_flip_y;
    vec2 renpy_target_size;
};
layout(set = 1, binding = 0) uniform texture2D tex0_t;
layout(set = 1, binding = 1) uniform sampler tex0_s;
layout(set = 1, binding = 2) uniform texture2D tex1_t;
layout(set = 1, binding = 3) uniform sampler tex1_s;
layout(location = 0) in vec2 v_tex_coord;
layout(location = 0) out vec4 renpy_FragColor;
vec4 renpy_FragCoord;








void renpy_main ( ) { 

vec4 src = texture ( sampler2D ( tex0_t , tex0_s ) , v_tex_coord . st , u_lod_bias ) ; 
vec4 mask = texture ( sampler2D ( tex1_t , tex1_s ) , v_tex_coord . st , u_lod_bias ) ; 

renpy_FragColor = src * ( mask . a * u_renpy_mask_multiplier + u_renpy_mask_offset ) ; 
} 
void main() {
    renpy_FragCoord = vec4(gl_FragCoord.x, renpy_flip_y > 0.0 ? renpy_target_size.y - gl_FragCoord.y : gl_FragCoord.y, gl_FragCoord.z, gl_FragCoord.w);
    renpy_main();
}
