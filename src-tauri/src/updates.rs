// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
//! Official signature verification behind main-window, private-feed and operation gates.
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, State};
use tauri_plugin_updater::{Update, UpdaterExt};
const RELEASES: &str = "https://github.com/krish-agarwal-11/PACKIT/releases";
const LIMIT: usize = 256 * 1024 * 1024;
#[derive(Default)]
pub struct UpdateState {
    running: AtomicBool,
    pending: Mutex<Option<Arc<Update>>>,
}
struct Action<'a>(&'a AtomicBool);
impl Drop for Action<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
fn begin(state: &UpdateState) -> Result<Action<'_>, String> {
    if state.running.swap(true, Ordering::AcqRel) {
        Err("An update action is already running.".into())
    } else {
        Ok(Action(&state.running))
    }
}
fn main_only(window: &tauri::Window) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("Updates are available only in the main PACKIT window.".into())
    }
}
fn enabled() -> bool {
    serde_json::from_str::<serde_json::Value>(include_str!("../update-policy.json"))
        .ok()
        .is_some_and(|p| p["feedReady"] == true)
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    state: &'static str,
    version: Option<String>,
    message: String,
}
#[tauri::command]
pub async fn check_updates(
    window: tauri::Window,
    app: tauri::AppHandle,
    state: State<'_, UpdateState>,
) -> Result<Reply, String> {
    main_only(&window)?;
    let _action = begin(&state)?;
    if !enabled() {
        return Ok(Reply{state:"disabled",version:None,message:"Updates are not enabled yet: the repository is private and an accessible signed release feed has not been validated.".into()});
    }
    let update=app.updater_builder().timeout(Duration::from_secs(15)).build().map_err(|_|"Could not initialize secure updates.")?.check().await.map_err(|_|"Could not check for updates. You may be offline, or a release is unavailable. Try again later or use the manual installer.")?;
    match update {
        Some(update) => {
            if update.download_url.scheme() != "https"
                || !update
                    .download_url
                    .as_str()
                    .starts_with("https://github.com/krish-agarwal-11/PACKIT/releases/download/")
                || update.signature.trim().is_empty()
            {
                return Err("The release metadata is invalid; no update will be installed.".into());
            }
            let version = update.version.clone();
            *state.pending.lock().unwrap() = Some(Arc::new(update));
            Ok(Reply {
                state: "available",
                version: Some(version),
                message:
                    "A signed update is available. Finish archive operations before installing."
                        .into(),
            })
        }
        None => {
            *state.pending.lock().unwrap() = None;
            Ok(Reply {
                state: "current",
                version: None,
                message: "You’re using the latest available PACKIT version.".into(),
            })
        }
    }
}
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    stage: &'static str,
    bytes: u64,
    total: Option<u64>,
}
#[tauri::command]
pub async fn install_update(
    window: tauri::Window,
    app: tauri::AppHandle,
    state: State<'_, UpdateState>,
) -> Result<(), String> {
    main_only(&window)?;
    if !enabled() {
        return Err("No accessible signed update feed has been enabled.".into());
    }
    let _action = begin(&state)?;
    let update = state
        .pending
        .lock()
        .unwrap()
        .clone()
        .ok_or("Check for an available update first.")?;
    if crate::tasks::is_active() {
        return Err("Finish archive operations before updating.".into());
    }
    let client = reqwest::Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| "Could not initialize secure download.")?;
    let mut response = client
        .get(update.download_url.clone())
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|_| "Update download failed. Use Retry or the manual installer.")?;
    let total = response.content_length();
    if total.is_some_and(|n| n > LIMIT as u64) {
        return Err("The update exceeds the 256 MiB download limit.".into());
    }
    let mut bytes = Vec::new();
    let mut tick = Instant::now() - Duration::from_secs(1);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Update download was interrupted; nothing was installed.")?
    {
        if bytes.len().saturating_add(chunk.len()) > LIMIT {
            return Err("The update exceeds the download limit.".into());
        }
        bytes.extend_from_slice(&chunk);
        if tick.elapsed() > Duration::from_millis(100) {
            let _ = app.emit(
                "updater-progress",
                Progress {
                    stage: "Downloading",
                    bytes: bytes.len() as u64,
                    total,
                },
            );
            tick = Instant::now();
        }
    }
    let _ = app.emit(
        "updater-progress",
        Progress {
            stage: "Verifying signature",
            bytes: bytes.len() as u64,
            total,
        },
    );
    update
        .verify(&bytes)
        .map_err(|_| "Update signature or signed version is invalid. Nothing was installed.")?;
    let _guard = crate::tasks::reserve_update()
        .map_err(|_| "An archive operation started; finish it before retrying the update.")?;
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        let executable =
            std::env::current_exe().map_err(|_| "Could not validate the installation path.")?;
        crate::installer::check(&[
            "packit".into(),
            "--installer-check".into(),
            executable
                .parent()
                .ok_or("Missing installation path")?
                .as_os_str()
                .to_owned(),
        ])
        .map_err(|_| "Close other PACKIT windows and operation dialogs before updating.")?;
    }
    let _ = app.emit(
        "updater-progress",
        Progress {
            stage: "Starting installer",
            bytes: bytes.len() as u64,
            total,
        },
    );
    // Official plugin verifies again, starts the standard NSIS installer and exits.
    update.install(&bytes).map_err(|_|"The signed installer could not be started. Nothing was force-closed; use the manual installer.".to_owned())
}
#[tauri::command]
pub fn open_update_fallback(window: tauri::Window) -> Result<(), String> {
    main_only(&window)?;
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
        let url: Vec<u16> = RELEASES.encode_utf16().chain(Some(0)).collect();
        let verb: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
        let result = unsafe {
            ShellExecuteW(
                None,
                PCWSTR(verb.as_ptr()),
                PCWSTR(url.as_ptr()),
                None,
                None,
                SW_SHOWNORMAL,
            )
        };
        if result.0 as isize <= 32 {
            return Err("Open github.com/krish-agarwal-11/PACKIT/releases in your browser.".into());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_feed_is_disabled_without_network() {
        assert!(!enabled());
    }
    #[test]
    fn concurrent_update_actions_are_rejected() {
        let state = UpdateState::default();
        let first = begin(&state).unwrap();
        assert!(begin(&state).is_err());
        drop(first);
        assert!(begin(&state).is_ok());
    }
    #[test]
    fn configured_public_key_is_the_public_file() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(
            config["plugins"]["updater"]["pubkey"].as_str().unwrap(),
            include_str!("../updater-public-key.pub").trim()
        );
        assert_eq!(config["plugins"]["updater"]["requireSignedVersion"], true);
    }
    #[test]
    fn official_plugin_rejects_invalid_signature_without_network_or_install() {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        context.config_mut().plugins = serde_json::from_value(config["plugins"].clone()).unwrap();
        let app = tauri::test::mock_builder()
            .plugin(
                tauri_plugin_updater::Builder::new()
                    .pubkey(include_str!("../updater-public-key.pub").trim())
                    .build(),
            )
            .build(context)
            .unwrap();
        let updater = app.updater_builder().build().unwrap();
        assert!(updater.restore_update(serde_json::json!({"version":"99.0.0","url":"https://github.com/krish-agarwal-11/PACKIT/releases/download/v99.0.0/PACKIT.exe"})).is_err());
        let update=updater.restore_update(serde_json::json!({"version":"99.0.0","url":"https://github.com/krish-agarwal-11/PACKIT/releases/download/v99.0.0/PACKIT.exe","signature":"invalid"})).unwrap().unwrap();
        assert!(update.verify(b"invalid unsigned installer").is_err());
        assert!(update.install(b"invalid unsigned installer").is_err());
    }
}
