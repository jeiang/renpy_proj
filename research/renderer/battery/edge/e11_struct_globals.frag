#version 120
uniform sampler2D tex0;
varying vec2 v_tex_coord;
struct Light { vec3 dir; float power; };
float g_scale = 1.5;
vec4 shade(Light l, vec4 c) { return c * dot(l.dir, vec3(0.0, 0.0, 1.0)) * l.power * g_scale; }
void main() {
    Light l = Light(normalize(vec3(1.0, 1.0, 1.0)), 2.0);
    gl_FragColor = shade(l, texture2D(tex0, v_tex_coord));
}
