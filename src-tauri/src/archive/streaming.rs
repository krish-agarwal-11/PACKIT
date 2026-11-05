// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use super::{
    output, ArchiveEntry, ArchiveError, ArchiveInspection, EntryKind, FormatCapabilities,
    OperationReport, Result,
};
use std::{
    fs::File,
    io::{self, BufWriter, Read, Write},
    path::Path,
};

pub(super) const MAX_FILE: u64 = 8 << 30;
pub(super) const MAX_TOTAL: u64 = 16 << 30;
pub(super) const MAX_ENTRIES: usize = 20_000;
pub(super) const BUFFER: usize = 64 << 10;

pub(super) fn open(path: &Path) -> Result<File> {
    let mut options = File::options();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Pin input against writes/deletion; inspect reparse points themselves.
        options.custom_flags(0x00200000).share_mode(1);
    }
    let file = options
        .open(path)
        .map_err(|e| ArchiveError::io("Could not safely open archive", e))?;
    let metadata = file
        .metadata()
        .map_err(|e| ArchiveError::io("Could not read archive metadata", e))?;
    #[cfg(windows)]
    let reparse = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let reparse = false;
    if !metadata.is_file() || reparse {
        return Err(ArchiveError::new(
            "invalid_input",
            "Choose a regular archive file without a reparse point.",
        ));
    }
    Ok(file)
}

pub(super) fn limit(message: &str) -> ArchiveError {
    ArchiveError::new("archive_limit", message)
}
pub(super) fn damaged(error: io::Error) -> ArchiveError {
    if error.raw_os_error().is_some() {
        ArchiveError::io("Could not read archive", error)
    } else {
        ArchiveError::new(
            "corrupt_entry",
            format!("Archive data is truncated or invalid: {error}"),
        )
    }
}

pub(super) fn add(
    plan: &mut output::Plan,
    entry: &ArchiveEntry,
    count: usize,
    total: &mut u64,
) -> Result<()> {
    if count >= MAX_ENTRIES
        || entry.uncompressed_bytes > MAX_FILE
        || total.saturating_add(entry.uncompressed_bytes) > MAX_TOTAL
    {
        return Err(limit(
            "Archive exceeds 20,000 entries, 8 GiB per entry or 16 GiB total.",
        ));
    }
    *total += entry.uncompressed_bytes;
    plan.add(&entry.path, entry.kind)
}

pub(super) fn entry_data(
    reader: &mut dyn Read,
    entry: &ArchiveEntry,
    output: Option<&mut output::Output>,
    buffer: &mut [u8],
) -> Result<u32> {
    crate::tasks::file(&entry.path);
    crate::tasks::check()?;
    let mut out = match output
        .filter(|_| entry.kind == EntryKind::File && crate::tasks::selected(&entry.path, false))
    {
        Some(out) => Some(BufWriter::with_capacity(BUFFER, out.new_file(&entry.path)?)),
        None => None,
    };
    let mut sink = io::sink();
    let crc = transfer(
        reader,
        entry.uncompressed_bytes,
        buffer,
        match &mut out {
            Some(file) => file,
            None => &mut sink,
        },
    )?;
    if let Some(mut out) = out {
        out.flush()
            .and_then(|_| out.get_ref().sync_all())
            .map_err(|e| ArchiveError::io("Could not finish extracted file", e))?;
    }
    if entry.kind == EntryKind::File {
        crate::tasks::file_done();
    }
    Ok(crc)
}

pub(crate) fn transfer(
    reader: &mut dyn Read,
    mut remaining: u64,
    buffer: &mut [u8],
    out: &mut dyn Write,
) -> Result<u32> {
    let mut crc = crc32fast::Hasher::new();
    while remaining != 0 {
        crate::tasks::check()?;
        let amount = remaining.min(buffer.len() as u64) as usize;
        reader.read_exact(&mut buffer[..amount]).map_err(damaged)?;
        crc.update(&buffer[..amount]);
        out.write_all(&buffer[..amount])
            .map_err(|e| ArchiveError::io("Could not write extracted file", e))?;
        remaining -= amount as u64;
        crate::tasks::bytes(amount as u64)?;
    }
    Ok(crc.finalize())
}

pub(super) fn inspection(
    name: &str,
    folder: String,
    bytes: u64,
    entries: Vec<ArchiveEntry>,
    capabilities: FormatCapabilities,
) -> ArchiveInspection {
    ArchiveInspection {
        name: name.into(),
        output_folder: folder,
        archive_bytes: bytes,
        total_uncompressed_bytes: entries
            .iter()
            .map(|e| u128::from(e.uncompressed_bytes))
            .sum(),
        entries,
        capabilities,
    }
}

pub(super) fn report(
    entries: &[ArchiveEntry],
    directories: usize,
    output_path: Option<String>,
) -> OperationReport {
    OperationReport {
        files: entries
            .iter()
            .filter(|e| e.kind == EntryKind::File && crate::tasks::selected(&e.path, false))
            .count(),
        directories,
        processed_bytes: entries
            .iter()
            .filter(|e| crate::tasks::selected(&e.path, e.kind == EntryKind::Directory))
            .map(|e| e.uncompressed_bytes)
            .sum(),
        output_path,
    }
}
