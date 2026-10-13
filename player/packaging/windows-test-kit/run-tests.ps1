<#
.SYNOPSIS
    Runs urp.exe against the synthetic test games and writes a results folder and zip.

.DESCRIPTION
    Started by run-tests.bat. For each game it runs these stages when the game has them:
      lint         urp lints the game; the "Statistics:" line must appear.
      patch-seed   Synth7Patch only: builds the patch library the probe needs.
      probe        a scripted run to the main menu and through the first lines of story.
      checks       engine checks on the probe reports (Ren'Py 7 and engine-copy games).
      route        the game's own route plan, with screenshots (best effort).
      save-resume  a save at a set line, then a second launch that loads the save.
    The kit contains no developer tools. Each game runs in its own copy under work\, and
    its saves and logs live under data\, so the kit folder stays clean.

.PARAMETER Only
    Comma-separated game ids to run. Default: every game in games.json.

.PARAMETER Exe
    Path to urp.exe. Default: urp\urp.exe in this kit.
#>
[CmdletBinding()]
param(
    [string]$Only = '',
    [string]$Exe = ''
)

$ErrorActionPreference = 'Stop'
$Kit = $PSScriptRoot
if ($PSVersionTable.PSVersion.Major -lt 5) {
    Write-Host 'This runner needs Windows PowerShell 5.1 or later.'
    exit 2
}
if ($Exe -eq '') { $Exe = [System.IO.Path]::Combine($Kit, 'urp', 'urp.exe') }
# Windows uses System.Drawing for screenshots and CIM for hardware facts. The Linux test runs use grim and the
# environment facts only. The OS decides, so the same runner file works on both.
$OnWindows = ([System.Environment]::OSVersion.Platform -eq [System.PlatformID]::Win32NT)
$HarnessRpy = [System.IO.Path]::Combine($Kit, 'harness', 'zz_harness.rpy')
$Results = [System.IO.Path]::Combine($Kit, 'results')
$Work = [System.IO.Path]::Combine($Kit, 'work')
$Data = [System.IO.Path]::Combine($Kit, 'data')
$LogsDir = [System.IO.Path]::Combine($Results, 'logs')
$ShotDir = [System.IO.Path]::Combine($Results, 'shots')
$StageFiles = [System.IO.Path]::Combine($Results, 'stagefiles')
$AppData = [System.IO.Path]::Combine($Data, 'appdata')
$Utf8 = New-Object System.Text.UTF8Encoding -ArgumentList $false
$script:Rows = New-Object System.Collections.ArrayList
$script:SeedOk = @{}
$script:ProbeOk = @{}
$script:Current = $null
$script:Detail = ''
$script:Warn = $false
$script:Notes = $null

# Stage time limits in seconds. games.json can override them per game ("stages").
$Defaults = @{
    'boot' = 60; 'menu' = 60; 'ack' = 10; 'done' = 10; 'save' = 30; 'start' = 30; 'jump' = 30;
    'loaded' = 30; 'say' = 15; 'quit' = 45; 'lint-boot' = 60; 'lint' = 900
}

function PathJoin([string[]]$Parts) {
    $r = $Parts[0]
    for ($i = 1; $i -lt $Parts.Count; $i++) { $r = [System.IO.Path]::Combine($r, $Parts[$i]) }
    return $r
}

function New-Dir([string]$Path) {
    [void][System.IO.Directory]::CreateDirectory($Path)
}

function Log([string]$Message) {
    $line = (Get-Date -Format 'HH:mm:ss') + ' ' + $Message
    Write-Host $line
    if ($script:RunLog) { [System.IO.File]::AppendAllText($script:RunLog, $line + "`r`n", $Utf8) }
}

function Get-Limit($g, [string]$Name) {
    $s = $g.stages
    if ($s -and $s.PSObject.Properties[$Name]) { return [double]$s.$Name }
    return [double]$Defaults[$Name]
}

function Copy-Tree([string]$Src, [string]$Dst) {
    if (Test-Path -LiteralPath $Dst) { Remove-Item -LiteralPath $Dst -Recurse -Force }
    New-Dir (Split-Path -Parent $Dst)
    Copy-Item -LiteralPath $Src -Destination $Dst -Recurse -Force
}

function Read-Shared([string]$Path) {
    if (-not [System.IO.File]::Exists($Path)) { return '' }
    try {
        $share = [System.IO.FileShare]([int][System.IO.FileShare]::ReadWrite -bor [int][System.IO.FileShare]::Delete)
        $fs = New-Object System.IO.FileStream -ArgumentList $Path, ([System.IO.FileMode]::Open), ([System.IO.FileAccess]::Read), $share
        $sr = New-Object System.IO.StreamReader -ArgumentList $fs, ([System.Text.Encoding]::UTF8)
        try { return $sr.ReadToEnd() } finally { $sr.Dispose() }
    } catch {
        return ''
    }
}

function New-Progress([string]$File) {
    return @{
        File = $File; Pos = [long]0; Partial = ''
        Dec = [System.Text.Encoding]::UTF8.GetDecoder()
        Lines = New-Object System.Collections.ArrayList
    }
}

# Reads the bytes the game appended to progress.txt since the last call and splits them into lines.
function Update-Progress($p) {
    if (-not [System.IO.File]::Exists($p.File)) { return }
    $fs = $null
    $n = 0
    $buf = $null
    try {
        $share = [System.IO.FileShare]([int][System.IO.FileShare]::ReadWrite -bor [int][System.IO.FileShare]::Delete)
        $fs = New-Object System.IO.FileStream -ArgumentList $p.File, ([System.IO.FileMode]::Open), ([System.IO.FileAccess]::Read), $share
    } catch {
        return
    }
    try {
        $len = $fs.Length
        if ($len -lt $p.Pos) {
            $p.Pos = [long]0; $p.Partial = ''
            $p.Dec = [System.Text.Encoding]::UTF8.GetDecoder()
        }
        $avail = [int]($len - $p.Pos)
        if ($avail -gt 0) {
            $null = $fs.Seek($p.Pos, [System.IO.SeekOrigin]::Begin)
            $buf = [byte[]]::new($avail)
            $n = $fs.Read($buf, 0, $avail)
        }
    } finally {
        $fs.Close()
    }
    if ($n -le 0) { return }
    $p.Pos = $p.Pos + $n
    $cc = $p.Dec.GetCharCount($buf, 0, $n)
    $chars = [char[]]::new($cc)
    $null = $p.Dec.GetChars($buf, 0, $n, $chars, 0)
    $text = $p.Partial + [string]::new($chars)
    $parts = $text.Split([char]10)
    $p.Partial = $parts[$parts.Length - 1]
    for ($i = 0; $i -lt $parts.Length - 1; $i++) {
        [void]$p.Lines.Add($parts[$i].TrimEnd([char]13))
    }
}

# Waits for a progress line equal to $token, or starting with "$token ", from index $from.
# $gap = the timer restarts on every new line (used for advance-done, which runs by say lines).
# A "cmd-error" line fails the wait at once. Returns the matched index.
function Wait-Line($ctx, [string]$token, [int]$from, [double]$seconds, [bool]$gap) {
    $deadline = [DateTime]::UtcNow.AddSeconds($seconds)
    $idx = $from
    while ($true) {
        $exited = $ctx.Proc.HasExited
        Update-Progress $ctx.Progress
        $lines = $ctx.Progress.Lines
        while ($idx -lt $lines.Count) {
            $l = [string]$lines[$idx]
            $idx++
            if ($l -eq $token -or $l.StartsWith($token + ' ')) {
                return ($idx - 1)
            }
            if ($l.StartsWith('cmd-error') -and -not $token.StartsWith('cmd-error')) {
                throw ('the game reported: ' + $l)
            }
            if ($gap) { $deadline = [DateTime]::UtcNow.AddSeconds($seconds) }
        }
        if ($exited) {
            throw ('the player exited with code ' + $ctx.Proc.ExitCode + " while waiting for '" + $token + "'")
        }
        if ([DateTime]::UtcNow -gt $deadline) {
            throw ("no '" + $token + "' within " + $seconds + ' s')
        }
        Start-Sleep -Milliseconds 200
    }
}

# Waits for a state line from the cursor $ctx.Seen, then moves the cursor past it. Each state line is matched once.
function Wait-State($ctx, [string]$token, [double]$seconds, [bool]$gap) {
    $ctx.Seen = (Wait-Line $ctx $token $ctx.Seen $seconds $gap) + 1
}

# Writes one command to cmd.txt. The game deletes the file once it has read the command.
function Send-Cmd($ctx, [string]$text) {
    $cmdFile = PathJoin @($ctx.Hz, 'cmd.txt')
    $tmpFile = PathJoin @($ctx.Hz, 'cmd.tmp')
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    while ([System.IO.File]::Exists($cmdFile)) {
        if ($ctx.Proc.HasExited) { throw 'the player exited while a command was still pending' }
        if ([DateTime]::UtcNow -gt $deadline) { throw 'the game did not take the previous command within 30 s' }
        Start-Sleep -Milliseconds 100
    }
    [System.IO.File]::WriteAllText($tmpFile, $text + "`n", $Utf8)
    Move-Item -LiteralPath $tmpFile -Destination $cmdFile -Force
}

function Run-Cmd($ctx, [string]$text) {
    $text = $text.Trim()
    $parts = $text -split '\s+', 2
    $verb = $parts[0]
    $rest = ''
    if ($parts.Count -gt 1) { $rest = $parts[1] }
    if ($verb -eq 'movie') { throw 'the kit plans do not use movie commands' }
    Update-Progress $ctx.Progress
    $sent = $ctx.Progress.Lines.Count
    Send-Cmd $ctx $text
    $null = Wait-Line $ctx ('cmd-ack ' + $text) $sent (Get-Limit $ctx.Game 'ack') $false
    if ($verb -eq 'start') {
        $token = 'label start'; $key = 'start'
    } elseif ($verb -eq 'jump') {
        $token = 'label ' + $rest; $key = 'jump'
    } elseif ($verb -eq 'load') {
        $token = 'loaded'; $key = 'loaded'
    } elseif ($verb -eq 'save') {
        $token = 'saved ' + $rest; $key = 'save'
    } else {
        $token = 'cmd-done ' + $text; $key = 'done'
    }
    $null = Wait-Line $ctx $token $sent (Get-Limit $ctx.Game $key) $false
}

function Wait-Op($ctx, [string]$token) {
    $g = $ctx.Game
    if ($token.StartsWith('menu')) {
        $secs = Get-Limit $g 'menu'; $gap = $false
    } elseif ($token.StartsWith('advance-done')) {
        $secs = Get-Limit $g 'say'; $gap = $true
    } else {
        $secs = Get-Limit $g 'save'; $gap = $false
    }
    Wait-State $ctx $token $secs $gap
}

function Save-Shot([string]$Path) {
    if (-not $OnWindows) {
        # Linux test runs: grim captures the sway output of the slot (WAYLAND_DISPLAY is set by slotrun.py).
        & grim $Path
        if ($LASTEXITCODE -ne 0) { throw 'grim failed' }
        return
    }
    Add-Type -AssemblyName System.Drawing
    Add-Type -AssemblyName System.Windows.Forms
    $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bmp = New-Object System.Drawing.Bitmap -ArgumentList $b.Width, $b.Height
    try {
        $gr = [System.Drawing.Graphics]::FromImage($bmp)
        try { $gr.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size) } finally { $gr.Dispose() }
        $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $bmp.Dispose()
    }
}

function Take-Shot($ctx, [string]$arg) {
    $name = ($arg -split '\s+')[0]
    $path = PathJoin @($ShotDir, ($ctx.Name + '-' + $name + '.png'))
    try {
        New-Dir $ShotDir
        Save-Shot $path
        [void]$ctx.Notes.Add('shot ' + $name + ' saved')
    } catch {
        $ctx.Warn = $true
        [void]$ctx.Notes.Add('shot ' + $name + ' not taken (' + $_.Exception.Message + ')')
    }
}

function Quit-Player($ctx) {
    Update-Progress $ctx.Progress
    Send-Cmd $ctx 'quit'
    $secs = Get-Limit $ctx.Game 'quit'
    if (-not $ctx.Proc.WaitForExit([int]($secs * 1000))) {
        throw ("the game did not exit within " + $secs + " s of 'quit'")
    }
    Update-Progress $ctx.Progress
}

function After-Start($ctx) {
    $list = @($ctx.Game.after_start)
    if ($list.Count -eq 0) { return }
    Start-Sleep -Seconds 6
    foreach ($c in $list) {
        Run-Cmd $ctx ([string]$c)
        Start-Sleep -Seconds 1
    }
}

function Run-Ops($ctx, [string[]]$ops) {
    foreach ($op in $ops) {
        $parts = $op.Trim() -split '\s+', 2
        $verb = $parts[0]
        $arg = ''
        if ($parts.Count -gt 1) { $arg = $parts[1] }
        switch ($verb) {
            'cmd' { Run-Cmd $ctx $arg }
            'wait' { Wait-Op $ctx $arg }
            'settle' {
                Start-Sleep -Milliseconds ([int]([double]$arg * 1000))
                if ($ctx.Proc.HasExited) { throw 'the game exited during settle' }
            }
            'shot' { Take-Shot $ctx $arg }
            'note' { [void]$ctx.Notes.Add($arg) }
            'quit' { Quit-Player $ctx; return }
            'after_start' { After-Start $ctx }
            'end' { return }
            default { throw ("unknown plan op '" + $verb + "'") }
        }
    }
}

function Read-Ops([string]$Path, [string]$Lines) {
    $text = [System.IO.File]::ReadAllText($Path)
    $text = $text.Replace('{lines}', $Lines)
    $out = New-Object System.Collections.ArrayList
    foreach ($raw in ($text -split "`r?`n")) {
        $t = $raw.Trim()
        if ($t -eq '' -or $t.StartsWith('#')) { continue }
        [void]$out.Add($t)
    }
    return [string[]]$out.ToArray()
}

function Get-PlayerEnv($g, [string]$SavesDir, [string]$Hz, [bool]$Harness) {
    $after = @($g.after_start)
    $afterJson = '[]'
    if ($after.Count -gt 0) {
        $afterJson = '[' + (($after | ForEach-Object { '"' + [string]$_ + '"' }) -join ',') + ']'
    }
    $e = @{
        RENPY_PATH_TO_SAVES = $SavesDir
        PLAYER_COMPAT_NOTICE = 'off'
        RENPY_DISABLE_BACKUPS = 'I take responsibility for this.'
        HZ_INPUT_ANSWER = 'Tester'
        HZ_INPUT_EXPLICIT = '0'
        HZ_AFTER_START = $afterJson
        HZ_INPUT_LIMIT = '3'
        HZ_SCREEN_ACTIONS = ''
        HZ_DRIVER = ''
        APPDATA = $AppData
        LOCALAPPDATA = $AppData
        PYTHONUNBUFFERED = '1'
        HARNESS_DIR = $null
    }
    if ($Harness) { $e['HARNESS_DIR'] = $Hz }
    return $e
}

# Starts urp.exe with the given arguments. Environment changes are applied only while the
# process starts, then restored, so stages do not leak settings into each other.
function Start-Player([string]$ExePath, [string[]]$ArgList, [string]$WorkDir, $EnvVars, [string]$OutFile, [string]$ErrFile) {
    $saved = @{}
    foreach ($k in $EnvVars.Keys) {
        $saved[$k] = [System.Environment]::GetEnvironmentVariable($k, 'Process')
        [System.Environment]::SetEnvironmentVariable($k, [string]$EnvVars[$k], 'Process')
    }
    try {
        $quoted = @()
        foreach ($a in $ArgList) { $quoted += ('"' + $a + '"') }
        $p = Start-Process -FilePath $ExePath -ArgumentList ($quoted -join ' ') -WorkingDirectory $WorkDir -PassThru -RedirectStandardOutput $OutFile -RedirectStandardError $ErrFile
    } finally {
        foreach ($k in $EnvVars.Keys) {
            [System.Environment]::SetEnvironmentVariable($k, $saved[$k], 'Process')
        }
    }
    return $p
}

function Stop-Proc($p) {
    if ($p -and -not $p.HasExited) {
        try { $p.Kill() } catch { }
        $null = $p.WaitForExit(5000)
    }
}

function Open-Stage($g, [string]$stage, [string]$SrcBase, [string]$DataDir, [string[]]$RenpyArgs, [bool]$Harness) {
    $base = PathJoin @($Work, $g.id, $stage)
    Copy-Tree $SrcBase $base
    New-Dir $DataDir
    New-Dir ([System.IO.Path]::Combine($DataDir, 'logs'))
    $SavesDir = PathJoin @($DataDir, 'saves')
    New-Dir $SavesDir
    # The patch library is kept per game (see Stage-PatchSeed) and copied into every stage that loads it.
    $patchSrc = PathJoin @($Data, $g.id, 'patches')
    if (Test-Path -LiteralPath $patchSrc) {
        $patchDst = PathJoin @($DataDir, 'patches')
        New-Dir $patchDst
        Copy-Item -Path (PathJoin @($patchSrc, '*')) -Destination $patchDst -Recurse -Force
    }
    $hz = PathJoin @($Work, ('hz-' + $g.id + '-' + $stage))
    New-Dir $hz
    $name = $g.id + '-' + $stage
    $logBase = PathJoin @($LogsDir, $name)
    New-Dir $LogsDir
    $ctx = @{
        Game = $g; Stage = $stage; Name = $name; Base = $base; Data = $DataDir; Hz = $hz
        Progress = (New-Progress ([System.IO.Path]::Combine($hz, 'progress.txt')))
        Seen = 0; Notes = (New-Object System.Collections.ArrayList); Warn = $false; Proc = $null
        Out = ($logBase + '.out.txt'); Err = ($logBase + '.err.txt')
    }
    $argList = @($base, '--data', $DataDir, '--logdir', ([System.IO.Path]::Combine($DataDir, 'logs')), '--harness-script', $HarnessRpy)
    $argList += $RenpyArgs
    $envVars = Get-PlayerEnv $g $SavesDir $hz $Harness
    $ctx.Proc = Start-Player $Exe $argList $base $envVars $ctx.Out $ctx.Err
    $script:Current = $ctx
    return $ctx
}

function Test-Traceback($ctx) {
    if (Test-Path -LiteralPath (PathJoin @($ctx.Base, 'traceback.txt'))) { return $true }
    $found = @(Get-ChildItem -LiteralPath $ctx.Data -Recurse -Filter traceback.txt -ErrorAction SilentlyContinue)
    return ($found.Count -gt 0)
}

function Expect-Clean($ctx) {
    if (-not $ctx.Proc.HasExited) { throw 'the game was still running at the end of the plan' }
    $code = $ctx.Proc.ExitCode
    if ($code -ne 0) { throw ('the game exited with code ' + $code) }
    if (Test-Traceback $ctx) { throw 'traceback.txt was written' }
}

function Close-Stage($ctx) {
    if ($null -eq $ctx) { return }
    try {
        Stop-Proc $ctx.Proc
        Update-Progress $ctx.Progress
        $files = PathJoin @($StageFiles, $ctx.Name)
        New-Dir $files
        foreach ($f in @('log.txt', 'traceback.txt')) {
            $src = PathJoin @($ctx.Base, $f)
            if (Test-Path -LiteralPath $src) { Copy-Item -LiteralPath $src -Destination $files -Force }
        }
        if ([System.IO.File]::Exists($ctx.Progress.File)) {
            Copy-Item -LiteralPath $ctx.Progress.File -Destination $files -Force
        }
        $rep = PathJoin @($ctx.Data, 'reports')
        if (Test-Path -LiteralPath $rep) {
            Copy-Item -LiteralPath $rep -Destination (PathJoin @($files, 'reports')) -Recurse -Force
        }
        if (Test-Path -LiteralPath $ctx.Base) { Remove-Item -LiteralPath $ctx.Base -Recurse -Force -ErrorAction SilentlyContinue }
        if (Test-Path -LiteralPath $ctx.Hz) { Remove-Item -LiteralPath $ctx.Hz -Recurse -Force -ErrorAction SilentlyContinue }
    } catch {
        Log ('warning: cleanup of ' + $ctx.Name + ' failed: ' + $_.Exception.Message)
    }
}

function Get-Tail($ctx) {
    if ($null -eq $ctx) { return '' }
    $out = New-Object System.Collections.ArrayList
    $n = $ctx.Progress.Lines.Count
    $start = [Math]::Max(0, $n - 40)
    for ($i = $start; $i -lt $n; $i++) { [void]$out.Add('progress: ' + $ctx.Progress.Lines[$i]) }
    $errLines = @((Read-Shared $ctx.Err) -split "`r?`n" | Where-Object { $_ -ne '' })
    $errStart = [Math]::Max(0, $errLines.Count - 20)
    for ($i = $errStart; $i -lt $errLines.Count; $i++) { [void]$out.Add('stderr: ' + $errLines[$i]) }
    return ($out -join "`n")
}

function Add-Row([string]$Game, [string]$Stage, [string]$Status, [string]$Detail, [double]$Seconds, [string]$Tail) {
    $row = @{ game = $Game; stage = $Stage; status = $Status; detail = $Detail; seconds = [Math]::Round($Seconds, 1); tail = $Tail }
    [void]$script:Rows.Add($row)
    Log ('{0,-5} {1,-14} {2,-16} {3}' -f $Status, $Game, $Stage, $Detail)
}

# Runs one stage function. The function sets $script:Detail and $script:Warn, and throws on failure.
function Invoke-Stage($g, [string]$Stage, [string]$Fn) {
    $script:Current = $null
    $script:Detail = ''
    $script:Warn = $false
    $script:Notes = $null
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $status = 'PASS'
    $detail = ''
    $tail = ''
    try {
        $null = & $Fn $g
        $detail = $script:Detail
        if ($script:Warn) { $status = 'WARN' }
    } catch {
        $status = 'FAIL'
        $detail = $_.Exception.Message
        $tail = Get-Tail $script:Current
    }
    $sw.Stop()
    if ($script:Current -and $script:Current.Notes.Count -gt 0) {
        $detail = $detail + ' [' + (($script:Current.Notes | ForEach-Object { [string]$_ }) -join '; ') + ']'
    }
    Add-Row $g.id $Stage $status $detail $sw.Elapsed.TotalSeconds $tail
    return $status
}

function Has-Output($ctx) {
    if ((Get-Item -LiteralPath $ctx.Out -ErrorAction SilentlyContinue).Length -gt 0) { return $true }
    if ((Get-Item -LiteralPath $ctx.Err -ErrorAction SilentlyContinue).Length -gt 0) { return $true }
    if (Test-Path -LiteralPath (PathJoin @($ctx.Base, 'log.txt'))) { return $true }
    $logs = @(Get-ChildItem -LiteralPath $ctx.Data -Recurse -Filter log.txt -ErrorAction SilentlyContinue)
    return ($logs.Count -gt 0)
}

function Get-LintText($ctx) {
    return (Read-Shared $ctx.Out) + "`n" + (Read-Shared $ctx.Err) + "`n" + (Read-Shared (PathJoin @($ctx.Base, 'log.txt')))
}

function Src-Of($g) {
    return PathJoin @($Kit, 'games', $g.folder)
}

function Stage-Lint($g) {
    $dataDir = PathJoin @($Data, $g.id, 'lint')
    $ctx = Open-Stage $g 'lint' (Src-Of $g) $dataDir @('lint') $false
    try {
        $bootSec = Get-Limit $g 'lint-boot'
        $t0 = [DateTime]::UtcNow
        while (-not (Has-Output $ctx)) {
            if ($ctx.Proc.HasExited) { break }
            if (([DateTime]::UtcNow - $t0).TotalSeconds -gt $bootSec) { throw ('no output within ' + $bootSec + ' s') }
            Start-Sleep -Milliseconds 200
        }
        $limit = Get-Limit $g 'lint'
        $t1 = [DateTime]::UtcNow
        $found = $false
        $text = ''
        while ($true) {
            $exited = $ctx.Proc.HasExited
            $text = Get-LintText $ctx
            if ($text.Contains('Statistics:')) { $found = $true; break }
            if ($exited) { break }
            if (([DateTime]::UtcNow - $t1).TotalSeconds -gt $limit) {
                throw ("no 'Statistics:' line within " + $limit + ' s')
            }
            Start-Sleep -Milliseconds 500
        }
        if (-not $found) { throw ("the game exited (code " + $ctx.Proc.ExitCode + ") without a 'Statistics:' line") }
        $secs = ([DateTime]::UtcNow - $t1).TotalSeconds
        if ($text.Contains('Traceback (most recent call last)')) { throw 'the lint output has a traceback' }
        if (Test-Traceback $ctx) { throw 'traceback.txt was written' }
        if (-not $ctx.Proc.WaitForExit(60000)) {
            Stop-Proc $ctx.Proc
            [void]$ctx.Notes.Add('lint still running 60 s after Statistics; stopped')
        }
        $script:Detail = ("'Statistics:' seen after " + [Math]::Round($secs, 1) + ' s')
    } finally {
        Close-Stage $ctx
    }
}

function Stage-PatchSeed($g) {
    $seedSrc = PathJoin @($Kit, 'seed', 'py7patch-patch.toml')
    $p1 = PathJoin @($Data, $g.id, 'patch-seed')
    New-Dir $p1
    New-Dir $LogsDir
    $base = PathJoin @($Work, $g.id, 'patch-seed')
    Copy-Tree (Src-Of $g) $base
    $envVars = @{
        PLAYER_PATCHES_APPLY_TEST = '1'
        PLAYER_COMPAT_NOTICE = 'off'
        RENPY_DISABLE_BACKUPS = 'I take responsibility for this.'
        APPDATA = $AppData
        LOCALAPPDATA = $AppData
        PYTHONUNBUFFERED = '1'
        HARNESS_DIR = $null
        RENPY_PATH_TO_SAVES = $null
    }
    $out1 = PathJoin @($LogsDir, ($g.id + '-patch-seed-1.out.txt'))
    $err1 = PathJoin @($LogsDir, ($g.id + '-patch-seed-1.err.txt'))
    $out2 = PathJoin @($LogsDir, ($g.id + '-patch-seed-2.out.txt'))
    $err2 = PathJoin @($LogsDir, ($g.id + '-patch-seed-2.err.txt'))
    try {
        $p = Start-Player $Exe @($base, '--data', $p1) $base $envVars $out1 $err1
        if (-not $p.WaitForExit(120000)) { Stop-Proc $p; throw 'the apply-test run did not finish in 120 s' }
        $text1 = Read-Shared $out1
        $m = [regex]::Match($text1, '(?m)^fingerprint: (\w+)')
        if (-not $m.Success) { throw 'no fingerprint line in the apply-test output' }
        $fp = $m.Groups[1].Value
        $fpDir = PathJoin @($p1, 'patches', $fp)
        New-Dir $fpDir
        Copy-Item -LiteralPath $seedSrc -Destination (PathJoin @($fpDir, 'patch.toml')) -Force
        $p2 = Start-Player $Exe @($base, '--data', $p1) $base $envVars $out2 $err2
        if (-not $p2.WaitForExit(120000)) { Stop-Proc $p2; throw 'the second apply-test run did not finish in 120 s' }
        $text2 = Read-Shared $out2
        $h = [regex]::Match($text2, 'is (sha1:[0-9a-f]{12})')
        if (-not $h.Success) { throw 'no node hash in the apply-test output' }
        $hash = $h.Groups[1].Value
        $seedText = ([System.IO.File]::ReadAllText($seedSrc)).Replace('sha1:0000000000', $hash)
        $dst = PathJoin @($Data, $g.id, 'patches', $fp)
        New-Dir $dst
        [System.IO.File]::WriteAllText((PathJoin @($dst, 'patch.toml')), $seedText, $Utf8)
        $script:SeedOk[$g.id] = $true
        $script:Detail = ('fingerprint ' + $fp + ', node hash ' + $hash)
    } finally {
        if (Test-Path -LiteralPath $base) { Remove-Item -LiteralPath $base -Recurse -Force -ErrorAction SilentlyContinue }
    }
}

function Stage-Probe($g) {
    if ($g.id -eq 'Synth7Patch' -and -not $script:SeedOk[$g.id]) { throw 'skipped: the patch seed did not run' }
    $dataDir = PathJoin @($Data, $g.id, 'probe')
    $ops = Read-Ops ([System.IO.Path]::Combine($Kit, 'plans', 'probe.plan.tmpl')) ([string]$g.probe_lines)
    $ctx = Open-Stage $g 'probe' (Src-Of $g) $dataDir @() $true
    try {
        $null = Wait-Line $ctx 'boot' 0 (Get-Limit $g 'boot') $false
        Run-Ops $ctx $ops
        Expect-Clean $ctx
        $script:Detail = ('menu, ' + $g.probe_lines + ' lines and quit as planned')
        $script:ProbeOk[$g.id] = $true
    } finally {
        Close-Stage $ctx
    }
}

function Stage-Route($g) {
    $planPath = [System.IO.Path]::Combine($Kit, $g.plan)
    $ops = Read-Ops $planPath ''
    $dataDir = PathJoin @($Data, $g.id, 'route')
    $ctx = Open-Stage $g 'route' (Src-Of $g) $dataDir @() $true
    try {
        $null = Wait-Line $ctx 'boot' 0 (Get-Limit $g 'boot') $false
        Run-Ops $ctx $ops
        Expect-Clean $ctx
        $script:Detail = ('plan of ' + $ops.Count + ' ops finished and quit')
        $script:Warn = $ctx.Warn
        $script:Current = $ctx
    } finally {
        Close-Stage $ctx
    }
}

function Stage-Resume($g) {
    $src = Src-Of $g
    $ops1 = @(
        'cmd auto on', 'cmd click on', 'wait menu True', 'cmd start',
        ('cmd advance-to ' + $g.save_after), 'wait advance-done', 'cmd save kit-slot', 'quit'
    )
    $ctx1 = Open-Stage $g 'resume-1' $src (PathJoin @($Data, $g.id, 'resume-1')) @() $true
    try {
        $null = Wait-Line $ctx1 'boot' 0 (Get-Limit $g 'boot') $false
        Run-Ops $ctx1 $ops1
        Expect-Clean $ctx1
    } finally {
        Close-Stage $ctx1
    }
    # The player writes saves under <data>\saves. Only *.save files count: persistent is written on every run.
    $saves1 = PathJoin @($Data, $g.id, 'resume-1', 'saves')
    $written = @(Get-ChildItem -LiteralPath $saves1 -Recurse -File -Filter '*.save' -ErrorAction SilentlyContinue)
    if ($written.Count -eq 0) { throw 'the save did not write a file' }

    # The resume run starts with its own data dir, so it gets a copy of the first run's saves.
    $saves2 = PathJoin @($Data, $g.id, 'resume-2', 'saves')
    New-Dir $saves2
    Copy-Item -Path (PathJoin @($saves1, '*')) -Destination $saves2 -Recurse -Force

    $ops2 = @(
        'cmd auto on', 'cmd click on', 'wait menu True', 'cmd load kit-slot',
        ('cmd advance ' + $g.resume_lines), 'wait advance-done', 'quit'
    )
    $ctx2 = Open-Stage $g 'resume-2' $src (PathJoin @($Data, $g.id, 'resume-2')) @() $true
    try {
        $null = Wait-Line $ctx2 'boot' 0 (Get-Limit $g 'boot') $false
        Run-Ops $ctx2 $ops2
        Expect-Clean $ctx2
        $adv = @($ctx2.Progress.Lines | Where-Object { $_.StartsWith('advance-done ') })
        $last = 'none'
        if ($adv.Count -gt 0) { $last = $adv[$adv.Count - 1] }
        $script:Detail = ('saved after ' + $g.save_after + ' lines, loaded, advanced ' + $g.resume_lines + ' (' + $last + ')')
    } finally {
        Close-Stage $ctx2
    }
}

# ---- Ren'Py 7 report checks on the probe reports (mirrors harness/testgames/run.py)

$REWRITES = @('division', 'list-keys', 'list-values', 'list-items', 'list-map', 'list-filter', 'list-zip', 'iter*', 'has_key',
    'sorted', 'sort', 'round', '__metaclass__', 'dunder-alias', '__eq__-hash', '__cmp__', 'exec-in-function')
$SYNTAX = @('no block accepted', 'has no value')

function Read-Reports($dataDir) {
    $rep = PathJoin @($dataDir, 'reports')
    $events = New-Object System.Collections.ArrayList
    $pre = $null
    if (Test-Path -LiteralPath $rep) {
        $dirs = @(Get-ChildItem -LiteralPath $rep -Directory | Sort-Object FullName)
        foreach ($d in $dirs) {
            $rt = PathJoin @($d.FullName, 'runtime.jsonl')
            if (Test-Path -LiteralPath $rt) {
                foreach ($line in (Get-Content -LiteralPath $rt)) {
                    if ($line.Trim() -eq '') { continue }
                    try { [void]$events.Add(($line | ConvertFrom-Json)) } catch { }
                }
            }
            $pf = PathJoin @($d.FullName, 'preflight.json')
            if (Test-Path -LiteralPath $pf) {
                $pre = (Get-Content -Raw -LiteralPath $pf) | ConvertFrom-Json
            }
        }
    }
    return @{ Events = $events; Pre = $pre }
}

function Events-Of($r, [string]$Kind) {
    return @($r.Events | Where-Object { $_.kind -eq $Kind })
}

function Check-Py7Preflight($r) {
    $p = New-Object System.Collections.ArrayList
    if ($null -eq $r.Pre) { [void]$p.Add('no preflight.json in the probe launch') }
    elseif ($r.Pre.renpy7 -ne $true) { [void]$p.Add('preflight: renpy7 is not true') }
    return [string[]]$p.ToArray()
}

function Check-Rewrites($r) {
    $p = New-Object System.Collections.ArrayList
    $seen = (Events-Of $r 'rewrite' | ForEach-Object { [string]$_.detail }) -join ' '
    foreach ($name in $REWRITES) {
        if (-not $seen.Contains($name)) { [void]$p.Add('no rewrite event for rule ' + $name) }
    }
    $syn = @(Events-Of $r 'syntax' | ForEach-Object { [string]$_.detail })
    foreach ($s in $SYNTAX) {
        $hit = @($syn | Where-Object { $_.Contains($s) })
        if ($hit.Count -eq 0) { [void]$p.Add('no syntax event containing ' + $s) }
    }
    $py2fix = @($syn | Where-Object { $_.Contains('column') })
    if ($py2fix.Count -lt 8) { [void]$p.Add('only ' + $py2fix.Count + ' py2fix sites (expected 8 or more)') }
    $fixes = @(Events-Of $r 'fix')
    if ($fixes.Count -lt 4) { [void]$p.Add($fixes.Count + ' fix events (expected 4)') }
    $skips = @(Events-Of $r 'skip' | Where-Object { ([string]$_.file).Contains('un.rpyc') })
    if ($skips.Count -eq 0) { [void]$p.Add('un.rpyc was not skipped (no skip event)') }
    return [string[]]$p.ToArray()
}

function Check-Patch($r) {
    $p = New-Object System.Collections.ArrayList
    $applied = @(Events-Of $r 'patch' | Where-Object {
        ([string]$_.detail).Contains('applied') -and -not ([string]$_.detail).Contains('not applied')
    })
    if ($applied.Count -eq 0) { [void]$p.Add('the port patch was not applied (no patch applied event)') }
    return [string[]]$p.ToArray()
}

function Check-EngineCopy($r) {
    $p = New-Object System.Collections.ArrayList
    if ($null -eq $r.Pre) { [void]$p.Add('no preflight report in the probe launch'); return [string[]]$p.ToArray() }
    $entries = @($r.Pre.unsupported | Where-Object { $_.feature -eq 'game-engine-copy' })
    if ($entries.Count -ne 1) {
        [void]$p.Add('expected one game-engine-copy entry, found ' + $entries.Count)
    } elseif ($entries[0].severity -ne 'info') {
        [void]$p.Add('game-engine-copy has severity ' + $entries[0].severity + ', expected info')
    }
    return [string[]]$p.ToArray()
}

function Run-Check($g, [string]$name) {
    $dataDir = PathJoin @($Data, $g.id, 'probe')
    $r = Read-Reports $dataDir
    if ($name -eq 'py7-preflight') { $problems = @(Check-Py7Preflight $r) }
    elseif ($name -eq 'py7-rewrites') { $problems = @(Check-Rewrites $r) }
    elseif ($name -eq 'patch') { $problems = @(Check-Patch $r) }
    elseif ($name -eq 'engine-copy') { $problems = @(Check-EngineCopy $r) }
    else { $problems = @('unknown check ' + $name) }
    if ($problems.Count -eq 0) {
        Add-Row $g.id ('check:' + $name) 'PASS' 'report matches' 0 ''
    } else {
        Add-Row $g.id ('check:' + $name) 'FAIL' ($problems -join '; ') 0 ''
    }
}

function Get-SystemInfo($exePath) {
    $lines = New-Object System.Collections.ArrayList
    [void]$lines.Add('computer: ' + [System.Environment]::MachineName)
    if ($OnWindows) {
        try {
            $os = Get-CimInstance Win32_OperatingSystem
            [void]$lines.Add('os: ' + $os.Caption + ' ' + $os.Version)
            [void]$lines.Add('ram_gb: ' + [Math]::Round($os.TotalVisibleMemorySize / 1MB, 1))
        } catch { [void]$lines.Add('os: unavailable') }
        try {
            $cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
            [void]$lines.Add('cpu: ' + $cpu.Name + ' (' + $cpu.NumberOfLogicalProcessors + ' logical)')
        } catch { [void]$lines.Add('cpu: unavailable') }
        try {
            $gpus = @(Get-CimInstance Win32_VideoController | ForEach-Object { [string]$_.Name })
            [void]$lines.Add('gpu: ' + ($gpus -join '; '))
        } catch { [void]$lines.Add('gpu: unavailable') }
    } else {
        [void]$lines.Add('os: ' + [System.Environment]::OSVersion.VersionString)
        [void]$lines.Add('cpu: ' + [System.Environment]::ProcessorCount + ' logical')
        [void]$lines.Add('gpu: not queried on this platform')
    }
    [void]$lines.Add('WGPU_BACKEND: ' + $env:WGPU_BACKEND)
    [void]$lines.Add('powershell: ' + $PSVersionTable.PSVersion.ToString())
    try {
        $h = Get-FileHash -LiteralPath $exePath -Algorithm SHA256
        [void]$lines.Add(([System.IO.Path]::GetFileName($exePath)) + ' sha256: ' + $h.Hash)
    } catch { [void]$lines.Add(([System.IO.Path]::GetFileName($exePath)) + ' sha256: unavailable') }
    return ($lines -join "`r`n") + "`r`n"
}

function Write-Summary([string]$Stamp, [string]$SysText) {
    $sum = New-Object System.Collections.ArrayList
    [void]$sum.Add('urp Windows test kit: results')
    [void]$sum.Add('run: ' + $Stamp)
    [void]$sum.Add('')
    $pass = @($script:Rows | Where-Object { $_.status -eq 'PASS' }).Count
    $warn = @($script:Rows | Where-Object { $_.status -eq 'WARN' }).Count
    $fail = @($script:Rows | Where-Object { $_.status -eq 'FAIL' }).Count
    [void]$sum.Add(('PASS ' + $pass + ', WARN ' + $warn + ', FAIL ' + $fail))
    [void]$sum.Add('')
    foreach ($r in $script:Rows) {
        $d = ([string]$r.detail) -replace "`r?`n", ' | '
        [void]$sum.Add(('{0,-5} {1,-14} {2,-16} {3,7}s  {4}' -f $r.status, $r.game, $r.stage, $r.seconds, $d))
    }
    [void]$sum.Add('')
    [void]$sum.Add('Not checked by this kit: frame and A/V comparison against stock Ren''Py, the stock baseline,')
    [void]$sum.Add('the full savescan, deep runs, and file hygiene checks. See README-TESTER.txt.')
    [System.IO.File]::WriteAllText([System.IO.Path]::Combine($Results, 'summary.txt'), (($sum -join "`r`n") + "`r`n"), $Utf8)
    [System.IO.File]::WriteAllText([System.IO.Path]::Combine($Results, 'system.txt'), $SysText, $Utf8)
    $json = @{
        run = $Stamp
        computer = [System.Environment]::MachineName
        counts = @{ pass = $pass; warn = $warn; fail = $fail }
        rows = @($script:Rows | ForEach-Object { $_ })
    } | ConvertTo-Json -Depth 6
    [System.IO.File]::WriteAllText([System.IO.Path]::Combine($Results, 'results.json'), $json, $Utf8)
}

# ---- main

if (-not (Test-Path -LiteralPath $Exe)) {
    Write-Host ('urp.exe not found at ' + $Exe)
    Write-Host 'Extract the whole zip first, then run run-tests.bat from the extracted folder.'
    exit 2
}
$gamesJson = [System.IO.Path]::Combine($Kit, 'games.json')
if (-not (Test-Path -LiteralPath $gamesJson)) {
    Write-Host 'games.json not found. Extract the whole zip first.'
    exit 2
}
try {
    $probeFile = [System.IO.Path]::Combine($Kit, 'kit-write-test.tmp')
    [System.IO.File]::WriteAllText($probeFile, 'x')
    Remove-Item -LiteralPath $probeFile -Force
} catch {
    Write-Host 'This folder is read-only. Extract the zip to a folder you can write to (for example Documents), then run again.'
    exit 2
}

foreach ($d in @($Results, $Work, $Data)) {
    if (Test-Path -LiteralPath $d) { Remove-Item -LiteralPath $d -Recurse -Force }
}
New-Dir $Results
New-Dir $LogsDir
New-Dir $ShotDir
New-Dir $StageFiles
New-Dir $AppData
$script:RunLog = [System.IO.Path]::Combine($Results, 'run.log')
$stamp = (Get-Date -Format 'yyyy-MM-dd HH:mm:ss')
Log ('urp Windows test kit, started ' + $stamp)

$manifest = (Get-Content -Raw -LiteralPath $gamesJson) | ConvertFrom-Json
$games = @($manifest.games)
$onlyIds = @()
if ($Only -ne '') { $onlyIds = @($Only -split ',' | ForEach-Object { $_.Trim() }) }

try {
    foreach ($g in $games) {
        if ($onlyIds.Count -gt 0 -and -not ($onlyIds -contains $g.id)) { continue }
        Log ('--- ' + $g.id + ' (' + $g.folder + ', Ren''Py ' + $g.renpy + ')')
        $null = Invoke-Stage $g 'lint' 'Stage-Lint'
        if ($g.id -eq 'Synth7Patch') {
            $seed = Invoke-Stage $g 'patch-seed' 'Stage-PatchSeed'
            $script:SeedOk[$g.id] = ($seed -ne 'FAIL')
        }
        $probeStatus = 'SKIP'
        if ($null -eq $g.probe_lines) {
            foreach ($c in @($g.checks)) {
                Add-Row $g.id ('check:' + [string]$c) 'SKIP' 'this game has no probe stage' 0 ''
            }
        }
        if ($null -ne $g.probe_lines) {
            $probeStatus = Invoke-Stage $g 'probe' 'Stage-Probe'
            if ($probeStatus -ne 'FAIL') {
                foreach ($c in @($g.checks)) { Run-Check $g ([string]$c) }
            } else {
                foreach ($c in @($g.checks)) {
                    Add-Row $g.id ('check:' + [string]$c) 'SKIP' 'the probe stage failed' 0 ''
                }
            }
        }
        if ($g.plan) { $null = Invoke-Stage $g 'route' 'Stage-Route' }
        if ($null -ne $g.save_after -and $null -ne $g.resume_lines) {
            $null = Invoke-Stage $g 'save-resume' 'Stage-Resume'
        }
    }
} catch {
    Log ('runner error: ' + $_.Exception.Message)
}

$stampEnd = (Get-Date -Format 'yyyyMMdd-HHmm')
$sys = Get-SystemInfo $Exe
Copy-Item -LiteralPath ([System.IO.Path]::Combine($Kit, 'manifest.txt')) -Destination $Results -Force -ErrorAction SilentlyContinue
Write-Summary $stamp $sys
Log ('results written to ' + $Results)

# Work and data are scratch. The results folder keeps the logs and report copies.
foreach ($d in @($Work, $Data)) {
    if (Test-Path -LiteralPath $d) { Remove-Item -LiteralPath $d -Recurse -Force -ErrorAction SilentlyContinue }
}

$zipName = 'results-' + [System.Environment]::MachineName + '-' + $stampEnd + '.zip'
$zipPath = [System.IO.Path]::Combine($Kit, $zipName)
if (Test-Path -LiteralPath $zipPath) { Remove-Item -LiteralPath $zipPath -Force }
Compress-Archive -Path ([System.IO.Path]::Combine($Results, '*')) -DestinationPath $zipPath
Log ('send this file back: ' + $zipPath)

$failed = @($script:Rows | Where-Object { $_.status -eq 'FAIL' }).Count
if ($failed -gt 0) { exit 1 }
exit 0
