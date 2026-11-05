// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use super::{Action, Launch};
use std::{
    cell::RefCell,
    ffi::c_void,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use windows::{
    core::{implement, IUnknown, Interface, BOOL, GUID, PCWSTR},
    Win32::{
        Foundation::{CLASS_E_NOAGGREGATION, E_FAIL, E_INVALIDARG, POINT},
        System::Com::*,
        UI::{Shell::*, WindowsAndMessaging::*},
    },
};
type Sink = Arc<dyn Fn(Launch) -> windows::core::Result<()> + Send + Sync>;
pub fn dynamic_class_id(action: Action, dev: bool) -> GUID {
    GUID::from_u128(
        crate::naming::DYNAMIC_BASE
            + crate::naming::ACTION_NAMES
                .iter()
                .position(|name| *name == action.name())
                .unwrap() as u128
            + if dev { 0x100 } else { 0 },
    )
}
pub fn class_id(action: Action, dev: bool) -> GUID {
    GUID::from_u128(
        0x7686c014_ef9b_40d5_b2af_f87ea6700100
            + Action::ALL.iter().position(|a| *a == action).unwrap() as u128
            + if dev { 0x100 } else { 0 },
    )
}
#[implement(IExecuteCommand, IObjectWithSelection)]
struct ShellCommand {
    action: Action,
    selection: RefCell<Option<IShellItemArray>>,
    sink: Sink,
}
impl IExecuteCommand_Impl for ShellCommand_Impl {
    fn SetKeyState(&self, _: u32) -> windows::core::Result<()> {
        Ok(())
    }
    fn SetParameters(&self, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn SetPosition(&self, _: &POINT) -> windows::core::Result<()> {
        Ok(())
    }
    fn SetShowWindow(&self, _: i32) -> windows::core::Result<()> {
        Ok(())
    }
    fn SetNoShowUI(&self, _: BOOL) -> windows::core::Result<()> {
        Ok(())
    }
    fn SetDirectory(&self, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn Execute(&self) -> windows::core::Result<()> {
        let selection = self.selection.borrow();
        let array = selection
            .as_ref()
            .ok_or_else(|| windows::core::Error::from(E_INVALIDARG))?;
        let count = unsafe { array.GetCount()? };
        if count == 0 || count > 20_000 {
            return Err(E_INVALIDARG.into());
        }
        let mut paths = Vec::with_capacity(count as usize);
        let mut bytes = 0_usize;
        for index in 0..count {
            let value = unsafe { array.GetItemAt(index)?.GetDisplayName(SIGDN_FILESYSPATH)? };
            let string = unsafe { value.to_string() };
            unsafe { CoTaskMemFree(Some(value.0.cast())) };
            let string = string?;
            bytes = bytes.saturating_add(string.len() + 64);
            if bytes > 4 * 1024 * 1024 {
                return Err(E_INVALIDARG.into());
            }
            paths.push(PathBuf::from(string));
        }
        let request = Launch {
            action: self.action,
            paths,
        };
        request
            .validate()
            .map_err(|_| windows::core::Error::from(E_INVALIDARG))?;
        (self.sink)(request)
    }
}
impl IObjectWithSelection_Impl for ShellCommand_Impl {
    fn SetSelection(
        &self,
        array: windows::core::Ref<IShellItemArray>,
    ) -> windows::core::Result<()> {
        *self.selection.borrow_mut() = array.cloned();
        Ok(())
    }
    fn GetSelection(
        &self,
        iid: *const GUID,
        result: *mut *mut c_void,
    ) -> windows::core::Result<()> {
        if iid.is_null() || result.is_null() {
            return Err(E_INVALIDARG.into());
        }
        let selection = self.selection.borrow();
        let array = selection
            .as_ref()
            .ok_or_else(|| windows::core::Error::from(E_INVALIDARG))?;
        unsafe { array.query(iid, result).ok() }
    }
}
#[implement(IClassFactory)]
struct Factory {
    action: Action,
    sink: Sink,
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
        let command: IExecuteCommand = ShellCommand {
            action: self.action,
            selection: RefCell::new(None),
            sink: self.sink.clone(),
        }
        .into();
        unsafe { command.query(iid, result).ok() }
    }
    fn LockServer(&self, _: BOOL) -> windows::core::Result<()> {
        Ok(())
    }
}
pub fn serve(action: Action, dev: bool) -> windows::core::Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }
    let activity = Arc::new(Mutex::new(Instant::now()));
    let last = activity.clone();
    let factory: IClassFactory = Factory {
        action,
        sink: Arc::new(move |request| {
            *last.lock().unwrap() = Instant::now();
            super::dispatch(&request).map_err(|_| {
                windows::core::Error::new(E_FAIL, "PACKIT could not accept this selection.")
            })
        }),
    }
    .into();
    let cookie = unsafe {
        CoRegisterClassObject(
            &class_id(action, dev),
            &factory,
            CLSCTX_LOCAL_SERVER,
            REGCLS_MULTIPLEUSE,
        )?
    };
    // STA message pump services marshalled selection arrays. Bounded idle life;
    // no DLL is loaded into Explorer and no selected content is executed.
    while activity.lock().unwrap().elapsed() < Duration::from_secs(20) {
        let mut message = MSG::default();
        while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() } {
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    unsafe {
        CoRevokeClassObject(cookie)?;
        CoUninitialize();
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn com_selection_array_preserves_multiple_unicode_paths_without_shell_parsing() {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
        }
        let root = tempfile::tempdir().unwrap();
        let paths = [
            root.path().join("café & 日本.txt"),
            root.path().join("space (two).txt"),
        ];
        let mut ids = Vec::new();
        for path in &paths {
            std::fs::write(path, b"data").unwrap();
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
                &ids.iter().map(|p| *p as *const _).collect::<Vec<_>>(),
            )
            .unwrap()
        };
        let received = Arc::new(Mutex::new(None));
        let output = received.clone();
        let command: IExecuteCommand = ShellCommand {
            action: Action::Zip,
            selection: RefCell::new(None),
            sink: Arc::new(move |value| {
                *output.lock().unwrap() = Some(value);
                Ok(())
            }),
        }
        .into();
        let selection: IObjectWithSelection = command.cast().unwrap();
        unsafe {
            selection.SetSelection(&array).unwrap();
            command.Execute().unwrap();
        }
        assert_eq!(received.lock().unwrap().as_ref().unwrap().paths, paths);
        drop(selection);
        drop(command);
        drop(array);
        for id in ids {
            unsafe { CoTaskMemFree(Some(id.cast())) }
        }
        unsafe {
            CoUninitialize();
        }
    }
}
