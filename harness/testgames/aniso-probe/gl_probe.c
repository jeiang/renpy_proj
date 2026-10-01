/* Raw OpenGL probe for the anisotropy investigation (harness/testgames/aniso/README.md, "Why Linux differed").
 *
 * The same picture as player/crates/gfx/examples/aniso_probe.rs (same textures, same tilted quad, same clip
 * coordinates), drawn with desktop GL through EGL without a window, with the sampler states that Mesa's radeonsi
 * can get from a GL program. Writes PPM files and prints the mean absolute difference between pairs.
 *
 *   gcc -O2 -o gl_probe gl_probe.c -lEGL -lGL -lm && ./gl_probe <out dir>
 */
#define GL_GLEXT_PROTOTYPES 1
#define EGL_EGLEXT_PROTOTYPES 1
#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <GL/gl.h>
#include <GL/glext.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define TEX 2048
#define W 1280
#define H 720
#ifndef GL_TEXTURE_MAX_ANISOTROPY_EXT
#define GL_TEXTURE_MAX_ANISOTROPY_EXT 0x84FE
#endif

typedef unsigned char u8;

static void checker(int x, int y, u8 *o) {
    if (x % 256 == 0 || y % 256 == 0) { o[0] = 255; o[1] = 0; o[2] = 0; o[3] = 255; return; }
    u8 v = ((x / 8 + y / 8) % 2 == 0) ? 230 : 25;
    o[0] = o[1] = o[2] = v; o[3] = 255;
}

static void grating(int x, int y, u8 *o) {
    int qx = x % (TEX / 2), qy = y % (TEX / 2), on;
    if (x < TEX / 2 && y < TEX / 2) on = qx % 4 < 2;
    else if (x >= TEX / 2 && y < TEX / 2) on = qy % 4 < 2;
    else if (x < TEX / 2) on = (qx + qy) % 6 < 3;
    else { float dx = qx - 512.f, dy = qy - 512.f; on = sinf((dx * dx + dy * dy) * 0.0016f) > 0.f; }
    u8 v = on ? 235 : 20;
    o[0] = o[1] = o[2] = v; o[3] = 255;
}

typedef struct { const char *name; GLenum min; float max_lod; float aniso; int max_level; float min_lod; float bias; } Cfg;

static double mad(const u8 *a, const u8 *b) {
    double s = 0;
    for (int i = 0; i < W * H * 3; i++) s += abs((int)a[i] - (int)b[i]);
    return s / (W * H * 3);
}

int main(int argc, char **argv) {
    const char *dir = argc > 1 ? argv[1] : NULL;
    PFNEGLGETPLATFORMDISPLAYEXTPROC getpd = (PFNEGLGETPLATFORMDISPLAYEXTPROC)eglGetProcAddress("eglGetPlatformDisplayEXT");
    EGLDisplay dpy = getpd(EGL_PLATFORM_SURFACELESS_MESA, EGL_DEFAULT_DISPLAY, NULL);
    if (!eglInitialize(dpy, NULL, NULL)) { fprintf(stderr, "eglInitialize failed\n"); return 1; }
    eglBindAPI(EGL_OPENGL_API);
    EGLint ca[] = { EGL_CONTEXT_MAJOR_VERSION, 3, EGL_CONTEXT_MINOR_VERSION, 2, EGL_CONTEXT_OPENGL_PROFILE_MASK, EGL_CONTEXT_OPENGL_COMPATIBILITY_PROFILE_BIT, EGL_NONE };
    EGLContext ctx = eglCreateContext(dpy, EGL_NO_CONFIG_KHR, EGL_NO_CONTEXT, ca);
    if (ctx == EGL_NO_CONTEXT) { fprintf(stderr, "no context (0x%x)\n", eglGetError()); return 1; }
    if (!eglMakeCurrent(dpy, EGL_NO_SURFACE, EGL_NO_SURFACE, ctx)) { fprintf(stderr, "make current failed\n"); return 1; }
    printf("GL_RENDERER %s\nGL_VERSION %s\n", glGetString(GL_RENDERER), glGetString(GL_VERSION));

    GLuint fbo, rb;
    glGenFramebuffers(1, &fbo); glBindFramebuffer(GL_FRAMEBUFFER, fbo);
    glGenRenderbuffers(1, &rb); glBindRenderbuffer(GL_RENDERBUFFER, rb);
    glRenderbufferStorage(GL_RENDERBUFFER, GL_RGBA8, W, H);
    glFramebufferRenderbuffer(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_RENDERBUFFER, rb);
    glViewport(0, 0, W, H);

    /* GL_LINEAR is what Ren'Py sets for gl_texture_scaling "linear". The others give the sampler a mip filter. */
    Cfg cfgs[] = {
        { "gl_linear_a1", GL_LINEAR, 1000, 1, 1000, 0, 0 },
        { "gl_linear_a16", GL_LINEAR, 1000, 16, 1000, 0, 0 },
        { "gl_linear_a8", GL_LINEAR, 1000, 8, 1000, 0, 0 },
        { "gl_linear_a4", GL_LINEAR, 1000, 4, 1000, 0, 0 },
        { "gl_linear_a2", GL_LINEAR, 1000, 2, 1000, 0, 0 },
        { "gl_lmn_lod0_a16", GL_LINEAR_MIPMAP_NEAREST, 0, 16, 1000, 0, 0 },
        { "gl_lml_lod0_a16", GL_LINEAR_MIPMAP_LINEAR, 0, 16, 1000, 0, 0 },
        { "gl_lml_lod0_a1", GL_LINEAR_MIPMAP_LINEAR, 0, 1, 1000, 0, 0 },
        { "gl_lml_maxlevel0_a16", GL_LINEAR_MIPMAP_LINEAR, 1000, 16, 0, 0, 0 },
        { "gl_lml_a16", GL_LINEAR_MIPMAP_LINEAR, 1000, 16, 1000, 0, 0 },
        { "gl_lml_a1", GL_LINEAR_MIPMAP_LINEAR, 1000, 1, 1000, 0, 0 },
        { "gl_lmn_a16", GL_LINEAR_MIPMAP_NEAREST, 1000, 16, 1000, 0, 0 },
        { "gl_lml_lod0p5_a16", GL_LINEAR_MIPMAP_LINEAR, 0.5f, 16, 1000, 0, 0 },
        { "gl_lml_lod1_a16", GL_LINEAR_MIPMAP_LINEAR, 1, 16, 1000, 0, 0 },
        { "gl_lml_lod2_a16", GL_LINEAR_MIPMAP_LINEAR, 2, 16, 1000, 0, 0 },
        { "gl_lml_fix1_a16", GL_LINEAR_MIPMAP_LINEAR, 1, 16, 1000, 1, 0 },
        { "gl_lml_fix2_a16", GL_LINEAR_MIPMAP_LINEAR, 2, 16, 1000, 2, 0 },
        { "gl_lmn_fix1_a16", GL_LINEAR_MIPMAP_NEAREST, 1, 16, 1000, 1, 0 },
        { "gl_lmn_fix2_a16", GL_LINEAR_MIPMAP_NEAREST, 2, 16, 1000, 2, 0 },
        { "gl_lmn_lod1_a16", GL_LINEAR_MIPMAP_NEAREST, 1, 16, 1000, 0, 0 },
        { "gl_lml0_bias-1", GL_LINEAR_MIPMAP_LINEAR, 0, 16, 1000, 0, -1 },
        { "gl_lml0_bias-.5", GL_LINEAR_MIPMAP_LINEAR, 0, 16, 1000, 0, -0.5f },
        { "gl_lml0_bias+.5", GL_LINEAR_MIPMAP_LINEAR, 0, 16, 1000, 0, 0.5f },
        { "gl_lml0_bias+1", GL_LINEAR_MIPMAP_LINEAR, 0, 16, 1000, 0, 1 },
        { "gl_linear_bias-.5", GL_LINEAR, 1000, 16, 1000, 0, -0.5f },
        { "gl_linear_bias+.5", GL_LINEAR, 1000, 16, 1000, 0, 0.5f },
    };
    int ncfg = sizeof cfgs / sizeof *cfgs;
    u8 **pics = malloc(sizeof(u8 *) * ncfg);
    u8 *rgba = malloc(W * H * 4);
    u8 *l0 = malloc((size_t)TEX * TEX * 4);

    for (int t = 0; t < 2; t++) {
        const char *tname = t ? "grating" : "checker";
        for (int y = 0; y < TEX; y++)
            for (int x = 0; x < TEX; x++) (t ? grating : checker)(x, y, l0 + ((size_t)y * TEX + x) * 4);
        GLuint tex;
        glGenTextures(1, &tex); glBindTexture(GL_TEXTURE_2D, tex);
        glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA8, TEX, TEX, 0, GL_RGBA, GL_UNSIGNED_BYTE, l0);
        glGenerateMipmap(GL_TEXTURE_2D);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
        glEnable(GL_TEXTURE_2D);
        glTexEnvi(GL_TEXTURE_ENV, GL_TEXTURE_ENV_MODE, GL_REPLACE);
        glMatrixMode(GL_PROJECTION); glLoadIdentity();
        glMatrixMode(GL_MODELVIEW); glLoadIdentity();
        for (int c = 0; c < ncfg; c++) {
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, cfgs[c].min);
            glTexParameterf(GL_TEXTURE_2D, GL_TEXTURE_MAX_ANISOTROPY_EXT, cfgs[c].aniso);
            glTexParameterf(GL_TEXTURE_2D, GL_TEXTURE_MAX_LOD, cfgs[c].max_lod);
            glTexParameterf(GL_TEXTURE_2D, GL_TEXTURE_MIN_LOD, cfgs[c].min_lod);
            glTexParameterf(GL_TEXTURE_2D, GL_TEXTURE_LOD_BIAS, cfgs[c].bias);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAX_LEVEL, cfgs[c].max_level);
            glClearColor(0.44f, 0.44f, 0.44f, 1); glClear(GL_COLOR_BUFFER_BIT);
            static const float cx[6] = { -1, 1, -1, -1, 1, 1 }, cy[6] = { -1, -1, 1, 1, -1, 1 };
            glBegin(GL_TRIANGLES);
            for (int i = 0; i < 6; i++) {
                float qx = cx[i] * 307.f, qy = cy[i] * 307.f, a = 72.f * (float)M_PI / 180.f;
                float y = qy * cosf(a), z = qy * sinf(a), f = 600.f, w = (f + z) / f;
                glTexCoord2f(cx[i] * 0.5f + 0.5f, cy[i] * 0.5f + 0.5f);
                glVertex4f(qx / 640.f, -y / 360.f, 0.5f * w, w);
            }
            glEnd();
            glReadPixels(0, 0, W, H, GL_RGBA, GL_UNSIGNED_BYTE, rgba);
            u8 *rgb = malloc(W * H * 3);
            for (int r = 0; r < H; r++)
                for (int p = 0; p < W; p++) memcpy(rgb + ((size_t)r * W + p) * 3, rgba + ((size_t)(H - 1 - r) * W + p) * 4, 3);
            pics[c] = rgb;
            if (dir) {
                char path[512]; snprintf(path, sizeof path, "%s/%s_%s.ppm", dir, tname, cfgs[c].name);
                FILE *f = fopen(path, "wb");
                if (f) { fprintf(f, "P6\n%d %d\n255\n", W, H); fwrite(rgb, 1, W * H * 3, f); fclose(f); }
            }
        }
        printf("== %s (mean abs difference, 0..255, whole %dx%d picture)\n", tname, W, H);
        printf("%22s %10s %10s\n", "", "vs linear16", "vs lml0_16");
        for (int a = 0; a < ncfg; a++)
            printf("%22s %10.3f %10.3f\n", cfgs[a].name, mad(pics[a], pics[1]), mad(pics[a], pics[6]));
        for (int c = 0; c < ncfg; c++) free(pics[c]);
        glDeleteTextures(1, &tex);
    }
    return 0;
}
