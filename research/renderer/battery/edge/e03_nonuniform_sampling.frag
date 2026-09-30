#version 120
uniform sampler2D tex0;
varying vec2 v_tex_coord;
void main() {
    gl_FragColor = vec4(0.0);
    if (v_tex_coord.x > 0.5) {
        gl_FragColor = texture2D(tex0, v_tex_coord);
    }
}
