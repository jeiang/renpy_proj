#version 120
uniform sampler2D tex0;
varying vec2 v_tex_coord;
float round(float x) { return floor(x + 0.5); }
float sinh(float x) { return 0.5 * (exp(x) - exp(-x)); }
void main() {
    float sample = round(v_tex_coord.x * 4.0) / 4.0;
    vec4 filter = texture2D(tex0, vec2(sample, sinh(v_tex_coord.y)));
    gl_FragColor = filter;
}
