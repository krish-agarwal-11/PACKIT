// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod archive;
#[cfg(all(windows, target_arch = "x86_64"))]
mod installer;
mod instance;
mod naming;
mod platform;
mod sessions;
mod settings;
mod shell;
mod tasks;
mod updates;

use archive::{ArchiveError, ArchiveInspection, OperationReport, OverwritePolicy};
use settings::{AppError, Settings};
use std::path::Path;
use tauri::Manager;

#[derive(serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum LegalDocument {
    License,
    Notices,
}
#[tauri::command]
fn legal_document(document: LegalDocument) -> &'static str {
    match document {
        LegalDocument::License => include_str!("../../LICENSE"),
        LegalDocument::Notices => include_str!("../../THIRD_PARTY_NOTICES.md"),
    }
}

#[tauri::command]
async fn load_settings(app: tauri::AppHandle) -> Result<Settings, AppError> {
    let path = app.path().app_config_dir().map_err(AppError::from_error)?;
    tauri::async_runtime::spawn_blocking(move || settings::load(&path))
        .await
        .map_err(AppError::from_error)?
}

#[tauri::command]
async fn save_settings(app: tauri::AppHandle, settings: Settings) -> Result<Settings, AppError> {
    let path = app.path().app_config_dir().map_err(AppError::from_error)?;
    tauri::async_runtime::spawn_blocking(move || settings::save(&path, settings))
        .await
        .map_err(AppError::from_error)?
}

#[tauri::command]
async fn inspect_archive(
    app: tauri::AppHandle,
    path: String,
    request: tasks::Request,
) -> Result<ArchiveInspection, ArchiveError> {
    let control = tasks::reserve(&request, app)?;
    tauri::async_runtime::spawn_blocking(move || {
        tasks::run(request, control, || {
            sessions::with_archive(Path::new(&path), || archive::inspect(Path::new(&path)))
        })
    })
    .await
    .map_err(ArchiveError::worker)?
}

#[tauri::command]
async fn test_archive(
    app: tauri::AppHandle,
    path: String,
    request: tasks::Request,
) -> Result<OperationReport, ArchiveError> {
    let control = tasks::reserve(&request, app)?;
    tauri::async_runtime::spawn_blocking(move || {
        tasks::run(request, control, || {
            sessions::with_archive(Path::new(&path), || archive::test(Path::new(&path)))
        })
    })
    .await
    .map_err(ArchiveError::worker)?
}

#[tauri::command]
async fn extraction_destination_exists(
    path: String,
    destination: String,
    here: Option<bool>,
) -> Result<bool, ArchiveError> {
    tauri::async_runtime::spawn_blocking(move || {
        if here.unwrap_or(false) {
            archive::here_conflict(Path::new(&path), Path::new(&destination))
        } else {
            archive::destination_exists(Path::new(&path), Path::new(&destination))
        }
    })
    .await
    .map_err(ArchiveError::worker)?
}

#[tauri::command]
async fn extract_archive(
    app: tauri::AppHandle,
    request: tasks::Request,
    path: String,
    destination: String,
    policy: OverwritePolicy,
) -> Result<OperationReport, ArchiveError> {
    let control = tasks::reserve(&request, app)?;
    tauri::async_runtime::spawn_blocking(move || {
        tasks::run(request, control, || {
            sessions::with_archive(Path::new(&path), || {
                archive::extract(Path::new(&path), Path::new(&destination), policy)
            })
        })
    })
    .await
    .map_err(ArchiveError::worker)?
}

#[tauri::command]
fn cancel_archive_operation(operation_id: String) -> bool {
    tasks::cancel(&operation_id)
}

#[tauri::command]
async fn create_archive(
    app: tauri::AppHandle,
    request: tasks::Request,
    options: archive::creation::Options,
) -> Result<OperationReport, ArchiveError> {
    if request.selection.is_some()
        || request.password.is_some()
        || request.destination_mode.is_some()
    {
        return Err(ArchiveError::new(
            "invalid_request",
            "Creation does not accept extraction selections or passwords.",
        ));
    }
    let control = tasks::reserve(&request, app)?;
    tauri::async_runtime::spawn_blocking(move || {
        tasks::run(request, control, || archive::creation::create(options))
    })
    .await
    .map_err(ArchiveError::worker)?
}
#[tauri::command]
async fn creation_destination_exists(
    destination: String,
    filename: String,
    format: archive::creation::Format,
) -> Result<bool, ArchiveError> {
    tauri::async_runtime::spawn_blocking(move || {
        archive::creation::exists(Path::new(&destination), &filename, format)
    })
    .await
    .map_err(ArchiveError::worker)?
}

#[tauri::command]
fn close_archive_session(session_id: String) {
    sessions::close(&session_id);
}

#[tauri::command]
async fn source_parent(path: String) -> Result<String, ArchiveError> {
    tauri::async_runtime::spawn_blocking(move || {
        let parent = Path::new(&path).parent().ok_or_else(|| {
            ArchiveError::new("invalid_shell_request", "Source has no parent directory.")
        })?;
        let resolved = std::fs::canonicalize(parent).map_err(ArchiveError::worker)?;
        Ok(resolved.to_string_lossy().into_owned())
    })
    .await
    .map_err(ArchiveError::worker)?
}

#[tauri::command]
fn next_main_request() -> Option<instance::Request> {
    instance::next()
}
#[tauri::command]
fn acknowledge_main_request(id: String) {
    instance::acknowledge(&id);
}

#[tauri::command]
fn shell_launch() -> Option<shell::Launch> {
    shell::LAUNCH.lock().unwrap().clone()
}
#[tauri::command]
fn normalize_creation_name(
    filename: String,
    format: archive::creation::Format,
) -> Result<String, ArchiveError> {
    archive::creation::filename(&filename, format)
}
#[tauri::command]
async fn suggest_creation_name(
    path: String,
    format: archive::creation::Format,
) -> Result<String, ArchiveError> {
    tauri::async_runtime::spawn_blocking(move || {
        archive::creation::suggest_name(Path::new(&path), format)
    })
    .await
    .map_err(ArchiveError::worker)?
}

fn startup_error(message: &str) {
    #[cfg(windows)]
    unsafe {
        let text: Vec<_> = message.encode_utf16().chain(Some(0)).collect();
        windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
            None,
            windows::core::PCWSTR(text.as_ptr()),
            windows::core::w!("PACKIT"),
            windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
        );
    }
    #[cfg(not(windows))]
    eprintln!("{message}");
}
fn position_popup(window: &tauri::WebviewWindow) {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::{
            Graphics::Gdi::{
                GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
            },
            UI::WindowsAndMessaging::GetForegroundWindow,
        };
        let monitor = MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            if let Ok(size) = window.outer_size() {
                let rect = info.rcWork;
                let x = rect.left + ((rect.right - rect.left - size.width as i32) / 2).max(0);
                let y = rect.top + ((rect.bottom - rect.top - size.height as i32) / 2).max(0);
                let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
                return;
            }
        }
    }
    let _ = window.center();
}

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    #[cfg(all(windows, target_arch = "x86_64"))]
    if args.get(1).is_some_and(|s| {
        s == "--installer-check"
            || s == "--installer-record-shell"
            || s == "--installer-clean-shell"
    }) {
        let result = if args[1] == "--installer-check" {
            installer::check(&args)
        } else {
            args.get(2)
                .ok_or_else(|| "Missing installation directory".to_owned())
                .and_then(|root| {
                    if args[1] == "--installer-record-shell" {
                        installer::record(Path::new(root))
                    } else {
                        installer::clean(Path::new(root))
                    }
                })
        };
        if let Err(error) = &result {
            eprintln!("PACKIT installation preflight: {error}");
        }
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }
    #[cfg(all(debug_assertions, windows, target_arch = "x86_64"))]
    if args.get(1).is_some_and(|arg| arg == "--instance-probe") {
        let result = instance::probe(&args);
        if let Err(error) = &result {
            eprintln!("{error:?}");
        }
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }
    #[cfg(windows)]
    if args.get(1).is_some_and(|arg| arg == "--shell-server") {
        let action = args
            .get(2)
            .and_then(|s| s.to_str())
            .and_then(shell::Action::parse);
        let result = action
            .ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_INVALIDARG))
            .and_then(|action| shell::windows::serve(action, args.iter().any(|s| s == "--dev")));
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }
    #[cfg(all(windows, target_arch = "x86_64"))]
    if args.get(1).is_some_and(|s| {
        s == "--shell-register" || s == "--shell-unregister" || s == "--shell-repair-iso"
    }) {
        let dev = args.iter().any(|s| s == "--dev");
        let result = std::env::current_exe()
            .map_err(ArchiveError::worker)
            .and_then(|path| {
                if args[1] == "--shell-register" {
                    shell::registry::register(&path, dev)
                } else if args[1] == "--shell-repair-iso" {
                    shell::registry::repair_iso(&path, dev)
                } else {
                    shell::registry::unregister(&path, dev)
                }
            });
        if let Err(error) = &result {
            eprintln!("PACKIT shell registration: {error:?}");
        }
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }
    let mut launch = match shell::from_args(&args) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("PACKIT launch: {error:?}");
            std::process::exit(1);
        }
    };
    let popup = launch
        .as_ref()
        .is_some_and(|l| l.action != shell::Action::Open);
    #[cfg(all(windows, target_arch = "x86_64"))]
    let _main_guard = if !popup {
        match instance::elect(instance::request(launch.take())) {
            Ok(Some(guard)) => Some(guard),
            Ok(None) => return,
            Err(error) => {
                startup_error(&format!("PACKIT could not route this request: {error:?}"));
                return;
            }
        }
    } else {
        None
    };
    *shell::LAUNCH.lock().unwrap() = launch;
    tracing_subscriber::fmt().json().with_target(false).init();
    tracing::info!(platform = platform::NAME, "PACKIT starting");
    if let Err(error) = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_updater::Builder::new()
                .pubkey(include_str!("../updater-public-key.pub").trim())
                .build(),
        )
        .manage(updates::UpdateState::default())
        .setup(move |app| {
            #[cfg(all(windows, target_arch = "x86_64"))]
            if let Ok(path) = app.path().resource_dir() {
                archive::seven_zip::set_resource_dir(path.join("vendor").join("7zip"));
            }
            let initial = shell::LAUNCH.lock().unwrap().clone();
            let quick = initial.as_ref().is_some_and(|l| {
                matches!(
                    l.action,
                    shell::Action::ExtractHere
                        | shell::Action::ExtractTo
                        | shell::Action::Zip
                        | shell::Action::SevenZip
                )
            });
            let label = if popup { "operation" } else { "main" };
            let popup_title = initial
                .as_ref()
                .and_then(|launch| launch.paths.first())
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .map(|name| format!("PACKIT · {name}"))
                .unwrap_or_else(|| "PACKIT · Archive operation".into());
            let url = tauri::WebviewUrl::App(
                if popup {
                    "index.html?operation=1"
                } else {
                    "index.html"
                }
                .into(),
            );
            let window = tauri::WebviewWindowBuilder::new(app, label, url)
                .title(if popup { &popup_title } else { "PACKIT" })
                .inner_size(
                    if popup { 660.0 } else { 1120.0 },
                    if popup { 680.0 } else { 760.0 },
                )
                .min_inner_size(
                    if popup { 480.0 } else { 760.0 },
                    if popup { 350.0 } else { 540.0 },
                )
                .maximizable(!popup)
                .theme(Some(tauri::Theme::Dark))
                .visible(false)
                .focused(!popup || !quick)
                .icon(tauri::include_image!("icons/icon.png"))?
                .build()?;
            if popup {
                position_popup(&window);
            } else {
                let _ = window.center();
                instance::attach(app.handle().clone());
            }
            let closing = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let close_window = window.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    if tasks::cancel_active() {
                        api.prevent_close();
                        if !closing.swap(true, std::sync::atomic::Ordering::AcqRel) {
                            let window = close_window.clone();
                            std::thread::spawn(move || {
                                while tasks::is_active() {
                                    std::thread::sleep(std::time::Duration::from_millis(50));
                                }
                                let _ = window.close();
                            });
                        }
                    }
                }
            });
            window.show()?;
            if !popup || !quick {
                let _ = window.set_focus();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            updates::check_updates,
            updates::install_update,
            updates::open_update_fallback,
            legal_document,
            load_settings,
            save_settings,
            inspect_archive,
            test_archive,
            extraction_destination_exists,
            extract_archive,
            cancel_archive_operation,
            close_archive_session,
            create_archive,
            creation_destination_exists,
            shell_launch,
            next_main_request,
            acknowledge_main_request,
            source_parent,
            normalize_creation_name,
            suggest_creation_name
        ])
        .run(tauri::generate_context!())
    {
        tracing::error!(%error, "Desktop runtime failed");
        std::process::exit(1);
    }
}
