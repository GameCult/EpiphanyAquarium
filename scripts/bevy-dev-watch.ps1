param(
    [string]$ManifestPath = "bevy-aquarium\Cargo.toml",
    [string]$FeatureSet = "dev"
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$manifest = (Resolve-Path (Join-Path $repoRoot $ManifestPath)).Path
$watchRoot = Join-Path $repoRoot "bevy-aquarium"
$script:child = $null
$script:lastRestart = Get-Date "2000-01-01"

function Stop-Child {
    if ($script:child -and -not $script:child.HasExited) {
        Write-Host "Stopping Bevy client $($script:child.Id)..."
        & taskkill.exe /PID $script:child.Id /T /F | Out-Null
    }
    $script:child = $null
}

function Start-Child {
    Stop-Child
    Write-Host "Starting Bevy client with feature '$FeatureSet'..."
    $script:child = Start-Process `
        -FilePath "cargo.exe" `
        -ArgumentList @("run", "--manifest-path", $manifest, "--features", $FeatureSet) `
        -WorkingDirectory $repoRoot `
        -NoNewWindow `
        -PassThru
    $script:lastRestart = Get-Date
}

function Request-Restart {
    $now = Get-Date
    if (($now - $script:lastRestart).TotalMilliseconds -lt 900) {
        return
    }
    Start-Child
}

$watcher = [System.IO.FileSystemWatcher]::new($watchRoot)
$watcher.IncludeSubdirectories = $true
$watcher.EnableRaisingEvents = $true
$watcher.NotifyFilter = [System.IO.NotifyFilters]"FileName, LastWrite, Size"

$action = {
    $path = $Event.SourceEventArgs.FullPath
    if ($path -match "\\target\\") {
        return
    }
    if ($path -notmatch "\.(rs|ron|toml|wgsl|glsl|png|jpg|jpeg|hdr|gltf|glb)$") {
        return
    }
    Write-Host "Change detected: $path"
    Request-Restart
}

$subscriptions = @(
    Register-ObjectEvent $watcher Changed -Action $action,
    Register-ObjectEvent $watcher Created -Action $action,
    Register-ObjectEvent $watcher Renamed -Action $action,
    Register-ObjectEvent $watcher Deleted -Action $action
)

try {
    Start-Child
    Write-Host "Watching $watchRoot. Press Ctrl+C to stop."
    while ($true) {
        Start-Sleep -Seconds 1
        if ($script:child -and $script:child.HasExited) {
            Write-Host "Bevy client exited with code $($script:child.ExitCode). Waiting for edits..."
            $script:child = $null
        }
    }
}
finally {
    Stop-Child
    foreach ($subscription in $subscriptions) {
        Unregister-Event -SubscriptionId $subscription.Id -ErrorAction SilentlyContinue
    }
    $watcher.Dispose()
}
