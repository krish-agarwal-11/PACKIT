// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
//! Creation uses a bounded, no-follow source snapshot, then one reader
//! verification before publishing through the extraction commit machinery.
use super::{
    output, streaming as common, ArchiveEntry, ArchiveError, EntryKind, OperationReport,
    OverwritePolicy, Result,
};
use cap_fs_ext::{FollowSymlinks, MetadataExt, OpenOptionsFollowExt};
use cap_std::fs::{Dir, OpenOptions};
use serde::Deserialize;
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Zip,
    SevenZip,
    Tar,
    TarGz,
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    Fast,
    Balanced,
    Maximum,
}
impl Preset {
    fn level(self) -> u32 {
        match self {
            Self::Fast => 1,
            Self::Balanced => 6,
            Self::Maximum => 9,
        }
    }
}
impl Format {
    fn extension(self) -> &'static str {
        match self {
            Self::Zip => ".zip",
            Self::SevenZip => ".7z",
            Self::Tar => ".tar",
            Self::TarGz => ".tar.gz",
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub sources: Vec<PathBuf>,
    pub destination: PathBuf,
    pub filename: String,
    pub format: Format,
    pub preset: Preset,
    pub policy: OverwritePolicy,
}
struct Source {
    parent: Dir,
    leaf: String,
    entry: ArchiveEntry,
    identity: (u64, u64, u64),
    modified: cap_std::time::SystemTime,
}
fn unsafe_source() -> ArchiveError {
    ArchiveError::new("unsafe_source","Sources must be regular files or real directories, without links, reparse points or special entries.")
}
fn changed() -> ArchiveError {
    ArchiveError::new(
        "source_changed",
        "A source changed while creating the archive. Output was not published.",
    )
}
fn regular(metadata: &cap_std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use cap_std::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    !metadata.file_type().is_symlink() && (metadata.is_file() || metadata.is_dir())
}
pub fn filename(name: &str, format: Format) -> Result<String> {
    let name = name.trim();
    let mut stem = name;
    loop {
        let lower = stem.to_ascii_lowercase();
        if let Some(suffix) = [".tar.gz", ".zip", ".7z", ".tar", ".tgz"]
            .iter()
            .find(|s| lower.ends_with(**s))
        {
            stem = &stem[..stem.len() - suffix.len()];
        } else {
            break;
        }
    }
    let result = format!("{stem}{}", format.extension());
    output::path_parts(&result, EntryKind::File)?;
    if stem.is_empty() || result.contains('/') {
        return Err(ArchiveError::new(
            "invalid_filename",
            "Choose a safe, nonempty archive filename.",
        ));
    }
    Ok(result)
}

/// Shared by native pickers and Explorer; preserve the source's received order.
pub fn suggest_name(source: &Path, format: Format) -> Result<String> {
    let metadata = std::fs::symlink_metadata(source).map_err(common::damaged)?;
    let name = source
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(unsafe_source)?;
    filename(
        &crate::naming::creation_name(name, metadata.is_dir(), format.extension()),
        format,
    )
}
pub fn exists(destination: &Path, name: &str, format: Format) -> Result<bool> {
    output::destination_exists(destination, &filename(name, format)?)
}
// Explicit traversal and accounting context keeps recursive authority bounded.
#[allow(clippy::too_many_arguments)]
fn enumerate(
    parent: Dir,
    leaf: String,
    path: String,
    absolute: &Path,
    excluded: &Path,
    plan: &mut output::Plan,
    list: &mut Vec<Source>,
    total: &mut u64,
) -> Result<()> {
    crate::tasks::check()?;
    #[cfg(windows)]
    let is_output = absolute
        .to_string_lossy()
        .eq_ignore_ascii_case(&excluded.to_string_lossy());
    #[cfg(not(windows))]
    let is_output = absolute == excluded;
    if is_output {
        return Ok(());
    }
    let metadata = parent.symlink_metadata(&leaf).map_err(common::damaged)?;
    if !regular(&metadata) {
        return Err(unsafe_source());
    }
    let directory = metadata.is_dir();
    let entry = ArchiveEntry {
        path: path.clone(),
        kind: if directory {
            EntryKind::Directory
        } else {
            EntryKind::File
        },
        compressed_bytes: None,
        uncompressed_bytes: if directory { 0 } else { metadata.len() },
        method: String::new(),
        modified: None,
        issue: None,
    };
    common::add(plan, &entry, list.len(), total)?;
    if directory {
        let dir = output::open_directory(&parent, &leaf).map_err(common::damaged)?;
        let mut children = dir
            .entries()
            .map_err(common::damaged)?
            .map(|item| item.map(|item| item.file_name()))
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(common::damaged)?;
        children.sort();
        if children.len() > common::MAX_ENTRIES {
            return Err(common::limit(
                "Source directory exceeds the entry-count limit.",
            ));
        }
        for child in children {
            let name = child.to_str().ok_or_else(unsafe_source)?.to_owned();
            enumerate(
                dir.try_clone().map_err(common::damaged)?,
                name.clone(),
                format!("{path}/{name}"),
                &absolute.join(&name),
                excluded,
                plan,
                list,
                total,
            )?;
        }
    }
    list.push(Source {
        parent,
        leaf,
        entry,
        identity: (metadata.dev(), metadata.ino(), metadata.len()),
        modified: metadata.modified().map_err(common::damaged)?,
    });
    Ok(())
}
fn snapshot(list: &[Source], root: &Path) -> Result<()> {
    let entries: Vec<_> = list.iter().map(|s| s.entry.clone()).collect();
    crate::tasks::phase("preparing", Some(&entries));
    let mut buffer = vec![0; common::BUFFER];
    for source in list {
        crate::tasks::check()?;
        let target = root.join(&source.entry.path);
        if source.entry.kind == EntryKind::Directory {
            std::fs::create_dir_all(target).map_err(common::damaged)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(common::damaged)?;
        }
        let mut input = source
            .parent
            .open_with(
                &source.leaf,
                OpenOptions::new().read(true).follow(FollowSymlinks::No),
            )
            .map_err(common::damaged)?;
        let check = |m: cap_std::fs::Metadata| -> Result<()> {
            if !regular(&m)
                || (m.dev(), m.ino(), m.len()) != source.identity
                || m.modified().map_err(common::damaged)? != source.modified
            {
                Err(changed())
            } else {
                Ok(())
            }
        };
        check(input.metadata().map_err(common::damaged)?)?;
        let mut file = File::options()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(common::damaged)?;
        crate::tasks::file(&source.entry.path);
        common::transfer(
            &mut input,
            source.entry.uncompressed_bytes,
            &mut buffer,
            &mut file,
        )?;
        if input.read(&mut [0]).map_err(common::damaged)? != 0 {
            return Err(changed());
        }
        check(input.metadata().map_err(common::damaged)?)?;
        check(
            source
                .parent
                .symlink_metadata(&source.leaf)
                .map_err(common::damaged)?,
        )?;
        crate::tasks::file_done();
    }
    Ok(())
}
fn pack_tar(writer: impl Write, entries: &[ArchiveEntry], root: &Path) -> Result<()> {
    let mut tar = tar::Builder::new(writer);
    for entry in entries {
        crate::tasks::check()?;
        crate::tasks::file(&entry.path);
        let mut header = tar::Header::new_ustar();
        header.set_mode(if entry.kind == EntryKind::Directory {
            0o755
        } else {
            0o644
        });
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_size(entry.uncompressed_bytes);
        header.set_entry_type(if entry.kind == EntryKind::Directory {
            tar::EntryType::Directory
        } else {
            tar::EntryType::Regular
        });
        // Emit POSIX PAX rather than GNU long-name records, matching the reader.
        if header.set_path(&entry.path).is_err() || !entry.path.is_ascii() {
            tar.append_pax_extensions([("path", entry.path.as_bytes())])
                .map_err(common::damaged)?;
            header.set_path("PaxEntry").map_err(common::damaged)?;
        }
        header.set_cksum();
        if entry.kind == EntryKind::Directory {
            tar.append(&header, std::io::empty())
                .map_err(common::damaged)?;
        } else {
            tar.append(
                &header,
                ProgressReader(File::open(root.join(&entry.path)).map_err(common::damaged)?),
            )
            .map_err(map_stream_error)?;
            crate::tasks::file_done();
        }
    }
    tar.finish().map_err(map_stream_error)?;
    Ok(())
}
struct ProgressReader(File);
impl Read for ProgressReader {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        if crate::tasks::check().is_err() {
            return Err(std::io::Error::other("cancelled"));
        }
        let n = self.0.read(bytes)?;
        crate::tasks::bytes(n as u64).map_err(|_| std::io::Error::other("cancelled"))?;
        Ok(n)
    }
}
fn map_stream_error(error: std::io::Error) -> ArchiveError {
    crate::tasks::check()
        .err()
        .unwrap_or_else(|| common::damaged(error))
}
pub fn create(options: Options) -> Result<OperationReport> {
    if options.sources.is_empty() || options.sources.len() > 20_000 {
        return Err(ArchiveError::new(
            "invalid_sources",
            "Choose between 1 and 20,000 sources.",
        ));
    }
    let name = filename(&options.filename, options.format)?;
    output::open_destination(&options.destination)?;
    let destination = std::fs::canonicalize(&options.destination).map_err(common::damaged)?;
    let excluded = destination.join(&name);
    if matches!(options.policy, OverwritePolicy::Refuse)
        && output::destination_exists(&destination, &name)?
    {
        return Err(ArchiveError::new(
            "destination_exists",
            "An archive with this name already exists.",
        ));
    }
    let workspace = tempfile::Builder::new()
        .prefix("packit-create-")
        .tempdir()
        .map_err(common::damaged)?;
    let snapshot_dir = tempfile::Builder::new()
        .prefix("packit-sources-")
        .tempdir()
        .map_err(common::damaged)?;
    let mut plan = output::Plan::default();
    let mut list = Vec::new();
    let mut total = 0;
    for path in &options.sources {
        if !path.is_absolute() {
            return Err(unsafe_source());
        }
        let parent_path = path.parent().ok_or_else(unsafe_source)?;
        let parent_path = std::fs::canonicalize(parent_path).map_err(common::damaged)?;
        let leaf = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(unsafe_source)?
            .to_owned();
        enumerate(
            output::open_destination(&parent_path)?,
            leaf.clone(),
            leaf.clone(),
            &parent_path.join(&leaf),
            &excluded,
            &mut plan,
            &mut list,
            &mut total,
        )?;
    }
    if list.is_empty() {
        return Err(ArchiveError::new(
            "invalid_sources",
            "The output archive cannot be its own only source.",
        ));
    }
    list.sort_by(|a, b| a.entry.path.cmp(&b.entry.path));
    let entries: Vec<_> = list.iter().map(|s| s.entry.clone()).collect();
    if options.format == Format::SevenZip
        && (list.len() > 512
            || total > 1 << 30
            || entries.iter().any(|e| e.uncompressed_bytes > 512 << 20))
    {
        return Err(common::limit(
            "7z creation is limited to 512 entries, 512 MiB per file and 1 GiB total.",
        ));
    }
    snapshot(&list, snapshot_dir.path())?;
    let target = workspace
        .path()
        .join(format!("output{}", options.format.extension()));
    crate::tasks::phase(
        if options.format == Format::Tar {
            "packaging"
        } else {
            "compressing"
        },
        Some(&entries),
    );
    match options.format {
        Format::Zip => {
            let mut zip = zip::ZipWriter::new(File::create(&target).map_err(common::damaged)?);
            for entry in &entries {
                crate::tasks::check()?;
                crate::tasks::file(&entry.path);
                let opts = zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated)
                    .compression_level(Some(options.preset.level() as i64))
                    .large_file(entry.uncompressed_bytes >= u32::MAX as u64);
                if entry.kind == EntryKind::Directory {
                    zip.add_directory(format!("{}/", entry.path), opts)
                        .map_err(ArchiveError::worker)?;
                } else {
                    zip.start_file(&entry.path, opts)
                        .map_err(ArchiveError::worker)?;
                    common::transfer(
                        &mut File::open(snapshot_dir.path().join(&entry.path))
                            .map_err(common::damaged)?,
                        entry.uncompressed_bytes,
                        &mut vec![0; common::BUFFER],
                        &mut zip,
                    )?;
                    crate::tasks::file_done();
                }
            }
            zip.finish()
                .map_err(ArchiveError::worker)?
                .sync_all()
                .map_err(common::damaged)?;
        }
        Format::Tar => {
            let mut file = File::create(&target).map_err(common::damaged)?;
            pack_tar(&mut file, &entries, snapshot_dir.path())?;
            file.sync_all().map_err(common::damaged)?;
        }
        Format::TarGz => {
            let mut file = File::create(&target).map_err(common::damaged)?;
            let mut encoder = flate2::write::GzEncoder::new(
                &mut file,
                flate2::Compression::new(options.preset.level()),
            );
            pack_tar(&mut encoder, &entries, snapshot_dir.path())?;
            encoder.finish().map_err(map_stream_error)?;
            file.sync_all().map_err(common::damaged)?;
        }
        Format::SevenZip => {
            #[cfg(all(windows, target_arch = "x86_64"))]
            {
                let manifest = workspace.path().join("sources.txt");
                let top: std::collections::BTreeSet<_> = entries
                    .iter()
                    .map(|e| e.path.split('/').next().unwrap())
                    .collect();
                std::fs::write(&manifest, top.into_iter().collect::<Vec<_>>().join("\n"))
                    .map_err(common::damaged)?;
                // Native encoding has no reliable live byte callback.
                crate::tasks::phase("compressing", None);
                super::seven_zip::create(snapshot_dir, &target, options.preset.level(), &manifest)?;
            }
            #[cfg(not(all(windows, target_arch = "x86_64")))]
            {
                return Err(ArchiveError::new(
                    "backend_unavailable",
                    "7z creation requires the bundled Windows x64 encoder.",
                ));
            }
        }
    }
    crate::tasks::check()?;
    crate::tasks::phase("verifying", None);
    // Each reader performs one full scan, checking data and its available CRCs.
    let mut backend = super::open_backend(&target)?;
    let actual = backend.list_entries()?;
    let key = |e: &ArchiveEntry| {
        (
            e.path.trim_end_matches('/').to_owned(),
            e.kind == EntryKind::Directory,
            e.uncompressed_bytes,
        )
    };
    let mut wanted: Vec<_> = entries.iter().map(key).collect();
    wanted.sort();
    let mut found: Vec<_> = actual.iter().map(key).collect();
    found.sort();
    if wanted != found {
        return Err(ArchiveError::new(
            "verification_failed",
            "Created archive metadata did not match the source snapshot.",
        ));
    }
    if matches!(options.format, Format::Zip | Format::SevenZip) {
        backend.verify_created()?;
    }
    let report = common::report(
        &entries,
        entries
            .iter()
            .filter(|e| e.kind == EntryKind::Directory)
            .count(),
        None,
    );
    let mut reader = File::open(&target).map_err(common::damaged)?;
    crate::tasks::phase("committing", None);
    let published = output::publish(&destination, &name, options.policy, &mut reader)?;
    Ok(OperationReport {
        files: report.files,
        directories: entries
            .iter()
            .filter(|e| e.kind == EntryKind::Directory)
            .count(),
        processed_bytes: total,
        output_path: Some(destination.join(published).to_string_lossy().into_owned()),
    })
}
