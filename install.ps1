# install.ps1
# Windows version of install.sh: put this repo's Claude Code and Codex CLI helpers on your
# PATH as small .cmd launchers in a bin folder (default ~\.local\bin, where Claude
# Code's installer puts claude.exe). The launchers run the scripts in this repo, so
# a git pull updates them, and they work from PowerShell and cmd alike. Your PATH
# and PowerShell profile are not changed.
#
# Usage:
#   .\install.ps1 [-Bin DIR] [-Force]    add the commands
#   .\install.ps1 [-Bin DIR] -Uninstall  remove the launchers this script made
#
#   cl                      claude with a ~\.claude-<name> profile: cl work --resume <id>
#   cx                      codex with a ~\.codex-<name> home: cx work resume <id>
#   claude-where            where a session is stored, by ID or ID prefix
#   claude-recent-sessions  recent sessions in every profile and home, with resume commands and plan usage
#   claude-usage            plan usage left for every profile and home
#   claude-move-session     copy a session to another profile (move it with --no-keep)
#   claude-z, claude-k      claude on Z.ai or Kimi instead of Anthropic (cl.ps1 --as claude-z)
#   claude-provider         the key and model for those; claude-z reconfig sets them up
#
#   -Force  replace .cmd files that already exist under those names
param(
    [string]$Bin = (Join-Path $(if ($env:USERPROFILE) { $env:USERPROFILE } else { $HOME }) ".local\bin"),
    [switch]$Force,
    [switch]$Uninstall
)
$ErrorActionPreference = "Stop"
$repo = $PSScriptRoot
$marker = "rem made by hatcheck install.ps1"
# Launchers made before the repo was renamed from claude-multi-account carry the old marker.
$markers = @($marker, "rem made by claude-multi-account install.ps1")
$python = if (Get-Command py -CommandType Application -ErrorAction SilentlyContinue) { "py -3" } else { "python" }
$powershell = "powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -File"
$commands = [ordered]@{
    "cl"                     = "$powershell `"$repo\cl.ps1`" %*"
    "cx"                     = "$powershell `"$repo\cx.ps1`" %*"
    "claude-where"           = "$powershell `"$repo\claude-where.ps1`" %*"
    "claude-recent-sessions" = "$python `"$repo\claude-recent-sessions.py`" %*"
    "claude-usage"           = "$python `"$repo\claude-usage.py`" %*"
    "claude-move-session"    = "$python `"$repo\claude-move-session.py`" %*"
    "claude-provider"        = "$python `"$repo\claude-provider.py`" %*"
    "claude-z"               = "$powershell `"$repo\cl.ps1`" --as claude-z %*"
    "claude-k"               = "$powershell `"$repo\cl.ps1`" --as claude-k %*"
}

function Test-Ours($path) {
    (Test-Path -LiteralPath $path -PathType Leaf) -and (Select-String -LiteralPath $path -SimpleMatch $markers -Quiet)
}

if ($Uninstall) {
    foreach ($name in $commands.Keys) {
        $path = Join-Path $Bin "$name.cmd"
        if (Test-Ours $path) {
            Remove-Item -LiteralPath $path
            Write-Output "Removed $path"
        } elseif (Test-Path -LiteralPath $path) {
            Write-Output "Left $path alone: this script didn't make it"
        }
    }
    exit 0
}

if ($repo -match '[^\x20-\x7e]') {
    Write-Warning "The repo path has non-ASCII characters, which .cmd files may not read correctly: $repo"
}
New-Item -ItemType Directory -Force -Path $Bin | Out-Null
$skipped = 0
foreach ($name in $commands.Keys) {
    $path = Join-Path $Bin "$name.cmd"
    $text = "@echo off`r`n$marker`r`n$($commands[$name])`r`n"
    if ((Test-Path -LiteralPath $path) -and -not (Test-Ours $path) -and -not $Force) {
        Write-Output "Skipped ${name}: $path already exists (run again with -Force to replace it)"
        $skipped++
        continue
    }
    if ((Test-Ours $path) -and ([IO.File]::ReadAllText($path) -eq $text)) {
        Write-Output "$name (already installed)"
        continue
    }
    [IO.File]::WriteAllText($path, $text, [Text.Encoding]::ASCII)
    Write-Output "$name -> $path"
}

$onPath = $env:Path -split ";" | Where-Object { $_ -and ($_.TrimEnd("\") -ieq $Bin.TrimEnd("\")) }
if (-not $onPath) {
    Write-Output "$Bin isn't on your PATH. To add it for your user account, run:"
    Write-Output "  [Environment]::SetEnvironmentVariable('Path', `"$Bin;`" + [Environment]::GetEnvironmentVariable('Path', 'User'), 'User')"
}
if (-not (Get-Command py, python -CommandType Application -ErrorAction SilentlyContinue)) {
    Write-Output "claude-recent-sessions, claude-usage, claude-move-session and claude-provider need Python 3 (python.org or: winget install Python.Python.3.13)"
}
if (-not (Get-Command claude -CommandType Application -ErrorAction SilentlyContinue)) {
    Write-Output "cl runs claude, which isn't on your PATH: install Claude Code first"
}
$compiler = Get-Command cl.exe -CommandType Application -ErrorAction SilentlyContinue
if ($compiler) {
    Write-Output "Note: $($compiler.Source) (the MSVC compiler) is also called cl; whichever comes first on PATH wins."
}
# The claude-z repo's installers put their own claude-z.cmd and claude-k.cmd in
# %LOCALAPPDATA%\claude-z and \claude-k, on PATH; whichever folder comes first wins.
# Their uninstallers also delete %APPDATA%\claude-z, the config these read, so don't
# point people at those.
foreach ($name in "claude-z", "claude-k") {
    $first = Get-Command $name -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($first -and -not (Test-Ours $first.Source)) {
        $folder = Split-Path $first.Source
        Write-Output "Note: $($first.Source) comes first on PATH, so $name runs that one."
        Write-Output "  If it's from the claude-z repo, delete $folder and take it off your PATH. Keep"
        Write-Output "  $(Join-Path $env:APPDATA $name): it holds the key this $name reads."
    }
}
if ($skipped) { exit 1 }
