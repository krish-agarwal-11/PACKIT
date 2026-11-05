// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use super::{
    windows::{class_id, dynamic_class_id},
    Action,
};
use crate::archive::ArchiveError;
use std::{path::Path, ptr};
use windows_sys::Win32::System::Registry::*;
type Result<T> = std::result::Result<T, ArchiveError>;
const EXTENSIONS: [&str; 8] = [
    ".zip", ".7z", ".rar", ".tar", ".tgz", ".gz", ".iso", ".zipx",
];
// ISO remains a Windows disk image, never an advertised PACKIT default type.
fn association_extensions() -> impl Iterator<Item = &'static str> {
    EXTENSIONS.into_iter().filter(|ext| *ext != ".iso")
}
fn clean_iso_association(base: &str, dev: bool) -> Result<()> {
    let progid = format!("{}.Archive", tag(dev));
    let extension = format!("{base}\\.iso");
    remove_value(&format!("{extension}\\OpenWithProgids"), &progid)?;
    remove_value(
        &format!("{base}\\{}.Capabilities\\FileAssociations", tag(dev)),
        ".iso",
    )?;
    // Only a legacy default that names our exact ProgID is ours to remove.
    // Never edit Explorer's protected UserChoice or any other handler/icon.
    if get(&extension, "")?.is_some_and(|value| value.eq_ignore_ascii_case(&progid)) {
        remove_value(&extension, "")?;
    }
    Ok(())
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}
fn error(action: &str, code: u32) -> ArchiveError {
    ArchiveError::new(
        "shell_registration",
        format!("{action}: Windows error {code}."),
    )
}
fn set(path: &str, name: &str, value: &str) -> Result<()> {
    let path = wide(path);
    let name = wide(name);
    let value = wide(value);
    let mut key = ptr::null_mut();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            0,
            ptr::null(),
            0,
            KEY_WRITE,
            ptr::null(),
            &mut key,
            ptr::null_mut(),
        )
    };
    if status != 0 {
        return Err(error("Could not create PACKIT registration", status));
    }
    let key = Key(key);
    let code = unsafe {
        RegSetValueExW(
            key.0,
            name.as_ptr(),
            0,
            REG_SZ,
            value.as_ptr().cast(),
            (value.len() * 2) as u32,
        )
    };
    if code != 0 {
        return Err(error("Could not write PACKIT registration", code));
    }
    Ok(())
}
fn get(path: &str, name: &str) -> Result<Option<String>> {
    let mut key = ptr::null_mut();
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            wide(path).as_ptr(),
            0,
            KEY_READ,
            &mut key,
        )
    };
    if status == 2 {
        return Ok(None);
    }
    if status != 0 {
        return Err(error("Could not read PACKIT registration", status));
    }
    let key = Key(key);
    let mut data = vec![0_u16; 32768];
    let mut length = (data.len() * 2) as u32;
    let mut kind = 0;
    let code = unsafe {
        RegQueryValueExW(
            key.0,
            wide(name).as_ptr(),
            ptr::null(),
            &mut kind,
            data.as_mut_ptr().cast(),
            &mut length,
        )
    };
    if code == 2 {
        return Ok(None);
    }
    if code != 0 || kind != REG_SZ || !length.is_multiple_of(2) {
        return Err(error("Invalid PACKIT registration", code));
    }
    data.truncate(length as usize / 2);
    while data.last() == Some(&0) {
        data.pop();
    }
    Ok(Some(
        String::from_utf16(&data).map_err(ArchiveError::worker)?,
    ))
}
fn delete(path: &str) -> Result<()> {
    let code = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, wide(path).as_ptr()) };
    if code != 0 && code != 2 {
        return Err(error("Could not remove PACKIT registration", code));
    }
    Ok(())
}
fn remove_value(path: &str, name: &str) -> Result<()> {
    let mut key = ptr::null_mut();
    let code = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            wide(path).as_ptr(),
            0,
            KEY_SET_VALUE,
            &mut key,
        )
    };
    if code == 2 {
        return Ok(());
    }
    if code != 0 {
        return Err(error("Could not open PACKIT association", code));
    }
    let key = Key(key);
    let code = unsafe { RegDeleteValueW(key.0, wide(name).as_ptr()) };
    if code != 0 && code != 2 {
        return Err(error("Could not remove PACKIT association", code));
    }
    Ok(())
}
fn tag(dev: bool) -> &'static str {
    if dev {
        "PACKIT.Dev"
    } else {
        "PACKIT"
    }
}
fn verb_paths(base: &str, dev: bool) -> Vec<(String, Action)> {
    let mut result = Vec::new();
    let tag = tag(dev);
    for action in Action::ALL {
        if action.creation() {
            for root in ["*", "Directory"] {
                result.push((
                    format!("{base}\\{root}\\shell\\{tag}.{}", action.name()),
                    action,
                ));
            }
        } else {
            for ext in EXTENSIONS {
                result.push((
                    format!(
                        "{base}\\SystemFileAssociations\\{ext}\\shell\\{tag}.{}",
                        action.name()
                    ),
                    action,
                ));
            }
        }
    }
    result
}
fn owned(base: &str, dev: bool) -> String {
    format!("{base}\\{}.ShellOwner", tag(dev))
}
pub fn register(exe: &Path, dev: bool) -> Result<()> {
    register_at("Software\\Classes", exe, dev)?;
    notify();
    Ok(())
}
/// Repair only an association owned by this exact installation. No new menu
/// registrations, default-handler selection, or UserChoice writes.
pub fn repair_iso(exe: &Path, dev: bool) -> Result<()> {
    let base = "Software\\Classes";
    if get(&owned(base, dev), "Executable")?
        .is_some_and(|owner| owner.eq_ignore_ascii_case(&exe.to_string_lossy()))
    {
        clean_iso_association(base, dev)?;
        notify();
    }
    Ok(())
}
pub fn unregister(exe: &Path, dev: bool) -> Result<()> {
    unregister_at("Software\\Classes", exe, dev)?;
    notify();
    Ok(())
}
fn dynamic_dll(exe: &Path) -> Result<std::path::PathBuf> {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../../vendor/shell/manifest.json"))
            .map_err(ArchiveError::worker)?;
    let filename = manifest["filename"].as_str().ok_or_else(|| {
        ArchiveError::new("shell_registration", "Missing shell extension manifest.")
    })?;
    if filename.contains(['/', '\\']) || !filename.ends_with(".dll") {
        return Err(ArchiveError::new(
            "shell_registration",
            "Invalid shell extension manifest.",
        ));
    }
    let path = exe.parent().unwrap().join("vendor/shell").join(filename);
    if path.is_file() {
        return Ok(path);
    }
    #[cfg(any(debug_assertions, test))]
    {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("vendor/shell")
            .join(filename);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(ArchiveError::new("shell_registration","PACKIT's dynamic shell extension is missing. Run npm run shell:build or repair the installation."))
}
fn register_at(base: &str, exe: &Path, dev: bool) -> Result<()> {
    let exe = exe.to_str().ok_or_else(|| {
        ArchiveError::new("shell_registration", "Executable path is not Unicode.")
    })?;
    if !Path::new(exe).is_absolute() || exe.contains(['\"', '\0']) {
        return Err(ArchiveError::new(
            "shell_registration",
            "Invalid executable path.",
        ));
    }
    let tag = tag(dev);
    let dll = dynamic_dll(Path::new(exe))?;
    set(&owned(base, dev), "Executable", exe)?;
    for action in Action::ALL {
        let clsid = format!("{{{:?}}}", class_id(action, dev));
        let key = format!("{base}\\CLSID\\{clsid}");
        set(&key, "", &format!("{tag} {}", action.name()))?;
        let dynamic = format!(
            "{base}\\CLSID\\{{{:?}}}\\InprocServer32",
            dynamic_class_id(action, dev)
        );
        set(&dynamic, "", &dll.to_string_lossy())?;
        set(&dynamic, "ThreadingModel", "Apartment")?;
        set(
            &format!("{key}\\LocalServer32"),
            "",
            &format!(
                "\"{exe}\" --shell-server {}{}",
                action.name(),
                if dev { " --dev" } else { "" }
            ),
        )?;
    }
    for (key, action) in verb_paths(base, dev) {
        delete(&key)?; // Remove only the old PACKIT-owned static verb.
        set(
            &key,
            "ExplorerCommandHandler",
            &format!("{{{:?}}}", dynamic_class_id(action, dev)),
        )?;
        set(&key, "MultiSelectModel", "Player")?;
    }
    let progid = format!("{tag}.Archive");
    let key = format!("{base}\\{progid}");
    set(&key, "", "PACKIT archive")?;
    set(&format!("{key}\\Application"), "ApplicationName", "PACKIT")?;
    set(
        &format!("{key}\\Application"),
        "ApplicationDescription",
        "Local archive inspection, extraction and creation",
    )?;
    set(&format!("{key}\\DefaultIcon"), "", &format!("\"{exe}\",0"))?;
    set(
        &format!("{key}\\shell\\open\\command"),
        "",
        &format!("\"{exe}\" --shell open -- \"%1\""),
    )?;
    set(
        &format!("{key}\\shell\\open\\command"),
        "DelegateExecute",
        &format!("{{{:?}}}", class_id(Action::Open, dev)),
    )?;
    clean_iso_association(base, dev)?;
    for ext in association_extensions() {
        set(&format!("{base}\\{ext}\\OpenWithProgids"), &progid, "")?;
    }
    let capabilities = format!("{base}\\{tag}.Capabilities");
    set(
        &capabilities,
        "ApplicationName",
        if dev {
            "PACKIT (development)"
        } else {
            "PACKIT"
        },
    )?;
    set(
        &capabilities,
        "ApplicationDescription",
        "Local archive inspection, extraction and creation",
    )?;
    for ext in association_extensions() {
        set(&format!("{capabilities}\\FileAssociations"), ext, &progid)?;
    }
    let registered = if base == "Software\\Classes" {
        "Software\\RegisteredApplications".to_owned()
    } else {
        format!("{base}\\RegisteredApplications")
    };
    set(&registered, tag, &capabilities)?;
    // Never change extension defaults, UserChoice hashes, or another app's ProgID.
    Ok(())
}
fn unregister_at(base: &str, exe: &Path, dev: bool) -> Result<()> {
    let Some(owner) = get(&owned(base, dev), "Executable")? else {
        return Ok(());
    };
    if !owner.eq_ignore_ascii_case(&exe.to_string_lossy()) {
        return Ok(());
    }
    clean_iso_association(base, dev)?;
    for (key, _) in verb_paths(base, dev) {
        delete(&key)?;
    }
    for action in Action::ALL {
        delete(&format!("{base}\\CLSID\\{{{:?}}}", class_id(action, dev)))?;
        delete(&format!(
            "{base}\\CLSID\\{{{:?}}}",
            dynamic_class_id(action, dev)
        ))?;
    }
    let progid = format!("{}.Archive", tag(dev));
    for ext in EXTENSIONS {
        remove_value(&format!("{base}\\{ext}\\OpenWithProgids"), &progid)?;
    }
    let registered = if base == "Software\\Classes" {
        "Software\\RegisteredApplications".to_owned()
    } else {
        format!("{base}\\RegisteredApplications")
    };
    remove_value(&registered, tag(dev))?;
    delete(&format!("{base}\\{}.Capabilities", tag(dev)))?;
    delete(&format!("{base}\\{progid}"))?;
    delete(&owned(base, dev))?;
    Ok(())
}
fn notify() {
    unsafe {
        windows::Win32::UI::Shell::SHChangeNotify(
            windows::Win32::UI::Shell::SHCNE_ASSOCCHANGED,
            windows::Win32::UI::Shell::SHCNF_IDLIST,
            None,
            None,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Cleanup(String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = delete(&self.0);
        }
    }
    #[test]
    fn registration_is_idempotent_owned_and_preserves_unrelated_associations() {
        let key = format!(
            "Software\\PACKIT\\ShellTests\\{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let _cleanup = Cleanup(key.clone());
        let exe = std::env::current_exe().unwrap();
        let unrelated = format!("{key}\\.zip");
        set(&unrelated, "", "Other.Program").unwrap();
        set(
            &format!("{unrelated}\\OpenWithProgids"),
            "Other.Program",
            "",
        )
        .unwrap();
        let iso = format!("{key}\\.iso");
        set(&iso, "", "Windows.IsoFile").unwrap();
        set(&format!("{iso}\\OpenWithProgids"), "PACKIT.Dev.Archive", "").unwrap();
        set(&format!("{iso}\\OpenWithProgids"), "Other.Iso", "").unwrap();
        set(
            &format!("{key}\\PACKIT.Dev.Capabilities\\FileAssociations"),
            ".iso",
            "PACKIT.Dev.Archive",
        )
        .unwrap();
        register_at(&key, &exe, true).unwrap();
        assert_eq!(get(&iso, "").unwrap().as_deref(), Some("Windows.IsoFile"));
        assert!(
            get(&format!("{iso}\\OpenWithProgids"), "PACKIT.Dev.Archive")
                .unwrap()
                .is_none()
        );
        assert!(get(&format!("{iso}\\OpenWithProgids"), "Other.Iso")
            .unwrap()
            .is_some());
        assert!(get(
            &format!("{key}\\PACKIT.Dev.Capabilities\\FileAssociations"),
            ".iso"
        )
        .unwrap()
        .is_none());
        set(&iso, "", "PACKIT.Dev.Archive").unwrap();
        clean_iso_association(&key, true).unwrap();
        assert!(get(&iso, "").unwrap().is_none());
        assert!(verb_paths(&key, true)
            .iter()
            .any(|(path, _)| path.contains(".iso\\shell")));
        register_at(&key, &exe, true).unwrap();
        assert_eq!(
            get(&unrelated, "").unwrap().as_deref(),
            Some("Other.Program")
        );
        unregister_at(&key, Path::new("C:\\different.exe"), true).unwrap();
        assert!(get(&owned(&key, true), "Executable").unwrap().is_some());
        unregister_at(&key, &exe, true).unwrap();
        unregister_at(&key, &exe, true).unwrap();
        assert_eq!(
            get(&unrelated, "").unwrap().as_deref(),
            Some("Other.Program")
        );
        assert!(
            get(&format!("{unrelated}\\OpenWithProgids"), "Other.Program")
                .unwrap()
                .is_some()
        );
        assert!(get(&owned(&key, true), "Executable").unwrap().is_none());
        delete(&key).unwrap();
        assert!(!verb_paths("test", false)
            .iter()
            .any(|(key, _)| key.contains("rar_create")));
    }
    #[test]
    #[ignore = "Requires built PACKIT_SHELL_TEST_EXE; temporarily registers only one owned development CLSID"]
    fn native_out_of_process_com_activation() {
        use windows::{
            core::Interface,
            Win32::{
                System::Com::*,
                UI::Shell::{IExecuteCommand, IObjectWithSelection},
            },
        };
        let exe = std::env::var_os("PACKIT_SHELL_TEST_EXE")
            .expect("Set PACKIT_SHELL_TEST_EXE to built packit.exe");
        let exe = std::path::PathBuf::from(exe);
        assert!(exe.is_absolute() && exe.is_file());
        let key = format!(
            "Software\\Classes\\CLSID\\{{{:?}}}",
            class_id(Action::Open, true)
        );
        assert!(
            get(&format!("{key}\\LocalServer32"), "").unwrap().is_none(),
            "Development COM class already registered; refusing to overwrite it"
        );
        let _cleanup = Cleanup(key.clone());
        set(
            &format!("{key}\\LocalServer32"),
            "",
            &format!("\"{}\" --shell-server open --dev", exe.display()),
        )
        .unwrap();
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
        }
        let command: IExecuteCommand = unsafe {
            CoCreateInstance(&class_id(Action::Open, true), None, CLSCTX_LOCAL_SERVER).unwrap()
        };
        let selection: IObjectWithSelection = command.cast().unwrap();
        unsafe {
            command.SetKeyState(0).unwrap();
        }
        drop(selection);
        drop(command);
        unsafe {
            CoUninitialize();
        }
        delete(&key).unwrap();
    }
    #[test]
    #[ignore = "Requires built PACKIT_SHELL_TEST_EXE and its versioned DLL; temporary owned development CLSID only"]
    fn native_in_process_dynamic_titles_use_real_selection() {
        use windows::{
            core::PCWSTR,
            Win32::{System::Com::*, UI::Shell::*},
        };
        let exe = std::path::PathBuf::from(std::env::var_os("PACKIT_SHELL_TEST_EXE").unwrap());
        let dll = dynamic_dll(&exe).unwrap();
        let id = dynamic_class_id(Action::Zip, true);
        let key = format!("Software\\Classes\\CLSID\\{{{id:?}}}");
        assert!(get(&format!("{key}\\InprocServer32"), "")
            .unwrap()
            .is_none());
        let _cleanup = Cleanup(key.clone());
        set(
            &format!("{key}\\InprocServer32"),
            "",
            &dll.to_string_lossy(),
        )
        .unwrap();
        set(
            &format!("{key}\\InprocServer32"),
            "ThreadingModel",
            "Apartment",
        )
        .unwrap();
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
        }
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("Photos.2026 日本");
        std::fs::create_dir(&folder).unwrap();
        let file = root.path().join("Other.mp4");
        std::fs::write(&file, b"data").unwrap();
        let mut ids = Vec::new();
        for path in [&folder, &file] {
            let wide: Vec<_> = path
                .to_str()
                .unwrap()
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let mut id = std::ptr::null_mut();
            unsafe {
                SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut id, 0, None).unwrap();
            }
            ids.push(id);
        }
        let array = unsafe {
            SHCreateShellItemArrayFromIDLists(
                &ids.iter().map(|id| *id as *const _).collect::<Vec<_>>(),
            )
            .unwrap()
        };
        let command: IExplorerCommand =
            unsafe { CoCreateInstance(&id, None, CLSCTX_INPROC_SERVER).unwrap() };
        let title = unsafe { command.GetTitle(&array).unwrap() };
        assert_eq!(
            unsafe { title.to_string().unwrap() },
            "Add to \"Photos.2026 日本.zip\" (development)"
        );
        unsafe {
            CoTaskMemFree(Some(title.0.cast()));
        }
        let icon = unsafe { command.GetIcon(&array).unwrap() };
        assert!(unsafe { icon.to_string().unwrap() }.contains("packit.exe"));
        unsafe {
            CoTaskMemFree(Some(icon.0.cast()));
        }
        assert_eq!(unsafe { command.GetCanonicalName().unwrap() }, id);
        drop(command);
        drop(array);
        for id in ids {
            unsafe {
                CoTaskMemFree(Some(id.cast()));
            }
        }
        unsafe {
            CoUninitialize();
        }
        delete(&key).unwrap();
    }
}
