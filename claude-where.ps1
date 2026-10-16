# claude-where <session id or prefix>
# Windows version of ./claude-where: find a Claude Code session in ~\.claude and
# every ~\.claude-* profile, or a Codex session in ~\.codex and every ~\.codex-*
# home. For Claude Code it prints the transcript, plus its data folder if it has
# one: the profile in the path says how to resume it (claude, or cl <profile>), and
# the projects\ folder is the directory it ran in, with every non-alphanumeric
# character turned into -. For Codex it prints the session file, under sessions\ by
# the date it started, or under archived_sessions\: the home says how to resume it
# (codex resume <id>, or cx <home> resume <id>).
if ($args.Count -lt 1 -or -not $args[0]) {
    [Console]::Error.WriteLine("usage: claude-where <session id or prefix>")
    exit 2
}
$prefix = [Management.Automation.WildcardPattern]::Escape([string]$args[0])
$home_dir = if ($env:USERPROFILE) { $env:USERPROFILE } else { $HOME }  # what Claude Code and Codex call home
$found = @(Get-ChildItem -Path (Join-Path $home_dir ".claude*\projects\*") -Directory -Force -ErrorAction SilentlyContinue |
    Get-ChildItem -Force -ErrorAction SilentlyContinue | Where-Object { $_.Name -like "$prefix*" } |
    ForEach-Object { $_.FullName })
$codex = "rollout-????-??-??T??-??-??-$prefix*.jsonl"
# A wildcard path gives the items it matches: the session files four levels under sessions\
# (year\month\day\file), and those right in archived_sessions\.
$found += @(foreach ($dir in "sessions\*\*\*\*", "archived_sessions\*") {
    Get-ChildItem -Path (Join-Path $home_dir ".codex*\$dir") -File -Force -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -like $codex } | ForEach-Object { $_.FullName }
})
$found
if (-not $found) { exit 1 }  # nothing matched
