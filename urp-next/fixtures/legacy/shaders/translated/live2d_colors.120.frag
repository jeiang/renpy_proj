#version 450
layout(std140, set = 0, binding = 0) uniform RenpyUniforms {
    vec4 u_multiply;
    vec4 u_screen;
    mat4 u_transform;
    float renpy_flip_y;
    vec2 renpy_target_size;
};
layout(location = 0) out vec4 renpy_FragColor;
vec4 renpy_FragCoord;




void renpy_main ( ) { 

renpy_FragColor . rgb = renpy_FragColor . rgb * u_multiply . rgb ; 
renpy_FragColor . rgb = ( renpy_FragColor . rgb + u_screen . rgb * renpy_FragColor . a ) - ( renpy_FragColor . rgb * u_screen . rgb ) ; 
} 
void main() {
    renpy_FragCoord = vec4(gl_FragCoord.x, renpy_flip_y > 0.0 ? renpy_target_size.y - gl_FragCoord.y : gl_FragCoord.y, gl_FragCoord.z, gl_FragCoord.w);
    renpy_main();
}
