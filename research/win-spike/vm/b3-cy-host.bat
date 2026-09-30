@echo off
rem Link the MSVC-built Ren'Py Cython objects into the host next to the PBS-object CPython, embed renpy/*.py in the blob.
call C:\spike\env.bat
set OUT=C:\spike\out\c
mkdir %OUT% 2>nul
for %%f in (ffi libssl libcrypto) do copy /y C:\spike\vcpkg\installed\x64-windows-static-md\lib\%%f.lib C:\spike\out\a\ >nul
for %%f in (python312.lib pythonXY.lib pbsext.lib liblzma.lib ffi.lib libssl.lib libcrypto.lib) do copy /y C:\spike\out\a\%%f %OUT%\ >nul
(echo implementation=CPython& echo version=3.12& echo shared=false& echo abi3=false& echo lib_name=python312& echo lib_dir=%OUT%& echo pointer_width=64& echo build_flags=& echo suppress_build_script_link_lines=true) > %OUT%\pyo3-config.txt
dir /b C:\spike\out\cy\*.obj > C:\spike\out\cy.rsp.tmp
del %OUT%\cy.rsp 2>nul
for /f %%i in (C:\spike\out\cy.rsp.tmp) do @echo C:\spike\out\cy\%%i>> %OUT%\cy.rsp
cl /nologo /c /Fo%OUT%\ffi_imp_shim.obj C:\spike\ws\vm\ffi_imp_shim.c
echo %OUT%\ffi_imp_shim.obj>> %OUT%\cy.rsp
lib /nologo /OUT:%OUT%\cy.lib @%OUT%\cy.rsp
rem blob = stdlib + probe scripts + renpy/*.py (Ren'Py's Python layer)
python C:\spike\ws\mkzip.py C:\spike\pbs\install\Lib C:\spike\blob\stdlib.zip C:\spike\ws\probe_main.py C:\spike\ws\cy_probe.py C:\spike\ws\probe_pkg C:\spike\renpy-src\renpy C:\spike\ws\vcv\renpy C:\spike\tools\py312\Lib\site-packages\ecdsa C:\spike\tools\py312\Lib\site-packages\six.py
set FORCE_MULTIPLE=1
set PYLIB_NAMES=python312 pbsext cy
set EXTRA_LIBS=liblzma winmm comdlg32 ffi libssl libcrypto crypt32 gdi32
set MODS=_socket select unicodedata _bz2 _lzma pyexpat _elementtree _overlapped _asyncio _decimal _multiprocessing _queue _zoneinfo _uuid _ctypes _hashlib _ssl
for /f %%i in ('dir /b C:\spike\out\cy\*.obj ^| findstr /v "\.x0\."') do @call set CYM=%%CYM%% %%~ni
set MODS=%MODS% %CYM%
echo %MODS% > %OUT%\mods.txt
call C:\spike\ws\vm\a3-host.bat %OUT%
