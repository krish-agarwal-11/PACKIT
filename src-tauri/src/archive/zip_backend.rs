// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use super::{
    directory, output, ArchiveBackend, ArchiveEntry, ArchiveError, ArchiveInspection, EntryIssue,
    EntryKind, FormatCapabilities, Integrity, IssueCode, OperationReport, OverwritePolicy, Result,
};
use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Read, Write},
    path::Path,
};
use zip::{read::ZipFile, result::ZipError, CompressionMethod, ZipArchive};

const GIB: u64 = 1024 * 1024 * 1024;
const MAX_FILE: u64 = 8 * GIB;
const MAX_TOTAL: u64 = 16 * GIB;
const MAX_RATIO: u64 = 1000;
const RATIO_THRESHOLD: u64 = 1024 * 1024;
const BUFFER_SIZE: usize = 64 * 1024;

pub(super) struct ZipBackend {
    reader: ZipArchive<directory::MetadataReader<BufReader<File>>>,
    name: String,
    archive_bytes: u64,
    directory_start: u64,
}

impl ZipBackend {
    pub fn open(path: &Path) -> Result<Self> {
        // Refuse special inputs before opening; devices and pipes can block.
        let metadata = std::fs::metadata(path)
            .map_err(|e| ArchiveError::io("Could not access the archive", e))?;
        if !metadata.is_file() {
            return Err(ArchiveError::new(
                "invalid_input",
                "Choose a regular ZIP file.",
            ));
        }
        let file =
            File::open(path).map_err(|e| ArchiveError::io("Could not open the archive", e))?;
        let metadata = file
            .metadata()
            .map_err(|e| ArchiveError::io("Could not read archive metadata", e))?;
        if !metadata.is_file() {
            return Err(ArchiveError::new(
                "invalid_input",
                "Choose a regular ZIP file.",
            ));
        }
        let archive_bytes = metadata.len();
        let mut input = BufReader::with_capacity(BUFFER_SIZE, file);
        let header = directory::preflight(&mut input, archive_bytes)?;
        let directory::Directory { start, count, .. } = header;
        let (bounded, payload_enabled) = directory::MetadataReader::new(input, header);
        let reader = ZipArchive::new(bounded).map_err(zip_error)?;
        payload_enabled.set(true);
        if reader.len() != count {
            return Err(ArchiveError::new(
                "path_conflict",
                "ZIP contains duplicate entry names or an inconsistent entry count.",
            ));
        }
        if reader.central_directory_start() != start || reader.offset() != 0 {
            return Err(ArchiveError::new(
                "invalid_zip",
                "ZIP directory offsets are inconsistent.",
            ));
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        Ok(Self {
            reader,
            name,
            archive_bytes,
            directory_start: start,
        })
    }

    fn run(
        &mut self,
        entries: &[ArchiveEntry],
        mut root: Option<&mut output::Output>,
        directories: usize,
    ) -> Result<OperationReport> {
        let mut total = 0;
        let mut files = 0;
        let mut buffer = vec![0; BUFFER_SIZE];
        crate::tasks::phase(
            if root.is_some() {
                "running"
            } else {
                "verifying"
            },
            Some(entries),
        );
        for (index, entry) in entries.iter().enumerate() {
            crate::tasks::check()?;
            crate::tasks::file(&entry.path);
            let mut file = crate::tasks::password(|password| match password {
                Some(password) => self.reader.by_index_decrypt(index, password),
                None => self.reader.by_index(index),
            })
            .map_err(zip_error)?;
            let expected_size = file.size();
            let expected_crc = if file.encrypted() {
                None
            } else {
                Some(file.crc32())
            };
            let written = if let Some(root) = root.as_deref_mut().filter(|_| {
                entry.kind == EntryKind::File && crate::tasks::selected(&entry.path, false)
            }) {
                let mut output = BufWriter::with_capacity(BUFFER_SIZE, root.new_file(&entry.path)?);
                let count = stream(
                    &mut file,
                    &mut output,
                    &mut buffer,
                    expected_size,
                    expected_crc,
                    &entry.path,
                )?;
                output
                    .flush()
                    .and_then(|_| output.get_ref().sync_all())
                    .map_err(|e| ArchiveError::io("Could not finish writing extracted file", e))?;
                count
            } else {
                stream(
                    &mut file,
                    &mut io::sink(),
                    &mut buffer,
                    expected_size,
                    expected_crc,
                    &entry.path,
                )?
            };
            if crate::tasks::selected(&entry.path, entry.kind == EntryKind::Directory) {
                total += written;
                files += usize::from(entry.kind == EntryKind::File);
            }
            if entry.kind == EntryKind::File {
                crate::tasks::file_done();
            }
        }
        Ok(OperationReport {
            files,
            directories,
            processed_bytes: total,
            output_path: None,
        })
    }
}

fn zip_error(error: ZipError) -> ArchiveError {
    match error {
        ZipError::InvalidPassword => {
            ArchiveError::new("incorrect_password", "The ZIP password is incorrect.")
        }
        ZipError::UnsupportedArchive(ZipError::PASSWORD_REQUIRED) => {
            ArchiveError::new("password_required", "Enter the ZIP archive password.")
        }
        ZipError::UnsupportedArchive(reason) => {
            ArchiveError::new("unsupported_zip", format!("Unsupported ZIP: {reason}."))
        }
        ZipError::Io(error)
            if !matches!(
                error.kind(),
                io::ErrorKind::UnexpectedEof | io::ErrorKind::InvalidData
            ) =>
        {
            ArchiveError::io("Could not read ZIP", error)
        }
        other => ArchiveError::new("invalid_zip", format!("Invalid or damaged ZIP: {other}.")),
    }
}

fn kind(file: &ZipFile<'_, impl Read>) -> EntryKind {
    match file.unix_mode().map_or(0, |mode| mode & 0o170000) {
        0o120000 => EntryKind::Link,
        0o040000 if file.is_dir() => EntryKind::Directory,
        0o100000 if !file.is_dir() => EntryKind::File,
        0 if file.is_dir() => EntryKind::Directory,
        0 => EntryKind::File,
        _ => EntryKind::Special,
    }
}

fn issue(code: IssueCode, message: impl Into<String>) -> EntryIssue {
    EntryIssue {
        code,
        message: message.into(),
    }
}

impl ArchiveBackend for ZipBackend {
    fn output_name(&self) -> String {
        output::output_folder(&self.name, &[".zip"])
    }
    fn inspect(&mut self) -> Result<ArchiveInspection> {
        let entries = self.list_entries()?;
        if entries.iter().any(|e| {
            e.issue
                .as_ref()
                .is_some_and(|i| i.code == IssueCode::Encrypted)
        }) {
            return Err(ArchiveError::new(
                "password_required",
                "Enter the ZIP archive password.",
            ));
        }
        if crate::tasks::has_password() {
            self.run(&entries, None, 0)?;
        }
        let total: u128 = entries
            .iter()
            .map(|entry| u128::from(entry.uncompressed_bytes))
            .sum();
        Ok(ArchiveInspection {
            name: self.name.clone(),
            output_folder: self.output_name(),
            archive_bytes: self.archive_bytes,
            total_uncompressed_bytes: total,
            entries,
            capabilities: self.capabilities(),
        })
    }

    fn list_entries(&mut self) -> Result<Vec<ArchiveEntry>> {
        let mut entries = Vec::with_capacity(self.reader.len());
        let mut ranges = Vec::with_capacity(self.reader.len());
        for index in 0..self.reader.len() {
            let file = self.reader.by_index_raw(index).map_err(zip_error)?;
            let name = file.name();
            let start = file.header_start();
            let end = file
                .data_start()
                .and_then(|data| data.checked_add(file.compressed_size()));
            let Some(end) = end.filter(|end| *end <= self.directory_start && *end > start) else {
                return Err(ArchiveError::new(
                    "invalid_zip",
                    format!("ZIP entry {name:?} points outside its data area."),
                ));
            };
            ranges.push((start, end));
            let kind = kind(&file);
            let compression = file.compression();
            let entry_issue = if matches!(kind, EntryKind::Link | EntryKind::Special)
                || (kind == EntryKind::Directory && file.size() != 0)
            {
                Some(issue(
                    IssueCode::UnsafeEntry,
                    "Links, special files, and folders with data are unsupported.",
                ))
            } else if file.encrypted() && !crate::tasks::has_password() {
                Some(issue(
                    IssueCode::Encrypted,
                    "Enter a password to read this encrypted ZIP entry.",
                ))
            } else if !matches!(
                compression,
                CompressionMethod::Stored | CompressionMethod::Deflated
            ) {
                Some(issue(
                    IssueCode::UnsupportedCompression,
                    format!("Unsupported ZIP compression: {compression:?}."),
                ))
            } else {
                output::path_parts(name, kind)
                    .err()
                    .map(|error| issue(IssueCode::UnsafePath, error.message))
            };
            entries.push(ArchiveEntry {
                path: name.to_owned(),
                kind,
                compressed_bytes: Some(file.compressed_size()),
                uncompressed_bytes: file.size(),
                method: format!("{compression:?}"),
                modified: file.last_modified().map(|date| date.to_string()),
                issue: entry_issue,
            });
        }
        ranges.sort_unstable();
        if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
            return Err(ArchiveError::new(
                "invalid_zip",
                "ZIP contains overlapping entry data.",
            ));
        }
        Ok(entries)
    }

    fn extract(&mut self, destination: &Path, policy: OverwritePolicy) -> Result<OperationReport> {
        let entries = self.list_entries()?;
        validate_limits(&entries, true)?;
        let mut plan = output::Plan::default();
        for entry in &entries {
            plan.add(&entry.path, entry.kind)?;
        }
        let name = self.output_name();
        let (name, mut report) = output::extract_to(destination, &name, policy, &plan, |root| {
            let directories = plan.create_directories(root)?;
            self.run(&entries, Some(root), directories)
        })?;
        report.output_path = Some(destination.join(name).to_string_lossy().into_owned());
        Ok(report)
    }

    fn test(&mut self) -> Result<OperationReport> {
        let entries = self.list_entries()?;
        validate_limits(&entries, false)?;
        let directories = entries
            .iter()
            .filter(|entry| entry.kind == EntryKind::Directory)
            .count();
        self.run(&entries, None, directories)
    }

    fn capabilities(&self) -> FormatCapabilities {
        FormatCapabilities {
            format: "ZIP",
            zip64: true,
            inspection: true,
            extraction: true,
            integrity: Integrity::Checksums,
            limitations: "Stored and Deflate; ZipCrypto and supported WinZip AES encryption. Split ZIPs and encrypted directory headers are unsupported.",
            compression_methods: &["Stored", "Deflate"],
            encryption: true,
            multipart: false,
        }
    }
}

fn validate_limits(entries: &[ArchiveEntry], extracting: bool) -> Result<()> {
    let mut total = 0_u64;
    for entry in entries {
        if let Some(issue) = &entry.issue {
            let code = match issue.code {
                IssueCode::UnsafePath if !extracting => None,
                IssueCode::UnsafePath => Some("unsafe_path"),
                IssueCode::UnsafeEntry => Some("unsafe_entry"),
                IssueCode::Encrypted => Some("password_required"),
                IssueCode::UnsupportedCompression => Some("unsupported_compression"),
            };
            if let Some(code) = code {
                return Err(ArchiveError::new(
                    code,
                    format!("{:?}: {}", entry.path, issue.message),
                ));
            }
        }
        let size = entry.uncompressed_bytes;
        let compressed = entry.compressed_bytes.unwrap_or(0);
        total = total.saturating_add(size);
        if size > MAX_FILE || total > MAX_TOTAL {
            return Err(ArchiveError::new(
                "archive_limit",
                "ZIP exceeds the 8 GiB per entry or 16 GiB total decompression limit.",
            ));
        }
        if (size > 0 && compressed == 0)
            || (size >= RATIO_THRESHOLD && size > compressed.saturating_mul(MAX_RATIO))
        {
            return Err(ArchiveError::new(
                "archive_limit",
                format!(
                    "ZIP entry {:?} declares a suspicious compression ratio.",
                    entry.path
                ),
            ));
        }
    }
    Ok(())
}

fn stream(
    input: &mut impl Read,
    output: &mut impl Write,
    buffer: &mut [u8],
    size: u64,
    crc: Option<u32>,
    path: &str,
) -> Result<u64> {
    let mut count = 0;
    let mut checksum = crc32fast::Hasher::new();
    loop {
        crate::tasks::check()?;
        let read = input.read(buffer).map_err(|error| {
            if error.raw_os_error().is_some()
                || matches!(
                    error.kind(),
                    io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound
                )
            {
                ArchiveError::io("Could not read ZIP entry", error)
            } else {
                ArchiveError::new(
                    if crate::tasks::has_password() {
                        "incorrect_password"
                    } else {
                        "corrupt_entry"
                    },
                    format!("Could not decompress or verify {path:?}: {error}."),
                )
            }
        })?;
        if read == 0 {
            break;
        }
        count += read as u64;
        if count > size {
            return Err(ArchiveError::new(
                "corrupt_entry",
                format!("ZIP entry {path:?} expanded beyond its declared size."),
            ));
        }
        crate::tasks::bytes(read as u64)?;
        checksum.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .map_err(|error| ArchiveError::io("Could not write extracted file", error))?;
    }
    if count != size || crc.is_some_and(|crc| checksum.finalize() != crc) {
        return Err(ArchiveError::new(
            if crate::tasks::has_password() {
                "incorrect_password"
            } else {
                "corrupt_entry"
            },
            format!("ZIP entry {path:?} failed its size or CRC check."),
        ));
    }
    Ok(count)
}
