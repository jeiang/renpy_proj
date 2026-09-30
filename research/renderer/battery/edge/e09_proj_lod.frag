#version 120
uniform sampler2D tex0;
varying vec2 v_tex_coord;
void main() {
    gl_FragColor = texture2DProj(tex0, vec3(v_tex_coord, 1.0)) + texture2DLod(tex0, v_tex_coord, 2.0);
}
