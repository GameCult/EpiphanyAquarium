param(
    [string]$Version = "0.7.7",
    [string]$LogPath = ".epiphany-aquarium\dx-install.log",
    [string]$StatusPath = ".epiphany-aquarium\dx-install-status.json"
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$log = Join-Path $repoRoot $LogPath
$status = Join-Path $repoRoot $StatusPath
$tools = Join-Path $repoRoot ".epiphany-aquarium\tools"
$dxRoot = Join-Path $tools "dx-$Version"
$dxExe = Join-Path $dxRoot "dx.exe"

function Write-InstallStatus($state, $detail, $exitCode = $null) {
    [ordered]@{
        state = $state
        detail = $detail
        exitCode = $exitCode
        updatedUtc = (Get-Date).ToUniversalTime().ToString("o")
        log = $log
        dx = $dxExe
    } | ConvertTo-Json -Compress | Set-Content -LiteralPath $status -Encoding UTF8
}

$existing = Get-Command dx -ErrorAction SilentlyContinue
if ($existing) {
    Write-InstallStatus "completed" "dx already installed at $($existing.Source)" 0
    & dx --version
    exit 0
}

if (Test-Path $dxExe) {
    Write-InstallStatus "completed" "dx already installed at $dxExe" 0
    & $dxExe --version
    exit 0
}

try {
    Write-InstallStatus "running" "downloading Dioxus CLI $Version"
    New-Item -ItemType Directory -Force $tools | Out-Null
    $assetBase = "https://github.com/DioxusLabs/dioxus/releases/download/v$Version"
    $zip = Join-Path $tools "dx-x86_64-pc-windows-msvc-$Version.zip"
    "[$((Get-Date).ToUniversalTime().ToString("o"))] downloading $assetBase/dx-x86_64-pc-windows-msvc.zip" |
        Set-Content -LiteralPath $log -Encoding UTF8
    Invoke-WebRequest -Uri "$assetBase/dx-x86_64-pc-windows-msvc.zip" -OutFile $zip
    Remove-Item -Recurse -Force $dxRoot -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $dxRoot | Out-Null
    Expand-Archive -LiteralPath $zip -DestinationPath $dxRoot -Force
    $downloaded = Get-ChildItem -Path $dxRoot -Filter dx.exe -Recurse | Select-Object -First 1
    if (-not $downloaded) {
        throw "Downloaded Dioxus CLI archive did not contain dx.exe"
    }
    if ($downloaded.FullName -ne $dxExe) {
        Copy-Item -LiteralPath $downloaded.FullName -Destination $dxExe -Force
    }
    Write-InstallStatus "completed" "Dioxus CLI downloaded" 0
    & $dxExe --version
} catch {
    $_ | Out-String | Add-Content -LiteralPath $log -Encoding UTF8
    Write-InstallStatus "failed" $_.Exception.Message 1
    exit 1
}
