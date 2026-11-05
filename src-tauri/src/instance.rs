// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
//! Per-user/session main-window election and bounded acknowledged IPC queue.
use crate::{
    archive::ArchiveError,
    shell::{Action, Launch},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, VecDeque},
    sync::{Mutex, OnceLock},
};
#[cfg(all(windows, target_arch = "x86_64"))]
mod windows;
#[cfg(all(windows, target_arch = "x86_64"))]
pub use windows::elect;
#[cfg(all(debug_assertions, windows, target_arch = "x86_64"))]
pub use windows::probe;
#[derive(Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub launch: Option<Launch>,
}
#[derive(Default)]
struct Queue {
    pending: VecDeque<Request>,
    seen: BTreeSet<String>,
}
static QUEUE: OnceLock<Mutex<Queue>> = OnceLock::new();
fn queue() -> &'static Mutex<Queue> {
    QUEUE.get_or_init(|| Mutex::new(Queue::default()))
}
static APP: OnceLock<tauri::AppHandle> = OnceLock::new();
pub fn attach(app: tauri::AppHandle) {
    let _ = APP.set(app);
}
fn focus() {
    if let Some(app) = APP.get() {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            use tauri::Manager;
            if let Some(window) = handle.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        });
    }
}
pub fn enqueue(request: Request) -> Result<(), ArchiveError> {
    if request.id.is_empty() || request.id.len() > 128 {
        return Err(ArchiveError::new(
            "invalid_request",
            "Invalid main-window request.",
        ));
    }
    if let Some(launch) = &request.launch {
        launch.validate()?;
        if launch.action != Action::Open {
            return Err(ArchiveError::new(
                "invalid_request",
                "Operation dialogs cannot be routed into the main window.",
            ));
        }
    }
    let mut q = queue().lock().unwrap();
    if q.seen.contains(&request.id) {
        return Ok(());
    }
    let requests: Vec<Request> = if let Some(launch) = &request.launch {
        launch
            .paths
            .iter()
            .enumerate()
            .map(|(index, path)| Request {
                id: format!("{}-{index}", request.id),
                launch: Some(Launch {
                    action: Action::Open,
                    paths: vec![path.clone()],
                }),
            })
            .collect()
    } else {
        vec![request.clone()]
    };
    if q.pending.len() + requests.len() > 128 {
        return Err(ArchiveError::new("operation_busy","The main window has 128 pending requests. Try again after it finishes opening archives."));
    }
    q.seen.insert(request.id.clone());
    if q.seen.len() > 1024 {
        q.seen = q
            .pending
            .iter()
            .map(|r| r.id.clone())
            .chain(Some(request.id.clone()))
            .collect();
    }
    q.pending.extend(requests);
    drop(q);
    focus();
    Ok(())
}
pub fn next() -> Option<Request> {
    queue().lock().unwrap().pending.front().cloned()
}
pub fn acknowledge(id: &str) {
    let mut q = queue().lock().unwrap();
    if q.pending.front().is_some_and(|r| r.id == id) {
        q.pending.pop_front();
    }
}
pub fn request(launch: Option<Launch>) -> Request {
    Request {
        id: format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ),
        launch,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queue_retains_order_deduplicates_retries_and_requires_ack() {
        let first = request(None);
        let second = request(None);
        enqueue(first.clone()).unwrap();
        enqueue(first.clone()).unwrap();
        enqueue(second.clone()).unwrap();
        assert_eq!(next().unwrap().id, first.id);
        acknowledge(&second.id);
        assert_eq!(next().unwrap().id, first.id);
        acknowledge(&first.id);
        assert_eq!(next().unwrap().id, second.id);
        acknowledge(&second.id);
        assert!(next().is_none());
    }
}

#[cfg(all(test, windows, target_arch = "x86_64"))]
mod process_tests {
    #[test]
    #[ignore = "Requires built PACKIT_SHELL_TEST_EXE; no windows or Explorer registry entries are created"]
    fn concurrent_main_election_and_acknowledged_handoffs() {
        use std::{
            io::{BufRead, BufReader},
            os::windows::process::CommandExt,
            process::{Command, Stdio},
        };
        let exe = std::env::var_os("PACKIT_SHELL_TEST_EXE").expect("Set PACKIT_SHELL_TEST_EXE");
        let root = tempfile::tempdir().unwrap();
        let report = root.path().join("received.json");
        let token = format!(
            "PackitProbe{}{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let make = |id: &str| {
            let mut cmd = Command::new(&exe);
            cmd.arg("--instance-probe")
                .arg(&token)
                .arg(&report)
                .arg("17")
                .arg(id)
                .creation_flags(0x08000000)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            cmd
        };
        let mut first = make("primary").spawn().unwrap();
        let mut line = String::new();
        BufReader::new(first.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert_eq!(line.trim(), "PRIMARY");
        let children: Vec<_> = (0..16)
            .map(|n| make(&format!("client{n}")).spawn().unwrap())
            .collect();
        for child in children {
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "FORWARDED");
        }
        let output = first.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let received: Vec<String> =
            serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
        assert_eq!(received.len(), 17);
        assert_eq!(
            received
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            17
        );
    }
}
