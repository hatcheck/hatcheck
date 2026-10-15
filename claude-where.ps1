# claude-where <session id or prefix>
# Windows version of ./claude-where: find a Claude Code session in ~\.claude and
# every ~\.claude-* profile. Prints its transcript, plus its data folder if it has
# one. The profile in the path says how to resume it (claude, or cl <profile>), and
# the projects\ folder is the directory it ran in, with every non-alphanumeric
# character turned into -.
if ($args.Count -lt 1 -or -not $args[0]) {
    [Console]::Error.WriteLine("usage: claude-where <session id or prefix>")
    exit 2
}
$pattern = [Management.Automation.WildcardPattern]::Escape([string]$args[0]) + "*"
$home_dir = if ($env:USERPROFILE) { $env:USERPROFILE } else { $HOME }  # what Claude Code calls home
$found = @(Get-ChildItem -Path (Join-Path $home_dir ".claude*\projects\*") -Directory -Force -ErrorAction SilentlyContinue |
    Get-ChildItem -Force -ErrorAction SilentlyContinue | Where-Object { $_.Name -like $pattern } |
    ForEach-Object { $_.FullName })
$found
if (-not $found) { exit 1 }  # nothing matched
