# Release asset plan

This document describes intended assets, not published downloads. The local staging inventory and SHA256SUMS.txt record actual bytes. Never commit installers or signed artifacts.

| Asset | Purpose / readiness |
| --- | --- |
| PACKIT_1.0.0_x64-setup.exe | Per-user Windows x64 NSIS candidate; not Authenticode signed |
| PACKIT_1.0.0_windows-x64-source.tar.gz | Deterministic source candidate, matched to installer via build-record and source-tree digest |
| PACKIT_1.0.0_legal-materials.zip | Reviewable copy of LICENSE, third-party notices and legal/source documentation; also embedded in installer/source |
| SHA256SUMS.txt | Hashes of actual staged assets; regenerate after any rebuild or signing that changes bytes |
| build-record.json | Local build, target, toolchain and artifact pairing; local review material |
| asset-inventory.json | Actual assets, hashes, byte counts and pending signature status; local review material |
| PACKIT_1.0.0_x64-setup.exe.sig | Pending owner-only signing; public detached updater signature |
| latest.json | Pending real signature, official verification and release approval; windows-x86_64 manifest |

Production checks are enabled; feed availability awaits owner-approved public stable publication. Endpoint: https://github.com/krish-agarwal-11/PACKIT/releases/latest/download/latest.json

Manifest artifact URL: https://github.com/krish-agarwal-11/PACKIT/releases/download/v1.0.0/PACKIT_1.0.0_x64-setup.exe

Neither URL is claimed to contain assets. Use direct release assets, not repository-root binaries. Owner publication/visibility/tag decisions are separate approvals. Preserve the stable application identity org.packit.desktop, currentUser NSIS scope and ownership-checked shell lifecycle for future signed updates.

The source archive contains versioned source/build scripts, locks, notices, approved PNG artwork, Windows-selected Cargo source, runtime npm archives/preferred source and exact native source. General compiler/SDK/npm build tools remain prerequisites. Uncertain RAR fixtures and all experiments are excluded; optional fixture recovery is documented. Recipient-side clean reconstruction and final source delivery remain required.
