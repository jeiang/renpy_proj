#version 120
uniform sampler2D tex0;
uniform sampler2D tex1;
varying vec2 v_tex_coord;
vec4 fetch(sampler2D s, vec2 uv) { return texture2D(s, uv); }
void main() {
    gl_FragColor = fetch(tex0, v_tex_coord) + fetch(tex1, v_tex_coord);
}
