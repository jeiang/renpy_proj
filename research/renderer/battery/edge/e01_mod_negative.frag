#version 120
uniform sampler2D tex0;
uniform float u_t;
varying vec2 v_tex_coord;
void main() {
    vec2 p = mod(v_tex_coord - 0.5 + u_t, vec2(0.25));
    gl_FragColor = texture2D(tex0, p);
}
