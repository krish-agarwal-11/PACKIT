# Focused licensing validation

Target: Windows 11 x64 first public-release planning. This is a licensing/evidence validation, **not legal certification or release clearance**.

## Checks completed

- Official GNU GPLv3 downloaded from `https://www.gnu.org/licenses/gpl-3.0.txt`: root LICENSE is byte-identical, 35,149 bytes, SHA-256 `3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986`. Required end/application sections are present. It was already the same complete text; no unofficial edits were applied.
- GPL-3.0-only agrees in both first-party Cargo manifests, npm package metadata and root npm lock license fields. Fifty-seven first-party source files carry SPDX identifiers and the owner-confirmed copyright. No GPL-or-later change was made.
- Every third-party name/version in both Cargo lockfiles is represented: 476 unique records. All 476 local registry source archives match their locked SHA-256. Notice bytes were compared with the corresponding archive entries for staged Cargo evidence; no differences were found.
- All 138 npm lock records are represented. Installed package versions/license declarations agree with corresponding lock declarations. Only four cached npm tarballs were available for independent integrity comparison and matched; the remaining installed contents are package evidence, not independently reauthenticated registry archives.
- 625 copied license/NOTICE/SPDX/copyright evidence files are present with recorded hashes, including the observed Rust library copyright collection and development-only pycdlib LGPL text. SPDX metadata is not misrepresented as a complete license document. Common MPL text matches the preserved original cssparser license bytes.
- 67 asset records and four configured native/source artifacts have hashes. Original approved branding bytes match their unchanged source hashes. Fonts were not added; system font references are not bundled font files. Unknown scaffold/old assets are excluded from ownership claims and flagged.
- Existing PE files were read as data (not executed) to record imports; WebView2 native loader version/static-link evidence and the observed Rust toolchain library notice were inspected separately from package metadata.
- Source-distribution documentation distinguishes local availability from actual public delivery. No fabricated public source URL, written offer, permission, publisher email or legal clearance is present.

Run:

```powershell
python scripts/audit_licenses.py check
```

To refresh the local evidence inventory after an authorized dependency change, put Cargo on PATH and run `python scripts/audit_licenses.py generate` (offline, metadata only). Re-run `check`; re-review the legal findings. No application build or unrelated runtime test suite was run for this licensing-only stage.

## Findings deliberately left open

Nineteen package-level findings remain for missing exact texts/attribution or MPL review (including build/source-scope findings). The broader release ledger additionally covers unRAR/aggregation, native Microsoft terms, preferred-source delivery/toolchain mapping, asset/fixture rights and future payload review. The existing package-lock root version is 0.0.0 while package/Tauri/Cargo remain 0.1.0; this pre-existing drift was documented rather than changing version metadata against the stage scope.

The future legal staging structure is prepared. It is not wired into an installer and is not complete distribution approval. No publication, version promotion, installer development or application UI changes were performed.
