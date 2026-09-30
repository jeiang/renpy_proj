#version 120
uniform sampler2D tex0;
varying vec2 v_tex_coord;
void main() {
    if (gl_FragCoord.y < 10.0) discard;
    gl_FragColor = texture2D(tex0, v_tex_coord) * (gl_FragCoord.x > 100.0 ? 0.5 : 1.0);
}
