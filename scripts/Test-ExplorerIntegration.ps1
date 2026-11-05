# SPDX-License-Identifier: GPL-3.0-only
# SPDX-FileCopyrightText: 2026 Krish Agarwal
param([switch]$Visible, [string]$Executable = (Join-Path $PSScriptRoot '..\src-tauri\target\debug\packit.exe'))
$ErrorActionPreference = 'Stop'
if (-not $Visible) {
    Write-Host 'No registry changes made. Run with -Visible for a temporary Explorer test; entries are removed in finally after you press Enter.'
    exit 0
}
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$owner = 'HKCU:\Software\Classes\PACKIT.Dev.ShellOwner'
if (Test-Path -LiteralPath $owner) { throw 'A PACKIT development registration already exists. Remove it with its owning executable --shell-unregister --dev before testing.' }
try {
    $result = Start-Process -FilePath $Executable -ArgumentList '--shell-register','--dev' -WindowStyle Hidden -Wait -PassThru
    if ($result.ExitCode -ne 0) { throw 'Development shell registration failed.' }
    Write-Host 'Use Explorer > Show more options. Test entries marked (development), including a mixed multi-selection. Default apps are unchanged.'
    Read-Host 'Press Enter to remove the development entries' | Out-Null
} finally {
    $result = Start-Process -FilePath $Executable -ArgumentList '--shell-unregister','--dev' -WindowStyle Hidden -Wait -PassThru
    if ($result.ExitCode -ne 0) { Write-Warning "Cleanup failed. Run: & '$Executable' --shell-unregister --dev" }
}
