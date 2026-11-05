// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
#[cfg(all(windows, target_arch = "x86_64"))]
mod containment;
pub(crate) mod creation;
mod directory;
mod iso_backend;
mod output;
mod pax;
mod rar_backend;
#[cfg(all(windows, target_arch = "x86_64"))]
pub(crate) mod seven_zip;
pub(crate) mod streaming;
mod tar_backend;
mod udf_backend;
mod volumes;
mod zip_backend;

use serde::{Deserialize, Serialize, Serializer};
use std::{io, path::Path};

#[derive(Debug, Serialize)]
pub struct ArchiveError {
    pub(crate) code: &'static str,
    message: String,
}

impl ArchiveError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn io(action: &str, error: io::Error) -> Self {
        let code = match error.kind() {
            io::ErrorKind::AlreadyExists => "destination_exists",
            io::ErrorKind::PermissionDenied => "permission_denied",
            io::ErrorKind::StorageFull => "disk_full",
            io::ErrorKind::NotFound => "not_found",
            _ => "io_error",
        };
        Self::new(code, format!("{action}: {error}"))
    }

    pub fn worker(error: impl std::fmt::Display) -> Self {
        Self::new(
            "worker_error",
            format!("The archive worker stopped unexpectedly: {error}"),
        )
    }
}

type Result<T> = std::result::Result<T, ArchiveError>;

// Preserve exact byte counts across the JavaScript number boundary.
fn decimal<T: std::fmt::Display, S: Serializer>(
    value: &T,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EntryKind {
    File,
    Directory,
    Link,
    Special,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum IssueCode {
    Encrypted,
    UnsupportedCompression,
    UnsafeEntry,
    UnsafePath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct EntryIssue {
    code: IssueCode,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveEntry {
    path: String,
    pub(crate) kind: EntryKind,
    #[serde(serialize_with = "optional_decimal")]
    compressed_bytes: Option<u64>,
    #[serde(serialize_with = "decimal")]
    pub(crate) uncompressed_bytes: u64,
    method: String,
    modified: Option<String>,
    issue: Option<EntryIssue>,
}

fn optional_decimal<S: Serializer>(
    value: &Option<u64>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    match value {
        Some(value) => decimal(value, serializer),
        None => serializer.serialize_none(),
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Integrity {
    Checksums,
    Structure,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatCapabilities {
    format: &'static str,
    zip64: bool,
    inspection: bool,
    extraction: bool,
    integrity: Integrity,
    limitations: &'static str,
    compression_methods: &'static [&'static str],
    encryption: bool,
    multipart: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInspection {
    name: String,
    output_folder: String,
    #[serde(serialize_with = "decimal")]
    archive_bytes: u64,
    #[serde(serialize_with = "decimal")]
    total_uncompressed_bytes: u128,
    entries: Vec<ArchiveEntry>,
    capabilities: FormatCapabilities,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationReport {
    files: usize,
    directories: usize,
    #[serde(serialize_with = "decimal")]
    processed_bytes: u64,
    output_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverwritePolicy {
    Refuse,
    Replace,
    KeepBoth,
}

/// A backend represents one opened archive. Future backends can supply their own
/// format probe, credentials, and volume handling without changing these operations.
trait ArchiveBackend {
    fn output_name(&self) -> String;
    fn inspect(&mut self) -> Result<ArchiveInspection>;
    fn list_entries(&mut self) -> Result<Vec<ArchiveEntry>>;
    fn extract(&mut self, destination: &Path, policy: OverwritePolicy) -> Result<OperationReport>;
    fn test(&mut self) -> Result<OperationReport>;
    fn verify_created(&mut self) -> Result<OperationReport> {
        self.test()
    }
    fn capabilities(&self) -> FormatCapabilities;
}

fn with_archive<T>(
    path: &Path,
    operation: impl FnOnce(&mut dyn ArchiveBackend) -> Result<T>,
) -> Result<T> {
    let result = open_backend(path).and_then(|mut archive| operation(archive.as_mut()));
    if let Err(error) = &result {
        tracing::warn!(code = error.code, "Archive operation failed");
    }
    result
}

fn open_backend(path: &Path) -> Result<Box<dyn ArchiveBackend>> {
    match volumes::resolve(path)? {
        volumes::Set::Seven { paths, name } => {
            let (file, size) = volumes::join_7z(&paths)?;
            return native_archive(file, name, size, "7z");
        }
        volumes::Set::Rar { paths, name } => {
            #[cfg(all(windows, target_arch = "x86_64"))]
            {
                return Ok(Box::new(seven_zip::SevenZip::open_volumes(paths, name)?));
            }
            #[cfg(not(all(windows, target_arch = "x86_64")))]
            {
                let _ = (paths, name);
                return Err(ArchiveError::new(
                    "unsupported_format",
                    "Multipart RAR requires the Windows x64 backend.",
                ));
            }
        }
        volumes::Set::Single => {}
    }
    use std::io::{Read, Seek};
    let mut file = streaming::open(path)?;
    let length = file.metadata().map_err(streaming::damaged)?.len();
    let mut header = Vec::with_capacity(512);
    (&mut file)
        .take(512)
        .read_to_end(&mut header)
        .map_err(streaming::damaged)?;
    file.rewind().map_err(streaming::damaged)?;
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    if iso_backend::probe(&mut file)? {
        if udf_backend::probe(&mut file)? {
            return Ok(Box::new(udf_backend::UdfBackend::new(file, name, length)));
        }
        return Ok(Box::new(iso_backend::IsoBackend::new(file, name, length)));
    }
    if header.starts_with(b"7z\xbc\xaf\x27\x1c") {
        return native_archive(file, name, length, "7z");
    }
    if header.starts_with(b"Rar!\x1a\x07\x01\x00") {
        return native_archive(file, name, length, "Rar5");
    }
    if header.starts_with(rar_backend::SIGNATURE) {
        let mut stored = rar_backend::RarBackend::new(
            file.try_clone().map_err(streaming::damaged)?,
            name.clone(),
            length,
        );
        match stored.list_entries() {
            Ok(_) => return Ok(Box::new(stored)),
            Err(error)
                if matches!(
                    error.code,
                    "unsupported_compression" | "unsupported_variant"
                ) =>
            {
                return native_archive(file, name, length, "Rar")
            }
            Err(error) => return Err(error),
        }
    }
    if header.starts_with(b"\x1f\x8b") {
        let mut decoded = Vec::with_capacity(512);
        flate2::read::MultiGzDecoder::new(&mut file)
            .take(512)
            .read_to_end(&mut decoded)
            .map_err(streaming::damaged)?;
        if !tar_backend::looks_like_tar(&decoded) {
            return Err(ArchiveError::new("unsupported_format", "This gzip stream does not contain a recognized TAR header. Standalone gzip is unsupported."));
        }
        file.rewind().map_err(streaming::damaged)?;
        return Ok(Box::new(tar_backend::TarBackend::new(
            file, name, length, true,
        )));
    }
    let zip_signature = [b"PK\x03\x04", b"PK\x05\x06", b"PK\x07\x08"]
        .iter()
        .any(|magic| header.starts_with(*magic));
    if !zip_signature && tar_backend::looks_like_tar(&header) {
        return Ok(Box::new(tar_backend::TarBackend::new(
            file, name, length, false,
        )));
    }
    // Retain the ZIP reader's structure probe, including ZIPs with a different
    // filename extension or without a local entry header (empty ZIPs).
    match zip_backend::ZipBackend::open(path) {
        Ok(zip) => Ok(Box::new(zip)),
        Err(error)
            if !zip_signature
                && error.code == "invalid_zip"
                && !name.to_ascii_lowercase().ends_with(".zip") =>
        {
            Err(ArchiveError::new(
                "unsupported_format",
                "Unrecognized archive. Open ZIP, 7z, RAR, TAR, TAR.GZ or a supported ISO image.",
            ))
        }
        Err(error) => Err(error),
    }
}

fn native_archive(
    file: std::fs::File,
    name: String,
    length: u64,
    kind: &'static str,
) -> Result<Box<dyn ArchiveBackend>> {
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        Ok(Box::new(seven_zip::SevenZip::open(
            file, name, length, kind,
        )?))
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        let _ = (file, name, length, kind);
        Err(ArchiveError::new(
            "unsupported_format",
            "The bundled 7z and compressed RAR decoder is only enabled on Windows x64.",
        ))
    }
}

pub fn inspect(path: &Path) -> Result<ArchiveInspection> {
    with_archive(path, |archive| archive.inspect())
}

pub fn extract(
    path: &Path,
    destination: &Path,
    policy: OverwritePolicy,
) -> Result<OperationReport> {
    let sources = match volumes::resolve(path)? {
        volumes::Set::Seven { paths, .. } | volumes::Set::Rar { paths, .. } => paths,
        volumes::Set::Single => vec![path.to_owned()],
    };
    crate::tasks::protect_sources(&sources)?;
    with_archive(path, |archive| archive.extract(destination, policy))
}

pub fn here_conflict(path: &Path, destination: &Path) -> Result<bool> {
    let mut backend = open_backend(path)?;
    output::here_conflict(destination, &backend.list_entries()?)
}

pub fn destination_exists(path: &Path, destination: &Path) -> Result<bool> {
    let archive = open_backend(path)?;
    output::destination_exists(destination, &archive.output_name())
}

pub fn test(path: &Path) -> Result<OperationReport> {
    with_archive(path, |archive| archive.test())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod multiformat_tests;

#[cfg(test)]
mod creation_tests;

#[cfg(test)]
mod iso_tests;

#[cfg(test)]
mod udf_tests;
