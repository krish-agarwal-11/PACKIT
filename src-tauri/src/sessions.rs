// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use crate::{archive::ArchiveError, tasks};
use cap_fs_ext::MetadataExt;
use std::{
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::SystemTime,
};
use zeroize::Zeroizing;

#[derive(PartialEq, Eq)]
struct Identity {
    path: PathBuf,
    device: u64,
    inode: u64,
    length: u64,
    modified: SystemTime,
}

fn identity(path: &Path) -> Result<Identity, ArchiveError> {
    let path = std::fs::canonicalize(path).map_err(|_| {
        ArchiveError::new(
            "session_changed",
            "The opened archive is no longer accessible.",
        )
    })?;
    let file = std::fs::File::open(&path).map_err(|_| {
        ArchiveError::new(
            "session_changed",
            "The opened archive is no longer accessible.",
        )
    })?;
    let metadata = cap_std::fs::File::from_std(file)
        .metadata()
        .map_err(|_| ArchiveError::new("session_changed", "Could not verify archive identity."))?;
    Ok(Identity {
        path,
        device: metadata.dev(),
        inode: metadata.ino(),
        length: metadata.len(),
        modified: metadata
            .modified()
            .map_err(|_| {
                ArchiveError::new(
                    "session_changed",
                    "Could not verify archive modification time.",
                )
            })?
            .into_std(),
    })
}

struct Session {
    id: String,
    identity: Identity,
    password: Option<Zeroizing<Vec<u8>>>,
}
static SESSION: OnceLock<Mutex<Option<Session>>> = OnceLock::new();
fn store() -> &'static Mutex<Option<Session>> {
    SESSION.get_or_init(|| Mutex::new(None))
}

pub fn close(id: &str) {
    let mut session = store().lock().unwrap();
    if session.as_ref().is_some_and(|current| current.id == id) {
        *session = None;
    }
}

/// Called inside the reserved operation's thread-local context. Secrets never
/// travel back to the frontend and are only reused for this file identity.
pub fn with_archive<T>(
    path: &Path,
    work: impl FnOnce() -> Result<T, ArchiveError>,
) -> Result<T, ArchiveError> {
    let Some(id) = tasks::session_id() else {
        return work();
    };
    let current_identity = match identity(path) {
        Ok(identity) => identity,
        Err(error) => {
            close(&id);
            return Err(error);
        }
    };
    {
        let mut slot = store().lock().unwrap();
        if !slot
            .as_ref()
            .is_some_and(|s| s.id == id && s.identity == current_identity)
        {
            *slot = Some(Session {
                id: id.clone(),
                identity: current_identity,
                password: None,
            });
        }
        if !tasks::has_password() {
            if let Some(secret) = slot.as_ref().and_then(|s| s.password.as_ref()) {
                tasks::set_password(Zeroizing::new(secret.to_vec()));
            }
        }
    }
    let result = work();
    let mut slot = store().lock().unwrap();
    if let Some(session) = slot.as_mut().filter(|s| s.id == id) {
        match &result {
            Ok(_) => {
                // Recheck identity after validation; never keep credentials
                // learned while a different file was being substituted.
                if identity(path).is_ok_and(|identity| identity == session.identity) {
                    if let Some(secret) =
                        tasks::password(|password| password.map(|p| Zeroizing::new(p.to_vec())))
                    {
                        session.password = Some(secret);
                    }
                } else {
                    session.password = None;
                }
            }
            Err(error)
                if matches!(
                    error.code,
                    "incorrect_password" | "password_required" | "archive_changed"
                ) =>
            {
                session.password = None;
            }
            _ => {}
        }
    }
    result
}
