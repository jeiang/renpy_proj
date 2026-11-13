#version 450
layout(std140, set = 0, binding = 0) uniform RenpyUniforms {
    float u_lod_bias;
    float u_renpy_dissolve_multiplier;
    float u_renpy_dissolve_offset;
    mat4 u_transform;
    float renpy_flip_y;
    vec2 renpy_target_size;
};
layout(set = 1, binding = 0) uniform texture2D tex0_t;
layout(set = 1, binding = 1) uniform sampler tex0_s;
layout(set = 1, binding = 2) uniform texture2D tex1_t;
layout(set = 1, binding = 3) uniform sampler tex1_s;
layout(set = 1, binding = 4) uniform texture2D tex2_t;
layout(set = 1, binding = 5) uniform sampler tex2_s;
layout(location = 0) in vec2 v_tex_coord;
layout(location = 0) out vec4 renpy_FragColor;
vec4 renpy_FragCoord;

#ifdef GL_FRAGMENT_PRECISION_HIGH


#else


#endif








void renpy_main ( ) { 

vec4 color0 = texture ( sampler2D ( tex0_t , tex0_s ) , v_tex_coord . st , u_lod_bias ) ; 
vec4 color1 = texture ( sampler2D ( tex1_t , tex1_s ) , v_tex_coord . st , u_lod_bias ) ; 
vec4 color2 = texture ( sampler2D ( tex2_t , tex2_s ) , v_tex_coord . st , u_lod_bias ) ; 

float a = clamp ( ( color0 . a + u_renpy_dissolve_offset ) * u_renpy_dissolve_multiplier , 0.0 , 1.0 ) ; 
renpy_FragColor = mix ( color1 , color2 , a ) ; 
} 
void main() {
    renpy_FragCoord = vec4(gl_FragCoord.x, renpy_flip_y > 0.0 ? renpy_target_size.y - gl_FragCoord.y : gl_FragCoord.y, gl_FragCoord.z, gl_FragCoord.w);
    renpy_main();
}
