#version 450
layout(std140, set = 0, binding = 0) uniform RenpyUniforms {
    float u_renpy_blur_log2;
    mat4 u_transform;
    float renpy_flip_y;
    vec2 renpy_target_size;
};
layout(set = 1, binding = 0) uniform texture2D tex0_t;
layout(set = 1, binding = 1) uniform sampler tex0_s;
layout(location = 0) in vec2 v_tex_coord;
layout(location = 0) out vec4 renpy_FragColor;
vec4 renpy_FragCoord;





void renpy_main ( ) { 

renpy_FragColor = vec4 ( 0. ) ; 
float renpy_blur_norm = 0. ; 

for ( float i = - 5. ; i < 1. ; i += 1. ) { 
float renpy_blur_weight = exp ( - 0.5 * pow ( u_renpy_blur_log2 - i , 2. ) ) ; 
renpy_blur_norm += renpy_blur_weight ; 
} 

renpy_FragColor += renpy_blur_norm * texture ( sampler2D ( tex0_t , tex0_s ) , v_tex_coord . xy , 0. ) ; 

for ( float i = 1. ; i < 14. ; i += 1. ) { 

if ( i >= u_renpy_blur_log2 + 5. ) { 
break ; 
} 

float renpy_blur_weight = exp ( - 0.5 * pow ( u_renpy_blur_log2 - i , 2. ) ) ; 
renpy_FragColor += renpy_blur_weight * texture ( sampler2D ( tex0_t , tex0_s ) , v_tex_coord . xy , i ) ; 
renpy_blur_norm += renpy_blur_weight ; 
} 

if ( renpy_blur_norm > 0.0 ) { 
renpy_FragColor /= renpy_blur_norm ; 
} else { 
renpy_FragColor = texture ( sampler2D ( tex0_t , tex0_s ) , v_tex_coord . xy , 0.0 ) ; 
} 
} 
void main() {
    renpy_FragCoord = vec4(gl_FragCoord.x, renpy_flip_y > 0.0 ? renpy_target_size.y - gl_FragCoord.y : gl_FragCoord.y, gl_FragCoord.z, gl_FragCoord.w);
    renpy_main();
}
