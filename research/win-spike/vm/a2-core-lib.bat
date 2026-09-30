@echo off
rem Approach (a): archive PBS's 210 core COFF objects into python312.lib, build host against it via pyo3-ffi (shared=false).
call C:\spike\env.bat
set OUT=C:\spike\out\a
mkdir %OUT% 2>nul
cd C:\spike\pbs
dir /b build\core\*.obj > %OUT%\core.rsp
for /f %%i in (%OUT%\core.rsp) do @echo build\core\%%i>> %OUT%\core2.rsp
lib /nologo /OUT:%OUT%\python312.lib @%OUT%\core2.rsp
(echo implementation=CPython& echo version=3.12& echo shared=false& echo abi3=false& echo lib_name=python312& echo lib_dir=%OUT%& echo pointer_width=64& echo build_flags=& echo suppress_build_script_link_lines=true) > %OUT%\pyo3-config.txt
dir %OUT%\python312.lib
rem pyo3-ffi still emits `link(name = "pythonXY")` on Windows; satisfy it with an empty lib (all symbols come from python312.lib)
echo /* empty */ > %OUT%\empty.c
cl /nologo /c /Fo%OUT%\empty.obj %OUT%\empty.c
lib /nologo /OUT:%OUT%\pythonXY.lib %OUT%\empty.obj
