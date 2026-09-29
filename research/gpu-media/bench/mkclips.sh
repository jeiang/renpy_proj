#!/bin/sh
# 6 s, 3840x2160@30, noisy content so decode cost is realistic (not trivially compressible).
src="mandelbrot=s=3840x2160:rate=30,noise=alls=25:allf=t,format=yuv420p"
ffmpeg -hide_banner -loglevel error -y -f lavfi -i "$src" -t 6 -c:v libvpx-vp9 -deadline realtime -cpu-used 8 -row-mt 1 -b:v 40M -pix_fmt yuv420p vp9_4k.webm
ffmpeg -hide_banner -loglevel error -y -f lavfi -i "$src" -t 6 -c:v libsvtav1 -preset 10 -b:v 40M -pix_fmt yuv420p av1_4k.mkv
ffmpeg -hide_banner -loglevel error -y -f lavfi -i "$src" -t 6 -c:v libx264 -preset ultrafast -b:v 40M h264_4k.mp4
