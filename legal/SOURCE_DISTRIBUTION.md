# Corresponding Source preparation and delivery

**PREPARED internal source candidate, not fulfilled public delivery or legal clearance.** GPL-3.0-only remains unchanged. No binding written offer or hosting URL has been created.

## Workflow

1. Keep locks, exact upstream notices and this ledger versioned. `python scripts/validate_compliance.py` checks evidence without rebuilding the app.
2. `python scripts/prepare_source_bundle.py` creates ignored `experiments/license-audit/source-candidate.tar.gz`, its SHA-256 and an inspectable manifest. Tar member order, ownership, mode and timestamps and gzip timestamp are deterministic. No publication occurs. Run twice with unchanged inputs to compare hashes. On a clean preparation machine, run locked npm ci and cargo fetch for both manifests first; crate-cache lookup does not require the original machine-specific metadata cache. Missing runtime npm/upstream archives can be recovered from recorded URLs only if their recorded SHA-256 matches. Final configuration changes require updating the inventory, not bypassing its lock/hash checks.
3. The generator includes PACKIT source, build/shell scripts, locks/configuration, legal documents, approved artwork and icons, Windows-selected locked Cargo sources with Cargo directory-source checksums, six lock-verified runtime npm archives plus preferred upstream trees, Tailwind CSS inputs, exact native source and notices. General unmodified npm/compiler/SDK build tools are prerequisites, not redistributed node_modules. Non-Windows offline graphs are not certified.
4. It rejects source links/reparse points and unsafe archive names, excludes target/node_modules/experiments/dist/caches, and excludes all explicitly uncertain RAR fixture bytes. Optional fixture restoration is documented separately; source tests remain but those tests require explicit restoration. No normal build needs test data. Do not substitute a recursive working-tree ZIP for this allowlisted output.
5. For final build pairing, provide `--build-record <workspace-relative JSON>` with target `x86_64-pc-windows-msvc`, nonempty profile, features, toolchain, source_tree_sha256 from the generated manifest, and artifacts containing file/SHA-256. Every artifact and tree digest is checked. This is a preparation hook, not final legal signoff. Add actual final installation/build scripts before recapturing the tree. A private Stage 4 installer is now prepared; final public installer configuration and delivery verification remain pending.

## What remains

Final clean reconstruction using the exact selected target/features/toolchain, generated-source/notice preservation, paired installer hashes and final installation inputs are pending. The internal source candidate is not claimed to be a fully verified final Corresponding Source distribution. Lucide upstream uses a placeholder source version; its release/commit association is recorded from npm publisher provenance, whose signature has not been independently verified. All other runtime source package versions match. Final reconstruction should check generated code against those inputs.

Seven-Zip's official 26.05 source and build scripts are included, not just a URL. Its binary/source mapping was independently checked (7zip-release-provenance.json). SDK object libraries are included only as build inputs: unavailable native WebView2 loader implementation source / System Library treatment remains a legal blocker. Do not mistake .lib bytes for preferred source. See the two external questions in RELEASE_REVIEW.md.

For network binary delivery, the proposed route is GPL section 6(d): make matching source available alongside the binary, with equivalent access and no added source charge. Confirm the actual destination and test recipient download/rebuild before conveyance. For other delivery methods select an applicable GPL/LGPL mechanism; this is not a section 6(b) written offer. Preserve applicable LGPL replacement/modification rights and Installation Information if required for the delivered User Product. Do not add installer/signing restrictions that defeat those rights.

Approved PNGs are included. The owner confirmed the approved PNGs are the only available original artwork source files. Rust library notices are retained; actual release toolchain/source or System Library treatment must be pinned with the final build. Current compiler tools are not included as distributed executables. Build-tool packages omitted from conveyance do not acquire runtime obligations merely because they appear in a lockfile.

This workflow prepares real source bytes now but deliberately does not claim absent installer inputs, a public service, proprietary source access or legal clearance.

## Stage 4 private installer pairing

The private installer uses the same source-candidate workflow with its actual build record and generated install hooks/scripts included. Preserve .cargo/config.toml static CRT settings in both the normal build and source package. Future public installer and matching source downloads are planned for GitHub; no actual repository URL or hosted service exists yet. The WebView2 static-loader and 7-Zip/unRAR questions remain open for public distribution; development acceptance is not legal clearance.

## Stage 5 repository and updater addition

The actual repository is https://github.com/krish-agarwal-11/PACKIT and is currently private. No public release/source download endpoint is asserted. Official updater dependency additions and newly selected runtime sources are preserved in updater-dependency-delta.json; prior legal-review decisions are unchanged. The public verification key is source material; private signing keys/passwords are never source-bundle inputs. Exact notices/artwork bytes are preserved by the repository .gitattributes. Source archives for future releases must match the signed installer and build record, not an arbitrary branch snapshot.
