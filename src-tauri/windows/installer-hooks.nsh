; SPDX-License-Identifier: GPL-3.0-only
; SPDX-FileCopyrightText: 2026 Krish Agarwal
; Supported Tauri NSIS hooks, current-user only; never alter UserChoice.
!define PACKIT_HOOK_DIR "${__FILEDIR__}"
!include "${PACKIT_HOOK_DIR}\..\vendor\shell\installer-shell.nsh"

; Tauri's default helper force-closes the application in silent/passive mode.
; Override only that supported utility macro: defer instead of killing archive work.
!macroundef CheckIfAppIsRunning
!macro CheckIfAppIsRunning executablePath productName
  !define PackitCheckID ${__LINE__}
  packit_check_again_${PackitCheckID}:
  StrCpy $R0 "${executablePath}"
  IfFileExists "$PLUGINSDIR\packit-maintenance.exe" 0 +2
    StrCpy $R0 "$PLUGINSDIR\packit-maintenance.exe"
  ExecWait '"$R0" --installer-check "$INSTDIR"' $0
  ${If} $0 != 0
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "PACKIT is still running or the installation path cannot be validated. Close PACKIT and its operation windows, then Retry." /SD IDCANCEL IDRETRY packit_check_again_${PackitCheckID}
    SetErrorLevel 1603
    Abort
  ${EndIf}
  !undef PackitCheckID
!macroend

!macro NSIS_HOOK_PREINSTALL
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "This private PACKIT build requires Windows 10 or Windows 11 x64." /SD IDOK
    SetErrorLevel 1603
    Abort
  ${EndIf}
  System::Call 'kernel32::IsWow64Process2(p -1, *i .r0, *i .r1) i .r2'
  IntOp $1 $1 & 0xffff
  ${If} $2 == 0
  ${OrIf} $1 != 34404
    MessageBox MB_ICONSTOP "This PACKIT installer supports native Windows x64 only." /SD IDOK
    SetErrorLevel 1603
    Abort
  ${EndIf}
  ; The build value is REG_SZ on Windows; read it as a string for comparison.
  ReadRegStr $0 HKLM "SOFTWARE\Microsoft\Windows NT\CurrentVersion" "CurrentBuildNumber"
  ${If} $0 < 18362
    MessageBox MB_ICONSTOP "PACKIT requires Windows 10 version 1903 (build 18362) or newer, including Windows 11." /SD IDOK
    SetErrorLevel 1603
    Abort
  ${EndIf}
  ; No runtime installer is bundled or downloaded: require the Windows Evergreen runtime.
  ReadRegStr $1 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${If} $1 == ""
    ReadRegStr $1 HKCU "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${EndIf}
  ${If} $1 == ""
  ${OrIf} $1 == "0.0.0.0"
    MessageBox MB_ICONSTOP "Microsoft Edge WebView2 Runtime is required. Install Microsoft's Evergreen WebView2 Runtime, then run this installer again. No changes have been made to PACKIT." /SD IDOK
    SetErrorLevel 1603
    Abort
  ${EndIf}
  InitPluginsDir
  File /oname=$PLUGINSDIR\packit-maintenance.exe "${MAINBINARYSRCPATH}"
  File /oname=$PLUGINSDIR\packit-shell.dll "${PACKIT_HOOK_DIR}\..\vendor\shell\${PACKIT_SHELL_FILENAME}"
  packit_preflight_retry:
  ExecWait '"$PLUGINSDIR\packit-maintenance.exe" --installer-check "$INSTDIR" "$PLUGINSDIR\packit-shell.dll"' $0
  ${If} $0 != 0
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "Close PACKIT and all archive operation windows before continuing. A conflicting or unsafe installation must be repaired. No operation will be forcibly terminated." /SD IDCANCEL IDRETRY packit_preflight_retry
    SetErrorLevel 1603
    Abort
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Content-named DLLs are never overwritten while Explorer has them loaded.
  SetOutPath "$INSTDIR\vendor\shell"
  SetOverwrite off
  File /oname=${PACKIT_SHELL_FILENAME} "${PACKIT_HOOK_DIR}\..\vendor\shell\${PACKIT_SHELL_FILENAME}"
  SetOverwrite on
  SetOutPath "$INSTDIR"
  ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --installer-record-shell "$INSTDIR"' $0
  ${If} $0 == 0
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --shell-register' $0
  ${EndIf}
  ${If} $0 != 0
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --shell-unregister' $1
    MessageBox MB_ICONSTOP "PACKIT files were installed, but Explorer registration failed and was removed. Run this installer again to repair, or use Apps & Features to uninstall." /SD IDOK
    SetErrorLevel 1603
    Abort
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  packit_uninstall_preflight:
  ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --installer-check "$INSTDIR"' $0
  ${If} $0 != 0
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "Close PACKIT and all archive operation windows, then retry. Your operations will not be forcibly terminated." /SD IDCANCEL IDRETRY packit_uninstall_preflight
    SetErrorLevel 1603
    Abort
  ${EndIf}
  ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --shell-unregister' $0
  ${If} $0 != 0
    MessageBox MB_ICONSTOP "PACKIT could not remove its owned Explorer commands. No application files were removed. Retry uninstalling." /SD IDOK
    SetErrorLevel 1603
    Abort
  ${EndIf}
  packit_uninstall_shell_retry:
  ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --installer-clean-shell "$INSTDIR"' $0
  ${If} $0 != 0
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "Explorer is still using PACKIT's menu DLL.$\r$\n$\r$\n1. Close all File Explorer windows.$\r$\n2. If needed, press Ctrl + Shift + Esc to open Task Manager.$\r$\n3. Find Windows Explorer, right-click it and select Restart. The taskbar may briefly disappear.$\r$\n4. Return here and click Retry.$\r$\n$\r$\nCancel attempts to restore PACKIT's menu registration and keeps the application installed." /SD IDCANCEL IDRETRY packit_uninstall_shell_retry
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --shell-register' $1
    ${If} $1 != 0
      MessageBox MB_ICONEXCLAMATION "PACKIT remains installed, but its Explorer registration could not be restored. Run the installer again to repair it." /SD IDOK
    ${EndIf}
    SetErrorLevel 1603
    Abort
  ${EndIf}
!macroend
