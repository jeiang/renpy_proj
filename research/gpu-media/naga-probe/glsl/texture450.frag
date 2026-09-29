#version 450
layout(set=0, binding=0) uniform Params { float u_lod_bias; };
layout(set=0, binding=1) uniform texture2D tex0_t;
layout(set=0, binding=2) uniform sampler tex0_s;
layout(location=0) in vec2 v_tex_coord;
layout(location=0) out vec4 frag_color;
void main() {
    frag_color = texture(sampler2D(tex0_t, tex0_s), v_tex_coord.xy, u_lod_bias);
}
