@echo off
rem usage: run-probe.bat <exe> ; runs the probe with the census module list
set /p PROBE_MODS=<C:\spike\ws\mods.txt
%1
