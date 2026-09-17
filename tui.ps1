<#
.SYNOPSIS
    Launcher instan untuk menjalankan mode TUI ctrl-cli.
.EXAMPLE
    .\tui
    .\tui.ps1
#>
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$CliArgs
)

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$candidates = @(
    (Join-Path $scriptDir "target\release\ctrl-cli.exe"),
    (Join-Path $scriptDir "ctrl-cli\target\release\ctrl-cli.exe"),
    (Join-Path $scriptDir "target\debug\ctrl-cli.exe"),
    (Join-Path $scriptDir "ctrl-cli\target\debug\ctrl-cli.exe")
)

foreach ($cand in $candidates) {
    if (Test-Path $cand) {
        & $cand tui @CliArgs
        exit $LASTEXITCODE
    }
}

$localManifest = Join-Path $scriptDir "Cargo.toml"
$subManifest = Join-Path $scriptDir "ctrl-cli\Cargo.toml"
if (Test-Path $localManifest) {
    cargo run --manifest-path $localManifest --bin ctrl-cli -- tui @CliArgs
} else {
    cargo run --manifest-path $subManifest --bin ctrl-cli -- tui @CliArgs
}
exit $LASTEXITCODE
