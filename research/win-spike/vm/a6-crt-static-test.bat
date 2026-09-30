@echo off
rem Q3: does the /MD PBS object set link into a +crt-static (/MT) Rust host?
call C:\spike\env.bat
set RUSTFLAGS=-C target-feature=+crt-static
set OUT=C:\spike\out\a
set BLOB_DIR=C:\spike\blob
set PYO3_CONFIG_FILE=%OUT%\pyo3-config.txt
set PYLIB_DIR=%OUT%
set PYLIB_NAMES=python312
set CARGO_TARGET_DIR=C:\spike\target\a_mt
python C:\spike\ws\mkinittab.py C:\spike\ws\host\src\inittab_gen.rs
cd C:\spike\ws\host
cargo build --release 2>&1
