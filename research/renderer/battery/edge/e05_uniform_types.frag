#version 120
uniform sampler2D tex0;
uniform bool u_flag;
uniform bvec2 u_bflags;
uniform int u_count;
uniform ivec3 u_ivec;
uniform float u_weights[5];
uniform vec3 u_colors[3];
uniform mat2 u_m2;
uniform mat3 u_m3;
varying vec2 v_tex_coord;
void main() {
    vec4 c = texture2D(tex0, v_tex_coord);
    float w = 0.0;
    for (int i = 0; i < 5; i++) { w += u_weights[i]; }
    if (u_flag && u_bflags.x) { c.rgb *= u_colors[1]; }
    c.rg = u_m2 * c.rg;
    c.rgb = u_m3 * c.rgb * float(u_count + u_ivec.y) * w;
    gl_FragColor = c;
}
