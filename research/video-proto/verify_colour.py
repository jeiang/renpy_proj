#!/usr/bin/env python3
"""THROWAWAY: compare a dumped PPM (GPU shader output) with FFmpeg's swscale RGB for nearby frames of the same clip.
usage: verify_colour.py CLIP DUMP.ppm [first_frame last_frame]  (needs numpy; ffmpeg on PATH is a *reference tool only*)"""
import subprocess, sys, numpy as np
clip, dump = sys.argv[1], sys.argv[2]
lo, hi = (int(sys.argv[3]), int(sys.argv[4])) if len(sys.argv) > 4 else (90, 150)
W, H = 480, 270
def scaled(path_args):
    raw = subprocess.run(["ffmpeg","-v","error",*path_args,"-frames:v","1","-vf",f"scale={W}:{H}:flags=area,format=rgb24","-f","rawvideo","-"],capture_output=True).stdout
    return np.frombuffer(raw, np.uint8).reshape(H, W, 3).astype(float)
mine = scaled(["-i", dump])
best = None
for n in range(lo, hi):
    raw = subprocess.run(["ffmpeg","-v","error","-i",clip,"-vf",f"select=eq(n\\,{n}),scale={W}:{H}:flags=area:in_range=auto,format=rgb24","-frames:v","1","-f","rawvideo","-"],capture_output=True).stdout
    if len(raw) != W*H*3: continue
    ref = np.frombuffer(raw, np.uint8).reshape(H, W, 3).astype(float)
    d = np.abs(ref - mine).mean()
    if best is None or d < best[0]: best = (d, n, (ref-mine).mean(axis=(0,1)))
print(f"{clip.rsplit('/')[-1]}: best frame {best[1]}  mean|diff| {best[0]:.2f}/255  mean signed (R,G,B) {np.round(best[2],2)}")
