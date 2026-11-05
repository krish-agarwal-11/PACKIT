// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use super::{streaming, ArchiveError, Result};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub(super) enum Set {
    Single,
    Seven { paths: Vec<PathBuf>, name: String },
    Rar { paths: Vec<PathBuf>, name: String },
}

fn error(message: &str) -> ArchiveError {
    ArchiveError::new("missing_volume", message)
}

pub(super) fn resolve(path: &Path) -> Result<Set> {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| error("Unsupported volume filename."))?;
    let lower = name.to_ascii_lowercase();
    let (prefix, digits, suffix, rar) = if let Some(index) = lower.rfind(".7z.") {
        (&name[..index + 4], &name[index + 4..], "", false)
    } else if lower.ends_with(".rar") {
        if let Some(index) = lower.rfind(".part") {
            (
                &name[..index + 5],
                &name[index + 5..name.len() - 4],
                ".rar",
                true,
            )
        } else {
            return Ok(Set::Single);
        }
    } else {
        return Ok(Set::Single);
    };
    if digits.is_empty() || digits.len() > 4 || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return Ok(Set::Single);
    }
    let width = digits.len();
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut volumes = BTreeMap::new();
    for entry in std::fs::read_dir(dir)
        .map_err(|e| ArchiveError::io("Could not discover archive volumes", e))?
    {
        crate::tasks::check()?;
        let entry = entry.map_err(streaming::damaged)?;
        let Some(candidate) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !candidate
            .get(..prefix.len())
            .is_some_and(|s| s.eq_ignore_ascii_case(prefix))
            || !candidate
                .get(candidate.len().saturating_sub(suffix.len())..)
                .is_some_and(|s| s.eq_ignore_ascii_case(suffix))
        {
            continue;
        }
        let end = candidate.len().saturating_sub(suffix.len());
        let Some(number) = candidate.get(prefix.len()..end) else {
            continue;
        };
        if number.is_empty() || number.len() > 4 || !number.bytes().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let variable_width = rar && !digits.starts_with('0');
        if (variable_width && number.starts_with('0')) || (!variable_width && number.len() != width)
        {
            continue;
        }
        let index = number
            .parse::<usize>()
            .map_err(|_| error("Invalid volume number."))?;
        let metadata = entry
            .path()
            .symlink_metadata()
            .map_err(streaming::damaged)?;
        #[cfg(windows)]
        let reparse = {
            use std::os::windows::fs::MetadataExt;
            metadata.file_attributes() & 0x400 != 0
        };
        #[cfg(not(windows))]
        let reparse = metadata.file_type().is_symlink();
        if !metadata.is_file()
            || reparse
            || index == 0
            || index > 128
            || volumes.insert(index, entry.path()).is_some()
        {
            return Err(error(
                "Volumes must be regular files with unique numbers from 1 to 128.",
            ));
        }
    }
    if volumes.is_empty() || volumes.keys().copied().ne(1..=volumes.len()) {
        return Err(error(
            "An archive volume is missing. Keep every numbered part together in the same folder.",
        ));
    }
    let paths: Vec<_> = volumes.into_values().collect();
    let mut total = 0_u64;
    for path in &paths {
        let len = path.metadata().map_err(streaming::damaged)?.len();
        if len == 0 || total.saturating_add(len) > 2 << 30 {
            return Err(streaming::limit(
                "Volume sets are limited to 2 GiB and cannot contain empty parts.",
            ));
        }
        total += len;
    }
    if rar {
        let mut first = [0; 8];
        streaming::open(&paths[0])?
            .read_exact(&mut first)
            .map_err(streaming::damaged)?;
        if !(first.starts_with(b"Rar!\x1a\x07\x00") || first.starts_with(b"Rar!\x1a\x07\x01\x00")) {
            return Err(error("The first volume is not a RAR archive."));
        }
        let base = &name[..name.len() - 4 - width - 5];
        Ok(Set::Rar {
            paths,
            name: format!("{base}.rar"),
        })
    } else {
        let base = name[..name.len() - width - 1].to_owned();
        Ok(Set::Seven { paths, name: base })
    }
}

pub(super) fn join_7z(paths: &[PathBuf]) -> Result<(File, u64)> {
    let mut output = tempfile::tempfile().map_err(streaming::damaged)?;
    let mut total = 0;
    let mut buffer = vec![0; streaming::BUFFER];
    for (index, path) in paths.iter().enumerate() {
        let mut input = streaming::open(path)?;
        if index == 0 {
            let mut signature = [0; 6];
            input
                .read_exact(&mut signature)
                .map_err(streaming::damaged)?;
            if &signature != b"7z\xbc\xaf\x27\x1c" {
                return Err(error("The first split 7z part has no 7z signature."));
            }
            use std::io::Seek;
            input.rewind().map_err(streaming::damaged)?;
        }
        loop {
            crate::tasks::check()?;
            let n = input.read(&mut buffer).map_err(streaming::damaged)?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > 2 << 30 {
                return Err(streaming::limit("Split 7z exceeds 2 GiB."));
            }
            output.write_all(&buffer[..n]).map_err(streaming::damaged)?;
            crate::tasks::bytes(n as u64)?;
        }
    }
    Ok((output, total))
}
