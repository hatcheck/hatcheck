# cl <profile> [claude args...]
# Windows version of ./cl: run Claude Code with the ~\.claude-<profile> config dir,
# which has its own login, settings and sessions:
#   cl work --resume <id>  ==  $env:CLAUDE_CONFIG_DIR="$HOME\.claude-work"; claude --resume <id>
# install.ps1 puts a cl.cmd launcher for this on your PATH. CLAUDE_CONFIG_DIR is put
# back afterwards, so plain claude in the same window keeps using ~\.claude.
if ($args.Count -lt 1 -or -not $args[0]) {
    [Console]::Error.WriteLine("usage: cl <profile> [claude args...]  (uses ~\.claude-<profile>)")
    exit 2
}
if (-not (Get-Command claude -ErrorAction SilentlyContinue)) {
    [Console]::Error.WriteLine("cl: claude isn't on your PATH; install Claude Code first")
    exit 1
}
$previous = $env:CLAUDE_CONFIG_DIR
$home_dir = if ($env:USERPROFILE) { $env:USERPROFILE } else { $HOME }  # what Claude Code calls home
$env:CLAUDE_CONFIG_DIR = Join-Path $home_dir (".claude-" + $args[0])
$rest = @($args | Select-Object -Skip 1)
try {
    & claude @rest
    $code = $LASTEXITCODE
} finally {
    $env:CLAUDE_CONFIG_DIR = $previous
}
exit $code
