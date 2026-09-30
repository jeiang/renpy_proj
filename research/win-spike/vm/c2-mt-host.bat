@echo off
rem Approach (b): self-compiled CPython 3.12.8 (/MT, static) + PBS-independent Cython objects, linked into a +crt-static Rust host.
call C:\spike\env.bat
set OUT=C:\spike\out\mt
mkdir %OUT% 2>nul
rem 1. archive the /MT CPython objects
dir /b C:\spike\out\b\*.obj > C:\spike\out\mt.tmp
del %OUT%\py.rsp 2>nul
for /f %%i in (C:\spike\out\mt.tmp) do @echo C:\spike\out\b\%%i>> %OUT%\py.rsp
lib /nologo /OUT:%OUT%\python312.lib @%OUT%\py.rsp
rem 2. Cython C -> /MT objects against the same 3.12.8 headers
cd C:\spike\renpy-src
set CRT=/MT
set PY_STATIC=1
python C:\spike\ws\vm\b1_compile.py C:\spike\Python-3.12.8\Include,C:\spike\Python-3.12.8\PC C:\spike\out\cy_mt > C:\spike\out\cy_mt.log
dir /b C:\spike\out\cy_mt\*.obj > C:\spike\out\mt2.tmp
del %OUT%\cy.rsp 2>nul
for /f %%i in (C:\spike\out\mt2.tmp) do @echo C:\spike\out\cy_mt\%%i>> %OUT%\cy.rsp
lib /nologo /OUT:%OUT%\cy.lib @%OUT%\cy.rsp
rem 3. empty pythonXY.lib, pyo3 config, host
echo /* empty */ > %OUT%\empty.c
cl /nologo /c /Fo%OUT%\empty.obj %OUT%\empty.c
lib /nologo /OUT:%OUT%\pythonXY.lib %OUT%\empty.obj
(echo implementation=CPython& echo version=3.12& echo shared=false& echo abi3=false& echo lib_name=python312& echo lib_dir=%OUT%& echo pointer_width=64& echo build_flags=& echo suppress_build_script_link_lines=true) > %OUT%\pyo3-config.txt
set RUSTFLAGS=-C target-feature=+crt-static
set PYLIB_NAMES=python312 cy
for %%f in (ffi libssl libcrypto) do copy /y C:\spike\vcpkg\installed\x64-windows-static\lib\%%f.lib %OUT%\ >nul
set EXTRA_LIBS=winmm comdlg32 ffi libssl libcrypto crypt32 gdi32 oleaut32 ole32 user32
set MODS=_socket select unicodedata pyexpat _elementtree _overlapped _asyncio _decimal _multiprocessing _queue _zoneinfo _uuid _ctypes _bz2 _ssl _hashlib
set CYM=
for /f %%i in ('dir /b C:\spike\out\cy_mt\*.obj ^| findstr /v "\.x0\."') do @call set CYM=%%CYM%% %%~ni
set MODS=%MODS% %CYM%
python C:\spike\ws\mkzip.py C:\spike\pbs\install\Lib C:\spike\blob\stdlib.zip C:\spike\ws\probe_main.py C:\spike\ws\cy_probe.py C:\spike\ws\probe_pkg C:\spike\renpy-src\renpy C:\spike\ws\vcv\renpy C:\spike\tools\py312\Lib\site-packages\ecdsa C:\spike\tools\py312\Lib\site-packages\six.py
call C:\spike\ws\vm\a3-host.bat %OUT%
