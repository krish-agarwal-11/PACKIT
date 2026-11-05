// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use crate::archive::{ArchiveEntry, ArchiveError};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};
use tauri::Emitter;
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DestinationMode {
    Folder,
    Here,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub operation_id: String,
    pub selection: Option<Vec<String>>,
    pub password: Option<String>,
    pub session_id: Option<String>,
    pub destination_mode: Option<DestinationMode>,
}

impl Drop for Request {
    fn drop(&mut self) {
        if let Some(password) = &mut self.password {
            password.zeroize();
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub operation_id: String,
    pub state: String,
    pub current_file: Option<String>,
    pub processed_bytes: String,
    pub total_bytes: Option<String>,
    pub completed_files: usize,
    pub total_files: Option<usize>,
}

pub struct Control {
    pub cancelled: AtomicBool,
    #[cfg(test)]
    pub cancel_after_commit: AtomicBool,
    #[cfg(test)]
    pub cancel_byte_phase: Mutex<Option<String>>,
    progress: Mutex<(Progress, Instant)>,
    emit: Box<dyn Fn(Progress) + Send + Sync>,
}

struct Context {
    control: Arc<Control>,
    selection: Option<Vec<String>>,
    password: Option<Zeroizing<Vec<u8>>>,
    session_id: Option<String>,
    extract_here: bool,
    protected_sources: Vec<(u64, u64)>,
}

thread_local! { static CURRENT: RefCell<Option<Context>> = const { RefCell::new(None) }; }
static INSTALLING_UPDATE: AtomicBool = AtomicBool::new(false);
static ACTIVE: OnceLock<Mutex<Option<Arc<Control>>>> = OnceLock::new();
fn active() -> &'static Mutex<Option<Arc<Control>>> {
    ACTIVE.get_or_init(|| Mutex::new(None))
}

pub fn reserve(request: &Request, app: tauri::AppHandle) -> Result<Arc<Control>, ArchiveError> {
    if request.operation_id.is_empty()
        || request.operation_id.len() > 100
        || request
            .session_id
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.len() > 100)
        || request
            .selection
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.len() > 20_000 || s.iter().any(|p| p.len() > 1024))
        || request.password.as_ref().is_some_and(|p| p.len() > 1024)
    {
        return Err(ArchiveError::new(
            "invalid_request",
            "Invalid operation identifier, selection or password length.",
        ));
    }
    let mut slot = active().lock().unwrap();
    if slot.is_some() || INSTALLING_UPDATE.load(Ordering::Acquire) {
        return Err(ArchiveError::new(
            "operation_busy",
            "Another archive operation is active.",
        ));
    }
    let control = new_control(request.operation_id.clone(), move |progress| {
        let _ = app.emit("archive-progress", progress);
    });
    *slot = Some(control.clone());
    Ok(control)
}

fn new_control(id: String, emit: impl Fn(Progress) + Send + Sync + 'static) -> Arc<Control> {
    Arc::new(Control {
        cancelled: AtomicBool::new(false),
        #[cfg(test)]
        cancel_after_commit: AtomicBool::new(false),
        #[cfg(test)]
        cancel_byte_phase: Mutex::new(None),
        progress: Mutex::new((
            Progress {
                operation_id: id,
                state: "preparing".into(),
                current_file: None,
                processed_bytes: "0".into(),
                total_bytes: None,
                completed_files: 0,
                total_files: None,
            },
            Instant::now(),
        )),
        emit: Box::new(emit),
    })
}

pub struct UpdateGuard;
impl Drop for UpdateGuard {
    fn drop(&mut self) {
        INSTALLING_UPDATE.store(false, Ordering::Release);
    }
}
pub fn reserve_update() -> Result<UpdateGuard, ArchiveError> {
    let slot = active().lock().unwrap();
    if slot.is_some() || INSTALLING_UPDATE.swap(true, Ordering::AcqRel) {
        return Err(ArchiveError::new(
            "operation_busy",
            "Finish archive operations before updating.",
        ));
    }
    Ok(UpdateGuard)
}

pub fn is_active() -> bool {
    active().lock().unwrap().is_some()
}
pub fn cancel_active() -> bool {
    let slot = active().lock().unwrap();
    if let Some(control) = slot.as_ref() {
        control.cancelled.store(true, Ordering::Release);
        true
    } else {
        false
    }
}

pub fn cancel(id: &str) -> bool {
    let slot = active().lock().unwrap();
    let Some(control) = slot
        .as_ref()
        .filter(|c| c.progress.lock().unwrap().0.operation_id == id)
    else {
        return false;
    };
    control.cancelled.store(true, Ordering::Release);
    true
}

pub fn run<T>(
    mut request: Request,
    control: Arc<Control>,
    work: impl FnOnce() -> Result<T, ArchiveError>,
) -> Result<T, ArchiveError> {
    let password = request.password.take().map(|mut password| {
        let bytes = Zeroizing::new(password.as_bytes().to_vec());
        password.zeroize();
        bytes
    });
    CURRENT.with(|slot| {
        *slot.borrow_mut() = Some(Context {
            control: control.clone(),
            selection: request.selection.take(),
            password,
            session_id: request.session_id.take(),
            extract_here: matches!(request.destination_mode, Some(DestinationMode::Here)),
            protected_sources: Vec::new(),
        })
    });
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            CURRENT.with(|slot| *slot.borrow_mut() = None);
            *active().lock().unwrap() = None;
        }
    }
    let _guard = Guard;
    phase("preparing", None);
    let result = check().and_then(|_| work());
    let state = match &result {
        Ok(_) => "completed",
        Err(e) if e.code == "cancelled" => "cancelled",
        Err(_) => "failed",
    };
    phase(state, None);
    result
}

pub fn control() -> Option<Arc<Control>> {
    CURRENT.with(|s| s.borrow().as_ref().map(|c| c.control.clone()))
}

pub fn check() -> Result<(), ArchiveError> {
    if control().is_some_and(|c| c.cancelled.load(Ordering::Acquire)) {
        Err(ArchiveError::new("cancelled", "Operation cancelled."))
    } else {
        Ok(())
    }
}

pub fn has_password() -> bool {
    CURRENT.with(|s| s.borrow().as_ref().is_some_and(|c| c.password.is_some()))
}
pub fn password<T>(work: impl FnOnce(Option<&[u8]>) -> T) -> T {
    CURRENT.with(|s| {
        let context = s.borrow();
        work(
            context
                .as_ref()
                .and_then(|c| c.password.as_ref())
                .map(|s| s.as_slice()),
        )
    })
}

pub fn protect_sources(paths: &[std::path::PathBuf]) -> Result<(), ArchiveError> {
    use cap_fs_ext::MetadataExt;
    let identities = paths
        .iter()
        .map(|path| {
            let file = std::fs::File::open(path).map_err(ArchiveError::worker)?;
            let metadata = cap_std::fs::File::from_std(file)
                .metadata()
                .map_err(ArchiveError::worker)?;
            Ok((metadata.dev(), metadata.ino()))
        })
        .collect::<Result<Vec<_>, ArchiveError>>()?;
    CURRENT.with(|s| {
        if let Some(context) = s.borrow_mut().as_mut() {
            context.protected_sources = identities;
        }
    });
    Ok(())
}
pub fn is_source(device: u64, inode: u64) -> bool {
    CURRENT.with(|s| {
        s.borrow()
            .as_ref()
            .is_some_and(|context| context.protected_sources.contains(&(device, inode)))
    })
}

pub fn extract_here() -> bool {
    CURRENT.with(|s| s.borrow().as_ref().is_some_and(|c| c.extract_here))
}
#[cfg(test)]
pub fn set_extract_here() {
    CURRENT.with(|s| s.borrow_mut().as_mut().unwrap().extract_here = true);
}

pub fn session_id() -> Option<String> {
    CURRENT.with(|s| s.borrow().as_ref().and_then(|c| c.session_id.clone()))
}
pub fn set_password(password: Zeroizing<Vec<u8>>) {
    CURRENT.with(|s| {
        if let Some(c) = s.borrow_mut().as_mut() {
            c.password = Some(password);
        }
    });
}
#[cfg(test)]
pub fn set_session_id(id: &str) {
    CURRENT.with(|s| {
        s.borrow_mut().as_mut().unwrap().session_id = Some(id.into());
    });
}

pub fn selected(path: &str, directory: bool) -> bool {
    CURRENT.with(|slot| {
        let slot = slot.borrow();
        let Some(list) = slot.as_ref().and_then(|c| c.selection.as_ref()) else {
            return true;
        };
        let path = path.trim_end_matches('/');
        list.iter().any(|name| {
            let name = name.trim_end_matches('/');
            path == name
                || path
                    .strip_prefix(name)
                    .is_some_and(|rest| rest.starts_with('/'))
                || (directory
                    && name
                        .strip_prefix(path)
                        .is_some_and(|rest| rest.starts_with('/')))
        })
    })
}

pub fn validate_selection(exists: impl Fn(&str) -> bool) -> Result<(), ArchiveError> {
    CURRENT.with(|slot| {
        if slot
            .borrow()
            .as_ref()
            .and_then(|c| c.selection.as_ref())
            .is_some_and(|paths| paths.iter().any(|path| !exists(path.trim_end_matches('/'))))
        {
            Err(ArchiveError::new(
                "invalid_selection",
                "A selected entry is no longer in the archive.",
            ))
        } else {
            Ok(())
        }
    })
}

pub fn phase(state: &str, entries: Option<&[ArchiveEntry]>) {
    if let Some(c) = control() {
        let mut p = c.progress.lock().unwrap();
        p.0.state = state.into();
        if !matches!(state, "completed" | "failed" | "cancelled") {
            p.0.processed_bytes = "0".into();
            p.0.total_bytes = entries.map(|list| {
                list.iter()
                    .map(|e| e.uncompressed_bytes)
                    .sum::<u64>()
                    .to_string()
            });
            p.0.total_files = entries.map(|list| {
                list.iter()
                    .filter(|e| e.kind == crate::archive::EntryKind::File)
                    .count()
            });
            p.0.completed_files = 0;
            p.0.current_file = None;
        }
        (c.emit)(p.0.clone());
        p.1 = Instant::now();
    }
}
pub fn file(path: &str) {
    if let Some(c) = control() {
        c.progress.lock().unwrap().0.current_file = Some(path.into());
    }
}
pub fn bytes(bytes: u64) -> Result<(), ArchiveError> {
    check()?;
    if let Some(c) = control() {
        let mut p = c.progress.lock().unwrap();
        p.0.processed_bytes = (p.0.processed_bytes.parse::<u64>().unwrap_or(0) + bytes).to_string();
        #[cfg(test)]
        if bytes > 0 && c.cancel_byte_phase.lock().unwrap().as_deref() == Some(p.0.state.as_str()) {
            c.cancelled.store(true, Ordering::Release);
            return Err(ArchiveError::new("cancelled", "Operation cancelled."));
        }
        if p.1.elapsed() >= Duration::from_millis(100) {
            (c.emit)(p.0.clone());
            p.1 = Instant::now();
        }
    }
    Ok(())
}
pub fn file_done() {
    if let Some(c) = control() {
        let mut p = c.progress.lock().unwrap();
        p.0.completed_files += 1;
        #[cfg(test)]
        if p.0.state == "committing" && c.cancel_after_commit.load(Ordering::Acquire) {
            c.cancelled.store(true, Ordering::Release);
        }
    }
}

#[cfg(test)]
pub fn test_context<T>(
    selection: Option<Vec<String>>,
    password: Option<String>,
    work: impl FnOnce(Arc<Control>) -> T,
) -> T {
    let c = new_control("test".into(), |_| {});
    CURRENT.with(|s| {
        *s.borrow_mut() = Some(Context {
            control: c.clone(),
            selection,
            password: password.map(|password| Zeroizing::new(password.into_bytes())),
            session_id: None,
            extract_here: false,
            protected_sources: Vec::new(),
        })
    });
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            CURRENT.with(|s| *s.borrow_mut() = None);
        }
    }
    let _reset = Reset;
    work(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn selection_boundaries_and_real_byte_progress() {
        test_context(
            Some(vec!["dir".into(), "other/file".into()]),
            None,
            |control| {
                assert!(selected("dir/a", false));
                assert!(selected("dir/sub/b", false));
                assert!(selected("other", true));
                assert!(!selected("directory/a", false));
                assert!(!selected("other/unselected", false));
                let mut input = &b"actual bytes"[..];
                let mut output = Vec::new();
                phase("running", None);
                file("dir/a");
                crate::archive::streaming::transfer(&mut input, 12, &mut [0; 4], &mut output)
                    .unwrap();
                assert_eq!(output, b"actual bytes");
                assert_eq!(control.progress.lock().unwrap().0.processed_bytes, "12");
            },
        );
    }

    #[test]
    fn cancellation_interrupts_stream_transfer() {
        test_context(None, None, |control| {
            struct Input(Arc<Control>);
            impl Read for Input {
                fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                    bytes.fill(0x42);
                    self.0.cancelled.store(true, Ordering::Release);
                    Ok(bytes.len())
                }
            }
            let result = crate::archive::streaming::transfer(
                &mut Input(control),
                1024,
                &mut [0; 32],
                &mut Vec::new(),
            );
            assert_eq!(result.unwrap_err().code, "cancelled");
        });
    }
}

#[cfg(test)]
mod event_tests {
    use super::*;
    #[test]
    fn emitted_lifecycle_preserves_operation_identity_and_actual_counters() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let control = new_control("correct-operation".into(), move |event| {
            sink.lock().unwrap().push(event)
        });
        let secret = "security-test-password-not-an-event";
        let mut request = Request::default();
        request.password = Some(secret.into());
        let result = run(request, control, || {
            assert!(has_password());
            phase("running", None);
            file("nested/real.txt");
            bytes(37)?;
            file_done();
            Ok(())
        });
        result.unwrap();
        let events = events.lock().unwrap();
        assert!(!serde_json::to_string(&*events).unwrap().contains(secret));
        assert!(!has_password());
        assert_eq!(
            events.iter().map(|e| e.state.as_str()).collect::<Vec<_>>(),
            ["preparing", "running", "completed"]
        );
        assert!(events.iter().all(|e| e.operation_id == "correct-operation"));
        let last = events.last().unwrap();
        assert_eq!(last.processed_bytes, "37");
        assert_eq!(last.completed_files, 1);
        assert_eq!(last.current_file.as_deref(), Some("nested/real.txt"));
        assert!(last.total_bytes.is_none());
    }
}
