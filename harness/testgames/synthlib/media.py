"""Media generation with an LGPL FFmpeg (`nix build .#ffmpeg-synth`): image formats and movies.

The ffmpeg is a generator only; nothing links it. `find_ffmpeg` refuses a GPL build (it prints `--enable-gpl` in its
configuration) unless SYNTH_ALLOW_GPL=1 is set, so the corpus is always made by the licence-clean tool.
"""
import os
import pathlib
import shutil
import subprocess

ENCODERS = {"vp9": "libvpx-vp9", "opus": "libopus", "h264": "libopenh264", "theora": "libtheora", "av1": "libaom-av1",
            "webp": "libwebp", "mjpeg": "mjpeg", "gif": "gif"}
_ff = None


def find_ffmpeg():
    global _ff
    if _ff:
        return _ff
    cands = [os.environ.get("FFMPEG_SYNTH"), shutil.which("ffmpeg")]
    for c in cands:
        if not c:
            continue
        p = pathlib.Path(c)
        if p.is_dir():
            p = p / "bin" / "ffmpeg"
        if not p.exists():
            continue
        cfg = subprocess.run([str(p), "-hide_banner", "-version"], capture_output=True, text=True).stdout
        if "--enable-gpl" in cfg and os.environ.get("SYNTH_ALLOW_GPL") != "1":
            continue
        _ff = str(p)
        return _ff
    raise SystemExit("no LGPL ffmpeg with the codec libraries found: run `nix build .#ffmpeg-synth -o /tmp/ffmpeg-synth` and "
                     "set FFMPEG_SYNTH=/tmp/ffmpeg-synth (or SYNTH_ALLOW_GPL=1 to accept a GPL build as a generator)")


def run(args, **kw):
    r = subprocess.run([find_ffmpeg(), "-y", "-v", "error", "-hide_banner"] + [str(a) for a in args], capture_output=True, text=True, **kw)
    if r.returncode:
        raise SystemExit("ffmpeg failed: %s\n%s" % (" ".join(map(str, args)), r.stderr))


def have(encoder):
    out = subprocess.run([find_ffmpeg(), "-hide_banner", "-encoders"], capture_output=True, text=True).stdout
    return (" %s " % encoder) in out


def image(src_png, dest, fmt):
    """Encode `src_png` as jpg, webp, avif or gif."""
    dest = pathlib.Path(dest)
    dest.parent.mkdir(parents=True, exist_ok=True)
    if fmt == "jpg":
        run(["-i", src_png, "-c:v", "mjpeg", "-q:v", 2, "-pix_fmt", "yuvj444p", "-frames:v", 1, dest])
    elif fmt == "webp":
        run(["-i", src_png, "-c:v", "libwebp", "-quality", 95, "-frames:v", 1, dest])
    elif fmt == "avif":
        run(["-i", src_png, "-c:v", "libaom-av1", "-crf", 12, "-cpu-used", 8, "-still-picture", 1, "-pix_fmt", "yuv444p", "-frames:v", 1, "-f", "avif", dest])
    elif fmt == "gif":
        run(["-i", src_png, "-frames:v", 1, dest])
    else:
        raise ValueError(fmt)


def movie(src_png, dest, vcodec, secs=3, fps=24, audio=None, size=None):
    """A movie of one still image repeated, so every frame shows the same picture (a screenshot does not depend on
    the moment it is taken). `audio`: 'opus' adds a 440 Hz sine tone; None writes no audio track."""
    dest = pathlib.Path(dest)
    dest.parent.mkdir(parents=True, exist_ok=True)
    args = ["-loop", 1, "-framerate", fps, "-i", src_png]
    if audio:
        args += ["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000"]
    vf = "format=yuv420p" if not size else "scale=%d:%d,format=yuv420p" % size
    args += ["-t", secs, "-vf", vf, "-r", fps]
    if vcodec == "vp9":
        args += ["-c:v", "libvpx-vp9", "-b:v", "600k", "-deadline", "good", "-cpu-used", 4]
    elif vcodec == "h264":
        args += ["-c:v", "libopenh264", "-b:v", "600k"]
    elif vcodec == "theora":
        args += ["-c:v", "libtheora", "-q:v", 8]
    elif vcodec == "av1":
        args += ["-c:v", "libaom-av1", "-crf", 30, "-b:v", 0, "-cpu-used", 8]
    else:
        raise ValueError(vcodec)
    if audio == "opus":
        args += ["-c:a", "libopus", "-b:a", "64k", "-shortest"]
    elif audio == "vorbis":
        args += ["-c:a", "libvorbis", "-shortest"]
    run(args + [dest])
