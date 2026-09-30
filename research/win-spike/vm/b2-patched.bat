@echo off
call C:\spike\env.bat
cd C:\spike\renpy-src
python C:\spike\ws\vm\b2_patch.py
set RENPY_STATIC=1
set RENPY_REGENERATE_CYTHON=1
set RENPY_CYTHON=C:\spike\tools\py312\Scripts\cython.exe
python C:\spike\gen_cython.py > C:\spike\out\gen2.log 2>&1
python C:\spike\ws\vm\b1_compile.py C:\spike\pbs\install\include C:\spike\out\cy renpy.display.accelerator renpy.gl2.gl2model
