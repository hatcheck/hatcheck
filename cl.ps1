# cl [-z|-k] <profile> [claude args...]
# Windows version of ./cl: run Claude Code with the ~\.claude-<profile> config dir,
# which has its own login, settings and sessions:
#   cl work --resume <id>  ==  $env:CLAUDE_CONFIG_DIR="$HOME\.claude-work"; claude --resume <id>
# -z or -k runs it on Z.ai (GLM) or Kimi instead of Anthropic, with the key and model
# claude-provider keeps. install.ps1's claude-z.cmd and claude-k.cmd call this with
# --as claude-z or --as claude-k, which does that without a profile, so with ~\.claude
# or whatever CLAUDE_CONFIG_DIR already says; `claude-z reconfig` runs the setup.
# install.ps1 puts a cl.cmd launcher for this on your PATH. Everything set here is put
# back afterwards, so plain claude in the same window keeps using ~\.claude and Anthropic.
$rest = @($args)
$provider = $null
$profileName = $null
if ($rest.Count -ge 2 -and $rest[0] -eq "--as" -and $rest[1] -cmatch '^claude-([zk])$') {
    $provider = $Matches[1]
    $rest = @($rest | Select-Object -Skip 2)
    if ($rest.Count -gt 0 -and $rest[0] -ceq "reconfig") {
        & claude-provider setup $provider
        exit $LASTEXITCODE
    }
} else {
    if ($rest.Count -gt 0 -and ($rest[0] -ceq "-z" -or $rest[0] -ceq "-k")) {
        $provider = $rest[0].Substring(1)
        $rest = @($rest | Select-Object -Skip 1)
    }
    if ($rest.Count -lt 1 -or -not $rest[0] -or "$($rest[0])".StartsWith("-")) {
        [Console]::Error.WriteLine("usage: cl [-z|-k] <profile> [claude args...]  (uses ~\.claude-<profile>)")
        exit 2
    }
    $profileName = $rest[0]
    $rest = @($rest | Select-Object -Skip 1)
}
if (-not (Get-Command claude -ErrorAction SilentlyContinue)) {
    [Console]::Error.WriteLine("cl: claude isn't on your PATH; install Claude Code first")
    exit 1
}

# What to set, as name -> value ($null to remove), and what was there before.
$settings = [ordered]@{}
if ($profileName) {
    $home_dir = if ($env:USERPROFILE) { $env:USERPROFILE } else { $HOME }  # what Claude Code calls home
    $settings["CLAUDE_CONFIG_DIR"] = Join-Path $home_dir (".claude-" + $profileName)
}
if ($provider) {
    if (-not (Get-Command claude-provider -ErrorAction SilentlyContinue)) {
        [Console]::Error.WriteLine("cl: claude-provider isn't on your PATH; run hatcheck's install.ps1")
        exit 1
    }
    $json = & claude-provider env $provider --json
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    foreach ($p in (($json -join "`n") | ConvertFrom-Json).PSObject.Properties) {
        $settings[$p.Name] = $p.Value
    }
}
$previous = @{}
foreach ($name in $settings.Keys) {
    $previous[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
    [Environment]::SetEnvironmentVariable($name, $settings[$name], "Process")  # $null removes it
}
try {
    & claude @rest
    $code = $LASTEXITCODE
} finally {
    foreach ($name in $previous.Keys) {
        [Environment]::SetEnvironmentVariable($name, $previous[$name], "Process")
    }
}
exit $code
