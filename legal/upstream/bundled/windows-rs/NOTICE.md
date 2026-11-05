# Microsoft windows-rs bindings

PACKIT uses windows 0.62.2, windows-core 0.62.2 and the existing windows-sys bindings for its original Windows COM/registry integration code. These Microsoft Rust bindings are offered under MIT OR Apache-2.0; this distribution elects MIT and reproduces its copyright and permission notice in LICENSE-MIT.txt. Source: https://github.com/microsoft/windows-rs.

PACKIT supplies its own small GPLv3 in-process IExplorerCommand DLL for dynamic captions and typed process handoff. Source is in windows-shell/ and the shared src-tauri/src/naming.rs. No archive parser, decoder, password state or compression engine is loaded into Explorer. The existing out-of-process COM handler remains available for file-association activation. Windows system Shell/COM services are supplied by Windows; they are not redistributed by PACKIT. NSIS hook scripts are PACKIT code using Tauri's existing NSIS integration. Final release compliance verification remains required.
