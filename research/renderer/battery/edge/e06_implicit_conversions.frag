#version 120
uniform sampler2D tex0;
varying vec2 v_tex_coord;
const float K = 2;
void main() {
    float a = 1;
    vec2 b = vec2(1);
    float c = a * 2 + K;
    float arr[3] = float[3](0.1, 0.2, 0.3);
    gl_FragColor = texture2D(tex0, v_tex_coord) * (c + arr[1]) + vec4(b, 0, 0);
}
