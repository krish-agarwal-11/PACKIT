# SPDX-License-Identifier: GPL-3.0-only
# SPDX-FileCopyrightText: 2026 Krish Agarwal
param([switch]$Apply, [switch]$Development, [string]$Executable = (Join-Path $PSScriptRoot '..\src-tauri\target\debug\packit.exe'))
$ErrorActionPreference = 'Stop'
if (-not $Apply) {
    Write-Host 'No changes made. Use -Apply with the executable that owns the existing registration; add -Development for development entries.'
    Write-Host 'Removes only PACKIT-owned ISO association metadata. Never modifies UserChoice, another handler, or Windows mounting.'
    exit 0
}
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$arguments = @('--shell-repair-iso')
if ($Development) { $arguments += '--dev' }
$result = Start-Process -FilePath $Executable -ArgumentList $arguments -WindowStyle Hidden -Wait -PassThru
if ($result.ExitCode -ne 0) { throw 'PACKIT ISO association repair failed.' }
Write-Host 'Owned registration checked and association-change notification sent. No default app was selected.'
Write-Host 'If Windows still selects PACKIT for ISO, use Open with > Choose another app to select Windows Explorer. Protected UserChoice is intentionally unchanged.'
