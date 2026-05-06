param(
    [string]$PackageRoot = "bevy-aquarium",
    [string]$FeatureSet = "hotpatch"
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$crateRoot = (Resolve-Path (Join-Path $repoRoot $PackageRoot)).Path
$localDx = Join-Path $repoRoot ".epiphany-aquarium\tools\dx-0.7.7\dx.exe"

$dxCommand = Get-Command dx -ErrorAction SilentlyContinue
$dxPath = if ($dxCommand) { $dxCommand.Source } else { $null }
if (-not $dxPath -and (Test-Path $localDx)) {
    $dxPath = $localDx
}
if (-not $dxPath) {
    Write-Host "Dioxus CLI is required for real Bevy system hotpatching."
    Write-Host "Install it once with:"
    Write-Host "  npm run bevy:hot:install"
    exit 1
}

Push-Location $crateRoot
try {
    Write-Host "Starting Bevy hotpatch loop with dx from $crateRoot"
    Write-Host "Rust system edits should patch into the running app when the changed code is hotpatch-compatible."
    & $dxPath serve --hot-patch --features $FeatureSet
}
finally {
    Pop-Location
}
