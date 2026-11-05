# PACKIT v1.0.0 — Windows archive manager (draft)

**Release candidate only. Not published. Owner approval is required.**

PACKIT is a local, free and open-source Windows archive manager with a compact workspace and Explorer integration.

## Included

- Browse, extract and test ZIP/ZIP64, compressed 7z, tested RAR4/RAR5 methods, TAR and TAR.GZ/TGZ.
- Browse and extract supported ISO 9660, Joliet, basic Rock Ridge and UDF images. Image readability/structure checks are not file-content checksum verification.
- Create ZIP, LZMA2 7z, TAR and TAR.GZ archives with applicable compression presets.
- Selective extraction, supported multipart layouts, ZIP ZipCrypto/AES passwords and session reuse.
- Real progress, cancellation, verified staging, Replace / Keep Both / Cancel and best-effort recovery.
- Per-user Windows Explorer integration and compact operation windows.
- Dark, Light and System appearance; signed-updater infrastructure.

## Windows requirements

Windows 10 x64 build 18362+ or Windows 11 x64; Microsoft Edge WebView2 Runtime and a local per-user installation. The owner previously tested Windows 10 Pro 22H2. This 1.0.0 candidate still requires installation acceptance; Windows 11 acceptance has not been performed for this candidate. macOS/Linux builds are not provided.

## Limitations

No RAR creation, encrypted RAR/7z extraction or universal multipart/codec support. Native decoding fails closed if Windows containment cannot be established. Recovery is best effort, not guaranteed atomic rollback. ISO/UDF support is limited to implemented layouts.

Production HTTPS update checks are enabled. The private/unpublished feed is not currently claimed accessible; missing releases, manifests and offline checks report unavailable. Quiet startup checks do not interrupt normal use. Signed updater assets still require owner preparation and approval. Tauri updater signatures do not constitute Windows Authenticode signing. The owner chose to ship without Authenticode signing: Windows may show unknown-publisher or SmartScreen warnings; updater signatures do not remove these warnings.

PACKIT's code is GPL-3.0-only. Third-party components retain their licenses. The documented 7-Zip/unRAR and WebView2-loader interpretation questions remain outstanding public-distribution decisions; this draft is not legal clearance.

See [owner checklist](OWNER_CHECKLIST.md) and [asset plan](ASSET_PLAN.md). Do not publish until their gates are satisfied.

## Intended downloads (not uploaded)

- [Windows x64 installer](https://github.com/krish-agarwal-11/PACKIT/releases/download/v1.0.0/PACKIT_1.0.0_x64-setup.exe)
- [Matching source](https://github.com/krish-agarwal-11/PACKIT/releases/download/v1.0.0/PACKIT_1.0.0_windows-x64-source.tar.gz)
- [Checksums](https://github.com/krish-agarwal-11/PACKIT/releases/download/v1.0.0/SHA256SUMS.txt)

These are planned asset locations, not assertions that downloads exist. Install per-user with an installed WebView2 Runtime. Compare the published SHA-256 before running setup. Preserve the legal materials alongside source. Initial public release is v1.0.0 only; never publish the private 0.1.0/0.1.1 test builds.
