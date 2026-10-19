# install.ps1
# Windows version of install.sh: build hatcheck's commands from this repo with cargo and put
# them on your PATH as small .cmd launchers in a bin folder (default ~\.local\bin, where
# Claude Code's installer puts claude.exe). The launchers run the build, so after a git pull,
# run this again to build and update them; they work from PowerShell and cmd alike. Your
# PATH and PowerShell profile are not changed. Without a clone or Rust, install a release
# instead; see the README.
#
# Usage:
#   .\install.ps1 [-Bin DIR] [-Force]      build the commands and add their launchers
#   .\install.ps1 [-Bin DIR] -From DIR     add launchers for the binaries already built in DIR
#   .\install.ps1 [-Bin DIR] -Uninstall    remove the launchers this script made
#
#   hatcheck                recent sessions in every profile and home, with resume commands and
#                           plan usage; hatcheck usage, where, move and provider do the rest
#   hat                     the short name for hatcheck, unless another program called hat is
#                           on your PATH
#   cl                      claude with a ~\.claude-<name> profile: cl work --resume <id>
#   cx                      codex with a ~\.codex-<name> home: cx work resume <id>
#   claude-z, claude-k      claude on Z.ai or Kimi instead of Anthropic
#
# Launchers this script made before, for the Python hatcheck and the PowerShell cl and cx, are
# replaced. Those it made for the commands hatcheck replaced (claude-recent-sessions,
# claude-usage, claude-where, claude-move-session, claude-provider) are removed.
#
#   -Force  replace .cmd files that already exist under those names
param(
    [string]$Bin = (Join-Path $(if ($env:USERPROFILE) { $env:USERPROFILE } else { $HOME }) ".local\bin"),
    [string]$From = "",
    [switch]$Force,
    [switch]$Uninstall
)
$ErrorActionPreference = "Stop"
$repo = $PSScriptRoot
$marker = "rem made by hatcheck install.ps1"
# Launchers made before the repo was renamed from claude-multi-account carry the old marker.
$markers = @($marker, "rem made by claude-multi-account install.ps1")
$build = -not $From
$From = if ($From) { (Resolve-Path -LiteralPath $From).Path } else { Join-Path $repo "target\release" }
$commands = [ordered]@{}
foreach ($name in "hatcheck", "hat", "cl", "cx", "claude-z", "claude-k") {
    $commands[$name] = "`"$(Join-Path $From "$name.exe")`" %*"
}
# The commands hatcheck replaced, and its subcommand that does their job.
$replaced = [ordered]@{
    "claude-recent-sessions" = ""
    "claude-usage"           = " usage"
    "claude-where"           = " where"
    "claude-move-session"    = " move"
    "claude-provider"        = " provider"
}

# A hat that is hatcheck's own, from another install: it says so when asked.
function Test-HatcheckHat($path) {
    try { return ((& $path --version 2>$null) -match '^(hat|hatcheck) \d') } catch { return $false }
}

function Test-Ours($path) {
    (Test-Path -LiteralPath $path -PathType Leaf) -and (Select-String -LiteralPath $path -SimpleMatch $markers -Quiet)
}

# Launchers for the commands hatcheck replaced, whose scripts are gone from this repo.
function Remove-Replaced {
    foreach ($name in $replaced.Keys) {
        $path = Join-Path $Bin "$name.cmd"
        if (Test-Ours $path) {
            Remove-Item -LiteralPath $path
            Write-Output "Removed ${path}: it's hatcheck$($replaced[$name]) now"
        }
    }
}

if ($Uninstall) {
    Remove-Replaced
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

if ($build) {
    if (-not (Get-Command cargo -CommandType Application -ErrorAction SilentlyContinue)) {
        [Console]::Error.WriteLine("install.ps1 builds hatcheck with cargo, which isn't on your PATH. Install Rust")
        [Console]::Error.WriteLine("(https://rustup.rs) and run it again, or install a release instead; see the README.")
        exit 1
    }
    # cargo reports its progress on stderr, which Windows PowerShell turns into errors when
    # stderr is redirected; only its exit code says whether the build worked.
    $ErrorActionPreference = "Continue"
    & cargo build --release --locked -p hatcheck --manifest-path (Join-Path $repo "Cargo.toml")
    $built = $LASTEXITCODE
    $ErrorActionPreference = "Stop"
    if ($built -ne 0) { exit $built }
}
foreach ($name in $commands.Keys) {
    if (-not (Test-Path -LiteralPath (Join-Path $From "$name.exe") -PathType Leaf)) {
        [Console]::Error.WriteLine("There's no $name.exe in $From to add")
        exit 1
    }
}
if ($From -match '[^\x20-\x7e]') {
    Write-Warning "The build's path has non-ASCII characters, which .cmd files may not read correctly: $From"
}
New-Item -ItemType Directory -Force -Path $Bin | Out-Null
Remove-Replaced
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
    if ($name -eq "hat" -and -not $Force -and -not (Test-Ours $path)) {
        # Only a short name, so it gives way to another program called hat.
        $other = Get-Command hat -CommandType Application -ErrorAction SilentlyContinue |
            Where-Object { -not (Test-Ours $_.Source) -and -not (Test-HatcheckHat $_.Source) } | Select-Object -First 1
        if ($other) {
            Write-Output "Didn't add hat: $($other.Source) has that name (-Force adds it anyway); hatcheck does the same"
            continue
        }
    }
    [IO.File]::WriteAllText($path, $text, [Text.Encoding]::ASCII)
    Write-Output "$name -> $path"
}

$onPath = $env:Path -split ";" | Where-Object { $_ -and ($_.TrimEnd("\") -ieq $Bin.TrimEnd("\")) }
if (-not $onPath) {
    Write-Output "$Bin isn't on your PATH. To add it for your user account, run:"
    Write-Output "  [Environment]::SetEnvironmentVariable('Path', `"$Bin;`" + [Environment]::GetEnvironmentVariable('Path', 'User'), 'User')"
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
