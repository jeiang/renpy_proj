@echo off
rem Approach (b) prep: fetch CPython externals, build stock pythoncore with PCbuild. Side effects we need: the generated frozen headers and
rem Python\deepfreeze\deepfreeze.c that the tarball does not contain. Also yields the stock /MD python312.dll for comparison.
call C:\spike\env.bat
cd C:\spike\Python-3.12.8
if not exist externals\zlib-1.3.1 call PCbuild\get_externals.bat
msbuild PCbuild\_freeze_module.vcxproj /m /p:Configuration=Release /p:Platform=x64 /v:m
msbuild PCbuild\pythoncore.vcxproj /m /p:Configuration=Release /p:Platform=x64 /v:m
echo C0_DONE
