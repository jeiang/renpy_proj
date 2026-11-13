#version 120
uniform sampler2D tex0;
uniform float u_renpy_blur_log2;
varying vec2 v_tex_coord;

void main() {

        gl_FragColor = vec4(0.);
        float renpy_blur_norm = 0.;

        for (float i = -5.; i < 1.; i += 1.) {
            float renpy_blur_weight = exp(-0.5 * pow(u_renpy_blur_log2 - i, 2.));
            renpy_blur_norm += renpy_blur_weight;
        }

        gl_FragColor += renpy_blur_norm * texture2D(tex0, v_tex_coord.xy, 0.);

        for (float i = 1.; i < 14.; i += 1.) {

            if (i >= u_renpy_blur_log2 + 5.) {
                break;
            }

            float renpy_blur_weight = exp(-0.5 * pow(u_renpy_blur_log2 - i, 2.));
            gl_FragColor += renpy_blur_weight * texture2D(tex0, v_tex_coord.xy, i);
            renpy_blur_norm += renpy_blur_weight;
        }

        if (renpy_blur_norm > 0.0) {
            gl_FragColor /= renpy_blur_norm;
        } else {
            gl_FragColor = texture2D(tex0, v_tex_coord.xy, 0.0);
        }
   }
