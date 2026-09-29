#version 450
layout(set=0, binding=0) uniform Params { float u_renpy_blur_log2; };
layout(set=0, binding=1) uniform texture2D tex0_t;
layout(set=0, binding=2) uniform sampler tex0_s;
layout(location=0) in vec2 v_tex_coord;
layout(location=0) out vec4 gl_FragColor_;
void main() {
    vec4 c = vec4(0.);
    float renpy_blur_norm = 0.;
    for (float i = -5.; i < 1.; i += 1.) {
        float w = exp(-0.5 * pow(u_renpy_blur_log2 - i, 2.));
        renpy_blur_norm += w;
    }
    c += renpy_blur_norm * textureLod(sampler2D(tex0_t, tex0_s), v_tex_coord.xy, 0.);
    for (float i = 1.; i < 14.; i += 1.) {
        if (i >= u_renpy_blur_log2 + 5.) { break; }
        float w = exp(-0.5 * pow(u_renpy_blur_log2 - i, 2.));
        c += w * textureLod(sampler2D(tex0_t, tex0_s), v_tex_coord.xy, i);
        renpy_blur_norm += w;
    }
    if (renpy_blur_norm > 0.0) { c /= renpy_blur_norm; } else { c = textureLod(sampler2D(tex0_t, tex0_s), v_tex_coord.xy, 0.0); }
    gl_FragColor_ = c;
}
