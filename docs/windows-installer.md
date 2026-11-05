# Private Windows installer and future upgrade contract

This stage builds a **private, unsigned PACKIT 0.1.0 Windows 10/11 x64 NSIS installer**. It is not public release approval. The two questions in legal/RELEASE_REVIEW.md remain open; no new licensing investigation or updater is performed.

## Stable identity

Keep productName PACKIT, bundle identifier org.packit.desktop, publisher Krish Agarwal, executable packit.exe, currentUser scope, and production COM class identities stable. Tauri metadata remains authoritative for About/sidebar and the installer. Default installation is %LOCALAPPDATA%\PACKIT; normal installation and shell registration require no elevation. Do not run this current-user installer under another administrator account.

Use `npm ci`, then `npm run installer:build` from a Windows x64 MSVC development shell with Cargo on PATH. Both app and shell use .cargo/config.toml static CRT linkage. The desktop build explicitly compiles the shell in release mode. Tauri supplies NSIS (zlib installer compression), normal upgrade detection, Apps & Features registration, Start menu shortcut, optional desktop shortcut and the uninstaller. No independent installation/update service is used.

## Native dependencies and legal resources

Verified unmodified 7-Zip 26.05 exe/DLL/source/notices remain under vendor/7zip. Production content-named COM DLLs are copied by supported NSIS hooks, with an ownership ledger, rather than Tauri's unconditional resource overwrite; this permits reuse of a byte-identical loaded DLL and distinct filenames for changed builds. Shell notices and manifest remain ordinary resources.

Windows 10 x64 version 1903 (build 18362) or newer, including Windows 11 x64, and an existing Evergreen WebView2 Runtime are required. The preinstall hook checks the OS/native architecture and runtime registry record, and fails before replacing PACKIT files if absent. No runtime installer, VC redist, network download, startup task, background service or telemetry is installed. App/shell PE imports are checked for unexpected VCRUNTIME/MSVCP dependencies. This uses existing Windows WebView2, not a bundled fixed runtime.

Root LICENSE (official GPLv3) is the installer license page. LICENSE, THIRD_PARTY_NOTICES.md and the complete legal directory are installed without changing upstream terms. About offers built-in readers for the exact embedded license and notices; the corresponding disk files and source instructions remain accessible without network access. Original source remains GPL-3.0-only; separate native components retain their licenses.

## Upgrade, repair and uninstall

Preflight checks the exact installed application path and refuses while PACKIT or its operation/server processes are running. It never kills archive work or Explorer. The hooks override Tauri's force-close utility macro with the same non-terminating preflight, including silent/passive update mode. Tauri's normal reinstall/upgrade machinery and same-directory update mode are retained; no version bump or test of a nonexistent older installed version is claimed. Settings are not touched by hooks. Normal uninstall preserves data unless the user explicitly selects Tauri's delete-app-data checkbox.

Production registration runs from the installed executable, uses installed content-named DLLs, and is idempotent. It does not call temporary dev scripts, select defaults, write UserChoice, advertise ISO as a default PACKIT type, or replace unrelated associations. Registration failure attempts owned unregistration, aborts with nonzero status and leaves installed files available for repair/uninstall; this is not an atomic install rollback.

Uninstall first checks active operations, removes only owned registrations, then deletes only exact DLL names from its installer-created ledger. Unsafe paths/reparse points and invalid ledger names are rejected. If Explorer still holds a DLL, uninstall offers Retry/Cancel; close Explorer windows or restart Explorer manually and Retry. Cancel attempts to restore owned registration. Explorer is never forcibly restarted. All recorded historical DLLs are handled, so changed-content upgrades do not leave old registered paths. Unexpected errors/locked files may require repair/retry; no blanket recursive install-directory deletion is added. Unknown user files are preserved.

## Future official signed updater

Retain Tauri NSIS format, identity/scope/install location, version metadata and these hooks. Tauri's update mode must run the same preflight/registration and preserve settings. A future updater must defer while archive operations are active, close the main app and popups gracefully, and handle preflight failure without killing work. Content-named shell binaries avoid overwrite locks; uninstall can still require releasing Explorer's loaded DLL. Silent/passive failures cancel with a nonzero code rather than looping on prompts. Do not bypass locked-file failures.

No updater plugin, signatures/keys, placeholder public key, endpoints, GitHub assets or automatic network check is included. Future signed artifacts, actual repository metadata, user approval and manual installer fallback remain later work. Standard Windows code-signing/SmartScreen trust is also not established by this private unsigned build.

## Manual Windows 10/11 acceptance

1. Close PACKIT, run the private installer as the normal user; check publisher/name/icon, full GPL page, Start menu and optional desktop shortcut, and Apps & Features.
2. Launch from the installed shortcut. Check v0.1.0 in About/footer, both legal readers, themes and preserved branding.
3. Open/extract ZIP, compressed 7z/RAR, ISO/UDF; create ZIP/7z. Exercise passwords, selected extraction, conflicts, cancellation and existing-file preservation.
4. From Explorer test dynamic Open/Extract/Create entries, Unicode/multiple selections and compact popups with the main app closed. Confirm ISO's Windows icon/Mount/default remain unchanged.
5. Reinstall the same private version: verify one Apps entry/menu set and preserved settings. A real newer-version upgrade requires a separately approved later build; it is not simulated here.
6. Attempt upgrade/uninstall with active PACKIT windows; confirm preflight blocks without killing work. Close them and retry. If Explorer holds its DLL during uninstall, follow the explicit Retry guidance.
7. Uninstall without selecting data deletion. Confirm shortcuts, files and owned registrations disappear, settings/unrelated files/other handlers remain. Test missing-runtime, partial-failure and old-version upgrade paths in a disposable Windows VM.

Automated build/resource inspection does not replace these real install/upgrade/uninstall checks. Do not alter real user registrations/defaults merely for automated validation.

Build-host observation: automated tooling reports Windows 10 Pro x64 build 19045 with WebView2 156.0.4314.58. The corrected installer accepts this Windows 10 build. Automated payload/worker checks run on build 19045; actual installation/uninstallation remains manual. Build 18362 and Windows 11 were not exercised on physical machines/VMs in this correction.

Payload verification accounts for Tauri’s intentional `__TAURI_BUNDLE_TYPE_VAR_UNK` → `NSS` patch in the packaged application copy; the unbundled build executable is restored to UNK by Tauri. Every other application byte must match. The packaged-copy hash and original-build hash are recorded separately.

## Targeted Windows 10 compatibility correction

The installer minimum is now build 18362 (Windows 10 1903). Native AMD64, WebView2, active-process, ownership, DLL replacement and security checks are unchanged. IsWow64Process2 requires Windows 10 1709; LPAC process attributes are available from Windows 10 1703. AppContainer, Job Objects and the selected mitigations predate the new floor. No inspected application API requires Windows 11. Native containment still fails closed if the OS/policy cannot establish it. The existing static CRT/Windows system UCRT arrangement is unchanged.

References: [IsWow64Process2 requirements](https://learn.microsoft.com/en-us/windows/win32/api/wow64apiset/nf-wow64apiset-iswow64process2), [process attributes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute). Compatibility eligibility is not a claim of completed UI acceptance on every Windows 10 release. A compatible installed Evergreen WebView2 Runtime remains mandatory; none is downloaded by PACKIT.

## Stage 5 status

The owner confirmed Stage 4 hands-on Windows 10 installation, archive operations, Explorer commands, same-version reinstallation, settings preservation and uninstall. Windows 11 remains untested. Stage 5 preserves those mechanisms and expands the DLL-in-use warning with Close Explorer → Ctrl+Shift+Esc → restart Windows Explorer → Retry, including the brief taskbar disappearance. Retry/Cancel and owned registration restoration remain unchanged. The official updater is integrated but its private feed is disabled. Stage 5 UI wording and a genuine owner-signed update require the focused manual acceptance described in updates-and-releases.md.
