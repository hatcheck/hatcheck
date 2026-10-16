# cx <profile> [codex args...]
# Windows version of ./cx: run Codex with the ~\.codex-<profile> home (CODEX_HOME),
# which has its own login, config and sessions:
#   cx work resume <id>  ==  $env:CODEX_HOME="$HOME\.codex-work"; codex resume <id>
# Codex won't start with a CODEX_HOME that doesn't exist, so the folder is made on
# first use; sign in there with: cx work login. install.ps1 puts a cx.cmd launcher for
# this on your PATH. CODEX_HOME is put back afterwards, so plain codex in the same
# window keeps using ~\.codex.
# Without a profile name, in a folder tied to a home with `hatcheck bind`, it uses that
# one: cx resume <id> there means cx <home> resume <id>. A Codex command such as resume
# counts as a profile name only when ~\.codex-<it> exists.
$rest = @($args)
$home_dir = if ($env:USERPROFILE) { $env:USERPROFILE } else { $HOME }  # what Codex calls home
$codexCommands = @("agents", "exec", "e", "review", "login", "logout", "mcp", "plugin", "app-server",
    "remote-control", "app", "completion", "update", "doctor", "sandbox", "debug", "apply", "a", "resume",
    "queue", "archive", "delete", "migrate-rollouts", "unarchive", "fork", "cloud", "exec-server", "features", "help")
$profileName = $null
if ($rest.Count -ge 1 -and $rest[0] -and -not "$($rest[0])".StartsWith("-")) {
    if ($codexCommands -cnotcontains $rest[0] -or (Test-Path (Join-Path $home_dir (".codex-" + $rest[0])))) {
        $profileName = $rest[0]
    }
}
$bound = $null
if (-not $profileName -and (Get-Command hatcheck -ErrorAction SilentlyContinue)) {
    $out = @(& hatcheck bind --which codex 2>$null)
    if ($LASTEXITCODE -eq 0 -and $out.Count -gt 0) { $bound = "$($out[0])".Trim() }
}
$codexHome = $null
if ($bound) {
    $bound = "$bound".Trim()
    [Console]::Error.WriteLine("cx: this folder is bound to $bound")
    if ($bound -cne "default") {
        $codexHome = if ($bound -match '[\\/]') { $bound } else { Join-Path $home_dir (".codex-" + $bound) }
    }
} else {
    if (-not $profileName) {
        if ($rest.Count -ge 1 -and $rest[0] -and -not "$($rest[0])".StartsWith("-")) {  # a Codex command, with no home of that name
            [Console]::Error.WriteLine("cx: $($rest[0]) is a Codex command, not a home's name; run it with a home (cx <profile> $($rest[0]) ...)")
        }
        [Console]::Error.WriteLine("usage: cx <profile> [codex args...]  (uses ~\.codex-<profile>)")
        [Console]::Error.WriteLine("   or tie this folder to a home with: hatcheck bind <profile>")
        exit 2
    }
    $codexHome = Join-Path $home_dir (".codex-" + $rest[0])
    $rest = @($rest | Select-Object -Skip 1)
}
if (-not (Get-Command codex -ErrorAction SilentlyContinue)) {
    [Console]::Error.WriteLine("cx: codex isn't on your PATH; install Codex first")
    exit 1
}
if ($codexHome) {
    New-Item -ItemType Directory -Force -Path $codexHome | Out-Null
}
$previous = [Environment]::GetEnvironmentVariable("CODEX_HOME", "Process")
[Environment]::SetEnvironmentVariable("CODEX_HOME", $codexHome, "Process")  # $null: ~\.codex
try {
    & codex @rest
    $code = $LASTEXITCODE
} finally {
    [Environment]::SetEnvironmentVariable("CODEX_HOME", $previous, "Process")  # $null removes it
}
exit $code
