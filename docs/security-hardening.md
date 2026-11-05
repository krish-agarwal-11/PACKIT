# Native worker and filesystem hardening (Windows x64)

## Implemented boundary

Every native 7z/RAR listing, test and decoding subprocess, and native 7z encoder, now starts suspended through a dedicated Win32 launcher in a fresh **low-privilege AppContainer (LPAC)** with no capabilities. The launcher sets the ALL APPLICATION PACKAGES opt-out attribute, explicitly lists the only three inherited pipe endpoints, and checks the actual AppContainer token and exact package SID before resuming. It adds DEP, extension-point disable and remote-image-load mitigation policies. Failure to establish any required attribute, ACL, token identity or Job membership fails closed: there is no ordinary-user-token fallback.

Only a private copy of 7z.exe/7z.dll receives read/execute access. Copied archive inputs/explicit volume companions and encoder source snapshots/manifests receive read access (directory traversal where required). Originals, extraction destinations, saved settings and passwords are not granted to the worker. Decoding returns data through bounded pipes; trusted PACKIT independently validates listing limits, paths, entry lengths and CRCs before committing staged output. The encoder alone receives access to a separate low-integrity disposable scratch directory. After its process has exited, PACKIT opens the output without following a reparse point, copies it with a size bound, and uses the existing reader verification/publication path.

The cleared worker environment contains only required Windows/profile locations; it contains no password or inherited PATH. ZIP credentials remain in existing zeroizing backend/session state and are never delivered to the native worker. Existing encrypted native formats remain unsupported.

## Limits and tests

Existing one-process, 256 MiB process/job memory, 180-second wall deadline, cancellation and kill-on-job-close restrictions remain. Native inputs remain limited to 2 GiB, 512 entries, 512 MiB per file and the existing aggregate/ratio limits. Listing is capped at 4 MiB; stderr at 64 KiB; streaming uses bounded buffers. A 25 ms watchdog scans pinned scratch/profile directory handles without following reparse points, enforcing 1,024 nodes, eight levels and 2 GiB aggregate scratch size. This is a detection/termination budget, **not an atomic disk quota**; allocation can overshoot between polls, and a compromised worker can fill its own profile/scratch before detection.

Controlled probes check granted reads, denied unrelated reads/writes, denied input/code mutation, denied ALL APPLICATION PACKAGES-only file access (LPAC boundary), excluded inheritable handles, exact Unicode/quote argument transport, scratch-only encoder writes, output budget detection and memory allocation failure. Existing native cancellation, archive corruption, selective extraction, replacement/rollback and password-session tests remain active. Windows requires temporary per-user AppContainer profiles for these tests; a workspace sandbox that denies profile creation will correctly fail containment tests rather than silently skip them.

## Filesystem and Explorer review

Archive-controlled paths continue through shared portable-component validation and NFKC/case collision planning. Output operations retain capability directory handles, no-follow/reparse checks on each archive-controlled component, exact-name checks against 8.3 aliases, protected source identities, staged replacement and best-effort rollback. Inputs now open with OPEN_REPARSE_POINT, reject reparse/nonregular final objects and deny concurrent write/delete sharing. Junction regression tests check both destination roots and nested output paths; existing data must remain untouched.

Explorer requests and the COM extension now reject device/GLOBALROOT, ADS and unsafe relative component spellings while accepting ordinary drive/UNC and extended Windows picker paths, Unicode and spaces. Paths remain JSON stdin data, not shell command text. The in-process extension locates only the executable at its exact vendor/shell installation layout, rejecting missing/reparse executables instead of searching arbitrary ancestors. Per-user ownership/idempotence and ISO/UserChoice logic are unchanged.

## Remaining limitations — release blockers

* LPAC is a meaningful Windows OS security boundary, not complete isolation from the operating system. Required Windows runtime resources and per-container profile storage remain accessible. Kernel/OS vulnerabilities, shared resources explicitly granted to restricted applications, same-user hostile processes controlling PACKIT itself, and trusted-parent compromise are outside this boundary.
* AppContainer profiles are temporary but are Windows-managed persistent objects until deletion. Normal paths delete them; a crash or failed deletion can leave an orphan profile/private temporary data. Deletion failure emits a generic warning. Crash-recovery scavenging is not implemented.
* Input/DLL copies and ACL/profile setup occur for each native invocation; native operations can be slower, particularly for many individual members. Broad disk quota enforcement is not implemented. Full independent adversarial review, Windows 11 UI acceptance and policy/endpoint-protection compatibility testing remain required.
* Path safety is handle-oriented, but no claim is made of complete protection against a malicious same-user process that can change the trusted application's files or race its destination namespace. Rollback remains best effort with existing recovery reporting.
* Rust zeroization reduces credential lifetime; JavaScript strings, IPC/runtime copies, OS paging and crash dumps are not guaranteed to be wiped.

If containment is unavailable, affected operations are compressed 7z/RAR browsing/testing/extraction (including supported split sets), 7z creation and native verification. ZIP, TAR, TAR.GZ, ISO/UDF and the independent stored-RAR reader do not acquire an unrestricted fallback or need this worker. No new archive/encryption support is claimed.

Implementation references: [Microsoft AppContainer launch guidance](https://learn.microsoft.com/en-us/windows/win32/secauthz/implementing-an-appcontainer), [AppContainer isolation](https://learn.microsoft.com/en-us/windows/win32/secauthz/appcontainer-isolation). These describe the platform mechanisms, not an audit or approval of PACKIT.
