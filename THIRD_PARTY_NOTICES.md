# PACKIT third-party notices

PACKIT is a free and open-source desktop archive manager. First-party source is provisionally **GPL-3.0-only**; see [LICENSE](LICENSE) and [copyright scope](legal/COPYRIGHT.md). Third-party software retains its own terms. This notice is prepared for the Windows 11 x64 first-release scope; **distribution is not cleared**.

## Where the complete materials are

- [Dependency inventory](legal/DEPENDENCY_LICENSES.md): every locked Rust/npm name and version, Windows delivery assessment and notice links.
- [Machine-readable evidence](legal/inventory.json): declarations, lock integrity/checksums, package/source references, obligations, context and unresolved findings.
- [Unmodified upstream notices](legal/upstream/): exact copied package license, copyright, NOTICE and SPDX evidence files. The root license plus this file alone do not replace those component notices.
- [Source distribution plan](legal/SOURCE_DISTRIBUTION.md) and [release-review blockers](legal/RELEASE_REVIEW.md).

The inventory covers 476 third-party Cargo name/version records from both lockfiles and 138 npm lock records. Of these, the filtered Windows graph conservatively identifies 234 normal-dependency candidates and 61 build/proc-macro candidates; six npm packages are frontend runtime candidates. Build tools and non-Windows optional packages are separately identified, not represented as shipped runtime libraries. A final installer has not been produced, so exact final payload inclusion remains unverified.

## Principal frontend and framework components

React 19.3.1, React DOM 19.3.1 and scheduler 0.28.0 declare MIT. Lucide React 1.67.0 declares ISC; its full upstream LICENSE includes Cole Bemis/Feather MIT notices for designated icons, in addition to ISC and must travel with the used icon code. Tauri Rust 2.12.5 and its runtime/helpers, Tauri JavaScript API 2.12.4, dialog plugins and the Rust Windows bindings have their exact declarations/texts in the inventory; many offer MIT OR Apache-2.0. Tailwind 4.3.4 is a build-time CSS generator whose emitted CSS is included. Package classification follows imports/build configuration and a conservative dependency graph; it is not based solely on the npm dependencies/devDependencies labels.

Retain all collected attribution/disclaimer/NOTICE files. Where MIT/Apache or other alternatives are offered, preserve the upstream expression and texts; relying on MIT where genuinely offered does not erase separate AND/Unicode/data-code obligations. Apache patent/NOTICE requirements, BSD attribution/non-endorsement conditions, ISC/Zlib notices and Unicode/data notices must be respected as applicable. MPL-2.0-covered components use the documented section 3.3 Larger Work route with original source terms retained; exact unchanged archives are supplied in legal/sources/mpl. Final recipient source delivery is still required. A package-level permissive label never overrides a component-specific native SDK license.

## Bundled 7-Zip 26.05 and RAR decoding — separate terms

PACKIT uses parts of **7-Zip**, copyright Igor Pavlov. Upstream website/source: [7-zip.org](https://www.7-zip.org/) and the repository tag referenced in the inventory. The bundled executable/DLL are invoked as a separate, replaceable subprocess; 7z.dll is loaded by that executable, not linked into PACKIT's Rust executable. PACKIT creates 7z using LZMA2 and does not create RAR.

The exact bundled `License.txt` identifies:

- 7z.exe: LGPL-covered code.
- 7z.dll: LGPL-covered portions, LGPL with the **unRAR restriction** for designated RAR code, BSD-3-Clause LZFSE/Zstandard code, and BSD-2-Clause XXH64 code.
- Source documentation additionally identifies public-domain files only where the source expressly says so.

The upstream binary notice and full LGPL 2.1 text are preserved at [legal/upstream/bundled/7zip](legal/upstream/bundled/7zip). The source notice, full source license, unRAR terms and readme were copied byte-for-byte from the existing source archive into [legal/upstream/bundled/7zip-source](legal/upstream/bundled/7zip-source). The BSD copyright notices/disclaimers are included in those exact upstream license files, not paraphrased replacements.

The unRAR terms prohibit using the relevant sources to recreate RAR compression and include additional utility distribution terms. Do not represent those portions as GPLv3-compatible or apply PACKIT's GPL identifier to them. The claimed separate-program aggregation and exact restriction scope require review: **UNRESOLVED — RELEASE REVIEW REQUIRED**. Process separation is architectural evidence, not a legal conclusion. See [7-Zip review](legal/SEVEN_ZIP_REVIEW.md).

The source archive `src-tauri/vendor/7zip/7zip-26.05-source.tar.gz` is present. Stage 3B independently verified that the executable/DLL match the official 26.05 release installer and the tagged source matches the official source release in substance; see legal/7zip-release-provenance.json and legal/sources/7zip. Preserve the source, exact terms, replacement capability and applicable source/relinking rights. No GPL relicense of restricted RAR code is attempted.

## Windows shell, Microsoft SDK and operating system

The PACKIT shell extension is first-party GPL-3.0-only code. Its source and locked dependencies must accompany the corresponding-source delivery. The windows/windows-core/windows-sys bindings retain their own notices; actual package declarations/evidence are in the inventory. The previous `vendor/windows-rs` notice directory is not currently included by the resource globs—future legal staging must include the complete collected notice set.

`webview2-com-sys` includes Microsoft's native WebView2 loader binaries/static library. The wrapper's MIT declaration does not establish the license of those binaries. Exact SDK 1.0.3800.47 LICENSE is **BSD-3-Clause**, and all nine cached loader variants match the official NuGet package. Preserve the complete [Microsoft SDK LICENSE and NOTICE](legal/upstream/recovered/microsoft-webview2-1.0.3800.47/). GPL Corresponding Source / System Library treatment of the statically linked loader remains **UNRESOLVED / RELEASE BLOCKER**; the SDK does not include its implementation source. Runtime/VC installer terms require review only if those components are later conveyed. See [native review](legal/WINDOWS_NATIVE_REVIEW.md). Windows OS DLLs and the installed WebView2 Runtime are not relabeled as GPL. The existing shell DLL imports VCRUNTIME140.dll; no VC redistributable was bundled by this stage.

The observed Rust 1.99.1 standard-library copyright collection is also preserved under `legal/upstream/toolchain`; static runtime contributions are not fully described by Cargo.lock. See [toolchain review](legal/TOOLCHAIN_REVIEW.md) for remaining source/exception mapping.

## Assets, tests and development materials

Original PACKIT artwork ownership was confirmed by Krish Agarwal; see copyright scope. System fonts are referenced, not bundled. The unused legacy SVGs and other non-approved scaffold artwork have been removed and are excluded from the defined source/payload. Approved artwork was unchanged. RAR test fixtures derive from libarchive 3.8.11, with the supplied COPYING retained in `legal/upstream/bundled/fixtures`; top-level libarchive COPYING expressly defers to file-specific notices. The 13 uncertain RAR fixture files are excluded from the generated source package while preserved locally for tests; optional hash-checked retrieval is documented in legal/DEVELOPMENT_ASSETS.md. Permission review is still required before conveying those fixture bytes. The runtime application does not link libarchive. Original generated ZIP/7z/TAR/ISO fixtures and development-only pycdlib usage are described in the fixture README. The user's external multi-GB ISO is not included or licensed by PACKIT.

No legal certification, final licensing audit approval, installer development or publication is claimed by this file. Resolve the listed blockers before distributing a binary or source bundle containing unresolved third-party material.

## Supplemental exact-version evidence

Stage 3B recovered the seven previously flagged component notices at packaged VCS commits, recorded in legal/recovery-downloads.json and legal/evidence-manifest.json. The selector source permission header is preserved alongside the full MPL text. Original texts are unmodified. All six frontend runtime npm archives now match locked SHA-512 integrity; preferred source references/archives are recorded in legal/frontend-source-provenance.json. Unmodified build-tool binaries are not conveyed by the defined source candidate; incomplete build-tool notice evidence is not claimed resolved for a hypothetical future tool-binary distribution.

## Private Windows installer components

The private installer is generated by Tauri using NSIS 3.13, configured with zlib compression. Original NSIS COPYING (including its component-specific terms) is preserved in legal/upstream/installer/nsis-3.13/. Tauri nsis-tauri-utils 0.5.3 original notices are in legal/upstream/installer/nsis-tauri-utils-0.5.3/; its tagged source is in legal/sources/installer/. These separate installer materials are not relabeled as PACKIT first-party code. No full compiler/tool distribution is included. Final public distribution obligations remain subject to the documented review and actual payload validation.

## Stage 5 official updater dependencies

Tauri updater 2.14.1 (MIT OR Apache-2.0), minisign-verify and its selected HTTPS/Rustls/ring dependencies retain their original upstream terms. Exact locked-source notices for this incremental addition are preserved under legal/upstream/updater/ and listed in legal/updater-dependency-delta.json. Existing reqwest/Tokio/Tauri notices remain in the original inventory. No private signing material is distributed; signature verification uses the supplied public key. This focused addition does not close the existing public-distribution review questions.
