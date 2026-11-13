#version 450
layout(std140, set = 0, binding = 0) uniform RenpyUniforms {
    vec4 u_renpy_solid_color;
    mat4 u_transform;
    float renpy_flip_y;
    vec2 renpy_target_size;
};
layout(location = 0) in vec4 a_position;




void renpy_main ( ) { 

gl_Position = a_position ; 

gl_Position = u_transform * gl_Position ; 
} 
void main() {
    renpy_main();
    gl_Position.z = (gl_Position.z + gl_Position.w) * 0.5;
    gl_Position.y = gl_Position.y * renpy_flip_y;
}
