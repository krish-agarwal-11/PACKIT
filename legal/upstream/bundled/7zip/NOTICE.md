# 7-Zip 26.05 Windows x64 backend

PACKIT invokes the unmodified official x64 console executable and its adjacent DLL as a separate program. Binaries came from the official 7-Zip 26.05 x64 distribution. The exact tagged upstream source archive is included in this directory, alongside the upstream binary notice and the full LGPL 2.1 text. Upstream: https://github.com/ip7z/7zip/releases/tag/26.05 (source tag 26.05).

- 7z.exe SHA-256: 94205CFC054DD3BBEB978905BC619A65B3F04E9E73BB2717841110B6F46D962E
- 7z.dll SHA-256: 78BF6B668D75801E3AA180B4CCB16E6D8395FBA80DEC8A065055BB6F8DFABB82

7-Zip is copyright Igor Pavlov. The executable and DLL are separate from PACKIT and can be replaced with compatible builds; no source from the DLL is linked into PACKIT. Keep the corresponding source, license texts, notices, and ability to replace the backend when redistributing. The DLL includes LGPL, BSD and unRAR-restricted portions. The unRAR code must not be used to recreate the proprietary RAR compression algorithm. See the included upstream License.txt for exact terms and component notices. PACKIT uses the decoder for supported read formats and the LZMA2 encoder for 7z creation. It never creates RAR archives. The LGPL-covered encoder remains in the separate, replaceable executable/DLL; no unRAR-restricted code is linked into PACKIT. The unRAR license is not a GPLv3 license: retain its separate terms and corresponding notices for the separately aggregated backend; do not relicense that component as PACKIT GPLv3. A final distribution/legal audit remains required before release.
