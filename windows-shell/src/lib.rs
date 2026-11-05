// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
//! Original PACKIT classic-menu COM extension. Only titles and handoff run in
//! Explorer; no archive parser, password, compressor or extraction code is loaded.
#![cfg(windows)]
#[path = "../../src-tauri/src/naming.rs"]
mod naming;
use std::{
    ffi::c_void,
    path::PathBuf,
    sync::atomic::{AtomicIsize, AtomicUsize, Ordering},
};
use windows::{
    core::{implement, IUnknown, Interface, BOOL, GUID, HRESULT, PWSTR},
    Win32::{
        Foundation::*,
        System::{Com::*, LibraryLoader::GetModuleFileNameW, SystemServices::SFGAO_FOLDER},
        UI::Shell::*,
    },
};
static MODULE: AtomicIsize = AtomicIsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static LOCKS: AtomicUsize = AtomicUsize::new(0);
struct Life;
impl Life {
    fn new() -> Self {
        LIVE.fetch_add(1, Ordering::AcqRel);
        Self
    }
}
impl Drop for Life {
    fn drop(&mut self) {
        LIVE.fetch_sub(1, Ordering::AcqRel);
    }
}
// COM's standard object/server-lock accounting prevents premature unloading.
// Versioned filenames also avoid replacing a DLL currently held by Explorer.
#[unsafe(no_mangle)]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    if LIVE.load(Ordering::Acquire) == 0 && LOCKS.load(Ordering::Acquire) == 0 {
        S_OK
    } else {
        S_FALSE
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn DllMain(module: HINSTANCE, reason: u32, _: *mut c_void) -> BOOL {
    if reason == 1 {
        MODULE.store(module.0 as isize, Ordering::Release);
    }
    BOOL(1)
}
fn module_exe() -> windows::core::Result<PathBuf> {
    let mut path = vec![0_u16; 32768];
    let n = unsafe {
        GetModuleFileNameW(
            Some(HMODULE(MODULE.load(Ordering::Acquire) as *mut c_void)),
            &mut path,
        )
    };
    if n == 0 || n as usize >= path.len() {
        return Err(E_FAIL.into());
    }
    let path =
        String::from_utf16(&path[..n as usize]).map_err(|_| windows::core::Error::from(E_FAIL))?;
    let module = PathBuf::from(path);
    // Only this installation's exact vendor/shell layout is authorized. Never
    // search arbitrary ancestors for an executable if PACKIT is missing.
    let root = module
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .ok_or_else(|| windows::core::Error::from(E_FAIL))?;
    let candidate = root.join("packit.exe");
    use std::os::windows::fs::MetadataExt;
    if candidate
        .symlink_metadata()
        .is_ok_and(|m| m.is_file() && m.file_attributes() & 0x400 == 0)
    {
        return Ok(candidate);
    }

    Err(E_FAIL.into())
}

fn allocated(text: &str) -> windows::core::Result<PWSTR> {
    let wide: Vec<_> = text.encode_utf16().chain(Some(0)).collect();
    let buffer = unsafe { CoTaskMemAlloc(wide.len() * 2) } as *mut u16;
    if buffer.is_null() {
        return Err(E_OUTOFMEMORY.into());
    }
    unsafe {
        std::ptr::copy_nonoverlapping(wide.as_ptr(), buffer, wide.len());
    }
    Ok(PWSTR(buffer))
}
fn selection(array: &IShellItemArray) -> windows::core::Result<Vec<String>> {
    let count = unsafe { array.GetCount()? };
    if count == 0 || count > 20_000 {
        return Err(E_INVALIDARG.into());
    }
    let mut result = Vec::new();
    let mut size = 0;
    for index in 0..count {
        let value = unsafe { array.GetItemAt(index)?.GetDisplayName(SIGDN_FILESYSPATH)? };
        let name = unsafe { value.to_string() };
        unsafe { CoTaskMemFree(Some(value.0.cast())) };
        let name = name?;
        if !naming::safe_shell_path(&name) {
            return Err(E_INVALIDARG.into());
        }
        size += name.len() + 64;
        if size > 4 * 1024 * 1024 {
            return Err(E_INVALIDARG.into());
        }
        result.push(name);
    }
    Ok(result)
}
#[implement(IExplorerCommand)]
struct ExplorerCommand {
    index: usize,
    dev: bool,
    _life: Life,
}
impl IExplorerCommand_Impl for ExplorerCommand_Impl {
    fn GetTitle(&self, array: windows::core::Ref<IShellItemArray>) -> windows::core::Result<PWSTR> {
        let array = array
            .as_ref()
            .ok_or_else(|| windows::core::Error::from(E_INVALIDARG))?;
        let first = unsafe { array.GetItemAt(0)? };
        let value = unsafe { first.GetDisplayName(SIGDN_FILESYSPATH)? };
        let path = unsafe { value.to_string() };
        unsafe { CoTaskMemFree(Some(value.0.cast())) };
        let path = PathBuf::from(path?);
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| windows::core::Error::from(E_INVALIDARG))?;
        let folder = unsafe { first.GetAttributes(SFGAO_FOLDER)? }.0 & SFGAO_FOLDER.0 != 0;
        let mut title = naming::menu_title(naming::ACTION_NAMES[self.index], name, folder);
        if self.dev {
            title.push_str(" (development)");
        }
        // Classic Win32 menus use '&' for accelerators; double it so the
        // selected filename is displayed literally. This never changes output names.
        allocated(&title.replace('&', "&&"))
    }
    fn GetIcon(&self, _: windows::core::Ref<IShellItemArray>) -> windows::core::Result<PWSTR> {
        allocated(&format!("\"{}\",0", module_exe()?.display()))
    }
    fn GetToolTip(
        &self,
        array: windows::core::Ref<IShellItemArray>,
    ) -> windows::core::Result<PWSTR> {
        let title = self.GetTitle(array)?;
        let text = unsafe { title.to_string() };
        unsafe {
            CoTaskMemFree(Some(title.0.cast()));
        }
        allocated(&text?.replace("&&", "&"))
    }
    fn GetCanonicalName(&self) -> windows::core::Result<GUID> {
        Ok(GUID::from_u128(
            naming::DYNAMIC_BASE + self.index as u128 + if self.dev { 0x100 } else { 0 },
        ))
    }
    fn GetState(
        &self,
        array: windows::core::Ref<IShellItemArray>,
        _: BOOL,
    ) -> windows::core::Result<u32> {
        if array
            .as_ref()
            .is_some_and(|a| unsafe { a.GetCount().is_ok_and(|n| n > 0) })
        {
            Ok(ECS_ENABLED.0 as u32)
        } else {
            Ok(ECS_HIDDEN.0 as u32)
        }
    }
    fn Invoke(
        &self,
        array: windows::core::Ref<IShellItemArray>,
        _: windows::core::Ref<IBindCtx>,
    ) -> windows::core::Result<()> {
        use std::{
            io::Write,
            os::windows::process::CommandExt,
            process::{Command, Stdio},
        };
        let paths = selection(
            array
                .as_ref()
                .ok_or_else(|| windows::core::Error::from(E_INVALIDARG))?,
        )?;
        let action = naming::ACTION_NAMES[self.index];
        if !matches!(action, "add" | "zip" | "seven_zip") && paths.len() > 8 {
            return Err(E_INVALIDARG.into());
        }
        let send = |paths: &[String]| -> windows::core::Result<()> {
            let bytes = serde_json::to_vec(&serde_json::json!({"action":action,"paths":paths}))
                .map_err(|_| windows::core::Error::from(E_FAIL))?;
            if bytes.len() > 4 * 1024 * 1024 {
                return Err(E_INVALIDARG.into());
            }
            let mut child = Command::new(module_exe()?)
                .arg("--shell-stdin")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(0x08000000)
                .spawn()
                .map_err(|_| windows::core::Error::from(E_FAIL))?;
            if child.stdin.take().unwrap().write_all(&bytes).is_err() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(E_FAIL.into());
            }
            Ok(())
        };
        if matches!(action, "add" | "zip" | "seven_zip" | "open") {
            send(&paths)
        } else {
            for path in paths {
                send(&[path])?;
            }
            Ok(())
        }
    }
    fn GetFlags(&self) -> windows::core::Result<u32> {
        Ok(ECF_DEFAULT.0 as u32)
    }
    fn EnumSubCommands(&self) -> windows::core::Result<IEnumExplorerCommand> {
        Err(E_NOTIMPL.into())
    }
}
#[implement(IClassFactory)]
struct Factory {
    index: usize,
    dev: bool,
    _life: Life,
}
impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        outer: windows::core::Ref<IUnknown>,
        iid: *const GUID,
        result: *mut *mut c_void,
    ) -> windows::core::Result<()> {
        if outer.is_some() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        if iid.is_null() || result.is_null() {
            return Err(E_INVALIDARG.into());
        }
        let command: IExplorerCommand = ExplorerCommand {
            index: self.index,
            dev: self.dev,
            _life: Life::new(),
        }
        .into();
        unsafe { command.query(iid, result).ok() }
    }
    fn LockServer(&self, locked: BOOL) -> windows::core::Result<()> {
        if locked.as_bool() {
            LOCKS.fetch_add(1, Ordering::AcqRel);
        } else {
            let _ = LOCKS.try_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_sub(1)
            });
        }
        Ok(())
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// COM supplies valid GUID/interface-output pointers.
pub unsafe extern "system" fn DllGetClassObject(
    class: *const GUID,
    iid: *const GUID,
    result: *mut *mut c_void,
) -> HRESULT {
    if class.is_null() || iid.is_null() || result.is_null() {
        return E_INVALIDARG;
    }
    unsafe {
        *result = std::ptr::null_mut();
    }
    for dev in [false, true] {
        for index in 0..naming::ACTION_NAMES.len() {
            if unsafe { *class }
                == GUID::from_u128(
                    naming::DYNAMIC_BASE + index as u128 + if dev { 0x100 } else { 0 },
                )
            {
                let factory: IClassFactory = Factory {
                    index,
                    dev,
                    _life: Life::new(),
                }
                .into();
                return unsafe { factory.query(iid, result) };
            }
        }
    }
    CLASS_E_CLASSNOTAVAILABLE
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn distinct_classes_and_shared_names() {
        let classes: std::collections::BTreeSet<_> =
            (0..14).map(|n| naming::DYNAMIC_BASE + n as u128).collect();
        assert_eq!(classes.len(), 14);
        assert_eq!(
            naming::menu_title("zip", "Photos.2026", true),
            "Add to \"Photos.2026.zip\""
        );
    }
}
