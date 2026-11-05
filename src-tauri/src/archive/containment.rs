// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
//! Windows AppContainer worker. No fallback to an ordinary user-token process.
//! The package has no capabilities; only copied inputs/code receive read grants.
use super::{ArchiveError, Result};
use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io, mem,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::FromRawHandle,
        process::ExitStatusExt,
    },
    path::{Path, PathBuf},
    process::ExitStatus,
    ptr,
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, Isolation::*, *},
    Storage::FileSystem::*,
    System::{Pipes::CreatePipe, Threading::*},
};

fn fail(action: &str) -> ArchiveError {
    ArchiveError::new("containment_unavailable", format!("Native archive containment could not be established ({action}; Windows {}). No unrestricted worker was started.", io::Error::last_os_error()))
}
fn wide(value: &OsStr) -> Result<Vec<u16>> {
    let mut value: Vec<_> = value.encode_wide().collect();
    if value.contains(&0) {
        return Err(fail("invalid process path"));
    }
    value.push(0);
    Ok(value)
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub(super) struct Container {
    guards: Vec<cap_std::fs::Dir>,
    name: Vec<u16>,
    sid: PSID,
    // Code is copied, not granted access in the installation or user workspace.
    code: tempfile::TempDir,
    pub output: tempfile::TempDir,
    profile: PathBuf,
}
// Profile SID and immutable paths are shared only with the bounded watchdog.
unsafe impl Send for Container {}
unsafe impl Sync for Container {}
impl Container {
    pub fn new(tool: &Path) -> Result<Self> {
        let code = tempfile::Builder::new()
            .prefix("packit-worker-")
            .tempdir()
            .map_err(|_| fail("private code directory"))?;
        let output = tempfile::Builder::new()
            .prefix("packit-worker-output-")
            .tempdir()
            .map_err(|_| fail("private output directory"))?;
        let name = wide(OsStr::new(&format!(
            "PACKIT.Worker.{}",
            code.path().file_name().unwrap().to_string_lossy()
        )))?;
        let mut sid = ptr::null_mut();
        let hr = unsafe {
            CreateAppContainerProfile(
                name.as_ptr(),
                name.as_ptr(),
                name.as_ptr(),
                ptr::null(),
                0,
                &mut sid,
            )
        };
        if hr < 0 || sid.is_null() {
            return Err(ArchiveError::new("containment_unavailable",format!("AppContainer profile creation failed (HRESULT {hr:#x}); native processing was not started.")));
        }
        let mut container = Self {
            guards: Vec::new(),
            name,
            sid,
            code,
            output,
            profile: PathBuf::new(),
        };
        let mut sid_text = ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(sid, &mut sid_text) } == 0 {
            return Err(fail("package identity"));
        }
        let mut folder = ptr::null_mut();
        let hr = unsafe { GetAppContainerFolderPath(sid_text, &mut folder) };
        unsafe {
            LocalFree(sid_text.cast());
        }
        if hr < 0 {
            return Err(fail("profile location"));
        }
        let mut n = 0;
        unsafe {
            while *folder.add(n) != 0 {
                n += 1;
            }
        }
        container.profile = PathBuf::from(OsString::from_wide(unsafe {
            std::slice::from_raw_parts(folder, n)
        }));
        unsafe {
            windows_sys::Win32::System::Com::CoTaskMemFree(folder.cast());
        }
        container
            .guards
            .push(super::output::open_destination(container.output.path())?);
        container
            .guards
            .push(super::output::open_destination(&container.profile)?);
        container.grant(
            container.code.path(),
            FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
            false,
        )?;
        let source = super::streaming::open(tool)?;
        let mut target = File::options()
            .write(true)
            .create_new(true)
            .open(container.code.path().join("worker.exe"))
            .map_err(|_| fail("code copy"))?;
        io::copy(&mut source.take(64 << 20), &mut target).map_err(|_| fail("code copy"))?;
        drop(target);
        container.grant(
            &container.code.path().join("worker.exe"),
            FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
            false,
        )?;
        let dll = tool.with_file_name("7z.dll");
        if dll.is_file() {
            let mut source = super::streaming::open(&dll)?;
            let mut target = File::options()
                .write(true)
                .create_new(true)
                .open(container.code.path().join("7z.dll"))
                .map_err(|_| fail("codec copy"))?;
            io::copy(&mut source, &mut target).map_err(|_| fail("codec copy"))?;
            drop(target);
            container.grant(
                &container.code.path().join("7z.dll"),
                FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
                false,
            )?;
        }
        Ok(container)
    }
    fn grant(&self, path: &Path, rights: u32, inherit: bool) -> Result<()> {
        self.grant_as(path, rights, inherit, self.sid)
    }
    fn grant_as(&self, path: &Path, rights: u32, inherit: bool, sid: PSID) -> Result<()> {
        let path = wide(path.as_os_str())?;
        let mut old = ptr::null_mut();
        let mut descriptor = ptr::null_mut();
        if unsafe {
            GetNamedSecurityInfoW(
                path.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut old,
                ptr::null_mut(),
                &mut descriptor,
            )
        } != 0
        {
            return Err(fail("read workspace ACL"));
        }
        let entry = EXPLICIT_ACCESS_W {
            grfAccessPermissions: rights,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: if inherit {
                SUB_CONTAINERS_AND_OBJECTS_INHERIT
            } else {
                0
            },
            Trustee: TRUSTEE_W {
                pMultipleTrustee: ptr::null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_UNKNOWN,
                ptstrName: sid.cast(),
            },
        };
        let mut acl = ptr::null_mut();
        let result = unsafe { SetEntriesInAclW(1, &entry, old, &mut acl) };
        let set = if result == 0 {
            unsafe {
                SetNamedSecurityInfoW(
                    path.as_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    acl,
                    ptr::null(),
                )
            }
        } else {
            result
        };
        unsafe {
            LocalFree(descriptor);
            if !acl.is_null() {
                LocalFree(acl.cast());
            }
        }
        if set != 0 {
            return Err(fail("grant private workspace ACL"));
        }
        Ok(())
    }
    pub fn allow_read(&self, path: &Path) -> Result<()> {
        let metadata = std::fs::symlink_metadata(path).map_err(|_| fail("input metadata"))?;
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 || (!metadata.is_dir() && !metadata.is_file()) {
            return Err(fail("unsafe input type"));
        }
        self.grant(
            path,
            if metadata.is_dir() {
                FILE_GENERIC_READ | FILE_GENERIC_EXECUTE
            } else {
                FILE_GENERIC_READ
            },
            false,
        )
    }
    pub fn allow_tree(&self, path: &Path) -> Result<()> {
        fn walk(container: &Container, path: &Path, depth: usize, count: &mut usize) -> Result<()> {
            *count += 1;
            if depth > 34 || *count > 2048 {
                return Err(fail("snapshot limits"));
            }
            container.allow_read(path)?;
            if path.is_dir() {
                for entry in std::fs::read_dir(path).map_err(|_| fail("snapshot enumeration"))? {
                    walk(
                        container,
                        &entry.map_err(|_| fail("snapshot entry"))?.path(),
                        depth + 1,
                        count,
                    )?;
                }
            }
            Ok(())
        }
        walk(self, path, 0, &mut 0)
    }
    pub fn allow_output(&self) -> Result<()> {
        self.grant(
            self.output.path(),
            FILE_GENERIC_READ
                | FILE_GENERIC_WRITE
                | FILE_GENERIC_EXECUTE
                | DELETE
                | FILE_DELETE_CHILD,
            true,
        )?;
        // Low-integrity worker writes only its disposable encoder scratch.
        let mut descriptor = ptr::null_mut();
        let sddl = wide(OsStr::new("S:(ML;OICI;NW;;;LW)"))?;
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(fail("scratch integrity label"));
        }
        let mut present = 0;
        let mut defaulted = 0;
        let mut sacl = ptr::null_mut();
        let valid = unsafe {
            GetSecurityDescriptorSacl(descriptor, &mut present, &mut sacl, &mut defaulted)
        };
        let path = wide(self.output.path().as_os_str())?;
        let status = if valid != 0 {
            unsafe {
                SetNamedSecurityInfoW(
                    path.as_ptr(),
                    SE_FILE_OBJECT,
                    LABEL_SECURITY_INFORMATION,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null(),
                    sacl,
                )
            }
        } else {
            1
        };
        unsafe {
            LocalFree(descriptor);
        }
        if status != 0 {
            return Err(fail("scratch integrity label"));
        }
        Ok(())
    }
    pub fn output_within_limits(&self) -> bool {
        self.within_limits(2 << 30)
    }
    fn within_limits(&self, limit: u64) -> bool {
        fn scan(
            dir: &cap_std::fs::Dir,
            depth: usize,
            count: &mut usize,
            size: &mut u64,
            limit: u64,
        ) -> io::Result<()> {
            use cap_std::fs::MetadataExt;
            if depth > 8 {
                return Err(io::Error::other("worker scratch depth"));
            }
            for item in dir.entries()? {
                let item = item?;
                let name = item.file_name();
                let m = dir.symlink_metadata(&name)?;
                *count += 1;
                if *count > 1024 || m.file_attributes() & 0x400 != 0 {
                    return Err(io::Error::other("worker scratch type/count"));
                }
                if m.is_dir() {
                    scan(
                        &super::output::open_directory(dir, &name)?,
                        depth + 1,
                        count,
                        size,
                        limit,
                    )?;
                } else if m.is_file() {
                    *size = size
                        .checked_add(m.len())
                        .ok_or_else(|| io::Error::other("worker size"))?;
                    if *size > limit {
                        return Err(io::Error::other("worker scratch size"));
                    }
                } else {
                    return Err(io::Error::other("worker scratch type"));
                }
            }
            Ok(())
        }
        let mut count = 0;
        let mut size = 0;
        self.guards
            .iter()
            .all(|dir| scan(dir, 0, &mut count, &mut size, limit).is_ok())
    }

    pub fn spawn(&self, args: &[OsString], cwd: &Path) -> Result<Child> {
        let executable = self.code.path().join("worker.exe");
        let app = wide(executable.as_os_str())?;
        let mut command = quote(executable.as_os_str())?;
        for arg in args {
            command.push(b' ' as u16);
            command.extend(quote(arg)?);
        }
        command.push(0);
        let cwd = wide(cwd.as_os_str())?;
        if std::env::var_os("SystemRoot").is_none() {
            return Err(fail("SystemRoot"));
        }
        let mut env = Vec::new();
        let mut variables = std::collections::BTreeMap::new();
        for name in [
            "SystemRoot",
            "SystemDrive",
            "USERPROFILE",
            "ALLUSERSPROFILE",
        ] {
            if let Some(value) = std::env::var_os(name) {
                variables.insert(name, value);
            }
        }
        let local = self.profile.join("AC");
        let temp = local.join("Temp");
        std::fs::create_dir_all(&temp).map_err(|_| fail("private profile temp"))?;
        variables.insert("LOCALAPPDATA", local.into_os_string());
        variables.insert("TEMP", temp.clone().into_os_string());
        variables.insert("TMP", temp.into_os_string());
        for (name, value) in variables {
            let mut entry = OsString::from(name);
            entry.push("=");
            entry.push(value);
            env.extend(wide(&entry)?);
        }
        env.push(0);
        let (out_read, out_write) = pipe()?;
        let (err_read, err_write) = pipe()?;
        let (in_read, in_write) = pipe()?;
        // Only the three intended pipe endpoints are inheritable in this launch.
        for h in [out_read.0, err_read.0, in_write.0] {
            if unsafe { SetHandleInformation(h, HANDLE_FLAG_INHERIT, 0) } == 0 {
                return Err(fail("pipe inheritance"));
            }
        }
        let mut handles = [in_read.0, out_write.0, err_write.0];
        let capabilities = SECURITY_CAPABILITIES {
            AppContainerSid: self.sid,
            Capabilities: ptr::null_mut(),
            CapabilityCount: 0,
            Reserved: 0,
        };
        let mitigations: u64 = 1 | (1 << 32) | (1 << 52); // DEP, disable extension points, no remote image loads.
        let policy: u32 = 1; // PROCESS_CREATION_ALL_APPLICATION_PACKAGES_OPT_OUT (LPAC).
        let mut bytes = 0;
        unsafe {
            InitializeProcThreadAttributeList(ptr::null_mut(), 4, 0, &mut bytes);
        }
        if bytes == 0 {
            return Err(fail("process attributes size"));
        }
        let mut allocation = vec![0usize; bytes.div_ceil(mem::size_of::<usize>())];
        let attributes = allocation.as_mut_ptr().cast();
        if unsafe { InitializeProcThreadAttributeList(attributes, 4, 0, &mut bytes) } == 0 {
            return Err(fail("process attributes"));
        }
        struct Attributes(LPPROC_THREAD_ATTRIBUTE_LIST);
        impl Drop for Attributes {
            fn drop(&mut self) {
                unsafe {
                    DeleteProcThreadAttributeList(self.0);
                }
            }
        }
        let attributes = Attributes(attributes);
        for (key, value, size) in [
            (
                PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
                (&capabilities as *const SECURITY_CAPABILITIES).cast(),
                mem::size_of_val(&capabilities),
            ),
            (
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_mut_ptr().cast(),
                mem::size_of_val(&handles),
            ),
            (
                PROC_THREAD_ATTRIBUTE_MITIGATION_POLICY as usize,
                (&mitigations as *const u64).cast(),
                mem::size_of_val(&mitigations),
            ),
            (
                PROC_THREAD_ATTRIBUTE_ALL_APPLICATION_PACKAGES_POLICY as usize,
                (&policy as *const u32).cast(),
                mem::size_of_val(&policy),
            ),
        ] {
            if unsafe {
                UpdateProcThreadAttribute(
                    attributes.0,
                    0,
                    key,
                    value,
                    size,
                    ptr::null_mut(),
                    ptr::null(),
                )
            } == 0
            {
                return Err(fail("process security attributes"));
            }
        }
        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = mem::size_of::<STARTUPINFOEXW>() as u32;
        startup.lpAttributeList = attributes.0;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = in_read.0;
        startup.StartupInfo.hStdOutput = out_write.0;
        startup.StartupInfo.hStdError = err_write.0;
        let mut info = PROCESS_INFORMATION::default();
        if unsafe {
            CreateProcessW(
                app.as_ptr(),
                command.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                1,
                CREATE_NO_WINDOW
                    | CREATE_SUSPENDED
                    | EXTENDED_STARTUPINFO_PRESENT
                    | CREATE_UNICODE_ENVIRONMENT,
                env.as_ptr().cast(),
                cwd.as_ptr(),
                &startup.StartupInfo,
                &mut info,
            )
        } == 0
        {
            return Err(fail("AppContainer CreateProcess"));
        }
        let mut child = Child {
            process: Handle(info.hProcess),
            thread: Handle(info.hThread),
            stdout: None,
            stderr: None,
            finished: false,
        };
        // Verify the actual token before any decoder instruction can run.
        let mut token = ptr::null_mut();
        if unsafe { OpenProcessToken(child.process.0, TOKEN_QUERY, &mut token) } == 0 {
            return Err(fail("worker token"));
        }
        let token = Handle(token);
        let mut contained = 0u32;
        let mut length = 0;
        let valid = unsafe {
            GetTokenInformation(
                token.0,
                TokenIsAppContainer,
                (&mut contained as *mut u32).cast(),
                4,
                &mut length,
            )
        };
        unsafe {
            GetTokenInformation(
                token.0,
                TokenAppContainerSid,
                ptr::null_mut(),
                0,
                &mut length,
            );
        }
        if length == 0 || length > 4096 {
            return Err(fail("worker package SID length"));
        }
        let mut identity = vec![0usize; (length as usize).div_ceil(mem::size_of::<usize>())];
        let valid_sid = unsafe {
            GetTokenInformation(
                token.0,
                TokenAppContainerSid,
                identity.as_mut_ptr().cast(),
                length,
                &mut length,
            )
        };
        let actual = unsafe { &*identity.as_ptr().cast::<TOKEN_APPCONTAINER_INFORMATION>() };
        if valid == 0
            || contained != 1
            || valid_sid == 0
            || unsafe { EqualSid(actual.TokenAppContainer, self.sid) } == 0
        {
            return Err(fail("worker package token verification"));
        }
        drop(out_write);
        drop(err_write);
        drop(in_write);
        drop(in_read);

        // Transfer ownership of parent pipe handles to File exactly once.
        let out = out_read.0;
        let err = err_read.0;
        mem::forget(out_read);
        mem::forget(err_read);
        child.stdout = Some(unsafe { File::from_raw_handle(out) });
        child.stderr = Some(unsafe { File::from_raw_handle(err) });
        Ok(child)
    }
}
impl Drop for Container {
    fn drop(&mut self) {
        self.guards.clear();
        unsafe {
            if DeleteAppContainerProfile(self.name.as_ptr()) < 0 {
                tracing::warn!("Private native worker profile cleanup failed");
            }
            FreeSid(self.sid);
        }
    }
}
fn pipe() -> Result<(Handle, Handle)> {
    let mut r = ptr::null_mut();
    let mut w = ptr::null_mut();
    let security = SECURITY_ATTRIBUTES {
        nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: ptr::null_mut(),
        bInheritHandle: 1,
    };
    if unsafe { CreatePipe(&mut r, &mut w, &security, 0) } == 0 {
        return Err(fail("worker pipes"));
    }
    Ok((Handle(r), Handle(w)))
}
fn quote(arg: &OsStr) -> Result<Vec<u16>> {
    let arg = wide(arg)?;
    let mut out = vec![34];
    let mut slashes = 0;
    for &c in &arg[..arg.len() - 1] {
        if c == 92 {
            slashes += 1;
            continue;
        }
        if c == 34 {
            out.extend(std::iter::repeat_n(92, slashes * 2 + 1));
            out.push(34);
        } else {
            out.extend(std::iter::repeat_n(92, slashes));
            out.push(c);
        }
        slashes = 0;
    }
    out.extend(std::iter::repeat_n(92, slashes * 2));
    out.push(34);
    Ok(out)
}
pub(super) struct Child {
    process: Handle,
    thread: Handle,
    pub stdout: Option<File>,
    pub stderr: Option<File>,
    finished: bool,
}
impl Child {
    pub fn handle(&self) -> HANDLE {
        self.process.0
    }
    pub fn resume(&self) -> bool {
        unsafe { ResumeThread(self.thread.0) != u32::MAX }
    }
    pub fn kill(&mut self) -> io::Result<()> {
        if unsafe { TerminateProcess(self.process.0, 124) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        if unsafe { WaitForSingleObject(self.process.0, INFINITE) } != WAIT_OBJECT_0 {
            return Err(io::Error::last_os_error());
        }
        let mut code = 0;
        if unsafe { GetExitCodeProcess(self.process.0, &mut code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        self.finished = true;
        Ok(ExitStatus::from_raw(code))
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.kill();
            let _ = self.wait();
        }
    }
}
use std::io::Read;

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Read, os::windows::io::AsRawHandle, sync::OnceLock};
    use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
    fn probe() -> &'static Path {
        static PROBE: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
        &PROBE
            .get_or_init(|| {
                let root = tempfile::tempdir().unwrap();
                let source = root.path().join("probe.rs");
                let exe = root.path().join("probe.exe");
                std::fs::write(&source, include_str!("../../tests/support/worker_probe.rs"))
                    .unwrap();
                let status = std::process::Command::new("rustc")
                    .args(["--edition=2021", "-O"])
                    .arg(&source)
                    .arg("-o")
                    .arg(&exe)
                    .status()
                    .unwrap();
                assert!(status.success());
                (root, exe)
            })
            .1
    }
    #[test]
    fn isolated_worker_denies_user_files_writes_shared_package_access_and_extra_handles() {
        let root = tempfile::tempdir().unwrap();
        let input = root.path().join("allowed 日本.txt");
        let outside = root.path().join("private.txt");
        let forbidden = root.path().join("created-outside.txt");
        let shared = root.path().join("all-packages-readable.txt");
        std::fs::write(&input, b"allowed").unwrap();
        std::fs::write(&outside, b"PRIVATE_HANDLE_SENTINEL").unwrap();
        std::fs::write(&shared, b"shared sentinel").unwrap();
        let container = Container::new(probe()).unwrap();
        container.allow_read(root.path()).unwrap();
        container.allow_read(&input).unwrap();
        let mut sid = ptr::null_mut();
        let sid_text = wide(OsStr::new("S-1-15-2-1")).unwrap();
        assert_ne!(
            unsafe { ConvertStringSidToSidW(sid_text.as_ptr(), &mut sid) },
            0
        );
        container
            .grant_as(&shared, FILE_GENERIC_READ, false, sid)
            .unwrap();
        unsafe {
            LocalFree(sid);
        }
        let secret = File::open(&outside).unwrap();
        assert_ne!(
            unsafe {
                SetHandleInformation(
                    secret.as_raw_handle(),
                    HANDLE_FLAG_INHERIT,
                    HANDLE_FLAG_INHERIT,
                )
            },
            0
        );
        let code = container.code.path().join("worker.exe");
        let argument = "café \\\" & $(do-not-run) 日本\\";
        let args: Vec<OsString> = vec![
            input.as_os_str().into(),
            outside.as_os_str().into(),
            forbidden.as_os_str().into(),
            container.output.path().join("scratch").into_os_string(),
            code.into_os_string(),
            shared.into_os_string(),
            argument.into(),
            format!("{}", secret.as_raw_handle() as usize).into(),
        ];
        let job = super::super::seven_zip::Job::new().unwrap();
        let mut child = container.spawn(&args, container.code.path()).unwrap();
        assert_ne!(
            unsafe { AssignProcessToJobObject(job.0, child.handle()) },
            0
        );
        let mut leaked = ptr::null_mut();
        assert_eq!(
            unsafe {
                DuplicateHandle(
                    child.handle(),
                    secret.as_raw_handle(),
                    GetCurrentProcess(),
                    &mut leaked,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                )
            },
            0,
            "An unintended sentinel handle was inherited"
        );
        assert!(child.resume());
        let mut output = String::new();
        child
            .stdout
            .take()
            .unwrap()
            .take(4096)
            .read_to_string(&mut output)
            .unwrap();
        let status = child.wait().unwrap();
        let mut diagnostics = String::new();
        child
            .stderr
            .take()
            .unwrap()
            .take(4096)
            .read_to_string(&mut diagnostics)
            .unwrap();
        assert!(status.success(), "{status:?} {diagnostics} {output}");
        assert!(output.contains("read=true outsider=false input_write=false outside_write=false scratch_write=false code_write=false shared_read=false"),"{output}");
        assert!(output.contains(&format!("argument={argument}")));
        assert!(!forbidden.exists());
        assert_eq!(std::fs::read(&outside).unwrap(), b"PRIVATE_HANDLE_SENTINEL");
        // The exact same worker gets write access only to encoder scratch when requested.
        container.allow_output().unwrap();
        let mut child = container.spawn(&args, container.code.path()).unwrap();
        assert_ne!(
            unsafe { AssignProcessToJobObject(job.0, child.handle()) },
            0
        );
        assert!(child.resume());
        let mut output = String::new();
        child
            .stdout
            .take()
            .unwrap()
            .take(4096)
            .read_to_string(&mut output)
            .unwrap();
        assert!(child.wait().unwrap().success());
        assert!(
            output.contains("scratch_write=true code_write=false shared_read=false"),
            "{output}"
        );
        assert!(container.output_within_limits());
        std::fs::write(container.output.path().join("oversized"), [0; 17]).unwrap();
        assert!(!container.within_limits(16));
    }
    #[test]
    fn isolated_worker_is_subject_to_job_memory_limit() {
        let container = Container::new(probe()).unwrap();
        let job = super::super::seven_zip::Job::new().unwrap();
        let mut child = container
            .spawn(&["memory".into()], container.code.path())
            .unwrap();
        assert_ne!(
            unsafe { AssignProcessToJobObject(job.0, child.handle()) },
            0
        );
        assert!(child.resume());
        let mut text = String::new();
        child
            .stdout
            .take()
            .unwrap()
            .take(4096)
            .read_to_string(&mut text)
            .unwrap();
        let status = child.wait().unwrap();
        assert!(!status.success() || text.contains("memory_limited"));
    }
}

#[cfg(test)]
mod fail_closed_tests {
    use super::*;
    #[test]
    fn missing_worker_does_not_fall_back_to_an_uncontained_executable() {
        let root = tempfile::tempdir().unwrap();
        assert!(Container::new(&root.path().join("missing.exe")).is_err());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
        assert!(quote(OsStr::new("bad\0argument")).is_err());
    }
}
