# Windows PowerShell 5.1 and PowerShell 7; run beside the portable Ryne.exe.
[CmdletBinding()]
param([switch]$Unregister)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw 'This script requires Windows.'
}

$binary = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot 'Ryne.exe'))
if (-not $Unregister -and -not (Test-Path -LiteralPath $binary -PathType Leaf)) {
    throw "Keep this script beside Ryne.exe: $binary"
}
# REG_SZ, not REG_EXPAND_SZ: percent signs in the portable directory are literal.
# Explorer launches the EXE directly; neither cmd.exe nor PowerShell parses the file.
$command = '"' + $binary + '" "%1"'
$icon = '"' + $binary + '",0'
$extensions = @('.md', '.markdown', '.mdown', '.mkd')

function Set-RegistryText($Root, [string]$Path, [string]$Name, [string]$Value) {
    $key = $Root.CreateSubKey($Path)
    try { $key.SetValue($Name, $Value, [Microsoft.Win32.RegistryValueKind]::String) }
    finally { $key.Dispose() }
}

function Remove-OwnedRegistration($Root, [string]$Path) {
    $key = $Root.OpenSubKey("$Path\shell\open\command")
    if ($null -eq $key) { return }
    try { $owned = $key.GetValue('') -eq $command }
    finally { $key.Dispose() }
    if ($owned) { $Root.DeleteSubKeyTree($Path, $false) }
}

# Per-user registrations require no administrator rights and never write
# extension defaults, UserChoice, or another application's ProgID.
$classes = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Software\Classes')
try {
    $application = 'Applications\Ryne.exe'
    if ($Unregister) {
        Remove-OwnedRegistration $classes $application
    } else {
        Set-RegistryText $classes $application 'FriendlyAppName' 'Ryne'
        Set-RegistryText $classes "$application\DefaultIcon" '' $icon
        Set-RegistryText $classes "$application\shell\open" 'MultiSelectModel' 'Single'
        Set-RegistryText $classes "$application\shell\open\command" '' $command
        foreach ($extension in $extensions) {
            Set-RegistryText $classes "$application\SupportedTypes" $extension ''
        }
    }

    foreach ($extension in $extensions) {
        $verb = "SystemFileAssociations\$extension\shell\Ryne.Open"
        if ($Unregister) {
            $key = $classes.OpenSubKey("$verb\command")
            if ($null -ne $key) {
                try { $owned = $key.GetValue('') -eq $command }
                finally { $key.Dispose() }
                # An older portable copy must not remove a newer copy's entry.
                if ($owned) { $classes.DeleteSubKeyTree($verb, $false) }
            }
        } else {
            Set-RegistryText $classes $verb '' 'Open with Ryne'
            Set-RegistryText $classes $verb 'Icon' $icon
            Set-RegistryText $classes $verb 'MultiSelectModel' 'Single'
            Set-RegistryText $classes "$verb\command" '' $command
        }
    }
} finally { $classes.Dispose() }

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class RyneShellNotification {
    [DllImport("shell32.dll")]
    public static extern void SHChangeNotify(uint eventId, uint flags, IntPtr item1, IntPtr item2);
}
'@
# SHCNE_ASSOCCHANGED / SHCNF_IDLIST: refresh Explorer's association cache.
[RyneShellNotification]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)
if ($Unregister) {
    Write-Output 'Removed context-menu registrations owned by this Ryne folder.'
} else {
    Write-Output 'Registered Open with Ryne for Markdown files for the current user.'
    Write-Output 'On Windows 11, look under Show more options if needed.'
}
