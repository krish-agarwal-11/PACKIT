# 7-Zip / unRAR release review

**Status: UNRESOLVED — RELEASE REVIEW REQUIRED.** Do not label the whole bundled backend GPLv3-compatible or legally cleared.

## Verified local evidence

The PE version resources of the bundled x64 executable report 26.05. Their SHA-256 values match the pre-existing first-party provenance record:

| File | SHA-256 |
| --- | --- |
| 7z.exe | 94205cfc054dd3bbeb978905bc619a65b3f04e9e73bb2717841110b6f46d962e |
| 7z.dll | 78bf6b668d75801e3aa180b4ccb16e6d8395fba80dec8a065055bb6f8dfabb82 |
| 7zip-26.05-source.tar.gz | 989c658cef4c13124ed1f4ff7330a6ce5ab22cefdbbc9daae9b2c110dde9a63b |

All hashes are also recorded in `inventory.json`. Stage 3B independently downloaded the official 26.05 release installer and source archive, checked GitHub release asset SHA-256 digests, and compared the embedded exe/DLL byte-for-byte through the existing LPAC-contained decoder (installer never executed). See `7zip-release-provenance.json`. The archive contains the 26.05 source readme and DOC license files. All source-license/readme evidence copied into `upstream/bundled/7zip-source` matches the archive bytes. Exact upstream references are [26.05 source License.txt](https://github.com/ip7z/7zip/blob/26.05/DOC/License.txt), [unRAR terms](https://github.com/ip7z/7zip/blob/26.05/DOC/unRarLicense.txt), and [7-Zip FAQ](https://www.7-zip.org/faq.html). The binary comparison now independently ties the local files to that official release. The generic current website license has a different year from the exact local 26.05 notice; it did not replace the latter.

PACKIT launches a private copy of the backend as a separate process using bounded stdout listing/payload exchange and CLI encoding. It does not link the backend DLL into the Rust executable or COM extension. The bundled executable loads its adjacent 7z.dll. The official source release is preserved in `legal/sources/7zip/7z2605-src.tar.xz`. All 1,292 official source files match the existing tagged source after CRLF normalization; 1,197 have line-ending-only differences, with no substantive differences. The tagged archive additionally has .gitattributes and README.md. No PACKIT native source patch was found. Compatible user replacement is not blocked by a hash allowlist in the launcher; future signing/installer policy must preserve applicable modification/replacement rights.

## Component terms

| Portion | Exact notice identifies | Required treatment |
| --- | --- | --- |
| Most executable/DLL source | LGPL 2.1 or later | Keep copyright/license and warranty notices; satisfy applicable LGPL source, modification, replacement/relinking and recipient-rights requirements. |
| Rar* source / designated DLL decoding code | LGPL plus unRAR restriction | Preserve both sets of terms. Do not relicense as unrestricted LGPL or GPL. Restriction scope needs review. |
| LZFSE / Zstd decoder code | BSD-3-Clause | Preserve actual Apple/Facebook/Igor Pavlov notices and disclaimers; no endorsement. |
| XXH64 code | BSD-2-Clause | Preserve actual Yann Collet/Igor Pavlov notices and disclaimer. |
| Explicitly marked public-domain source files | File-specific public-domain statement | Retain provenance; do not assume unmarked files are public domain. |

The full unRAR source text additionally addresses utility distribution/fees and acceptance. The binary notice summarizes the recreation restriction. Whether all clauses apply to this separately shipped 7-Zip decoder/source combination must be established by qualified review; a free initial download does not answer downstream GPL redistribution questions.

## Questions that must be resolved before release

1. Does this exact separate-program communication/distribution qualify as aggregation, rather than a GPL combined work? [GNU FAQ on aggregation](https://www.gnu.org/licenses/gpl-faq.en.html#MereAggregation) makes clear that the boundary depends on communication semantics as well as mechanism. A subprocess alone is not clearance.
2. What exact unRAR obligations apply to the supplied executable, DLL, designated source files and included full source archive? No RAR encoder is included or implemented, but that alone does not resolve all restrictions.
3. RESOLVED provenance: exe/DLL equal official release members, source release digest and complete tree checked, with build/assembly scripts preserved. PREPARED delivery: pair the exact final payload with these source bytes and verify the chosen LGPL source mechanism. An independent bit-for-bit rebuild of upstream binaries was not performed.
4. Which LGPL delivery mechanism is selected for the final distribution? The proposed route is source delivered alongside the binary, not an invented written offer. Keep exact backend notices and replacement rights. If linkage/modifications change, repeat the analysis.

Do not solve this by adding an invented GPL linking exception, deleting upstream conditions, relabeling RAR code, or implementing restricted RAR encoding. A differently licensed/backend build would be a separate engineering decision after review.

## Strongest supportable arrangement (not legal clearance)

The interface uses a general-purpose separately replaceable 7-Zip executable and conventional CLI/stdout stream operations, with no shared address space, linked decoder library, bespoke internal object exchange or PACKIT modifications to native code. This supports a separate-program aggregation interpretation. Against that, the bundled tool is intentionally integrated for core PACKIT functionality. The legal boundary depends on communication semantics and the exact terms, not process separation alone. Qualified review should answer the two questions in RELEASE_REVIEW.md; restricted RAR code is not relicensed. General source delivery is proposed alongside the binaries, not by an unapproved written offer.
