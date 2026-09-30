@echo off
rem Static-CRT (/MT) libffi and OpenSSL for the self-built CPython variant.
call C:\spike\env.bat
C:\spike\vcpkg\vcpkg.exe install libffi:x64-windows-static openssl:x64-windows-static
echo VCPKG_MT_DONE
