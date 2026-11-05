// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use super::{enqueue, Request};
use crate::archive::ArchiveError;
use std::{
    ffi::c_void,
    ptr,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::{Pipes::*, RemoteDesktop::*, Threading::*, IO::*},
};
type Result<T> = std::result::Result<T, ArchiveError>;
fn fail(message: &str) -> ArchiveError {
    ArchiveError::new(
        "instance_ipc",
        format!("{message}: Windows error {}", unsafe { GetLastError() }),
    )
}
struct Handle(HANDLE);
impl Handle {
    fn raw(&self) -> HANDLE {
        self.0
    }
}
unsafe impl Send for Handle {}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub struct Guard(Handle);
impl Drop for Guard {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0 .0);
        }
    }
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
struct Security {
    descriptor: *mut c_void,
}
impl Security {
    fn new(sid: &str) -> Result<Self> {
        let mut descriptor = ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide(&format!("D:P(A;;GA;;;{sid})")).as_ptr(),
                1,
                &mut descriptor,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(fail("Could not restrict main-window IPC to this user"));
        }
        Ok(Self { descriptor })
    }
    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.descriptor,
            bInheritHandle: 0,
        }
    }
}
impl Drop for Security {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.descriptor);
        }
    }
}
fn identity() -> Result<String> {
    process_identity(unsafe { GetCurrentProcess() }, unsafe {
        GetCurrentProcessId()
    })
}
fn process_identity(process: HANDLE, pid: u32) -> Result<String> {
    unsafe {
        let mut token = ptr::null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return Err(fail("Could not identify current user"));
        }
        let token = Handle(token);
        let mut length = 0;
        GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut length);
        let mut data = vec![0_u64; (length as usize).div_ceil(8)];
        if GetTokenInformation(
            token.0,
            TokenUser,
            data.as_mut_ptr().cast(),
            length,
            &mut length,
        ) == 0
        {
            return Err(fail("Could not read user identity"));
        }
        let user = &*data.as_ptr().cast::<TOKEN_USER>();
        let mut sid = ptr::null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut sid) == 0 {
            return Err(fail("Could not format user identity"));
        }
        let mut count = 0;
        while *sid.add(count) != 0 {
            count += 1;
        }
        let value = String::from_utf16_lossy(std::slice::from_raw_parts(sid, count));
        LocalFree(sid.cast());
        let mut session = 0;
        if ProcessIdToSessionId(pid, &mut session) == 0 {
            return Err(fail("Could not identify Windows session"));
        }
        Ok(format!("{value}.{session}"))
    }
}
fn peer_is_expected(pipe: HANDLE, server: bool, expected: &str) -> bool {
    let mut pid = 0;
    let success = unsafe {
        if server {
            GetNamedPipeServerProcessId(pipe, &mut pid)
        } else {
            GetNamedPipeClientProcessId(pipe, &mut pid)
        }
    };
    if success == 0 {
        return false;
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return false;
    }
    let process = Handle(process);
    process_identity(process.0, pid).is_ok_and(|identity| identity == expected)
}
fn operation(pipe: HANDLE, buffer: *mut u8, length: u32, read: bool) -> Option<u32> {
    let event = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
    if event.is_null() {
        return None;
    }
    let event = Handle(event);
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    overlapped.hEvent = event.0;
    let mut count = 0;
    let done = unsafe {
        if read {
            ReadFile(pipe, buffer, length, &mut count, &mut overlapped)
        } else {
            WriteFile(
                pipe,
                buffer.cast_const(),
                length,
                &mut count,
                &mut overlapped,
            )
        }
    };
    if done == 0 {
        if unsafe { GetLastError() } != ERROR_IO_PENDING {
            return None;
        }
        if unsafe { WaitForSingleObject(event.0, 3000) } != WAIT_OBJECT_0 {
            unsafe {
                CancelIoEx(pipe, &overlapped);
                GetOverlappedResult(pipe, &overlapped, &mut count, 1);
            }
            return None;
        }
    }
    if unsafe { GetOverlappedResult(pipe, &overlapped, &mut count, 0) } == 0 {
        return None;
    }
    Some(count)
}
fn read_exact(pipe: HANDLE, bytes: &mut [u8]) -> bool {
    let mut at = 0;
    while at < bytes.len() {
        let Some(count) = operation(
            pipe,
            bytes[at..].as_mut_ptr(),
            (bytes.len() - at) as u32,
            true,
        ) else {
            return false;
        };
        if count == 0 {
            return false;
        }
        at += count as usize;
    }
    true
}
fn write_all(pipe: HANDLE, bytes: &[u8]) -> bool {
    let mut at = 0;
    while at < bytes.len() {
        let Some(count) = operation(
            pipe,
            bytes[at..].as_ptr().cast_mut(),
            (bytes.len() - at) as u32,
            false,
        ) else {
            return false;
        };
        if count == 0 {
            return false;
        }
        at += count as usize;
    }
    true
}
fn connect(pipe: HANDLE) -> bool {
    let event = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
    if event.is_null() {
        return false;
    }
    let event = Handle(event);
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    overlapped.hEvent = event.0;
    if unsafe { ConnectNamedPipe(pipe, &mut overlapped) } != 0 {
        return true;
    }
    match unsafe { GetLastError() } {
        ERROR_PIPE_CONNECTED => true,
        ERROR_IO_PENDING => (unsafe { WaitForSingleObject(event.0, INFINITE) }) == WAIT_OBJECT_0,
        _ => false,
    }
}
fn pipe(name: &[u16], security: &Security, first: bool) -> Result<Handle> {
    let attrs = security.attributes();
    let handle = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            PIPE_ACCESS_DUPLEX
                | FILE_FLAG_OVERLAPPED
                | if first {
                    FILE_FLAG_FIRST_PIPE_INSTANCE
                } else {
                    0
                },
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            4096,
            1024 * 1024,
            3000,
            &attrs,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(fail("Could not create secure main-window pipe"));
    }
    Ok(Handle(handle))
}
/// None means the acknowledged request was handed to the already-running main.
/// Popups bypass this function and remain independent short-lived processes.
pub fn elect(request: Request) -> Result<Option<Guard>> {
    elect_named(request, None)
}
fn elect_named(request: Request, namespace: Option<&str>) -> Result<Option<Guard>> {
    let user_session = identity()?;
    let sid = user_session.rsplit_once('.').unwrap().0;
    let identity = if let Some(namespace) = namespace {
        format!("{user_session}.Test.{namespace}")
    } else {
        user_session.clone()
    };
    let security = Security::new(sid)?;
    let attrs = security.attributes();
    let name = wide(&format!("Local\\PACKIT.Main.{identity}"));
    let mutex = unsafe { CreateMutexW(&attrs, 1, name.as_ptr()) };
    if mutex.is_null() {
        return Err(fail("Could not elect main window"));
    }
    let mutex = Handle(mutex);
    let exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    let pipe_name = wide(&format!("\\\\.\\pipe\\PACKIT.Main.{identity}"));
    if exists {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let state = unsafe { WaitForSingleObject(mutex.0, 0) };
            if state == WAIT_ABANDONED || state == WAIT_OBJECT_0 {
                break;
            }
            let connection = unsafe {
                CreateFileW(
                    pipe_name.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    ptr::null(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED,
                    ptr::null_mut(),
                )
            };
            if connection != INVALID_HANDLE_VALUE {
                let connection = Handle(connection);
                if !peer_is_expected(connection.0, true, &user_session) {
                    return Err(ArchiveError::new(
                        "instance_ipc",
                        "The named-pipe server does not belong to this Windows user session.",
                    ));
                }
                let bytes = serde_json::to_vec(&request).map_err(ArchiveError::worker)?;
                if bytes.len() > 1024 * 1024 {
                    return Err(ArchiveError::new(
                        "invalid_request",
                        "Main-window request exceeds 1 MiB.",
                    ));
                }
                let mut answer = [0];
                if write_all(connection.0, &(bytes.len() as u32).to_le_bytes())
                    && write_all(connection.0, &bytes)
                    && read_exact(connection.0, &mut answer)
                {
                    let _ = write_all(connection.0, &[1]);
                    if answer[0] == 1 {
                        return Ok(None);
                    }
                    return Err(ArchiveError::new("operation_busy","The main PACKIT window could not queue this request. Retry after it completes its current operation."));
                }
            }
            if Instant::now() >= deadline {
                return Err(ArchiveError::new("instance_ipc","The existing PACKIT main window did not acknowledge the request. No additional main window was opened."));
            }
            std::thread::sleep(Duration::from_millis(30));
        }
    }
    let listener = pipe(&pipe_name, &security, true)?;
    enqueue(request)?;
    std::thread::spawn(move || loop {
        let connected = connect(listener.raw());
        if connected {
            if !peer_is_expected(listener.raw(), false, &user_session) {
                unsafe {
                    DisconnectNamedPipe(listener.raw());
                }
                continue;
            }
            let mut length = [0; 4];
            let mut ok = false;
            if read_exact(listener.raw(), &mut length) {
                let length = u32::from_le_bytes(length) as usize;
                if length > 0 && length <= 1024 * 1024 {
                    let mut bytes = vec![0; length];
                    if read_exact(listener.raw(), &mut bytes) {
                        if let Ok(request) = serde_json::from_slice::<Request>(&bytes) {
                            ok = enqueue(request).is_ok();
                        }
                    }
                }
            }
            let _ = write_all(listener.raw(), &[u8::from(ok)]);
            let _ = read_exact(listener.raw(), &mut [0]);
            unsafe {
                DisconnectNamedPipe(listener.raw());
            }
        } else {
            break;
        }
    });
    Ok(Some(Guard(mutex)))
}

#[cfg(debug_assertions)]
pub fn probe(args: &[std::ffi::OsString]) -> Result<()> {
    use std::io::Write;
    if args.len() != 6 {
        return Err(ArchiveError::new(
            "invalid_request",
            "Invalid instance probe.",
        ));
    }
    let token = args[2].to_str().unwrap_or("");
    if token.len() < 20 || token.len() > 80 || !token.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(ArchiveError::new(
            "invalid_request",
            "Invalid test namespace.",
        ));
    }
    let expected = args[4]
        .to_str()
        .unwrap_or("")
        .parse::<usize>()
        .map_err(ArchiveError::worker)?;
    if expected == 0 || expected > 128 {
        return Err(ArchiveError::new(
            "invalid_request",
            "Invalid test request count.",
        ));
    }
    let id = args[5]
        .to_str()
        .ok_or_else(|| ArchiveError::new("invalid_request", "Invalid test ID."))?
        .to_owned();
    if let Some(_guard) = elect_named(Request { id, launch: None }, Some(token))? {
        println!("PRIMARY");
        std::io::stdout().flush().map_err(ArchiveError::worker)?;
        let mut received = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(15);
        while received.len() < expected && Instant::now() < deadline {
            if let Some(request) = super::next() {
                super::acknowledge(&request.id);
                received.push(request.id);
            } else {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        if received.len() != expected {
            return Err(ArchiveError::new(
                "instance_ipc",
                "Missing requests during concurrent instance probe.",
            ));
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&args[3])
            .map_err(ArchiveError::worker)?;
        file.write_all(&serde_json::to_vec(&received).map_err(ArchiveError::worker)?)
            .map_err(ArchiveError::worker)?;
    } else {
        println!("FORWARDED");
    }
    Ok(())
}
