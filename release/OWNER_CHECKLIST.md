# v1.0.0 candidate owner checklist

## Publication gates

- Review the two existing interpretation questions in legal/RELEASE_REVIEW.md: separately bundled 7-Zip/unRAR and the static WebView2 loader. No new clearance is asserted.
- Install this exact candidate and complete the Windows checklist below; perform Windows 11 acceptance separately.
- Supply genuine, privacy-reviewed application screenshots (see docs/screenshots/README.md).
- Reconstruct from the paired source on a clean build machine and verify delivery completeness.
- Approve repository visibility, commit/push, tag and publication separately. None is authorized by candidate preparation.

## Production updater activation

The final candidate enables src-tauri/update-policy.json feedReady=true and the normal HTTPS endpoint https://github.com/krish-agarwal-11/PACKIT/releases/latest/download/latest.json. Trusted public-key and signed-version verification are unchanged; no Stage 6 HTTP transport exception is present. No GitHub credentials are embedded. The repository remains private, so no usable feed is claimed today. Unavailable or not-published checks must not be reported as up to date. Startup failures remain quiet.

At owner-approved publication, create v1.0.0 as a stable, non-draft/non-prerelease release and explicitly mark it Latest; upload latest.json and the exact signed installer assets. The /latest/download/name URL resolves the named asset of GitHub's latest release, not a repository file. Drafts/prereleases are excluded from the stable latest lookup. Future stable releases must also include latest.json; otherwise the updater correctly fails unavailable. Keep 0.1.0/0.1.1 private test builds unpublished. Verify anonymously only after separate publication approval.

Evidence: [GitHub release linking](https://docs.github.com/en/repositories/releasing-projects-on-github/linking-to-releases) and [latest release API semantics](https://docs.github.com/en/rest/releases/releases#get-the-latest-release). This batch performs no publication.

## Owner-only signing (do not run through Codex)

Run from the repository root in your own local terminal. The private key stays under your exclusive control. Use the supported interactive password prompt; do not put the password in a command, script, log or chat. No signing environment variables are required with this file-based CLI command. Clear inherited TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD in your own terminal if previously set.

```powershell
npm run tauri -- signer sign --help
npm run tauri -- signer sign --private-key-path "$env:USERPROFILE\.tauri\packit-updater.key" --app-version 1.0.0 "release-staging/1.0.0/PACKIT_1.0.0_x64-setup.exe"
```

Use the existing key, never generate a substitute. The expected public signature is the adjacent .exe.sig file. The owner has chosen no Authenticode signing for v1.0.0; do not obtain a certificate or configure a signing service. SmartScreen/unknown-publisher warnings may remain. Any byte change invalidates the detached updater signature.

Verify the actual public signature against the exact installer and embedded public key without network or installation:

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml updates::tests::verify_owner_release_signature -- --ignored --exact --nocapture
python scripts/prepare_release_manifest.py --version 1.0.0 --installer release-staging/1.0.0/PACKIT_1.0.0_x64-setup.exe --signature release-staging/1.0.0/PACKIT_1.0.0_x64-setup.exe.sig --output release-staging/1.0.0/latest.json
```

The generator checks public signature structure/version only; the preceding official updater test performs cryptographic verification. Never supply a private key to the manifest generator. Do not upload until approval. No manifest is generated with a fabricated signature. Test the final rebuilt installer locally; verify the public feed anonymously after separate publication approval. Stage 6 temporary HTTP configuration must never be merged into production.

## Windows acceptance

1. Install over the existing development installation; confirm 1.0.0 in About/sidebar and one Apps & Features entry. Confirm settings/theme survived.
2. ZIP/7z creation and extraction, RAR extraction, TAR.GZ and ISO/UDF browse/select/extract; check passwords and a supported multipart set.
3. Test Replace / Keep Both / Cancel against disposable existing files, real progress and cancellation; confirm unrelated files survive.
4. Explorer dynamic menus, compact quick ZIP/7z and extraction, single main instance; ISO icon and Mount remain unchanged.
5. Check unavailable feed/offline reporting. In a separately approved signed-feed test, verify invalid signature has a failure heading, no Update Now, and Nothing was installed; Retry performs a fresh check. Never interrupt active archive work.
6. Reinstall/upgrade/uninstall using disposable files; preserve preferences by default, remove only owned shell entries. DLL-in-use Retry/Cancel must work without forced Explorer restart.

Manual installer recovery: close PACKIT normally after archive work finishes, run the approved same/newer installer and follow DLL-in-use instructions. Automatic rollback is not promised. Keep backups. Ordinary UI tests cannot prove native containment boundaries or legal compliance.

## Repeatable local asset preparation

After the installer build and its packaged-payload audit, run `python scripts/prepare_release_candidate.py`. It creates the paired source, exact build record, legal ZIP, inventory and checksums under ignored release-staging/1.0.0. It refuses to replace a staging directory containing an owner .sig or latest.json. Do not rerun after signing without preserving and explicitly superseding the signed candidate. Source pairing must be refreshed after any source/build/configuration change.

## Genuine manifest and checksums

The manifest generator emits version `1.0.0`, notes and platforms.windows-x86_64 with the exact GitHub v1.0.0 installer URL and the genuine public .exe.sig content. Windows NSIS uses the installer EXE itself as the updater payload; no legacy .nsis.zip is required by this installed Tauri 2 version. The detached .sig is base64 minisign text and binds the app version via --app-version. No latest.json exists until owner signing succeeds.

After official signature verification and manifest generation, refresh the asset inventory/checksums without touching installer/source bytes:

```powershell
Get-FileHash release-staging/1.0.0/PACKIT_1.0.0_x64-setup.exe -Algorithm SHA256
python scripts/finalize_release_assets.py
```

The finalizer validates installer/source/legal hashes against the prepared inventory before including the genuine .sig/latest.json hashes. It does not sign, publish or substitute cryptographic verification. Keep all final bytes together. Publication remains a separate approval.
