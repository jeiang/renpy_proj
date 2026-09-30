#version 120
uniform float u_x;
void main() { gl_FragColor = vec4(1.0, 0.0, 0.0, 1.0) * (1.0 + u_x); }
