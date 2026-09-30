# usage: measure.ps1 <exe> ; wall time of 20 runs of (a) interpreter start + bootstrap + `pass`, (b) the 104-module census import
param([string]$exe)
$env:PROBE_MODS = (Get-Content C:\spike\ws\mods.txt -Raw).Trim()
$env:PROBE_EXEC = 'C:\spike\empty.py'
$t = 1..20 | % { (Measure-Command { & $exe | Out-Null }).TotalMilliseconds }
"empty script: median {0:N1} ms  min {1:N1}  max {2:N1}" -f ($t | sort)[10], ($t | sort)[0], ($t | sort)[-1]
Remove-Item Env:PROBE_EXEC
$t = 1..20 | % { (Measure-Command { & $exe probe_main 2>&1 | Out-Null }).TotalMilliseconds }
"census import script: median {0:N1} ms  min {1:N1}  max {2:N1}" -f ($t | sort)[10], ($t | sort)[0], ($t | sort)[-1]
"size: {0:N2} MB" -f ((Get-Item $exe).Length / 1MB)
