param(
    [string]$PackageRoot = "bevy-aquarium",
    [string]$FeatureSet = "hotpatch",
    [switch]$KeepStaleBuilds
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$crateRoot = (Resolve-Path (Join-Path $repoRoot $PackageRoot)).Path
$localDx = Join-Path $repoRoot ".epiphany-aquarium\tools\dx-0.7.7\dx.exe"
$sessionCache = Join-Path $repoRoot ".epiphany-aquarium\dx-session"
$log = Join-Path $repoRoot ".epiphany-aquarium\dx-serve.log"

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

function Stop-StaleAquariumBuilds {
    $repoPattern = "*" + $repoRoot.Replace("\", "\\") + "*"
    $cratePattern = "*bevy-aquarium*"
    $dxPattern = "*" + $dxPath.Replace("\", "\\") + "*"
    $candidates = Get-CimInstance Win32_Process |
        Where-Object {
            $_.Name -match "^(dx|cargo|rustc|link|bevy-aquarium)\.exe$" -and
            (
                $_.CommandLine -like $repoPattern -or
                $_.CommandLine -like $cratePattern -or
                $_.CommandLine -like $dxPattern -or
                $_.ExecutablePath -like (Join-Path $repoRoot "*")
            )
        }
    foreach ($process in $candidates) {
        Write-Host "Stopping stale Aquarium build process $($process.Name) $($process.ProcessId)..."
        Stop-Process -Id $process.ProcessId -Force -ErrorAction SilentlyContinue
    }
}

if (-not $KeepStaleBuilds) {
    Stop-StaleAquariumBuilds
}

New-Item -ItemType Directory -Force $sessionCache | Out-Null

Push-Location $crateRoot
try {
    Write-Host "Starting Bevy hotpatch loop with dx from $crateRoot"
    Write-Host "dx log: $log"
    Write-Host "First run compiles dx's separate desktop-dev target; it may print 'Waiting for cargo-metadata' while rustc is actually building."
    Write-Host "Rust system edits should patch into the running app when the changed code is hotpatch-compatible."
    & $dxPath serve `
        --hot-patch `
        --features $FeatureSet `
        --package "bevy-aquarium" `
        --bin "bevy-aquarium" `
        --windows `
        --renderer native `
        --session-cache-dir $sessionCache `
        --log-to-file $log
}
finally {
    Pop-Location
}
