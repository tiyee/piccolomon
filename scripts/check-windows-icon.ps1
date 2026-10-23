# Load the actual EXE resource with the same ID used by GPUI, without running it.
[CmdletBinding()]
param([Parameter(Mandatory)][string]$Binary)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Icon resource verification requires Windows.' }

if (-not ('Ryne.Packaging.IconCheck' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;

namespace Ryne.Packaging {
    public static class IconCheck {
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        static extern IntPtr LoadLibraryExW(string path, IntPtr file, uint flags);
        [DllImport("kernel32.dll")]
        static extern bool FreeLibrary(IntPtr module);
        [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        static extern IntPtr LoadImageW(IntPtr module, IntPtr name, uint type, int cx, int cy, uint flags);
        [DllImport("user32.dll")]
        static extern bool DestroyIcon(IntPtr icon);

        public static void Verify(string path) {
            // Map as data/image resources only; never execute the application's code.
            IntPtr module = LoadLibraryExW(path, IntPtr.Zero, 0x02 | 0x20);
            if (module == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
            try {
                foreach (int size in new int[] { 16, 20, 24, 32, 40, 48, 64, 128, 256 }) {
                    IntPtr icon = LoadImageW(module, new IntPtr(1), 1, size, size, 0);
                    if (icon == IntPtr.Zero)
                        throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot load Ryne icon resource 1 at " + size + "px");
                    DestroyIcon(icon);
                }
            } finally {
                FreeLibrary(module);
            }
        }
    }
}
'@
}
[Ryne.Packaging.IconCheck]::Verify((Resolve-Path -LiteralPath $Binary).Path)
Write-Output "Verified embedded Ryne icon: $Binary"
