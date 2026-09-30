#version 120
uniform float u_weights[5];
uniform vec3 u_colors[3];
uniform mat2 u_m2;
uniform bool u_flag;
void main() {
    float w = 0.0;
    for (int i = 0; i < 5; i++) { w += u_weights[i]; }
    vec2 v = u_m2 * vec2(1.0, 0.0);
    gl_FragColor = vec4(w, u_colors[2].g, v.y, u_flag ? 1.0 : 0.0);
}
