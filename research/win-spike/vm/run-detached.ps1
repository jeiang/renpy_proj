# usage (on VM): powershell -NoProfile -File C:\spike\run-detached.ps1 <script.ps1> [args...]
# Runs a script as a one-shot SYSTEM scheduled task so it survives the SSH session; poll its log file.
param([string]$Script,[string]$ScriptArgs='')
$n='spike_'+[IO.Path]::GetFileNameWithoutExtension($Script)
schtasks /Delete /TN $n /F 2>$null | Out-Null
schtasks /Create /TN $n /SC ONCE /ST 23:59 /RU SYSTEM /RL HIGHEST /F /TR "powershell -NoProfile -ExecutionPolicy Bypass -File $Script $ScriptArgs" | Out-Null
schtasks /Run /TN $n
