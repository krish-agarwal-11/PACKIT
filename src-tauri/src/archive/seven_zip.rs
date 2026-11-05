// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use super::{
    output, streaming as common, ArchiveBackend, ArchiveEntry, ArchiveError, ArchiveInspection,
    EntryKind, FormatCapabilities, Integrity, OperationReport, OverwritePolicy, Result,
};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, Read, Seek, Write},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::*,
};

const MAX_ARCHIVE: u64 = 2 << 30;
const MAX_ENTRY: u64 = 512 << 20;
const MAX_ENTRIES: usize = 512;
const MAX_LIST: u64 = 4 << 20;
const TIMEOUT: Duration = Duration::from_secs(180);
static RESOURCE_DIR: OnceLock<PathBuf> = OnceLock::new();

pub(crate) fn set_resource_dir(path: PathBuf) {
    let _ = RESOURCE_DIR.set(path);
}

fn executable() -> Result<PathBuf> {
    let packaged = RESOURCE_DIR.get().map(|p| p.join("7z.exe"));
    if let Some(path) = packaged.filter(|p| p.is_file() && p.with_file_name("7z.dll").is_file()) {
        return Ok(path);
    }
    #[cfg(debug_assertions)]
    {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/7zip/7z.exe");
        if path.is_file() && path.with_file_name("7z.dll").is_file() {
            return Ok(path);
        }
    }
    Err(ArchiveError::new(
        "backend_unavailable",
        "The private Windows x64 7-Zip backend is missing.",
    ))
}

/// Encode only a validated private snapshot. The existing suspended-start job,
/// memory budget, timeout and cancellation watchdog also supervise the encoder.
pub(super) fn create(
    snapshot: tempfile::TempDir,
    output: &Path,
    level: u32,
    list: &Path,
) -> Result<()> {
    let backend = SevenZip {
        root: snapshot,
        input: output.to_owned(),
        tool: executable()?,
        name: "created.7z".into(),
        length: 0,
        kind: "7z",
        deadline: Instant::now() + TIMEOUT,
        volumes: 1,
        inputs: Vec::new(),
    };
    let level = format!("-mx={level}");
    let include = format!("-i@{}", list.to_str().ok_or_else(invalid_listing)?);
    backend.run(
        "a",
        &[
            "-m0=LZMA2",
            "-md=16m",
            "-ms=off",
            "-mhc=off",
            "-scsUTF-8",
            &level,
            &include,
        ],
        None,
        |stdout| {
            let count = io::copy(&mut stdout.take(MAX_LIST + 1), &mut io::sink())
                .map_err(common::damaged)?;
            if count > MAX_LIST {
                return Err(common::limit("Encoder diagnostics exceeded their limit."));
            }
            Ok(())
        },
    )
}

pub(super) struct SevenZip {
    root: tempfile::TempDir,
    input: PathBuf,
    tool: PathBuf,
    name: String,
    length: u64,
    kind: &'static str,
    deadline: Instant,
    volumes: usize,
    inputs: Vec<PathBuf>,
}

struct Member {
    entry: ArchiveEntry,
    selection: String,
    crc: u32,
}

impl SevenZip {
    pub fn open(mut source: File, name: String, length: u64, kind: &'static str) -> Result<Self> {
        if length > MAX_ARCHIVE {
            return Err(common::limit(
                "Native archives are limited to 2 GiB of input.",
            ));
        }
        let tool = executable()?;
        let root = tempfile::Builder::new()
            .prefix("packit-decoder-")
            .tempdir()
            .map_err(|e| ArchiveError::io("Could not create private decoder workspace", e))?;
        let input = root.path().join(if kind == "7z" {
            "input.7z"
        } else {
            "input.rar"
        });
        source.rewind().map_err(common::damaged)?;
        let mut copy = File::options()
            .write(true)
            .create_new(true)
            .open(&input)
            .map_err(|e| ArchiveError::io("Could not create decoder input", e))?;
        common::transfer(&mut source, length, &mut vec![0; common::BUFFER], &mut copy)?;
        if source.read(&mut [0]).map_err(common::damaged)? != 0 {
            return Err(ArchiveError::new(
                "archive_changed",
                "Archive changed while staging input.",
            ));
        }
        Ok(Self {
            root,
            inputs: vec![input.clone()],
            input,
            tool,
            name,
            length,
            kind,
            deadline: Instant::now() + TIMEOUT,
            volumes: 1,
        })
    }

    pub fn open_volumes(paths: Vec<PathBuf>, name: String) -> Result<Self> {
        let first = paths.first().ok_or_else(invalid_listing)?;
        let mut prefix = [0; 8];
        common::open(first)?
            .read_exact(&mut prefix)
            .map_err(common::damaged)?;
        let kind = if prefix.starts_with(b"Rar!\x1a\x07\x01") {
            "Rar5"
        } else {
            "Rar"
        };
        let mut backend = Self::open(
            common::open(first)?,
            name,
            first.metadata().map_err(common::damaged)?.len(),
            kind,
        )?;
        // Only same-directory, explicitly discovered companions are exposed.
        // Preserve the validated numbering scheme in the private workspace.
        let first_name = first.file_name().ok_or_else(invalid_listing)?;
        let renamed = backend.root.path().join(first_name);
        std::fs::rename(&backend.input, &renamed).map_err(common::damaged)?;
        backend.input = renamed;
        backend.inputs = vec![backend.input.clone()];
        for path in paths.iter().skip(1) {
            let target = backend
                .root
                .path()
                .join(path.file_name().ok_or_else(invalid_listing)?);
            backend.inputs.push(target.clone());
            let mut source = common::open(path)?;
            let size = source.metadata().map_err(common::damaged)?.len();
            backend.length = backend
                .length
                .checked_add(size)
                .ok_or_else(invalid_listing)?;
            if backend.length > MAX_ARCHIVE {
                return Err(common::limit("RAR volume set exceeds 2 GiB."));
            }
            let mut output = File::options()
                .write(true)
                .create_new(true)
                .open(target)
                .map_err(common::damaged)?;
            common::transfer(&mut source, size, &mut vec![0; common::BUFFER], &mut output)?;
            if source.read(&mut [0]).map_err(common::damaged)? != 0 {
                return Err(invalid_listing());
            }
        }
        backend.volumes = paths.len();
        Ok(backend)
    }

    fn run<T>(
        &self,
        verb: &str,
        options: &[&str],
        selection: Option<&str>,
        read: impl FnOnce(&mut dyn Read) -> Result<T>,
    ) -> Result<T> {
        crate::tasks::check()?;
        let timeout = self.deadline.saturating_duration_since(Instant::now());
        if timeout.is_zero() {
            return Err(timeout_error());
        }
        let container = Arc::new(super::containment::Container::new(&self.tool)?);
        container.allow_read(self.root.path())?;
        let encoded = container.output.path().join("output.7z");
        if verb == "a" {
            container.allow_tree(self.root.path())?;
            container.allow_output()?;
            for option in options {
                if let Some(list) = option.strip_prefix("-i@") {
                    container.allow_read(Path::new(list))?;
                }
            }
        } else {
            for input in &self.inputs {
                container.allow_read(input)?;
            }
        }
        let mut arguments: Vec<std::ffi::OsString> =
            [verb, "-sccUTF-8", "-bsp0", "-mmt=1", "-y", "-spd", "-ssc"]
                .into_iter()
                .map(Into::into)
                .collect();
        arguments.push(format!("-t{}", self.kind).into());
        arguments.extend(options.iter().map(std::ffi::OsString::from));
        arguments.push("--".into());
        arguments.push(if verb == "a" {
            encoded.as_os_str().to_owned()
        } else {
            self.input.as_os_str().to_owned()
        });
        if let Some(path) = selection {
            arguments.push(path.into());
        }
        let job = Job::new()?;
        let mut child = container.spawn(&arguments, self.root.path())?;
        let handle = child.handle();
        if unsafe { AssignProcessToJobObject(job.0, handle) } == 0 {
            return Err(job_error("Could not confine decoder to a Windows job"));
        }
        let signal = Arc::new((Mutex::new(false), Condvar::new()));
        let watchdog_signal = signal.clone();
        let watchdog_job = job.clone();
        let operation = crate::tasks::control();
        let monitored = container.clone();
        let watchdog = thread::spawn(move || {
            let (lock, wake) = &*watchdog_signal;
            let deadline = Instant::now() + timeout;
            let mut done = lock.lock().unwrap();
            loop {
                if *done {
                    return false;
                }
                if operation
                    .as_ref()
                    .is_some_and(|c| c.cancelled.load(std::sync::atomic::Ordering::Acquire))
                {
                    watchdog_job.stop();
                    return false;
                }
                if !monitored.output_within_limits() {
                    watchdog_job.stop();
                    return true;
                }
                if Instant::now() >= deadline {
                    watchdog_job.stop();
                    return true;
                }
                done = wake
                    .wait_timeout(done, Duration::from_millis(25))
                    .unwrap()
                    .0;
            }
        });
        let mut stderr = child.stderr.take().unwrap().take(65536);
        let diagnostic = thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stderr.read_to_end(&mut bytes);
            bytes
        });
        let resumed = child.resume();
        let mut stdout = child.stdout.take().unwrap();
        let result = if !resumed {
            Err(ArchiveError::new(
                "backend_unavailable",
                "Could not resume the confined decoder.",
            ))
        } else {
            read(&mut stdout)
        };
        if result.is_err() {
            job.stop();
        }
        drop(stdout);
        let status = child.wait();
        {
            let (lock, wake) = &*signal;
            *lock.lock().unwrap() = true;
            wake.notify_one();
        }
        let timed_out = watchdog.join().unwrap_or(true);
        crate::tasks::check()?;
        if timed_out {
            return Err(timeout_error());
        }
        let error = diagnostic.join().unwrap_or_default();
        let status = status.map_err(|e| ArchiveError::io("Could not wait for decoder", e))?;
        if result
            .as_ref()
            .is_err_and(|error| error.code == "unsupported_encryption")
        {
            return result;
        }
        if !status.success() && (result.is_ok() || status.code() != Some(124)) {
            return Err(decoder_error(&error));
        }
        let value = result?;
        if verb == "a" {
            // The worker is terminated/reaped before trusted code reads output.
            let mut input = common::open(&encoded)?;
            let length = input.metadata().map_err(common::damaged)?.len();
            if length > MAX_ARCHIVE {
                return Err(common::limit("Encoded archive exceeds 2 GiB."));
            }
            let mut target = File::options()
                .write(true)
                .create_new(true)
                .open(&self.input)
                .map_err(common::damaged)?;
            common::transfer(
                &mut input,
                length,
                &mut vec![0; common::BUFFER],
                &mut target,
            )?;
            target.sync_all().map_err(common::damaged)?;
        }
        Ok(value)
    }

    fn members(&self) -> Result<(Vec<Member>, output::Plan)> {
        let bytes = self.run("l", &["-slt"], None, |stdout| {
            let mut bytes = Vec::new();
            stdout
                .take(MAX_LIST + 1)
                .read_to_end(&mut bytes)
                .map_err(common::damaged)?;
            if bytes.len() as u64 > MAX_LIST {
                return Err(common::limit("Decoder listing exceeds 4 MiB."));
            }
            // With no console and closed stdin, 7-Zip reports the prompt
            // on stdout and only "Break signaled" on stderr.
            if bytes
                .windows(b"Enter password (will not be echoed):".len())
                .any(|line| line == b"Enter password (will not be echoed):")
            {
                return Err(decoder_error(b"password"));
            }
            Ok(bytes)
        })?;
        parse_listing(&bytes, self.kind, self.length, self.volumes)
    }

    fn decode(&self, members: &[Member], staged: &mut dyn Write) -> Result<()> {
        crate::tasks::phase("verifying", None);
        // Independently compare the decoder's whole-archive summary to the
        // parsed listing before any payload is written to PACKIT's output.
        self.run("t", &["-bb0", "-bse2"], None, |stdout| {
            let mut text = String::new();
            stdout
                .take(65537)
                .read_to_string(&mut text)
                .map_err(common::damaged)?;
            if text.len() > 65536 {
                return Err(invalid_listing());
            }
            let (_, summary) = text
                .split_once("Everything is Ok")
                .ok_or_else(invalid_listing)?;
            let value = |key: &str| {
                summary
                    .lines()
                    .find_map(|line| line.strip_prefix(key))
                    .and_then(|number| number.trim().parse::<u64>().ok())
            };
            let files = members
                .iter()
                .filter(|m| m.entry.kind == EntryKind::File)
                .count() as u64;
            let dirs = members.len() as u64 - files;
            let bytes = members
                .iter()
                .map(|m| m.entry.uncompressed_bytes)
                .sum::<u64>();
            if value("Files:").unwrap_or(1) != files
                || value("Folders:").unwrap_or(0) != dirs
                || value("Size:") != Some(bytes)
                || value("Compressed:") != Some(self.length)
            {
                return Err(invalid_listing());
            }
            Ok(())
        })?;
        self.read_members(members, staged, "running")
    }

    // Creation uses nonsolid archives produced from our private snapshot. Read
    // each member once and independently verify its CRC, without an extra `t`
    // pass. Existing untrusted-archive extraction retains its preliminary test.
    fn read_members(&self, members: &[Member], staged: &mut dyn Write, phase: &str) -> Result<()> {
        let entries: Vec<_> = members.iter().map(|m| m.entry.clone()).collect();
        crate::tasks::phase(phase, Some(&entries));
        let mut buffer = vec![0; common::BUFFER];
        for member in members.iter().filter(|m| m.entry.kind == EntryKind::File) {
            crate::tasks::file(&member.entry.path);
            self.run(
                "x",
                &["-so", "-bso0", "-bse2"],
                Some(&member.selection),
                |stdout| {
                    let crc = common::transfer(
                        stdout,
                        member.entry.uncompressed_bytes,
                        &mut buffer,
                        staged,
                    )?;
                    if stdout.read(&mut [0]).map_err(common::damaged)? != 0 || crc != member.crc {
                        return Err(ArchiveError::new(
                            "corrupt_entry",
                            format!("Decoded length or CRC mismatch: {:?}.", member.entry.path),
                        ));
                    }
                    Ok(())
                },
            )?;
            crate::tasks::file_done();
        }
        Ok(())
    }
}

fn timeout_error() -> ArchiveError {
    common::limit("The native archive worker exceeded its 180-second time or 2 GiB output budget.")
}
fn job_error(action: &str) -> ArchiveError {
    ArchiveError::io(action, io::Error::last_os_error())
}

pub(super) struct Job(pub(super) HANDLE);
// A Windows job handle may be shared between the launcher and its watchdog.
unsafe impl Send for Job {}
unsafe impl Sync for Job {}
impl Job {
    pub(super) fn new() -> Result<Arc<Self>> {
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(job_error("Could not create decoder job"));
        }
        let job = Arc::new(Self(job));
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_PROCESS_MEMORY
            | JOB_OBJECT_LIMIT_JOB_MEMORY
            | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        limits.BasicLimitInformation.ActiveProcessLimit = 1;
        limits.ProcessMemoryLimit = 256 << 20;
        limits.JobMemoryLimit = 256 << 20;
        if unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        } == 0
        {
            return Err(job_error("Could not configure decoder job limits"));
        }
        Ok(job)
    }
    pub(super) fn stop(&self) {
        unsafe {
            TerminateJobObject(self.0, 124);
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn decoder_error(bytes: &[u8]) -> ArchiveError {
    let text = String::from_utf8_lossy(bytes);
    let lower = text.to_ascii_lowercase();
    let (code, message) = if lower.contains("password") || lower.contains("encrypted") {
        ("unsupported_encryption", "Encrypted native archives require a secure callback backend; command-line passwords are deliberately disabled.")
    } else if lower.contains("unsupported method") {
        (
            "unsupported_compression",
            "The archive uses an unsupported compression method.",
        )
    } else if lower.contains("memory") {
        (
            "archive_limit",
            "The decoder could not allocate memory within its resource budget.",
        )
    } else if lower.contains("volume") {
        (
            "unsupported_variant",
            "The archive volume set is incomplete or uses an unsupported naming layout.",
        )
    } else {
        (
            "corrupt_entry",
            "The decoder rejected the archive or its data.",
        )
    };
    ArchiveError::new(code, message)
}

fn invalid_listing() -> ArchiveError {
    ArchiveError::new(
        "unsupported_variant",
        "The archive listing has missing, ambiguous or unsupported metadata.",
    )
}

fn properties<'a>(record: &'a str, allowed: &[&str]) -> Result<BTreeMap<&'a str, &'a str>> {
    let mut map = BTreeMap::new();
    for line in record.lines() {
        let (key, value) = line.split_once(" = ").ok_or_else(invalid_listing)?;
        if !allowed.contains(&key)
            || value.chars().any(char::is_control)
            || map.insert(key, value).is_some()
        {
            return Err(invalid_listing());
        }
    }
    Ok(map)
}

fn parse_listing(
    bytes: &[u8],
    kind: &str,
    archive_size: u64,
    volumes: usize,
) -> Result<(Vec<Member>, output::Plan)> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| invalid_listing())?
        .replace("\r\n", "\n");
    let (_, text) = text.split_once("\n--\n").ok_or_else(invalid_listing)?;
    let (header, entries) = text
        .split_once("\n----------\n")
        .ok_or_else(invalid_listing)?;
    let header = properties(
        header,
        &[
            "Path",
            "Type",
            "Physical Size",
            "Headers Size",
            "Method",
            "Solid",
            "Blocks",
            "Characteristics",
            "Encrypted",
            "Multivolume",
            "Volumes",
            "Offset",
            "Tail Size",
            "Total Physical Size",
            "Volume Index",
        ],
    )?;
    if header.get("Encrypted") == Some(&"+") {
        return Err(decoder_error(b"encrypted"));
    }
    if (volumes == 1 && header.get("Multivolume") == Some(&"+"))
        || header
            .get("Volumes")
            .is_some_and(|v| v.parse::<usize>().ok() != Some(volumes))
    {
        return Err(ArchiveError::new(
            "missing_volume",
            "A RAR volume is missing or the supplied set is inconsistent.",
        ));
    }
    if volumes > 1
        && header
            .get("Volume Index")
            .is_some_and(|index| *index != "0")
    {
        return Err(ArchiveError::new(
            "missing_volume",
            "The RAR set does not begin with its first volume.",
        ));
    }
    let physical = header
        .get(if volumes > 1 {
            "Total Physical Size"
        } else {
            "Physical Size"
        })
        .and_then(|s| s.parse::<u64>().ok());
    if header.get("Type") != Some(&kind)
        || header.contains_key("Offset")
        || header.contains_key("Tail Size")
        || physical != Some(archive_size)
    {
        return Err(invalid_listing());
    }
    let mut plan = output::Plan::default();
    let mut members = Vec::new();
    let mut total = 0;
    for record in entries
        .trim_end_matches('\n')
        .split("\n\n")
        .filter(|r| !r.is_empty())
    {
        if !record.starts_with("Path = ") {
            return Err(invalid_listing());
        }
        let fields = properties(
            record,
            &[
                "Path",
                "Folder",
                "Size",
                "Packed Size",
                "Modified",
                "Created",
                "Accessed",
                "Attributes",
                "CRC",
                "Encrypted",
                "Method",
                "Block",
                "Host OS",
                "Solid",
                "Commented",
                "Split Before",
                "Split After",
                "Version",
                "Volume Index",
                "Alternate Stream",
                "Characteristics",
                "Symbolic Link",
                "Hard Link",
                "Copy Link",
                "Checksum",
                "NT Security",
            ],
        )?;
        let get = |key| fields.get(key).copied().unwrap_or_default();
        if get("Encrypted") != "-" {
            return Err(decoder_error(b"encrypted"));
        }
        if volumes == 1
            && ["Split Before", "Split After", "Volume Index"]
                .iter()
                .any(|k| !matches!(get(k), "" | "-"))
        {
            return Err(decoder_error(b"volume"));
        }
        let selection = get("Path");
        if selection.starts_with('@')
            || [
                "Symbolic Link",
                "Hard Link",
                "Copy Link",
                "Alternate Stream",
            ]
            .iter()
            .any(|k| !matches!(get(k), "" | "-"))
        {
            return Err(ArchiveError::new(
                "unsafe_entry",
                "Archive links, list-file selectors and alternate streams are not supported.",
            ));
        }
        let attrs = get("Attributes");
        let mut mode = None;
        let mut directory = get("Folder") == "+";
        for token in attrs.split_ascii_whitespace() {
            if token.len() == 10
                && token.as_bytes()[1..]
                    .iter()
                    .all(|c| b"rwxstST-".contains(c))
            {
                if mode.replace(token.as_bytes()[0]).is_some() {
                    return Err(invalid_listing());
                }
            } else if token.bytes().all(|c| b"RHSDAINCOETP._".contains(&c)) {
                directory |= token.contains('D');
            } else {
                return Err(ArchiveError::new(
                    "unsafe_entry",
                    "An archive entry has unsupported filesystem attributes.",
                ));
            }
        }
        if mode.is_some_and(|m| !matches!(m, b'-' | b'd')) {
            return Err(ArchiveError::new(
                "unsafe_entry",
                "Archive links and special files are unsupported.",
            ));
        }
        directory |= mode == Some(b'd');
        let size = get("Size").parse::<u64>().map_err(|_| invalid_listing())?;
        let method = get("Method");
        if directory && size != 0 {
            return Err(invalid_listing());
        }
        if !method.is_empty() {
            let supported = if kind == "7z" {
                matches!(method.split(':').next(), Some("LZMA" | "LZMA2" | "Copy"))
                    && !method.contains(' ')
            } else {
                method
                    .split(':')
                    .any(|part| matches!(part, "m0" | "m1" | "m2" | "m3" | "m4" | "m5"))
            };
            if !supported {
                return Err(decoder_error(b"unsupported method"));
            }
        } else if size != 0 {
            return Err(invalid_listing());
        }
        let crc_text = get("CRC");
        let crc = if size == 0 && crc_text.is_empty() {
            0
        } else if crc_text.len() == 8 {
            u32::from_str_radix(crc_text, 16).map_err(|_| invalid_listing())?
        } else {
            return Err(ArchiveError::new(
                "unsupported_variant",
                "A file lacks a usable CRC32 for independent verification.",
            ));
        };
        let packed = get("Packed Size");
        let entry = ArchiveEntry {
            path: selection.replace('\\', "/"),
            kind: if directory {
                EntryKind::Directory
            } else {
                EntryKind::File
            },
            compressed_bytes: if packed.is_empty() {
                None
            } else {
                Some(packed.parse().map_err(|_| invalid_listing())?)
            },
            uncompressed_bytes: size,
            method: method.into(),
            modified: fields
                .get("Modified")
                .filter(|s| !s.is_empty())
                .map(|s| (*s).to_owned()),
            issue: None,
        };
        if members.len() >= MAX_ENTRIES || size > MAX_ENTRY || total + size > MAX_ARCHIVE {
            return Err(common::limit(
                "Native archives are limited to 512 entries, 512 MiB per file and 2 GiB total.",
            ));
        }
        common::add(&mut plan, &entry, members.len(), &mut total)?;
        if total >= 1 << 20 && total > archive_size.saturating_mul(1000) {
            return Err(common::limit(
                "The declared expansion exceeds the 1,000:1 limit.",
            ));
        }
        members.push(Member {
            entry,
            selection: selection.to_owned(),
            crc,
        });
    }
    Ok((members, plan))
}

impl ArchiveBackend for SevenZip {
    fn output_name(&self) -> String {
        output::output_folder(
            &self.name,
            if self.kind == "7z" {
                &[".7z"]
            } else {
                &[".rar"]
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
        Ok(self
            .members()?
            .0
            .into_iter()
            .map(|member| member.entry)
            .collect())
    }
    fn extract(&mut self, destination: &Path, policy: OverwritePolicy) -> Result<OperationReport> {
        let (members, plan) = self.members()?;
        let mut staged = tempfile::tempfile_in(self.root.path())
            .map_err(|e| ArchiveError::io("Could not open private payload staging", e))?;
        self.decode(&members, &mut staged)?;
        staged.rewind().map_err(common::damaged)?;
        crate::tasks::phase("committing", None);
        let (name, count) =
            output::extract_to(destination, &self.output_name(), policy, &plan, |out| {
                let count = plan.create_directories(out)?;
                let mut buffer = vec![0; common::BUFFER];
                for member in members.iter().filter(|m| m.entry.kind == EntryKind::File) {
                    let crc =
                        common::entry_data(&mut staged, &member.entry, Some(out), &mut buffer)?;
                    if crc != member.crc {
                        return Err(ArchiveError::new(
                            "corrupt_entry",
                            "Staged payload CRC mismatch.",
                        ));
                    }
                }
                Ok(count)
            })?;
        let entries: Vec<_> = members.into_iter().map(|member| member.entry).collect();
        Ok(common::report(
            &entries,
            count,
            Some(destination.join(name).to_string_lossy().into_owned()),
        ))
    }
    fn test(&mut self) -> Result<OperationReport> {
        let (members, _) = self.members()?;
        self.decode(&members, &mut io::sink())?;
        let entries: Vec<_> = members.into_iter().map(|member| member.entry).collect();
        Ok(common::report(
            &entries,
            entries
                .iter()
                .filter(|e| e.kind == EntryKind::Directory)
                .count(),
            None,
        ))
    }
    fn verify_created(&mut self) -> Result<OperationReport> {
        let (members, _) = self.members()?;
        self.read_members(&members, &mut io::sink(), "verifying")?;
        let entries: Vec<_> = members.into_iter().map(|member| member.entry).collect();
        Ok(common::report(
            &entries,
            entries
                .iter()
                .filter(|entry| entry.kind == EntryKind::Directory)
                .count(),
            None,
        ))
    }
    fn capabilities(&self) -> FormatCapabilities {
        FormatCapabilities {
            format: match self.kind { "7z" => "7z", "Rar" => "RAR4", _ => "RAR5" },
            zip64: false, inspection: true, extraction: true, integrity: Integrity::Checksums,
            compression_methods: if self.kind == "7z" { &["Copy", "LZMA", "LZMA2"] } else { &["Stored", "RAR compression"] },
            encryption: false, multipart: false,
            limitations: "Windows x64. Regular files and folders with CRC32 only; no links or encryption. Numbered split 7z and .partN.rar sets only.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn cancellation_terminates_a_native_decoder_blocked_on_output() {
        let bytes = include_bytes!("../../tests/fixtures/cancellation-stream.7z");
        let mut source = tempfile::tempfile().unwrap();
        source.write_all(bytes).unwrap();
        crate::tasks::test_context(None, None, |control| {
            let backend =
                SevenZip::open(source, "cancellation.7z".into(), bytes.len() as u64, "7z").unwrap();
            let start = Instant::now();
            let mut received = Vec::new();
            let result = backend.run(
                "x",
                &["-so", "-bso0", "-bse2"],
                Some("large.dat"),
                |stdout| {
                    control.cancelled.store(true, Ordering::Release);
                    // Keep the small OS pipe full until the watchdog terminates
                    // the real decoder; its one-MiB output cannot fit in the pipe.
                    thread::sleep(Duration::from_millis(150));
                    stdout.read_to_end(&mut received).map_err(common::damaged)
                },
            );
            assert_eq!(result.unwrap_err().code, "cancelled");
            assert!(start.elapsed() < Duration::from_secs(3));
            assert!(received.len() < 1024 * 1024);
        });
    }
}

#[cfg(test)]
#[path = "license_release_probe.rs"]
mod license_release_probe;
