#version 120
uniform sampler2D tex0;
uniform vec2 u_res;
varying vec2 v_tex_coord;
void main() {
    vec4 acc = vec4(0.0);
    for (int y = -2; y <= 2; y++) {
        for (int x = -2; x <= 2; x++) {
            acc += texture2D(tex0, v_tex_coord + vec2(float(x), float(y)) / u_res);
        }
    }
    mat3 m = mat3(1.0);
    m[1][1] = 0.5;
    gl_FragColor = vec4(m * acc.rgb / 25.0, acc.a / 25.0);
}
