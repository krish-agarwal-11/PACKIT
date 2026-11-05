// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use super::{ArchiveError, EntryKind, Result};
use cap_fs_ext::{DirExt, FollowSymlinks, MetadataExt, OpenOptionsFollowExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, DirBuilder, OpenOptions},
};
use std::{collections::BTreeMap, io, path::Path};
use unicode_normalization::UnicodeNormalization;

const MAX_OUTPUT_NODES: usize = 50_000;

pub(super) fn path_parts(name: &str, kind: EntryKind) -> Result<Vec<&str>> {
    let fail = || {
        ArchiveError::new(
            "unsafe_path",
            format!("Unsafe or nonportable ZIP path: {name:?}."),
        )
    };
    let path = if kind == EntryKind::Directory {
        name.strip_suffix('/').unwrap_or(name)
    } else {
        name
    };
    if path.len() > 1024 {
        return Err(fail());
    }
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() > 32 || parts.iter().any(|part| !safe_component(part)) {
        return Err(fail());
    }
    Ok(parts)
}

fn safe_component(part: &str) -> bool {
    if part.is_empty()
        || part.len() > 255
        || part.ends_with(['.', ' '])
        || part
            .chars()
            .any(|ch| ch.is_control() || "<>:\"\\|?*".contains(ch))
    {
        return false;
    }
    let canonical: String = part.nfkc().collect();
    let device = canonical
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    !matches!(
        device.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$"
    ) && !((device.starts_with("COM") || device.starts_with("LPT"))
        && device.len() == 4
        && device.as_bytes()[3].is_ascii_digit())
}

/// Strip the format's longest known suffix, or the last extension for a
/// structure-detected archive with a different extension.
pub(super) fn output_folder(name: &str, suffixes: &[&str]) -> String {
    crate::naming::output_folder(name, suffixes)
}

#[derive(Default)]
pub(super) struct Plan {
    nodes: BTreeMap<String, Node>,
}

struct Node {
    name: String,
    directory: bool,
    explicit: bool,
}

impl Plan {
    pub fn add(&mut self, path: &str, kind: EntryKind) -> Result<()> {
        let parts = path_parts(path, kind)?;
        let mut key = String::new();
        let mut name = String::new();
        for (index, part) in parts.iter().enumerate() {
            if index > 0 {
                key.push('/');
                name.push('/');
            }
            key.extend(part.nfkc().flat_map(char::to_uppercase));
            name.push_str(part);
            let explicit = index + 1 == parts.len();
            let directory = !explicit || kind == EntryKind::Directory;
            if let Some(node) = self.nodes.get_mut(&key) {
                if node.name != name || !directory || !node.directory || (explicit && node.explicit)
                {
                    return Err(ArchiveError::new(
                        "path_conflict",
                        format!(
                            "Conflicting ZIP output paths: {:?} and {path:?}.",
                            node.name
                        ),
                    ));
                }
                node.explicit |= explicit;
            } else {
                if self.nodes.len() >= MAX_OUTPUT_NODES {
                    return Err(ArchiveError::new(
                        "archive_limit",
                        "ZIP would create more than 50,000 output files and folders.",
                    ));
                }
                self.nodes.insert(
                    key.clone(),
                    Node {
                        name: name.clone(),
                        directory,
                        explicit,
                    },
                );
            }
        }
        Ok(())
    }

    pub fn create_directories(&self, root: &mut Output) -> Result<usize> {
        let mut count = 0;
        // Prefixes sort before their children, and each directory is created only once.
        for node in self
            .nodes
            .values()
            .filter(|node| node.directory && crate::tasks::selected(&node.name, true))
        {
            crate::tasks::check()?;
            root.create_directory(&node.name)?;
            count += 1;
        }
        Ok(count)
    }
}

fn create_private_directory(parent: &Dir, name: &str) -> io::Result<()> {
    #[cfg(unix)]
    let builder = {
        use cap_std::fs::DirBuilderExt;
        let mut builder = DirBuilder::new();
        builder.mode(0o700);
        builder
    };
    #[cfg(not(unix))]
    let builder = DirBuilder::new();
    parent.create_dir_with(name, &builder)
}

pub(super) fn open_directory(parent: &Dir, name: impl AsRef<Path>) -> io::Result<Dir> {
    let dir = parent.open_dir_nofollow(name)?;
    #[cfg(windows)]
    {
        use cap_std::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if dir.dir_metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Output directory is a reparse point",
            ));
        }
    }
    Ok(dir)
}

fn parent<'a>(root: &Dir, path: &'a str) -> Result<(Dir, &'a str)> {
    let mut parts = path.split('/').peekable();
    let mut dir = root
        .try_clone()
        .map_err(|error| ArchiveError::io("Could not access output folder", error))?;
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            return Ok((dir, part));
        }
        dir = open_directory(&dir, part).map_err(|error| {
            ArchiveError::io("Could not safely open an output subfolder", error)
        })?;
    }
    unreachable!("validated nonempty path")
}

pub(super) struct Output {
    root: Dir,
    created: Vec<(String, bool)>,
}

impl Output {
    pub fn new_file(&mut self, path: &str) -> Result<cap_std::fs::File> {
        let (dir, name) = parent(&self.root, path)?;
        let file = dir
            .open_with(name, OpenOptions::new().write(true).create_new(true))
            .map_err(|error| ArchiveError::io("Could not create output file", error))?;
        self.created.push((path.to_owned(), false));
        Ok(file)
    }

    fn create_directory(&mut self, path: &str) -> Result<()> {
        let (dir, name) = parent(&self.root, path)?;
        create_private_directory(&dir, name)
            .map_err(|error| ArchiveError::io("Could not create an archive folder", error))?;
        self.created.push((path.to_owned(), true));
        Ok(())
    }

    fn cleanup(self) -> Result<()> {
        // Never recursively delete: only remove names this operation created,
        // then remove empty folders, all through the pinned output directory.
        let mut failure = None;
        for (path, directory) in self.created.into_iter().rev() {
            let result = parent(&self.root, &path).and_then(|(dir, name)| {
                let result = if directory {
                    dir.remove_dir(name)
                } else {
                    dir.remove_file(name)
                };
                match result {
                    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                    other => other.map_err(|error| {
                        ArchiveError::io("Could not remove partial output", error)
                    }),
                }
            });
            if let Err(error) = result {
                failure = Some(error);
            }
        }
        let root = self
            .root
            .remove_open_dir()
            .map_err(|error| ArchiveError::io("Could not remove the output folder", error));
        failure.map_or(root, Err)
    }
}

pub(super) fn open_destination(destination: &Path) -> Result<Dir> {
    if !destination.is_absolute() {
        return Err(ArchiveError::new(
            "invalid_destination",
            "Choose an absolute extraction destination.",
        ));
    }
    let metadata = std::fs::symlink_metadata(destination)
        .map_err(|error| ArchiveError::io("Could not access the destination", error))?;
    #[cfg(windows)]
    let reparse = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let reparse = metadata.file_type().is_symlink();
    if !metadata.is_dir() || reparse {
        return Err(ArchiveError::new(
            "invalid_destination",
            "Choose a real folder rather than a link or junction.",
        ));
    }

    // Pin the selected directory, then do all output I/O relative to directory
    // handles. The capability implementation confines resolution; each
    // archive-controlled component is also opened without following links.

    if let (Some(parent), Some(name)) = (destination.parent(), destination.file_name()) {
        Dir::open_ambient_dir(parent, ambient_authority())
            .and_then(|parent| open_directory(&parent, name))
    } else {
        Dir::open_ambient_dir(destination, ambient_authority())
    }
    .map_err(|error| ArchiveError::io("Could not safely open the destination", error))
}

pub(super) fn destination_exists(destination: &Path, name: &str) -> Result<bool> {
    Ok(metadata(&open_destination(destination)?, name)?.is_some())
}

pub(super) fn here_conflict(destination: &Path, entries: &[super::ArchiveEntry]) -> Result<bool> {
    let base = open_destination(destination)?;
    for entry in entries {
        let parts = path_parts(&entry.path, entry.kind)?;
        let mut dir = base.try_clone().map_err(super::streaming::damaged)?;
        for (index, part) in parts.iter().enumerate() {
            let Some(info) = metadata(&dir, part)? else {
                break;
            };
            if index + 1 == parts.len() && entry.kind != EntryKind::Directory {
                return Ok(true);
            }
            if !info.is_dir() || info.file_type().is_symlink() {
                return Ok(true);
            }
            dir = open_directory(&dir, part).map_err(super::streaming::damaged)?;
        }
    }
    Ok(false)
}

fn metadata(dir: &Dir, name: &str) -> Result<Option<cap_std::fs::Metadata>> {
    match dir.symlink_metadata(name) {
        Ok(info) => Ok(Some(info)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ArchiveError::io("Could not check the output path", error)),
    }
}

fn file_identity(dir: &Dir, name: &str) -> Result<(u64, u64)> {
    if !metadata(dir, name)?.as_ref().is_some_and(regular) {
        return Err(ArchiveError::new(
            "destination_conflict",
            "An expected regular file is missing or unsafe.",
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let info = dir
        .open_with(name, &options)
        .and_then(|file| file.metadata())
        .map_err(|e| ArchiveError::io("Could not verify file identity", e))?;
    if !regular(&info) {
        return Err(ArchiveError::new(
            "destination_conflict",
            "A file changed during extraction.",
        ));
    }
    Ok((info.dev(), info.ino()))
}

fn regular(info: &cap_std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use cap_std::fs::MetadataExt;
        if info.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    info.is_file()
}

// Windows can resolve 8.3 short names to unrelated long names. Only merge
// into entries whose actual directory names match the requested spelling.
#[cfg(windows)]
#[derive(Default)]
struct Names(BTreeMap<String, std::collections::BTreeSet<String>>);

#[cfg(windows)]
impl Names {
    fn verify(&mut self, dir: &Dir, path: &str, leaf: &str) -> Result<()> {
        let parent_path = path.rsplit_once('/').map_or("", |(parent, _)| parent);
        let key = |name: &str| {
            name.chars()
                .flat_map(char::to_uppercase)
                .collect::<String>()
        };
        if !self.0.contains_key(parent_path) {
            let names = dir
                .entries()
                .and_then(|entries| {
                    entries
                        .map(|entry| entry.map(|entry| key(&entry.file_name().to_string_lossy())))
                        .collect()
                })
                .map_err(|error| ArchiveError::io("Could not verify destination names", error))?;
            self.0.insert(parent_path.into(), names);
        }
        if !self.0[parent_path].contains(&key(leaf)) {
            return Err(ArchiveError::new(
                "destination_conflict",
                format!("The destination {path:?} resolves through a filesystem alias."),
            ));
        }
        Ok(())
    }
}

fn create_output(base: &Dir, name: &str) -> Result<Output> {
    create_private_directory(base, name)
        .map_err(|error| ArchiveError::io("Could not create the output folder", error))?;
    let root = open_directory(base, name)
        .map_err(|error| ArchiveError::io("Could not safely open the new output folder", error))?;
    Ok(Output {
        root,
        created: Vec::new(),
    })
}

fn with_cleanup<T>(mut output: Output, write: impl FnOnce(&mut Output) -> Result<T>) -> Result<T> {
    match write(&mut output).and_then(|value| {
        crate::tasks::check()?;
        Ok(value)
    }) {
        Ok(value) => Ok(value),
        Err(mut error) => {
            if let Err(cleanup) = output.cleanup() {
                error.message.push_str(&format!(
                    " PACKIT could not remove all partial output: {}",
                    cleanup.message
                ));
            }
            Err(error)
        }
    }
}

pub(super) fn extract_to<T>(
    destination: &Path,
    name: &str,
    policy: super::OverwritePolicy,
    plan: &Plan,
    write: impl FnOnce(&mut Output) -> Result<T>,
) -> Result<(String, T)> {
    use super::OverwritePolicy::*;
    crate::tasks::check()?;
    crate::tasks::validate_selection(|path| plan.nodes.values().any(|node| node.name == path))?;
    let base = open_destination(destination)?;
    if crate::tasks::extract_here() && !matches!(policy, KeepBoth) {
        let value = replace(
            &base,
            &base,
            destination,
            plan,
            write,
            matches!(policy, Replace),
        )?;
        return Ok((String::new(), value));
    }
    match policy {
        Refuse => {
            let output = create_output(&base, name)?;
            with_cleanup(output, write).map(|value| (name.into(), value))
        }
        KeepBoth => {
            for n in 1..=10_000 {
                let candidate = format!("{name} ({n})");
                match create_output(&base, &candidate) {
                    Ok(output) => {
                        return with_cleanup(output, write).map(|value| (candidate, value))
                    }
                    Err(error) if error.code == "destination_exists" => continue,
                    Err(error) => return Err(error),
                }
            }
            Err(ArchiveError::new(
                "destination_exists",
                "No available numbered destination among the first 10,000 folders.",
            ))
        }
        Replace => {
            #[cfg(windows)]
            Names::default().verify(&base, name, name)?;
            let target = open_directory(&base, name).map_err(|error| {
                ArchiveError::io(
                    "Could not safely open the existing extraction folder",
                    error,
                )
            })?;
            let value = replace(&base, &target, destination, plan, write, true)?;
            Ok((name.into(), value))
        }
    }
}

fn temporary_output(base: &Dir) -> Result<(String, Output)> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    for _ in 0..1000 {
        let name = format!(
            ".packit-stage-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        match create_output(base, &name) {
            Ok(output) => return Ok((name, output)),
            Err(error) if error.code == "destination_exists" => continue,
            Err(error) => return Err(error),
        }
    }
    Err(ArchiveError::new(
        "destination_exists",
        "Could not reserve a temporary extraction folder.",
    ))
}

fn replace<T>(
    base: &Dir,
    target: &Dir,
    destination: &Path,
    plan: &Plan,
    write: impl FnOnce(&mut Output) -> Result<T>,
    allow_replace: bool,
) -> Result<T> {
    let (_, mut staged) = temporary_output(base)?;
    let mut backups = None;
    let mut keep_backups = false;
    let result = (|| {
        crate::tasks::phase("running", None);
        let value = write(&mut staged)?;
        crate::tasks::check()?;
        let (backup_name, backup) = temporary_output(base)?;
        backups = Some(backup);
        let backup = backups.as_mut().unwrap();
        crate::tasks::phase("committing", None);
        if let Err(error) = merge(
            target,
            &staged.root,
            backup,
            plan,
            &mut keep_backups,
            allow_replace,
        ) {
            if keep_backups {
                return Err(ArchiveError::new(
                    "rollback_failed",
                    format!(
                        "{} Rollback was incomplete. Recovery copies are in {:?}.",
                        error.message,
                        destination.join(backup_name)
                    ),
                ));
            }
            return Err(error);
        }
        Ok(value)
    })();
    let stage_cleanup = staged.cleanup();
    let backup_cleanup = if keep_backups {
        Ok(())
    } else {
        backups.map_or(Ok(()), Output::cleanup)
    };
    match (result, stage_cleanup.and(backup_cleanup)) {
        (Ok(value), Ok(())) => Ok(value),
        (Ok(_), Err(cleanup)) => Err(ArchiveError::new(
            "cleanup_failed",
            format!(
                "Extraction committed, but temporary cleanup failed: {}",
                cleanup.message
            ),
        )),
        (Err(error), Ok(())) => Err(error),
        (Err(mut error), Err(cleanup)) => {
            error
                .message
                .push_str(&format!(" Temporary cleanup failed: {}", cleanup.message));
            Err(error)
        }
    }
}

/// Publish one verified archive using the same pinned-directory merge and
/// backup/rollback machinery as extraction. Never recursively remove user data.
pub(super) fn publish(
    destination: &Path,
    name: &str,
    policy: super::OverwritePolicy,
    source: &mut std::fs::File,
) -> Result<String> {
    use super::OverwritePolicy;
    use std::io::Seek;
    path_parts(name, EntryKind::File)?;
    if name.contains('/') {
        return Err(ArchiveError::new(
            "unsafe_path",
            "Choose an archive filename, not a path.",
        ));
    }
    let base = open_destination(destination)?;
    let (stage_name, mut stage) = temporary_output(&base)?;
    let mut backup = None;
    let mut keep = false;
    let result = (|| {
        let candidate = match policy {
            OverwritePolicy::KeepBoth => {
                let (stem, suffix) = if let Some(stem) = name.strip_suffix(".tar.gz") {
                    (stem.to_owned(), ".tar.gz".to_owned())
                } else {
                    name.rsplit_once('.')
                        .map_or((name.to_owned(), String::new()), |(a, b)| {
                            (a.to_owned(), format!(".{b}"))
                        })
                };
                // Numbering is reserved by a no-clobber hard link below; retry
                // if another creator wins the same available number.
                (stem.to_owned(), suffix.to_owned())
            }
            _ => (String::new(), String::new()),
        };
        source.rewind().map_err(super::streaming::damaged)?;
        let size = source.metadata().map_err(super::streaming::damaged)?.len();
        let mut file = stage.new_file("payload")?;
        super::streaming::transfer(
            source,
            size,
            &mut vec![0; super::streaming::BUFFER],
            &mut file,
        )?;
        file.sync_all().map_err(super::streaming::damaged)?;
        drop(file);
        crate::tasks::check()?;
        crate::tasks::phase("committing", None);
        if !matches!(policy, OverwritePolicy::Replace) {
            for number in 0..=10_000 {
                let target = if number == 0 {
                    name.to_owned()
                } else {
                    format!("{} ({number}){}", candidate.0, candidate.1)
                };
                match stage.root.hard_link("payload", &base, &target) {
                    Ok(()) => return Ok(target),
                    Err(e)
                        if matches!(policy, OverwritePolicy::KeepBoth)
                            && e.kind() == io::ErrorKind::AlreadyExists =>
                    {
                        continue
                    }
                    Err(e) => {
                        return Err(ArchiveError::io(
                            "Could not publish archive without overwriting",
                            e,
                        ))
                    }
                }
            }
            return Err(ArchiveError::new(
                "destination_exists",
                "No available numbered filename.",
            ));
        }
        let (backup_name, saved) = temporary_output(&base)?;
        backup = Some((backup_name, saved));
        // Give the staged file its final name for the shared merger.
        stage
            .root
            .rename("payload", &stage.root, name)
            .map_err(super::streaming::damaged)?;
        stage.created = vec![(name.into(), false)];
        let mut plan = Plan::default();
        plan.add(name, EntryKind::File)?;
        let result = merge(
            &base,
            &stage.root,
            &mut backup.as_mut().unwrap().1,
            &plan,
            &mut keep,
            true,
        );
        if keep {
            return Err(ArchiveError::new(
                "rollback_failed",
                format!(
                    "Archive replacement recovery is incomplete. Recovery copies: {:?}.",
                    destination.join(&backup.as_ref().unwrap().0)
                ),
            ));
        }
        result?;
        Ok(name.into())
    })();
    let cleanup = stage.cleanup();
    if let Some((_, saved)) = backup {
        if !keep {
            saved.cleanup()?;
        }
    }
    cleanup.map_err(|e| {
        ArchiveError::new(
            "cleanup_failed",
            format!(
                "Archive operation ended but staging cleanup failed in {stage_name:?}: {}",
                e.message
            ),
        )
    })?;
    result
}

fn merge(
    target: &Dir,
    staged: &Dir,
    backup: &mut Output,
    plan: &Plan,
    keep_backups: &mut bool,
    allow_replace: bool,
) -> Result<()> {
    #[cfg(windows)]
    let mut names = Names::default();
    let mut directories = Vec::new();
    let mut committed = Vec::new();
    let result = (|| {
        // Resolve every planned component without following links. Missing
        // directories are journaled; no existing file is changed in preflight.
        for node in plan
            .nodes
            .values()
            .filter(|node| node.directory && crate::tasks::selected(&node.name, true))
        {
            crate::tasks::check()?;
            let (dir, leaf) = parent(target, &node.name)?;
            if metadata(&dir, leaf)?.is_none() {
                create_private_directory(&dir, leaf)
                    .map_err(|e| ArchiveError::io("Could not create a destination subfolder", e))?;
                directories.push(node.name.as_str());
            } else {
                #[cfg(windows)]
                names.verify(&dir, &node.name, leaf)?;
            }
            open_directory(&dir, leaf)
                .map_err(|e| ArchiveError::io("Destination subfolder is unsafe or blocked", e))?;
        }
        plan.create_directories(backup)?;
        let mut files = Vec::new();
        for node in plan
            .nodes
            .values()
            .filter(|node| !node.directory && crate::tasks::selected(&node.name, false))
        {
            crate::tasks::check()?;
            let (dir, leaf) = parent(target, &node.name)?;
            let existing = metadata(&dir, leaf)?;
            if existing.as_ref().is_some_and(|info| !regular(info)) {
                return Err(ArchiveError::new(
                    "destination_conflict",
                    format!(
                        "Cannot replace {:?}: the destination is a link, folder, or special file.",
                        node.name
                    ),
                ));
            }
            if existing
                .as_ref()
                .is_some_and(|info| info.permissions().readonly())
            {
                return Err(ArchiveError::new(
                    "permission_denied",
                    format!("Cannot replace read-only file {:?}.", node.name),
                ));
            }
            if existing
                .as_ref()
                .is_some_and(|info| crate::tasks::is_source(info.dev(), info.ino()))
            {
                return Err(ArchiveError::new(
                    "source_conflict",
                    "Extraction cannot overwrite the source archive or one of its volumes.",
                ));
            }
            let saved = existing.is_some();
            if saved && !allow_replace {
                return Err(ArchiveError::new("destination_exists", "Extract Here would replace an existing file. Choose an explicit conflict policy."));
            }
            if saved {
                #[cfg(windows)]
                names.verify(&dir, &node.name, leaf)?;
                let original = file_identity(&dir, leaf)?;
                let (backup_dir, backup_leaf) = parent(&backup.root, &node.name)?;
                dir.hard_link(leaf, &backup_dir, backup_leaf).map_err(|e| {
                    ArchiveError::io("Could not safeguard an existing file before replacement", e)
                })?;
                backup.created.push((node.name.clone(), false));
                if file_identity(&backup_dir, backup_leaf)? != original {
                    return Err(ArchiveError::new(
                        "destination_conflict",
                        "A destination file changed during backup.",
                    ));
                }
            }
            files.push((node.name.as_str(), saved));
        }
        for (path, saved) in files {
            crate::tasks::check()?;
            crate::tasks::file(path);
            let (dir, leaf) = parent(target, path)?;
            let (source, entry) = parent(staged, path)?;
            let installed = file_identity(&source, entry)?;
            if saved {
                let (backup_dir, backup_leaf) = parent(&backup.root, path)?;
                if file_identity(&dir, leaf)? != file_identity(&backup_dir, backup_leaf)? {
                    return Err(ArchiveError::new(
                        "destination_conflict",
                        format!("Destination file {path:?} changed during extraction."),
                    ));
                }
                source
                    .rename(entry, &dir, leaf)
                    .map_err(|e| ArchiveError::io(&format!("Could not replace {path:?}"), e))?;
            } else {
                source.hard_link(entry, &dir, leaf).map_err(|e| {
                    ArchiveError::io(
                        &format!("Could not install {path:?} without overwriting"),
                        e,
                    )
                })?;
            }
            committed.push((path, saved, installed));
            crate::tasks::file_done();
        }
        crate::tasks::check()?;
        Ok(())
    })();
    if let Err(error) = result {
        for (path, saved, installed) in committed.into_iter().rev() {
            let restored = parent(target, path).and_then(|(dir, leaf)| {
                if file_identity(&dir, leaf)? != installed {
                    return Err(ArchiveError::new(
                        "rollback_failed",
                        "Destination changed after commit.",
                    ));
                }
                if saved {
                    let (backup_dir, backup_leaf) = parent(&backup.root, path)?;
                    backup_dir.rename(backup_leaf, &dir, leaf)
                } else {
                    dir.remove_file(leaf)
                }
                .map_err(|e| ArchiveError::io("Could not restore the original destination", e))
            });
            if restored.is_err() {
                *keep_backups = true;
            }
        }
        for path in directories.into_iter().rev() {
            if parent(target, path)
                .and_then(|(dir, leaf)| {
                    dir.remove_dir(leaf).map_err(|e| {
                        ArchiveError::io("Could not remove a new destination subfolder", e)
                    })
                })
                .is_err()
            {
                *keep_backups = true;
            }
        }
        return Err(error);
    }
    Ok(())
}
