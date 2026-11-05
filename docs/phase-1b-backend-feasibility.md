# Phase 1B backend feasibility

This is an evaluation, not a list of formats supported by PACKIT. The application still uses its approved ZIP backend.

| Candidate | What it offers | What remains to validate |
| --- | --- | --- |
| [Libarchive 3.8.11](https://github.com/libarchive/libarchive/releases/tag/v3.8.11) | C streaming reader for TAR, 7z, RAR and RAR5 with variant limitations. Gzip, bzip2, XZ and Zstandard are build-dependent filters; a raw reader can expose single compressed streams. The entry API supports listing and choosing which entries to consume. Solid 7z/RAR and filtered TAR may require decoding preceding data. | Reproducible native builds on Windows, macOS and Linux; ensure no external filter fallback; isolate and limit decoder memory/CPU even while opening metadata; independently verify gzip and distinguish decompression from checksum verification. |
| [Official 7-Zip](https://www.7-zip.org/) | Upstream Windows, macOS and Linux binaries. Supports 7z (including LZMA/LZMA2), RAR extraction, TAR, GZIP, BZIP2 and XZ; the exact methods and variants require fixtures. A native SDK adapter can provide per-entry extraction and callbacks. The console has list, extract and test commands. | A safe structured adapter, redistribution review, and per-platform packaging. The CLI's text listings and path-based extraction are poor interfaces for PACKIT's existing validated output and rollback. Do not assume standalone Zstandard support from a codec name. |
| [LZMA SDK](https://www.7-zip.org/sdk.html) | Permissively distributed 7z/LZMA components that can be embedded, with Windows/macOS/Linux source builds. | Not a replacement for the full archive suite: no RAR backend. A split stack would need additional TAR, stream and RAR adapters and an independent test matrix. |

Libarchive is the leading candidate. Its [COPYING file](https://github.com/libarchive/libarchive/blob/v3.8.11/COPYING) describes BSD-style terms and component exceptions. A distribution would retain the relevant copyright, license and disclaimer notices, including those of statically linked codecs. The [7-Zip source license](https://github.com/ip7z/7zip/blob/main/DOC/License.txt) includes LGPL/BSD terms and an unRAR restriction on the RAR code. A 7-Zip-based distribution would need to account for the LGPL source and replacement requirements applicable to its integration as well as the unRAR terms.

## Security and integrity findings

- The [libarchive 3.8.11 gzip filter](https://github.com/libarchive/libarchive/blob/v3.8.11/libarchive/archive_read_support_filter_gzip.c) consumes the gzip trailer without verifying its CRC or uncompressed length. A Windows probe using the OS libarchive 3.8.9 extracted both a valid TAR.GZ and the same file with a flipped trailer CRC byte, returning success for both. Python's gzip decoder rejected the altered file. This was a backend feasibility probe; PACKIT did not process either file.
- The [XZ filter](https://github.com/libarchive/libarchive/blob/v3.8.11/libarchive/archive_read_support_filter_xz.c) uses an unlimited liblzma memory setting on modern liblzma; the [7z reader](https://github.com/libarchive/libarchive/blob/v3.8.11/libarchive/archive_read_support_format_7zip.c) initializes a raw decoder from archive properties. Counting bytes after the decoder returns does not limit the allocations made during decode or metadata parsing.
- Libarchive's [filter documentation](https://github.com/libarchive/libarchive/wiki/LibarchiveFormats) notes that some filters fall back to an external program if their native codecs are missing. A PACKIT build must fail closed if the expected compiled-in codec is missing.
- TAR header checksums do not authenticate file contents. Zstandard frame checksums are optional. Status text must reflect what was actually checked. No library success status alone is proof that a checksum existed and was verified.
- Libarchive has a sequential read API rather than an application-wide cancellation or percentage contract. PACKIT could measure bytes read from the decoded stream, but those are not a reliable percentage for all archives. Cancellation should terminate an isolated worker if a decoder call is stuck. No selective-extraction UI is proposed for Phase 1B.

A defensible integration would pin a security release, run native parsers behind a resource-bounded worker, register only expected compiled-in readers, stream through PACKIT's existing validation and output APIs, and use checksum-aware stream decoders where needed. The native parser should never create user output paths itself. Require real fixtures, including malformed headers, wrong checksums, oversized dictionaries, links and special files, before enabling each capability. Platform build checks must cover Windows x64/ARM64, macOS on Apple Silicon and Intel as supported, and the Linux distributions targeted for packaging.

## Reproducing the Windows integrity probe

1. Create a TAR containing a small text file. Compress it with Python's gzip module.
2. Copy the gzip bytes and flip the lowest bit of byte `[-8]`, the first byte of the trailer CRC. Keep the compressed data and length unchanged.
3. Run `tar.exe -xOf <file> <entry>` on both files and compare the exit codes with `gzip.decompress`. The observed Windows system tar returned 0 for both; the independent decoder rejected the altered file.
4. Treat this as a probe of the system reader, not a supported PACKIT workflow. For an application smoke test, launch with `npm.cmd run tauri dev` and use a known ZIP to verify browsing, Test Archive, and the existing extraction conflict dialog.

## Windows build result

The Visual Studio vcpkg probe built or restored the zlib, bzip2, liblzma and Zstandard dependencies under the x64 Windows static-library triplet. The temporary libarchive 3.8.11 overlay could not reuse the stock 3.8.9 port patches; the Windows crypto, libmd, and iconv hunks drifted. The probe did not produce a validated libarchive 3.8.11 artifact. No system-wide dependency or PACKIT build dependency was added.

The unchanged PACKIT baseline passed 22 Rust tests, rustfmt, Clippy, TypeScript, frontend lint and formatting, and the frontend production build on Windows 11. The Tauri debug rebuild failed at replacement of the running PACKIT executable (Windows OS error 5). The existing process was left running. macOS and Linux were not tested.
