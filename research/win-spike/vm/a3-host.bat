@echo off
rem Build the Rust host: %1 = out dir holding python312.lib (+ optional extra libs), inittab modules in and env MODS (module names for the inittab)
call C:\spike\env.bat
set OUT=%1
if defined RUSTFLAGS echo RUSTFLAGS=%RUSTFLAGS%
set BLOB_DIR=C:\spike\blob
set PYO3_CONFIG_FILE=%OUT%\pyo3-config.txt
set PYLIB_DIR=%OUT%
set CARGO_TARGET_DIR=C:\spike\target\%~n1
python C:\spike\ws\mkinittab.py C:\spike\ws\host\src\inittab_gen.rs %MODS%
cd C:\spike\ws\host
cargo build --release 2>&1
