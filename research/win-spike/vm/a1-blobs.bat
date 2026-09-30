@echo off
rem Stdlib zip + frozen encodings + probe scripts, from the PBS 3.12.14 Lib tree (pure-Python part only).
call C:\spike\env.bat
set WS=C:\spike\ws
mkdir C:\spike\blob\boot 2>nul
python %WS%\mkboot.py C:\spike\pbs\install\Lib C:\spike\blob\boot
python %WS%\mkzip.py C:\spike\pbs\install\Lib C:\spike\blob\stdlib.zip %WS%\probe_main.py %WS%\probe_pkg
