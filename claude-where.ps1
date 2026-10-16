# claude-where is now `hatcheck where`. This runs it, so launchers that install.ps1
# made before the rename keep working; it will go away.
$script = Join-Path $PSScriptRoot "claude-where"
if (Get-Command py -CommandType Application -ErrorAction SilentlyContinue) { & py -3 $script @args } else { & python $script @args }
exit $LASTEXITCODE
