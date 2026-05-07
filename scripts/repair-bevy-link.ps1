param(
    [string]$ManifestPath = "bevy-aquarium\Cargo.toml",
    [string]$FeatureSet = "hotpatch",
    [string]$Package = "aquarium_synth"
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$manifest = (Resolve-Path (Join-Path $repoRoot $ManifestPath)).Path

Write-Host "Cleaning stale incremental artifacts for package '$Package'..."
Push-Location $repoRoot
try {
    & cargo clean --manifest-path $manifest -p $Package
    Write-Host "Rebuilding Bevy host with feature '$FeatureSet'..."
    & cargo build --manifest-path $manifest --features $FeatureSet
}
finally {
    Pop-Location
}
