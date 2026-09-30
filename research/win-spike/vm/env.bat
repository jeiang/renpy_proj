@echo off
rem Build environment used for every step on the VM (VS 2022 Build Tools 14.44 x64 + rust + py312).
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
set RUSTUP_HOME=C:\spike\tools\rustup
set CARGO_HOME=C:\spike\tools\cargo
set PATH=C:\spike\tools\cargo\bin;C:\spike\tools\py312;C:\spike\tools\py312\Scripts;C:\Program Files\Git\cmd;%PATH%
set CARGO_BUILD_JOBS=4
set PYTHONUTF8=1
