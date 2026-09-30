@echo off
call C:\spike\env.bat
copy /y C:\spike\pbs\build\lib\liblzma.lib C:\spike\out\a\ >nul
set PYLIB_NAMES=python312 pbsext
set EXTRA_LIBS=liblzma winmm
set MODS=_socket select unicodedata _bz2 _lzma pyexpat _elementtree _overlapped _asyncio _decimal _multiprocessing _queue _zoneinfo _uuid
call C:\spike\ws\vm\a3-host.bat C:\spike\out\a
