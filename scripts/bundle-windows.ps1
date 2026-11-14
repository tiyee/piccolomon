# Build a portable Windows package; no installer or signing credentials required.
[CmdletBinding()]
param(
    [ValidateSet('debug', 'release')]
    [string]$Profile = 'release',
    [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9._-]*$')]
    [string]$PackageName,
    [ValidateSet('x86', 'x64', 'arm64')]
    [string]$Architecture = 'x64'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not $IsWindows) {
    throw 'This bundle script requires Windows and PowerShell 7.'
}

$root = Split-Path $PSScriptRoot -Parent
$Architecture = $Architecture.ToLowerInvariant()
$target = @{
    x86 = 'i686-pc-windows-msvc'
    x64 = 'x86_64-pc-windows-msvc'
    arm64 = 'aarch64-pc-windows-msvc'
}[$Architecture]
if (-not $PackageName) { $PackageName = "Ryne-windows-$Architecture" }
$cargoCommand = if ($env:CARGO) { $env:CARGO } else { 'cargo' }

# GPUI 0.2.2 compiles release shaders with fxc.exe. Discover the installed
# Windows SDK instead of relying on GPUI's hard-coded SDK version fallback.
if ($Profile -eq 'release') {
    if (-not $env:GPUI_FXC_PATH) {
        $sdkBin = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
        $fxc = Get-ChildItem -Path (Join-Path $sdkBin '*/x64/fxc.exe') -File |
            Sort-Object { [version]$_.Directory.Parent.Name } -Descending |
            Select-Object -First 1
        if (-not $fxc) {
            throw 'fxc.exe not found. Install the Windows 10/11 SDK with Visual Studio Build Tools.'
        }
        $env:GPUI_FXC_PATH = $fxc.FullName
    }
    if (-not (Test-Path -LiteralPath $env:GPUI_FXC_PATH -PathType Leaf)) {
        throw "Invalid GPUI_FXC_PATH: $env:GPUI_FXC_PATH"
    }
}

$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root 'target' }
if (-not [System.IO.Path]::IsPathRooted($targetDir)) {
    $targetDir = Join-Path $root $targetDir
}

# Use an explicit target so CRT flags do not affect host proc macros.
$buildArgs = @('build', '--locked', '--manifest-path', (Join-Path $root 'Cargo.toml'),
    '--package', 'ryne-app', '--bin', 'ryne', '--target', $target)
if ($Profile -eq 'release') { $buildArgs += '--release' }
$flagsVariable = 'CARGO_TARGET_' + $target.ToUpperInvariant().Replace('-', '_') + '_RUSTFLAGS'
$flagsPath = "Env:$flagsVariable"
$previousFlags = [Environment]::GetEnvironmentVariable($flagsVariable)
$staticCrt = '-C target-feature=+crt-static'
try {
    if ($previousFlags -ne $staticCrt) {
        Set-Item -LiteralPath $flagsPath -Value ("$previousFlags $staticCrt".Trim())
    }
    Push-Location $root
    try {
        & $cargoCommand @buildArgs
        if ($LASTEXITCODE -ne 0) { throw "Cargo build failed with exit code $LASTEXITCODE." }
    } finally {
        Pop-Location
    }
} finally {
    Set-Item -LiteralPath $flagsPath -Value $previousFlags
}

$binary = Join-Path $targetDir "$target/$Profile/ryne.exe"
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
    throw "Missing executable: $binary"
}
& (Join-Path $PSScriptRoot 'check-windows-icon.ps1') -Binary $binary

$dist = Join-Path $root 'dist'
$staging = Join-Path $targetDir ('.ryne-package-' + [guid]::NewGuid())
$bundle = Join-Path $staging 'Ryne'
$archive = Join-Path $dist "$PackageName.zip"
try {
    $notices = Join-Path $bundle 'licenses/stix-two-math'
    New-Item -ItemType Directory -Path $notices, $dist -Force | Out-Null
    Copy-Item -LiteralPath $binary -Destination (Join-Path $bundle 'Ryne.exe')
    foreach ($script in @('register-context-menu.ps1', 'Register-context-menu.cmd', 'Unregister-context-menu.cmd')) {
        Copy-Item -LiteralPath (Join-Path $root "packaging/windows/$script") -Destination $bundle
    }
    foreach ($notice in @('OFL.txt', 'FONTLOG.txt')) {
        Copy-Item -LiteralPath (Join-Path $root "assets/fonts/stix-two-math/$notice") -Destination $notices
    }
    $leteNotices = Join-Path $bundle 'licenses/lete-sans-math'
    New-Item -ItemType Directory -Path $leteNotices -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $root 'assets/fonts/lete-sans-math/OFL.txt') -Destination $leteNotices
    Copy-Item -LiteralPath (Join-Path $root 'README.md'), (Join-Path $root 'README.zh-CN.md') -Destination $bundle
    Compress-Archive -LiteralPath $bundle -DestinationPath $archive -Force
    $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $PackageName.zip" | Set-Content -LiteralPath "$archive.sha256" -Encoding utf8NoBOM
} finally {
    if (Test-Path -LiteralPath $staging) { Remove-Item -LiteralPath $staging -Recurse -Force }
}
Write-Output $archive
