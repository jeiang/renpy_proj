#version 120
uniform vec4 u_multiply;
uniform vec4 u_screen;

void main() {

        gl_FragColor.rgb = gl_FragColor.rgb * u_multiply.rgb;
        gl_FragColor.rgb = (gl_FragColor.rgb + u_screen.rgb * gl_FragColor.a) - (gl_FragColor.rgb * u_screen.rgb);
    }
