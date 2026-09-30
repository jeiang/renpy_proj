@echo off
rem Static libffi + OpenSSL for the builtin _ctypes/_ssl/_hashlib (PBS Windows objects import them from DLLs).
call C:\spike\env.bat
cd C:\spike
if not exist vcpkg git clone --depth 1 https://github.com/microsoft/vcpkg.git
call vcpkg\bootstrap-vcpkg.bat -disableMetrics
vcpkg\vcpkg.exe install libffi:x64-windows-static-md openssl:x64-windows-static-md
echo VCPKG_DONE
