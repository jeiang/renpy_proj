@echo off
rem Starts the urp Windows test kit. Double-click this file.
rem It runs run-tests.ps1 with PowerShell, which ships with Windows 10 and 11.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0run-tests.ps1"
echo.
echo The test run has finished. Send the results-*.zip file it made back to the tester contact.
pause
