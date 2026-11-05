// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
//! Typed shell requests. Selected paths are data, never executable command text.
use crate::archive::ArchiveError;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Mutex};
#[cfg(all(windows, target_arch = "x86_64"))]
pub mod registry;
#[cfg(windows)]
pub mod windows;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Open,
    ExtractFiles,
    ExtractHere,
    ExtractTo,
    Add,
    Zip,
    SevenZip,
}
impl Action {
    pub const ALL: [Self; 7] = [
        Self::Open,
        Self::ExtractFiles,
        Self::ExtractHere,
        Self::ExtractTo,
        Self::Add,
        Self::Zip,
        Self::SevenZip,
    ];
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "open" => Some(Self::Open),
            "extract_files" => Some(Self::ExtractFiles),
            "extract_here" => Some(Self::ExtractHere),
            "extract_to" => Some(Self::ExtractTo),
            "add" => Some(Self::Add),
            "zip" => Some(Self::Zip),
            "seven_zip" => Some(Self::SevenZip),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::ExtractFiles => "extract_files",
            Self::ExtractHere => "extract_here",
            Self::ExtractTo => "extract_to",
            Self::Add => "add",
            Self::Zip => "zip",
            Self::SevenZip => "seven_zip",
        }
    }
    pub fn creation(self) -> bool {
        matches!(self, Self::Add | Self::Zip | Self::SevenZip)
    }
}
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct Launch {
    pub action: Action,
    pub paths: Vec<PathBuf>,
}
impl Launch {
    pub fn validate(&self) -> Result<(), ArchiveError> {
        if self.paths.is_empty()
            || self.paths.len() > 20_000
            || (!self.action.creation() && self.paths.len() > 8)
            || self
                .paths
                .iter()
                .map(|p| p.as_os_str().len().saturating_mul(3).saturating_add(64))
                .sum::<usize>()
                > 4 * 1024 * 1024
            || self.paths.iter().any(|p| {
                !p.is_absolute()
                    || cfg!(windows) && !p.to_str().is_some_and(crate::naming::safe_shell_path)
                    || p.to_str()
                        .is_none_or(|s| s.len() > 32760 || s.chars().any(char::is_control))
            })
        {
            return Err(ArchiveError::new(
                "invalid_shell_request",
                "Shell selection contains invalid or excessive paths (up to 8 archives or 20,000 creation sources within 4 MiB).",
            ));
        }
        Ok(())
    }
}
pub static LAUNCH: Mutex<Option<Launch>> = Mutex::new(None);

pub fn from_args(args: &[std::ffi::OsString]) -> Result<Option<Launch>, ArchiveError> {
    if args.len() < 2 {
        return Ok(None);
    }
    let launch = if args[1] == "--shell-stdin" {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::io::stdin()
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(ArchiveError::worker)?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(ArchiveError::new(
                "invalid_shell_request",
                "Shell request exceeds 4 MiB.",
            ));
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| ArchiveError::new("invalid_shell_request", "Invalid shell request."))?
    } else if args[1] == "--shell" {
        if args.len() < 5 || args[3] != "--" {
            return Err(ArchiveError::new(
                "invalid_shell_request",
                "Expected --shell ACTION -- PATH...",
            ));
        }
        Launch {
            action: args[2].to_str().and_then(Action::parse).ok_or_else(|| {
                ArchiveError::new("invalid_shell_request", "Unknown shell action.")
            })?,
            paths: args[4..].iter().map(PathBuf::from).collect(),
        }
    } else if !args[1].to_string_lossy().starts_with('-') {
        Launch {
            action: Action::Open,
            paths: args[1..].iter().map(PathBuf::from).collect(),
        }
    } else {
        return Err(ArchiveError::new(
            "invalid_shell_request",
            "Unknown PACKIT argument.",
        ));
    };
    launch.validate()?;
    Ok(Some(launch))
}

#[cfg(windows)]
pub fn dispatch(launch: &Launch) -> Result<(), ArchiveError> {
    use std::{
        io::Write,
        os::windows::process::CommandExt,
        process::{Command, Stdio},
    };
    launch.validate()?;
    let send = |request: &Launch| -> Result<(), ArchiveError> {
        let data = serde_json::to_vec(request).map_err(ArchiveError::worker)?;
        if data.len() > 4 * 1024 * 1024 {
            return Err(ArchiveError::new(
                "invalid_shell_request",
                "Selection exceeds the 4 MiB handoff limit.",
            ));
        }
        let mut child = Command::new(std::env::current_exe().map_err(ArchiveError::worker)?)
            .arg("--shell-stdin")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x08000000)
            .spawn()
            .map_err(ArchiveError::worker)?;
        let result = child.stdin.take().unwrap().write_all(&data);
        if let Err(error) = result {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ArchiveError::worker(error));
        }
        // No paths or secrets are placed in a command line/environment/temp file.
        Ok(())
    };
    if launch.action.creation() || launch.action == Action::Open {
        send(launch)
    } else {
        for path in &launch.paths {
            send(&Launch {
                action: launch.action,
                paths: vec![path.clone()],
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arguments_preserve_unicode_spaces_order_and_never_interpret_shell_text() {
        let root = tempfile::tempdir().unwrap();
        let paths = vec![
            root.path().join("café & 日本.txt"),
            root.path().join("-strange (2).zip"),
        ];
        let mut args = vec![
            "packit.exe".into(),
            "--shell".into(),
            "add".into(),
            "--".into(),
        ];
        args.extend(paths.iter().map(|p| p.as_os_str().to_owned()));
        let request = from_args(&args).unwrap().unwrap();
        assert_eq!(request.paths, paths);
        assert_eq!(request.action, Action::Add);
        assert!(from_args(&[
            "p".into(),
            "--shell".into(),
            "execute".into(),
            "--".into(),
            paths[0].as_os_str().into()
        ])
        .is_err());
        for action in Action::ALL {
            assert_eq!(Action::parse(action.name()), Some(action));
        }
    }
}
