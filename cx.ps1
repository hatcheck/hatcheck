# cx <profile> [codex args...]
# Windows version of ./cx: run Codex with the ~\.codex-<profile> home (CODEX_HOME),
# which has its own login, config and sessions:
#   cx work resume <id>  ==  $env:CODEX_HOME="$HOME\.codex-work"; codex resume <id>
# Codex won't start with a CODEX_HOME that doesn't exist, so the folder is made on
# first use; sign in there with: cx work login. install.ps1 puts a cx.cmd launcher for
# this on your PATH. CODEX_HOME is put back afterwards, so plain codex in the same
# window keeps using ~\.codex.
$rest = @($args)
if ($rest.Count -lt 1 -or -not $rest[0] -or "$($rest[0])".StartsWith("-")) {
    [Console]::Error.WriteLine("usage: cx <profile> [codex args...]  (uses ~\.codex-<profile>)")
    exit 2
}
if (-not (Get-Command codex -ErrorAction SilentlyContinue)) {
    [Console]::Error.WriteLine("cx: codex isn't on your PATH; install Codex first")
    exit 1
}
$home_dir = if ($env:USERPROFILE) { $env:USERPROFILE } else { $HOME }  # what Codex calls home
$codexHome = Join-Path $home_dir (".codex-" + $rest[0])
New-Item -ItemType Directory -Force -Path $codexHome | Out-Null
$rest = @($rest | Select-Object -Skip 1)
$previous = [Environment]::GetEnvironmentVariable("CODEX_HOME", "Process")
[Environment]::SetEnvironmentVariable("CODEX_HOME", $codexHome, "Process")
try {
    & codex @rest
    $code = $LASTEXITCODE
} finally {
    [Environment]::SetEnvironmentVariable("CODEX_HOME", $previous, "Process")  # $null removes it
}
exit $code
