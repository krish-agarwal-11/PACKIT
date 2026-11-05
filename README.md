# PACKIT

A free, open-source desktop archive manager for Windows, built with Tauri 2, Rust, React and TypeScript. Archive processing stays on your computer. No account, telemetry or archive upload is required.

**Private development version: 0.1.0.** Repository: https://github.com/krish-agarwal-11/PACKIT. No public release or final legal clearance is claimed.

## Features

- Browse, extract and test ZIP/ZIP64, TAR, TAR.GZ/TGZ, LZMA/LZMA2 7z and tested compressed RAR4/RAR5 methods.
- Read supported ISO 9660, Joliet, basic Rock Ridge and UDF 1.02 images without mounting or executing them.
- Create ZIP/ZIP64, LZMA2 7z, TAR and TAR.GZ archives with real compression presets and post-creation validation.
- Selective extraction, ZIP ZipCrypto/AES password sessions, tested multipart layouts, real progress and cancellation.
- Replace Existing Files, numbered Keep Both and safe staging with best-effort rollback.
- Classic Explorer context menus with dynamic filenames, compact operation windows and one main application instance.
- Dark, Light and System appearance using the approved PACKIT artwork.

Formats have explicit limits; recognition does not imply every method or variant is supported. Encrypted 7z/RAR extraction and RAR creation are not implemented. Native workers require low-privilege AppContainer isolation and resource limits; no unrestricted fallback is used. See [security boundaries](docs/security-hardening.md).

## Platform status

Windows 10 x64 build 18362 or newer and Windows 11 x64 are eligible. Hands-on Stage 4 installation, archive operations, Explorer integration, reinstallation, settings preservation and uninstallation passed on the owner's Windows 10 PC. Windows 11 and the minimum 18362 build have not received equivalent installer acceptance. macOS/Linux release packaging is not prepared or verified.

An installed, compatible Evergreen WebView2 Runtime is required. The installer checks prerequisites rather than downloading a runtime. App/shell builds do not require a separately installed VC redistributable. See [installer and upgrade behavior](docs/windows-installer.md).

## Development

Install Git, Node.js/npm supported by the locked frontend toolchain, stable Rust (MSVC x64), Microsoft C++ Build Tools and a Windows SDK, plus WebView2 Runtime. [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

```powershell
npm ci
npm run tauri dev
```

Production frontend and private installer:

```powershell
npm run build
npm run installer:build
```

The desktop commands build the original shell extension before compiling PACKIT. For direct Cargo checks on a fresh checkout, run `npm run shell:build` first. The verified 7-Zip binaries/source and notices are supplied; no user installation of 7-Zip or WinRAR is needed. Preserve `.cargo/config.toml` static CRT settings.

Focused checks:

```powershell
npm run test:ui
npm run lint
npm run format:check
npm run icons:check
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
```

Native isolation/temporary registry probes require appropriate Windows permissions. Thirteen upstream RAR test fixtures with unresolved redistribution permissions are not tracked; existing local copies remain unchanged. See `legal/distribution-exclusions.json` and `python scripts/restore_test_fixtures.py --fetch` for optional version-pinned, hash-checked test-data restoration. Downloadability is not redistribution permission.

## Updates and releases

Official Tauri signed updater integration uses the committed **public** key. Private keys/passwords are never part of the repository. **Automatic release checks are disabled** while the GitHub repository/feed is private or inaccessible. About → Check for Updates explains that state without polling GitHub. Enabling a validated accessible feed requires an intentional developer build; no embedded GitHub credential or unsigned fallback is used.

When enabled, update checks are asynchronous and installation requires user approval, official signature/version verification and idle archive operations. The NSIS installer keeps its non-terminating preflight. See [manual release/signing and private update test procedure](docs/updates-and-releases.md). No release is published automatically.

## Architecture

- `src/`: React interface, archive workflow, shared components and theme styles.
- `src-tauri/src/archive/`: format adapters, bounded parsing, containment and output staging.
- `src-tauri/src/tasks.rs`: operation state/progress/cancellation and update-install exclusion.
- `src-tauri/src/updates.rs`: guarded official updater integration.
- `windows-shell/`: small original COM extension; archive processing never runs inside Explorer.
- `scripts/`: build, validation and source preparation tools.
- `branding/`: authoritative original PNG artwork, unchanged.
- `legal/`: preserved notices, inventories, source materials and review ledger.

## Licensing

PACKIT's original source and artwork are **GPL-3.0-only**, copyright 2026 Krish Agarwal. Read [LICENSE](LICENSE), [third-party notices](THIRD_PARTY_NOTICES.md) and [source-distribution instructions](legal/SOURCE_DISTRIBUTION.md). Third-party components retain their licenses, including the separately bundled 7-Zip LGPL/BSD/unRAR terms. The [documented review questions](legal/RELEASE_REVIEW.md) remain open for public distribution; private development acceptance is not definitive legal clearance. New updater dependency evidence is recorded separately without repeating the broad audit.

Future GitHub Releases must offer matching source archives alongside signed installer/update assets. No public source hosting service, binding written offer or release is claimed. Installer versions, source records and staged assets must be matched before publication.
