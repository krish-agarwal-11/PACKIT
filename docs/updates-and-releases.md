# Signed updates and manual GitHub release procedure

Repository: https://github.com/krish-agarwal-11/PACKIT (currently private). Development stays **0.1.0**. No release or push is authorized by this procedure alone.

## Security and current behavior

Official tauri-plugin-updater 2.14.1 is used through Rust. Its public key is imported from the owner's generated public-key file into src-tauri/updater-public-key.pub; only that public file/configuration is distributed. Private key and password are never needed by the agent or the repository. The Rust plugin reads the committed public file and tests require configuration consistency.

src-tauri/update-policy.json has feedReady=false. Startup performs no release polling; About's Check for Updates reports that the accessible feed is not ready. The configured future endpoint is https://github.com/krish-agarwal-11/PACKIT/releases/latest/download/latest.json. No usable release at that endpoint is asserted. Do not enable it against private GitHub assets, embed tokens or weaken TLS/signature checks.

The frontend has no updater:default/allow-install permissions. Only guarded application commands are exposed; they verify the actual main-window label. Compact operation windows cannot initiate updates. Automatic checks (when enabled) are asynchronous and silent unless a newer update is available; busy archive work defers the dialog. Installation is user-approved and blocked while archive operations or other installed PACKIT windows are active. An atomic local operation guard prevents a new local job racing installation. The standard installer independently rechecks active processes and never force-terminates them, including passive/silent modes.

Download bytes are bounded to 256 MiB and 120 seconds over valid HTTPS. The official plugin verifies the signature and signed version before installation, and verifies again in install(). Malformed, unsigned or invalid signatures are errors, never an unsigned fallback. requireSignedVersion=true prevents a manifest from pairing a newer version with a signature for an older artifact. GitHub JSON itself is TLS-delivered metadata, **not independently signed**; its artifact signature includes an authenticated version. Download/install failures show a manual-installer fallback. Windows exits PACKIT only after the official plugin successfully launches the installer; asynchronous installer failure is communicated by the installer and requires manual repair/retry.

## Asset convention and owner-only signing

Each approved release uses:

- PACKIT_<version>_x64-setup.exe (standard Tauri NSIS installer).
- The matching .exe.sig (public detached signature).
- latest.json (platform windows-x86_64, exact version, real download URL and signature).
- PACKIT_<version>_windows-x64-source.tar.gz, its SHA-256, and build-record.json.

Normal development builds remain unsigned and do not enable createUpdaterArtifacts. The agent must not execute signing or inspect private credentials. In a separate owner-controlled terminal, use Tauri's password prompt; do not put a password in command-line arguments or paste it into chat:

```powershell
# Replace the placeholder locally; only the OWNER executes this command.
npm run tauri signer sign -- --private-key-path "<owner-selected-private-key-file>" --app-version 0.1.0 "src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/PACKIT_0.1.0_x64-setup.exe"
```

The CLI reads the key on the owner's behalf and creates a public .sig. Never commit key/password/environment files. Signing the updater payload is distinct from Windows Authenticode/SmartScreen trust.

Generate the manifest only after an actual signature exists:

```powershell
python scripts/prepare_release_manifest.py --version 0.1.0 --installer src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/PACKIT_0.1.0_x64-setup.exe --signature src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/PACKIT_0.1.0_x64-setup.exe.sig
```

This prepares ignored release-staging/latest.json, checks signature/version structure, and does not itself certify cryptography. The official updater supplies cryptographic verification. Reuse prepare_source_bundle.py and record_private_build.py after the final installer, and match hashes to the exact uploaded bytes. The source-bundle/notice inventory must reflect the updater's incremental dependencies. Preserve the existing release-review questions for public distribution.

No GitHub Release is created here. After explicit approval and authentication, an owner may manually stage a draft, verify its exact assets/source and then approve publication. A private GitHub Release is not an anonymous update feed. Enable feedReady only after the intended clients can fetch a valid signed release without embedded credentials; release that enabled build as an initial manual installer when ready. Disabled clients cannot magically activate themselves remotely.

## Genuine private 0.1.0 → 0.1.1 acceptance test (not executed)

1. Use an isolated local clone and a disposable Windows user/VM. Keep main at 0.1.0. Preserve identifier, publisher, currentUser scope, COM identities, public key and all security flags.
2. An owner must supply an actually reachable HTTPS staging endpoint with a valid trusted certificate, accessible from the test machine. No staging service or credential is invented here. Alternatively wait for an approved accessible GitHub Releases feed. A token-protected private GitHub page alone cannot perform this test.
3. In the isolated clone only, point plugins.updater.endpoints at that real HTTPS test manifest, set feedReady=true, and adjust the hardcoded download URL prefix in updates.rs to that same real HTTPS staging asset prefix. This is a test-origin change, **not** a TLS/signature bypass. Keep requireSignedVersion=true and the exact production public key. Build/install this 0.1.0 test client manually.
4. In that isolated clone only, set Tauri/package/Cargo application and shell metadata/lock-root versions consistently to temporary 0.1.1. Build the standard NSIS installer. Owner signs the actual 0.1.1 installer with --app-version 0.1.1 using the existing private key, outside the agent. Host those exact bytes/signature and a matching 0.1.1 manifest on the approved test endpoint. No public GitHub release is needed for the private staging-host route.
5. Launch the 0.1.0 test client: verify quiet detection, Later, manual check, Update Now, signature verification, normal installer handoff, 0.1.1 on restart, settings and Explorer/ISO behavior.
6. Try an altered installer, invalid/missing signature, mismatched signed version, offline feed and active extraction/popup. Each must reject/defer safely and preserve data. Test installer failure/retry and the manual fallback. There is no successful live signed-update claim until this owner-signed test passes.
7. Remove the isolated test installation/clone using the normal ownership-safe lifecycle; do not merge temporary version/origin changes into main.

## Initial Git push handoff

No remote push is performed without explicit approval. Author identity is configured locally with the owner's confirmed GitHub noreply address. Authentication was unavailable non-interactively; after approval, run in your own terminal:

```powershell
git remote -v
git -c credential.interactive=always ls-remote origin
git fetch origin
```

Inspect any remote main/history before continuing. If the remote is empty and the initial commit is approved:

```powershell
git push -u origin main
```

If remote history exists, stop and reconcile it explicitly. Do not force-push, overwrite it or publish a release as part of this initial push.
