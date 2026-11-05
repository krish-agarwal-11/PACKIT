<p align="center"><img src="branding/packit-logo.png" width="100" alt="PACKIT logo"></p>

# PACKIT

**Pack, unpack, browse and manage your archives with ease.** PACKIT is a free, open-source Windows desktop archive manager built with Tauri, Rust and React. Archive processing stays local—no account, archive upload or telemetry.

**v1.0.0 release candidate — not published or approved for public distribution.**

## Download for Windows

### [Download / view GitHub Releases](https://github.com/krish-agarwal-11/PACKIT/releases)

The link points to the real Releases page. The repository is currently private and the v1.0.0 installer is **not uploaded**. Once approved and published, download the direct `PACKIT_1.0.0_x64-setup.exe` release asset and compare its published SHA-256. Installers belong in Releases, not the source repository.

## Features

- Browse, extract and test ZIP/ZIP64, TAR, TAR.GZ/TGZ, LZMA/LZMA2 7z and tested compressed RAR4/RAR5 methods.
- Read supported ISO 9660, Joliet, basic Rock Ridge and UDF 1.02 images without mounting or executing their contents.
- Create and verify ZIP/ZIP64, LZMA2 7z, TAR and TAR.GZ; choose Fast, Balanced or Maximum where compression applies.
- Selective extraction, ZIP ZipCrypto/AES password sessions, tested multipart layouts, real progress and cancellation.
- Replace Existing Files, numbered Keep Both and staged writes with best-effort rollback.
- Explorer context menus with dynamic filenames, compact operation windows and one main application instance.
- Approved PACKIT branding, compact Hybrid Workspace, Dark/Light/System appearance and keyboard focus support.

Not every archive variant is supported. Encrypted 7z/RAR extraction and RAR creation are unavailable. Integrity testing distinguishes available checksums from structural/readability validation. See the implementation's capability/error messages rather than assuming support from an extension.

## Windows installation and compatibility

Windows 10 x64 **build 18362 or newer**, including Windows 11 x64, is eligible. A compatible installed Evergreen WebView2 Runtime is required; PACKIT does not download a runtime automatically. Installation is per-user by default without administrator elevation, with Start menu integration, optional desktop shortcut and an Apps & Features uninstaller. Windows ISO defaults and Mount are preserved.

The owner has chosen **no Windows Authenticode signing for v1.0.0**. Windows may display unknown-publisher or SmartScreen warnings. Tauri updater signing authenticates update packages and does not remove those warnings.

The owner has manually validated the earlier Windows 10 installer lifecycle and a genuine signed 0.1.0→0.1.1 upgrade, settings/archive/Explorer preservation, offline failure and tampered-package rejection. The earlier **1.0.0 candidate passed owner installation, version, archive and Explorer checks**. This freshly rebuilt production-updater candidate still needs acceptance. Windows 11 and minimum-build 18362 have not received equivalent manual acceptance. No macOS/Linux release binaries are offered.

Close PACKIT operations before installing or upgrading. If uninstall reports a loaded Explorer DLL, follow its Close Explorer / Task Manager / Restart Windows Explorer / Retry instructions; the taskbar may briefly disappear. User settings are preserved by default unless you explicitly select their deletion.

## Updates

About → **Check for Updates** provides manual checks. Once enabled, quiet startup checks use the official Tauri signed updater. Update Now requires your approval, signature and signed-version verification, and idle archive operations. Failures do not claim that the app is up to date or that installation succeeded; verification failures offer Retry/Close and a manual installer route.

**This production-configured candidate enables HTTPS checks against the real GitHub Releases feed.** The repository is still private and no release manifest is claimed to exist. Until an approved stable release and its signed assets are anonymously accessible, checks report unavailable/not-published; quiet startup failures do not interrupt use. The present RC is not claimed to have a live production update feed. See [release activation and signing checklist](release/OWNER_CHECKLIST.md). Updater signatures and Windows Authenticode signing are separate; no Authenticode signing is claimed.

## Build from source

Install Git, a Node.js/npm version supported by the locked frontend toolchain, stable Rust MSVC x64, Microsoft C++ Build Tools plus Windows SDK, and WebView2. [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

```powershell
npm ci
npm run tauri dev
```

Private Windows installer and checks:

```powershell
npm run installer:build
npm run test:ui
npm run lint
npm run format:check
npm run icons:check
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
```

Desktop commands build the original shell extension first. Run `npm run shell:build` before direct Cargo checks on a fresh checkout. Preserve `.cargo/config.toml` static CRT flags. The verified 7-Zip executable/DLL/source and notices are bundled; no separate 7-Zip/WinRAR installation is needed.

Thirteen upstream RAR test fixtures remain local-only because their per-file redistribution permissions are unresolved. Existing local coverage is preserved. For optional, hash-checked restoration see `python scripts/restore_test_fixtures.py --fetch` and [fixture exclusions](legal/distribution-exclusions.json). Downloadability is not redistribution permission.

## Troubleshooting and security

- **WebView2 missing:** install Microsoft's compatible Evergreen Runtime, then retry setup.
- **Native decoder blocked:** native processing fails closed if AppContainer/policy/ACL requirements cannot be established. No unrestricted worker fallback is used. See [security boundaries](docs/security-hardening.md).
- **Update unavailable/offline:** keep using PACKIT, retry later or use the genuine release installer. An unavailable feed is not an up-to-date result.
- **Bad update signature:** nothing is installed; retry a fresh check or use a trusted manual installer. Never disable verification.
- **Conflicts/permissions:** use explicit Replace or Keep Both; preserve recovery information if best-effort rollback reports an incomplete restoration.

The application does not mount images or execute archive contents. Existing resource limits and native isolation are not a claim of perfect sandboxing.

## Licensing and source

PACKIT's original source/artwork are **GPL-3.0-only**, copyright 2026 Krish Agarwal. Read [LICENSE](LICENSE), [third-party notices](THIRD_PARTY_NOTICES.md), [source distribution instructions](legal/SOURCE_DISTRIBUTION.md) and the [release-review ledger](legal/RELEASE_REVIEW.md).

Third-party components keep their terms. Separately bundled 7-Zip LGPL/BSD/unRAR interpretation and static WebView2 loader Corresponding Source/System Library treatment remain outstanding public-distribution decisions. This RC does not claim legal clearance.

Approved releases must supply matching source, notices, build records and checksums alongside signed installer assets. [Draft release notes](release/RELEASE_NOTES.md) and [asset plan](release/ASSET_PLAN.md) are preparation documents, not a published release. No test server, private signing material or Stage 6 HTTP configuration belongs in the release.

## Screenshots

Genuine release-candidate captures are pending owner review; see [capture checklist](docs/screenshots/README.md). No mock screenshots are presented.
