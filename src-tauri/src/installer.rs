// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
//! Explicit installer maintenance only; no shell commands, defaults or forced termination.
use std::io::Write;
use std::{
    ffi::OsString,
    fs,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{Diagnostics::ToolHelp::*, Threading::*},
};
type Result<T> = std::result::Result<T, String>;
fn safe_directory(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err("Installation path must be absolute.".into());
    }
    for parent in path.ancestors() {
        if let Ok(meta) = fs::symlink_metadata(parent) {
            if !meta.is_dir() || meta.file_attributes() & 0x400 != 0 {
                return Err("Installation directories must not be reparse points.".into());
            }
        }
    }
    Ok(())
}
fn filename() -> String {
    serde_json::from_str::<serde_json::Value>(include_str!("../vendor/shell/manifest.json"))
        .unwrap()["filename"]
        .as_str()
        .unwrap()
        .into()
}
fn valid_name(name: &str) -> bool {
    name.strip_prefix("packit_shell_v4_")
        .and_then(|s| s.strip_suffix(".dll"))
        .is_some_and(|s| {
            s.len() == 12
                && s.bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        })
}
fn folder(root: &Path) -> Result<PathBuf> {
    let folder = root.join("vendor/shell");
    safe_directory(&folder)?;
    Ok(folder)
}
fn recorded(folder: &Path) -> Result<Vec<String>> {
    let file = folder.join("installed-shells.txt");
    let meta = match fs::symlink_metadata(&file) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    if !meta.is_file() || meta.file_attributes() & 0x400 != 0 || meta.len() > 65536 {
        return Err("Invalid installed shell ownership record.".into());
    }
    let text = fs::read_to_string(file).map_err(|e| e.to_string())?;
    let names: Vec<_> = text.lines().map(str::to_owned).collect();
    if !names.iter().all(|s| valid_name(s)) {
        return Err("Invalid shell ownership filename.".into());
    }
    Ok(names)
}
pub fn check(args: &[OsString]) -> Result<()> {
    let root = Path::new(args.get(2).ok_or("Missing installation directory")?);
    safe_directory(root)?;
    let executable = root.join("packit.exe");
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err("Could not check running PACKIT processes.".into());
        }
        let result: Result<()> = (|| {
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut more = Process32FirstW(snapshot, &mut entry);
            while more != 0 {
                if entry.th32ProcessID != GetCurrentProcessId() {
                    let end = entry
                        .szExeFile
                        .iter()
                        .position(|c| *c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    if String::from_utf16_lossy(&entry.szExeFile[..end])
                        .eq_ignore_ascii_case("packit.exe")
                    {
                        let process =
                            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.th32ProcessID);
                        if process.is_null() {
                            return Err("Close running PACKIT processes before continuing.".into());
                        }
                        let mut path = vec![0u16; 32768];
                        let mut length = path.len() as u32;
                        let success =
                            QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length);
                        CloseHandle(process);
                        if success == 0 {
                            return Err("Could not inspect running PACKIT process.".into());
                        }
                        if String::from_utf16_lossy(&path[..length as usize])
                            .eq_ignore_ascii_case(&executable.to_string_lossy())
                        {
                            return Err("Close PACKIT and all operation dialogs before continuing; active work will not be terminated.".into());
                        }
                    }
                }
                more = Process32NextW(snapshot, &mut entry);
            }
            Ok(())
        })();
        CloseHandle(snapshot);
        result?;
    }
    // A content-named DLL can be retained while Explorer holds it, but only if bytes match.
    if let Some(source) = args.get(3) {
        let target = folder(root)?.join(filename());
        if let Ok(meta) = fs::symlink_metadata(&target) {
            if !meta.is_file()
                || meta.file_attributes() & 0x400 != 0
                || meta.len() > 32 << 20
                || fs::read(&target).map_err(|e| e.to_string())?
                    != fs::read(source).map_err(|e| e.to_string())?
            {
                return Err(
                    "Existing shell DLL differs from this installer; repair it before retrying."
                        .into(),
                );
            }
        }
    }
    Ok(())
}
pub fn record(root: &Path) -> Result<()> {
    let directory = folder(root)?;
    let mut names = recorded(&directory)?;
    let name = filename();
    if !valid_name(&name) {
        return Err("Invalid build manifest.".into());
    }
    if !names.contains(&name) {
        names.push(name);
    }
    // Refuse a pre-existing unsafe ownership file; OpenOptions never truncates through a link.
    let path = directory.join("installed-shells.txt");
    if path.exists() {
        recorded(&directory)?;
    }
    let mut staging = tempfile::NamedTempFile::new_in(&directory).map_err(|e| e.to_string())?;
    staging
        .write_all((names.join("\n") + "\n").as_bytes())
        .map_err(|e| e.to_string())?;
    staging.as_file().sync_all().map_err(|e| e.to_string())?;
    staging.persist(path).map(|_| ()).map_err(|e| e.to_string())
}
pub fn clean(root: &Path) -> Result<()> {
    let directory = folder(root)?;
    let mut names = recorded(&directory)?;
    // A partial first installation may copy the current DLL before its ledger write fails.
    let current = filename();
    if fs::symlink_metadata(directory.join(&current)).is_ok() && !names.contains(&current) {
        names.push(current);
    }
    // Check all ownership-listed DLLs before removing anything; no glob deletion or Explorer termination.
    let mut handles = Vec::new();
    for name in &names {
        let path = directory.join(name);
        if !path.exists() {
            continue;
        }
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.file_attributes() & 0x400 != 0 {
            return Err("Unsafe installed shell file.".into());
        }
        handles.push(fs::OpenOptions::new().read(true).write(true).share_mode(0).open(&path).map_err(|_|"Explorer is still holding PACKIT's shell extension. Close Explorer windows or restart Explorer, then retry.".to_owned())?);
    }
    drop(handles);
    for name in names {
        match fs::remove_file(directory.join(name)) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.to_string()),
        }
    }
    match fs::remove_file(directory.join("installed-shells.txt")) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ownership_names_never_escape_or_match_arbitrary_files() {
        assert!(valid_name("packit_shell_v4_012345abcdef.dll"));
        for name in [
            "../file.dll",
            "C:\\file.dll",
            "packit_shell_v4_012345ABCDEf.dll",
            "file.txt",
            "packit_shell_v4_012345abcdef.dll:stream",
        ] {
            assert!(!valid_name(name));
        }
    }
    #[test]
    fn cleanup_preserves_unrelated_files() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("vendor/shell");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("unrelated.txt"), b"preserve").unwrap();
        let name = "packit_shell_v4_012345abcdef.dll";
        fs::write(dir.join(name), b"owned").unwrap();
        fs::write(dir.join("installed-shells.txt"), format!("{name}\n")).unwrap();
        clean(root.path()).unwrap();
        assert_eq!(fs::read(dir.join("unrelated.txt")).unwrap(), b"preserve");
        assert!(!dir.join(name).exists());
    }
}
