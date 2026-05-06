param(
    [switch]$SkipInstall
)

$ErrorActionPreference = "Stop"
$repoRoot = $PSScriptRoot

if (-not $SkipInstall) {
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $repoRoot "scripts\install-dioxus-cli.ps1")
}

& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $repoRoot "scripts\bevy-hotpatch.ps1")
