// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use super::{
    output, streaming as common, ArchiveBackend, ArchiveEntry, ArchiveError, ArchiveInspection,
    EntryKind, FormatCapabilities, Integrity, OperationReport, OverwritePolicy, Result,
};
use std::{
    fs::File,
    io::{BufReader, Read, Seek},
    path::Path,
};

pub(super) const SIGNATURE: &[u8] = b"Rar!\x1a\x07\x00";

pub(super) struct RarBackend {
    file: File,
    name: String,
    length: u64,
}

impl RarBackend {
    pub fn new(file: File, name: String, length: u64) -> Self {
        Self { file, name, length }
    }

    fn scan(
        &mut self,
        verify: bool,
        expected: Option<&[ArchiveEntry]>,
        mut out: Option<&mut output::Output>,
    ) -> Result<(Vec<ArchiveEntry>, output::Plan)> {
        crate::tasks::phase(
            if out.is_some() {
                "running"
            } else {
                "verifying"
            },
            expected,
        );
        self.file.rewind().map_err(common::damaged)?;
        let mut reader = BufReader::with_capacity(common::BUFFER, &mut self.file);
        let mut signature = [0; 7];
        reader.read_exact(&mut signature).map_err(common::damaged)?;
        if signature != SIGNATURE {
            return Err(corrupt("signature changed"));
        }

        let mut header = Vec::new();
        let (kind, flags) = read_header(&mut reader, &mut header)?;
        if kind != 0x73 || header.len() != 6 {
            return Err(unsupported("nonstandard main header"));
        }
        if flags & 0x80 != 0 {
            return Err(encrypted());
        }
        if flags != 0 {
            return Err(unsupported(
                "archive flags (including solid, multipart or recovery records)",
            ));
        }

        let mut entries = Vec::new();
        let mut plan = output::Plan::default();
        let mut total = 0;
        let mut buffer = vec![0; common::BUFFER];
        loop {
            crate::tasks::check()?;
            let (kind, flags) = read_header(&mut reader, &mut header)?;
            if kind == 0x7b {
                if flags & !0x4000 != 0 || !header.is_empty() {
                    return Err(unsupported("extended or multipart end record"));
                }
                if reader.read(&mut [0]).map_err(common::damaged)? != 0 {
                    return Err(unsupported("trailing bytes"));
                }
                break;
            }
            if kind != 0x74 {
                return Err(unsupported("additional or service records"));
            }
            if flags & 4 != 0 {
                return Err(encrypted());
            }
            if flags & !0x80e0 != 0 || flags & 0x8000 == 0 {
                return Err(unsupported(
                    "extended file flags (including Unicode-name records, large or split entries)",
                ));
            }
            if header.len() < 25 {
                return Err(corrupt("short file header"));
            }
            let packed = u64::from(word(&header, 0));
            let size = u64::from(word(&header, 4));
            let os = header[8];
            let crc = word(&header, 9);
            let method = header[18];
            let name_len = u16::from_le_bytes([header[19], header[20]]) as usize;
            let attributes = word(&header, 21);
            if method != 0x30 {
                return Err(ArchiveError::new("unsupported_compression", format!("RAR4 compression method 0x{method:02x} is unsupported. Only stored (uncompressed) entries are enabled.")));
            }
            if packed != size || name_len == 0 || name_len != header.len() - 25 {
                return Err(corrupt("inconsistent stored size or filename length"));
            }
            let path = std::str::from_utf8(&header[25..])
                .map_err(|_| unsupported("non-UTF-8 names"))?
                .replace('\\', "/");
            let directory = flags & 0xe0 == 0xe0;
            let normal = match os {
                0..=2 => attributes & 0x440 == 0 && (attributes & 0x10 == 0 || directory),
                3 => match attributes & 0o170000 {
                    0 => true,
                    0o040000 => directory,
                    0o100000 => !directory,
                    _ => false,
                },
                _ => false,
            };
            if !normal || (directory && size != 0) {
                return Err(ArchiveError::new(
                    "unsafe_entry",
                    format!("RAR entry {path:?} has unsafe or unsupported file attributes."),
                ));
            }
            let entry = ArchiveEntry {
                path,
                kind: if directory {
                    EntryKind::Directory
                } else {
                    EntryKind::File
                },
                compressed_bytes: Some(packed),
                uncompressed_bytes: size,
                method: "Stored".into(),
                modified: None,
                issue: None,
            };
            common::add(&mut plan, &entry, entries.len(), &mut total)?;
            if expected.is_some_and(|list| list.get(entries.len()) != Some(&entry)) {
                return Err(ArchiveError::new(
                    "archive_changed",
                    "RAR entries changed after validation.",
                ));
            }
            let pos = reader.stream_position().map_err(common::damaged)?;
            if pos.checked_add(size).is_none_or(|end| end > self.length) {
                return Err(corrupt("truncated payload"));
            }
            if verify {
                if common::entry_data(&mut reader, &entry, out.as_deref_mut(), &mut buffer)? != crc
                {
                    return Err(corrupt(&format!(
                        "payload CRC mismatch for {:?}",
                        entry.path
                    )));
                }
            } else {
                reader
                    .seek(std::io::SeekFrom::Start(pos + size))
                    .map_err(common::damaged)?;
            }
            entries.push(entry);
        }
        if expected.is_some_and(|list| list.len() != entries.len()) {
            return Err(ArchiveError::new(
                "archive_changed",
                "RAR entries changed after validation.",
            ));
        }
        Ok((entries, plan))
    }
}

fn word(bytes: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap())
}
fn corrupt(reason: &str) -> ArchiveError {
    ArchiveError::new("corrupt_entry", format!("Invalid RAR4 archive: {reason}."))
}
fn unsupported(reason: &str) -> ArchiveError {
    ArchiveError::new("unsupported_variant", format!("Unsupported RAR4 variant: {reason}. Only ordinary single-volume stored RAR4 is supported."))
}
fn encrypted() -> ArchiveError {
    ArchiveError::new(
        "unsupported_encryption",
        "Encrypted RAR archives require a secure callback backend and are not enabled.",
    )
}

fn read_header(input: &mut impl Read, body: &mut Vec<u8>) -> Result<(u8, u16)> {
    let mut fixed = [0; 7];
    input.read_exact(&mut fixed).map_err(common::damaged)?;
    let length = u16::from_le_bytes([fixed[5], fixed[6]]) as usize;
    if !(7..=1056).contains(&length) {
        return Err(unsupported("invalid or oversized header"));
    }
    body.resize(length - 7, 0);
    input.read_exact(body).map_err(common::damaged)?;
    let mut crc = crc32fast::Hasher::new();
    crc.update(&fixed[2..]);
    crc.update(body);
    if crc.finalize() as u16 != u16::from_le_bytes([fixed[0], fixed[1]]) {
        return Err(corrupt("header CRC mismatch"));
    }
    Ok((fixed[2], u16::from_le_bytes([fixed[3], fixed[4]])))
}

impl ArchiveBackend for RarBackend {
    fn output_name(&self) -> String {
        output::output_folder(&self.name, &[".rar"])
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
        Ok(self.scan(false, None, None)?.0)
    }
    fn extract(&mut self, destination: &Path, policy: OverwritePolicy) -> Result<OperationReport> {
        let (entries, plan) = self.scan(false, None, None)?;
        let (name, count) =
            output::extract_to(destination, &self.output_name(), policy, &plan, |out| {
                let count = plan.create_directories(out)?;
                self.scan(true, Some(&entries), Some(out))?;
                Ok(count)
            })?;
        Ok(common::report(
            &entries,
            count,
            Some(destination.join(name).to_string_lossy().into_owned()),
        ))
    }
    fn test(&mut self) -> Result<OperationReport> {
        let (entries, _) = self.scan(true, None, None)?;
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
        FormatCapabilities { format: "RAR4 (Stored)", zip64: false, inspection: true, extraction: true,
            integrity: Integrity::Checksums, compression_methods: &["Stored"], encryption: false, multipart: false,
            limitations: "Stored RAR4 only. RAR5, compressed entries, links, solid archives and extended filename records are unsupported." }
    }
}
