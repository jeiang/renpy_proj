#version 450
layout(binding=0) uniform Params { float u_lod_bias; };
layout(binding=1) uniform sampler2D tex0;
layout(location=0) in vec2 v_tex_coord;
layout(location=0) out vec4 o;
void main() {
o = texture(tex0, v_tex_coord.xy, u_lod_bias);
}
