[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidatePattern('^\d{1,5}\.\d{1,5}\.\d{1,5}\.0$')]
    [string]$Version,
    [Parameter(Mandatory)]
    [ValidatePattern('^[A-Za-z0-9.-]{3,50}$')]
    [string]$IdentityName,
    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$Publisher,
    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$PublisherDisplayName,
    [ValidateSet('x86', 'x64', 'arm64')]
    [string]$Architecture = 'x64'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'MSIX packaging requires Windows and PowerShell 7.' }
if (@($Version.Split('.') | Where-Object { [int]$_ -gt 65535 }).Count) {
    throw 'Each MSIX version component must be between 0 and 65535.'
}
if ([string]::IsNullOrWhiteSpace($Publisher) -or [string]::IsNullOrWhiteSpace($PublisherDisplayName)) {
    throw 'Publisher and PublisherDisplayName must come from Partner Center > Product identity.'
}

$root = Split-Path $PSScriptRoot -Parent
$cargoCommand = if ($env:CARGO) { $env:CARGO } else { 'cargo' }
$sdkBin = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
$makeAppx = Get-ChildItem -Path (Join-Path $sdkBin '*/x64/makeappx.exe') -File |
    Sort-Object { [version]$_.Directory.Parent.Name } -Descending |
    Select-Object -First 1
if (-not $makeAppx) { throw 'Install the Windows 10/11 SDK (makeappx.exe is required).' }
$makePri = Join-Path $makeAppx.Directory.FullName 'makepri.exe'
if (-not (Test-Path -LiteralPath $makePri -PathType Leaf)) {
    throw 'Install the Windows SDK UWP tools (makepri.exe is required).'
}

# Reuse the portable build, static CRT settings and original font notices.
$Architecture = $Architecture.ToLowerInvariant()
$portableName = "Ryne-store-input-$Architecture"
& (Join-Path $PSScriptRoot 'bundle-windows.ps1') -Profile release `
    -Architecture $Architecture -PackageName $portableName
$work = Join-Path ([System.IO.Path]::GetTempPath()) ('ryne-msix-' + [guid]::NewGuid())
$output = Join-Path $root 'dist/store'
$packageName = "Ryne-$Version-$Architecture.msix"
try {
    Expand-Archive -LiteralPath (Join-Path $root "dist/$portableName.zip") -DestinationPath $work
    $payload = Join-Path $work 'Ryne'
    $assets = Join-Path $payload 'Assets'
    $binary = Join-Path $payload 'Ryne.exe'
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw 'MSIX payload is missing Ryne.exe.' }
    Push-Location $root
    try {
        # The logo generator runs on the x64 packaging host for every target.
        & $cargoCommand run --locked --package ryne-app --example store-assets --release `
            --target x86_64-pc-windows-msvc -- `
            (Join-Path $root 'assets/logo.svg') $assets
        if ($LASTEXITCODE -ne 0) { throw "Store logo generation failed with exit code $LASTEXITCODE." }
    } finally {
        Pop-Location
    }

    $manifest = [System.Xml.XmlDocument]::new()
    $manifest.Load((Join-Path $root 'packaging/windows/AppxManifest.xml'))
    $namespaces = [System.Xml.XmlNamespaceManager]::new($manifest.NameTable)
    $namespaces.AddNamespace('p', $manifest.DocumentElement.NamespaceURI)
    $identity = $manifest.SelectSingleNode('/p:Package/p:Identity', $namespaces)
    $identity.SetAttribute('Name', $IdentityName)
    $identity.SetAttribute('Publisher', $Publisher)
    $identity.SetAttribute('Version', $Version)
    $identity.SetAttribute('ProcessorArchitecture', $Architecture)
    $manifest.SelectSingleNode('/p:Package/p:Properties/p:PublisherDisplayName', $namespaces).InnerText = $PublisherDisplayName
    $manifest.Save((Join-Path $payload 'AppxManifest.xml'))

    # Index the manifest logos with the real package identity for shell lookup.
    $priConfig = Join-Path $work 'priconfig.xml'
    & $makePri createconfig /cf $priConfig /dq lang-en-US /pv 10.0.0 /o
    if ($LASTEXITCODE -ne 0) { throw "MakePri configuration failed with exit code $LASTEXITCODE." }
    & $makePri new /pr $payload /cf $priConfig /mn (Join-Path $payload 'AppxManifest.xml') `
        /of (Join-Path $payload 'resources.pri') /o
    if ($LASTEXITCODE -ne 0) { throw "MakePri indexing failed with exit code $LASTEXITCODE." }
    if (-not (Test-Path -LiteralPath (Join-Path $payload 'resources.pri') -PathType Leaf)) {
        throw 'MakePri did not produce resources.pri.'
    }

    New-Item -ItemType Directory -Path $output -Force | Out-Null
    $package = Join-Path $output $packageName
    # Keep MakeAppx semantic validation enabled. Store signs the submitted MSIX.
    & $makeAppx.FullName pack /d $payload /p $package /o
    if ($LASTEXITCODE -ne 0) { throw "MakeAppx failed with exit code $LASTEXITCODE." }
    if (-not (Test-Path -LiteralPath $package -PathType Leaf)) { throw 'MakeAppx did not produce an MSIX.' }
    $hash = (Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $packageName" | Set-Content -LiteralPath "$package.sha256" -Encoding utf8NoBOM
    Copy-Item -LiteralPath (Join-Path $payload 'AppxManifest.xml') -Destination $output -Force
} finally {
    if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force }
}
Write-Output $package
