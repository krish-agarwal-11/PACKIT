// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use super::{
    output, pax, streaming as common, ArchiveBackend, ArchiveEntry, ArchiveError,
    ArchiveInspection, EntryKind, FormatCapabilities, Integrity, OperationReport, OverwritePolicy,
    Result,
};
use std::{
    fs::File,
    io::{BufReader, Read, Seek},
    path::Path,
};

const BLOCK: usize = 512;
const MAX_TAR_BYTES: u64 = common::MAX_TOTAL + (64 << 20);

pub(super) fn looks_like_tar(header: &[u8]) -> bool {
    header.len() >= BLOCK
        && (header[..BLOCK].iter().all(|b| *b == 0)
            || &header[257..262] == b"ustar"
            || valid_checksum(&header[..BLOCK]))
}

fn valid_checksum(block: &[u8]) -> bool {
    let header = tar::Header::from_byte_slice(block);
    header.cksum().is_ok_and(|expected| {
        expected
            == block
                .iter()
                .enumerate()
                .map(|(i, b)| u32::from(if (148..156).contains(&i) { b' ' } else { *b }))
                .sum::<u32>()
    })
}

pub(super) struct TarBackend {
    file: File,
    name: String,
    length: u64,
    gzip: bool,
}

impl TarBackend {
    pub fn new(file: File, name: String, length: u64, gzip: bool) -> Self {
        Self {
            file,
            name,
            length,
            gzip,
        }
    }

    fn scan(
        &mut self,
        expected: Option<&[ArchiveEntry]>,
        output: Option<&mut output::Output>,
    ) -> Result<(Vec<ArchiveEntry>, output::Plan)> {
        crate::tasks::phase(
            if output.is_some() {
                "running"
            } else {
                "verifying"
            },
            expected,
        );
        self.file.rewind().map_err(common::damaged)?;
        let input = BufReader::with_capacity(common::BUFFER, &mut self.file);
        let limit = if self.gzip {
            MAX_TAR_BYTES.min(self.length.saturating_mul(1000).max(1 << 20))
        } else {
            MAX_TAR_BYTES
        };
        if self.gzip {
            scan(
                flate2::bufread::MultiGzDecoder::new(input),
                limit,
                expected,
                output,
            )
        } else {
            scan(input, limit, expected, output)
        }
    }
}

fn scan(
    reader: impl Read,
    limit: u64,
    expected: Option<&[ArchiveEntry]>,
    mut output: Option<&mut output::Output>,
) -> Result<(Vec<ArchiveEntry>, output::Plan)> {
    let mut input = reader.take(limit + 1);
    let mut plan = output::Plan::default();
    let mut entries = Vec::new();
    let mut total = 0;
    let mut headers = 0;
    let mut global = pax::Fields::new();
    let mut local = pax::Fields::new();
    let mut local_pending = false;
    let mut metadata_bytes = 0_u64;
    let mut metadata_records = 0;
    let mut buffer = vec![0; common::BUFFER];
    let mut block = [0; BLOCK];
    let budget = || common::limit("TAR stream exceeds the decoded size or expansion limit.");
    loop {
        crate::tasks::check()?;
        input.read_exact(&mut block).map_err(common::damaged)?;
        if input.limit() == 0 {
            return Err(budget());
        }
        if block.iter().all(|b| *b == 0) {
            if local_pending {
                return Err(ArchiveError::new(
                    "invalid_pax",
                    "PAX local header has no following entry.",
                ));
            }
            input.read_exact(&mut block).map_err(common::damaged)?;
            if block.iter().any(|b| *b != 0) {
                return Err(ArchiveError::new(
                    "corrupt_entry",
                    "TAR requires two zero end blocks.",
                ));
            }
            // Drain the gzip footer (and any concatenated gzip members).
            loop {
                crate::tasks::check()?;
                let n = input.read(&mut buffer).map_err(common::damaged)?;
                if input.limit() == 0 {
                    return Err(budget());
                }
                if n == 0 {
                    break;
                }
                if buffer[..n].iter().any(|b| *b != 0) {
                    return Err(ArchiveError::new(
                        "unsupported_variant",
                        "Nonzero data after the TAR terminator is unsupported.",
                    ));
                }
            }
            break;
        }
        headers += 1;
        if headers > common::MAX_ENTRIES {
            return Err(common::limit("TAR exceeds 20,000 headers."));
        }
        if !valid_checksum(&block) {
            return Err(ArchiveError::new(
                "corrupt_entry",
                "TAR header checksum mismatch.",
            ));
        }
        let header = tar::Header::from_byte_slice(&block);
        let raw_size = header.entry_size().map_err(common::damaged)?;
        let entry_type = header.entry_type().as_byte();
        if matches!(entry_type, b'x' | b'g') {
            metadata_bytes = metadata_bytes.saturating_add(raw_size);
            if raw_size > pax::MAX_HEADER || metadata_bytes > pax::MAX_METADATA {
                return Err(common::limit(
                    "PAX metadata exceeds the 64 KiB per-header or 4 MiB archive limit.",
                ));
            }
            let padding = (512 - raw_size % 512) % 512;
            if raw_size + padding >= input.limit() {
                return Err(budget());
            }
            let mut bytes = vec![0; raw_size as usize];
            input.read_exact(&mut bytes).map_err(common::damaged)?;
            let fields = pax::parse(&bytes)?;
            metadata_records += fields.len();
            if metadata_records > pax::MAX_RECORDS {
                return Err(common::limit("PAX metadata exceeds 8,192 records."));
            }
            input
                .read_exact(&mut block[..padding as usize])
                .map_err(common::damaged)?;
            if block[..padding as usize].iter().any(|b| *b != 0) {
                return Err(ArchiveError::new("invalid_pax", "Nonzero PAX padding."));
            }
            if entry_type == b'g' {
                if local_pending {
                    return Err(ArchiveError::new(
                        "invalid_pax",
                        "Global PAX metadata interrupts a local header.",
                    ));
                }
                for (key, value) in fields {
                    if value.is_empty() {
                        global.remove(&key);
                    } else {
                        global.insert(key, value);
                    }
                }
            } else {
                for (key, value) in fields {
                    if local.insert(key, value).is_some() {
                        return Err(ArchiveError::new(
                            "invalid_pax",
                            "Conflicting consecutive local PAX records.",
                        ));
                    }
                }
                local_pending = true;
            }
            continue;
        }
        let name = header.path_bytes();
        let resolved = if let Some(path) = pax::effective(&local, &global, "path") {
            path.to_owned()
        } else {
            std::str::from_utf8(&name)
                .map_err(|_| {
                    ArchiveError::new("unsupported_variant", "TAR entry names must be UTF-8.")
                })?
                .to_owned()
        };
        let mut path = resolved.as_str();
        let size = pax::effective(&local, &global, "size")
            .map(|value| value.parse::<u64>())
            .transpose()
            .map_err(|_| ArchiveError::new("invalid_pax", "Invalid PAX size."))?
            .unwrap_or(raw_size);
        let modified =
            pax::effective(&local, &global, "mtime").map(|time| format!("{time} (Unix seconds)"));
        local.clear();
        local_pending = false;
        let kind = match header.entry_type().as_byte() {
            b'0' | 0 if !path.ends_with('/') => EntryKind::File,
            b'5' | b'0' | 0 if size == 0 => EntryKind::Directory,
            b'1' | b'2' | b'3' | b'4' | b'6' => return Err(ArchiveError::new("unsafe_entry", format!("TAR links and special files are unsupported: {path:?}."))),
            _ => return Err(ArchiveError::new("unsupported_variant", format!("Unsupported TAR entry type at {path:?} (including GNU extensions and sparse files)."))),
        };
        // A conventional leading ./ is relative to the archive root. Do not
        // normalize any internal dots, separators, or parent components.
        path = path.strip_prefix("./").unwrap_or(path);
        if kind == EntryKind::Directory && matches!(path, "" | ".") {
            continue;
        }
        let entry = ArchiveEntry {
            path: path.into(),
            kind,
            compressed_bytes: None,
            uncompressed_bytes: size,
            method: String::new(),
            modified,
            issue: None,
        };
        common::add(&mut plan, &entry, entries.len(), &mut total)?;
        if expected.is_some_and(|list| list.get(entries.len()) != Some(&entry)) {
            return Err(ArchiveError::new(
                "archive_changed",
                "TAR entries changed after validation.",
            ));
        }
        let padding = (BLOCK as u64 - size % BLOCK as u64) % BLOCK as u64;
        if size + padding >= input.limit() {
            return Err(budget());
        }
        common::entry_data(&mut input, &entry, output.as_deref_mut(), &mut buffer)?;
        input
            .read_exact(&mut block[..padding as usize])
            .map_err(common::damaged)?;
        entries.push(entry);
    }
    if expected.is_some_and(|list| list.len() != entries.len()) {
        return Err(ArchiveError::new(
            "archive_changed",
            "TAR entries changed after validation.",
        ));
    }
    Ok((entries, plan))
}

impl ArchiveBackend for TarBackend {
    fn output_name(&self) -> String {
        output::output_folder(
            &self.name,
            if self.gzip {
                &[".tar.gz", ".tgz", ".gz"]
            } else {
                &[".tar"]
            },
        )
    }
    fn inspect(&mut self) -> Result<ArchiveInspection> {
        let entries = self.list_entries()?;
        Ok(common::inspection(
            &self.name,
            self.output_name(),
            self.length,
            entries,
            self.capabilities(),
        ))
    }
    fn list_entries(&mut self) -> Result<Vec<ArchiveEntry>> {
        Ok(self.scan(None, None)?.0)
    }
    fn extract(&mut self, destination: &Path, policy: OverwritePolicy) -> Result<OperationReport> {
        let (entries, plan) = self.scan(None, None)?;
        let (name, count) =
            output::extract_to(destination, &self.output_name(), policy, &plan, |out| {
                let count = plan.create_directories(out)?;
                self.scan(Some(&entries), Some(out))?;
                Ok(count)
            })?;
        Ok(common::report(
            &entries,
            count,
            Some(destination.join(name).to_string_lossy().into_owned()),
        ))
    }
    fn test(&mut self) -> Result<OperationReport> {
        let entries = self.list_entries()?;
        Ok(common::report(
            &entries,
            entries
                .iter()
                .filter(|e| e.kind == EntryKind::Directory)
                .count(),
            None,
        ))
    }
    fn capabilities(&self) -> FormatCapabilities {
        FormatCapabilities {
            format: if self.gzip { "TAR.GZ" } else { "TAR" }, zip64: false,
            inspection: true, extraction: true,
            integrity: if self.gzip { Integrity::Checksums } else { Integrity::Structure },
            compression_methods: if self.gzip { &["Gzip/Deflate"] } else { &["Stored"] },
            encryption: false, multipart: false,
            limitations: "UTF-8 regular files and directories only; bounded POSIX PAX metadata supported; no GNU extension records, sparse files or links.",
        }
    }
}
