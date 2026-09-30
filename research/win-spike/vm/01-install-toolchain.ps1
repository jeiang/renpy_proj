# Run detached on the VM: installs VS 2022 Build Tools (VCTools), Git for Windows, rustup (MSVC), Python 3.12.8.
Start-Transcript -Path C:\spike\install.log -Force
$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = 'Tls12'
$ProgressPreference = 'SilentlyContinue'
New-Item -ItemType Directory -Force C:\spike\dl | Out-Null
cd C:\spike\dl
function Get($u,$o){ if(!(Test-Path $o)){ Invoke-WebRequest -UseBasicParsing $u -OutFile $o } }
Get https://aka.ms/vs/17/release/vs_BuildTools.exe vs_BuildTools.exe
Get https://github.com/git-for-windows/git/releases/download/v2.56.0.windows.1/Git-2.56.0-64-bit.exe Git-2.56.0-64-bit.exe
Get https://win.rustup.rs/x86_64 rustup-init.exe
Get https://www.python.org/ftp/python/3.12.8/python-3.12.8-amd64.exe python-3.12.8-amd64.exe
"downloads done"
Start-Process .\Git-2.56.0-64-bit.exe -ArgumentList '/VERYSILENT','/NORESTART','/SP-' -Wait
"git done"
Start-Process .\python-3.12.8-amd64.exe -ArgumentList '/quiet','InstallAllUsers=1','TargetDir=C:\spike\tools\py312','PrependPath=0','Include_test=0' -Wait
"python done"
Start-Process .\vs_BuildTools.exe -ArgumentList '--add','Microsoft.VisualStudio.Workload.VCTools','--includeRecommended','--quiet','--wait','--norestart','--nocache' -Wait
"vs done"
$env:RUSTUP_HOME='C:\spike\tools\rustup'; $env:CARGO_HOME='C:\spike\tools\cargo'
Start-Process .\rustup-init.exe -ArgumentList '-y','--default-host','x86_64-pc-windows-msvc','--profile','minimal' -Wait
"rust done"
"ALL DONE"
