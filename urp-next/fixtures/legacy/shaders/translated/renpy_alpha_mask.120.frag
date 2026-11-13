#version 450
layout(std140, set = 0, binding = 0) uniform RenpyUniforms {
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

vec4 src = texture ( sampler2D ( tex0_t , tex0_s ) , v_tex_coord . xy ) ; 
vec4 mask = texture ( sampler2D ( tex1_t , tex1_s ) , v_tex_coord . xy ) ; 

renpy_FragColor = vec4 ( src . r * mask . r , src . g * mask . r , src . b * mask . r , mask . r ) ; 
} 
void main() {
    renpy_FragCoord = vec4(gl_FragCoord.x, renpy_flip_y > 0.0 ? renpy_target_size.y - gl_FragCoord.y : gl_FragCoord.y, gl_FragCoord.z, gl_FragCoord.w);
    renpy_main();
}
