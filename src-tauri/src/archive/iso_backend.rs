// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
//! Bounded, read-only single-volume ISO 9660/Joliet reader. No mounting.
use super::{
    output, streaming as common, ArchiveBackend, ArchiveEntry, ArchiveError, ArchiveInspection,
    EntryKind, FormatCapabilities, Integrity, OperationReport, OverwritePolicy, Result,
};
use std::{
    collections::{BTreeMap, VecDeque},
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
const SECTOR: u64 = 2048;
const MAX_DIRECTORY: u64 = 64 << 20;
const MAX_DIRECTORY_TOTAL: u64 = 256 << 20;
fn invalid() -> ArchiveError {
    ArchiveError::new(
        "invalid_iso",
        "Malformed, truncated or inconsistent ISO image.",
    )
}
fn unsupported() -> ArchiveError {
    ArchiveError::new("unsupported_variant", "Unsupported ISO layout or Rock Ridge extension (UDF, multi-extent, relocation, continuation and links are disabled).")
}
fn both32(b: &[u8]) -> Result<u64> {
    if b.len() < 8 {
        return Err(invalid());
    }
    let a = u32::from_le_bytes(b[..4].try_into().unwrap());
    if a != u32::from_be_bytes(b[4..8].try_into().unwrap()) {
        return Err(invalid());
    }
    Ok(a.into())
}
fn both16(b: &[u8]) -> Result<u16> {
    if b.len() < 4 {
        return Err(invalid());
    }
    let a = u16::from_le_bytes(b[..2].try_into().unwrap());
    if a != u16::from_be_bytes(b[2..4].try_into().unwrap()) {
        return Err(invalid());
    }
    Ok(a)
}
pub(super) fn probe(file: &mut File) -> Result<bool> {
    if file.metadata().map_err(common::damaged)?.len() < 17 * SECTOR {
        return Ok(false);
    }
    file.seek(SeekFrom::Start(16 * SECTOR))
        .map_err(common::damaged)?;
    let mut b = [0; 7];
    file.read_exact(&mut b).map_err(common::damaged)?;
    file.rewind().map_err(common::damaged)?;
    Ok(&b[1..6] == b"CD001" || matches!(&b[1..6], b"BEA01" | b"NSR02" | b"NSR03"))
}
struct Member {
    entry: ArchiveEntry,
    offset: u64,
}
pub(super) struct IsoBackend {
    file: File,
    name: String,
    length: u64,
}
impl IsoBackend {
    pub fn new(file: File, name: String, length: u64) -> Self {
        Self { file, name, length }
    }
    fn read(&mut self, offset: u64, size: usize) -> Result<Vec<u8>> {
        crate::tasks::check()?;
        if offset
            .checked_add(size as u64)
            .is_none_or(|end| end > self.length)
        {
            return Err(invalid());
        }
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(common::damaged)?;
        let mut data = vec![0; size];
        self.file.read_exact(&mut data).map_err(common::damaged)?;
        Ok(data)
    }
    fn members(&mut self) -> Result<(Vec<Member>, output::Plan)> {
        // A UDF bridge can put its VRS after the ISO terminator and expose
        // only a README in the ISO tree. Never mistake that stub for content.
        for sector in 16..80 {
            if sector * SECTOR + 7 > self.length {
                break;
            }
            let signature = self.read(sector * SECTOR, 7)?;
            if signature[0] == 0
                && signature[6] == 1
                && matches!(&signature[1..6], b"NSR02" | b"NSR03")
            {
                return Err(ArchiveError::new("unsupported_udf", "This image contains a UDF filesystem, which PACKIT does not yet support. Its ISO compatibility tree may contain only a README; it is not the full image."));
            }
        }
        let mut primary = None;
        let mut joliet = None;
        let mut end = None;
        for sector in 16..80 {
            let b = self.read(sector * SECTOR, 2048)?;
            if matches!(&b[1..6], b"BEA01" | b"NSR02" | b"NSR03" | b"TEA01") {
                return Err(ArchiveError::new(
                    "unsupported_udf",
                    "UDF-only disk images are not supported. Use an ISO 9660/Joliet image.",
                ));
            }
            if &b[1..6] != b"CD001" || b[6] != 1 {
                return Err(unsupported());
            }
            match b[0] {
                1 if primary.is_none() => primary = Some(b),
                2 if matches!(&b[88..91], b"%/@" | b"%/C" | b"%/E") => joliet = Some(b),
                255 => {
                    end = Some((sector + 1) * SECTOR);
                    break;
                }
                0 => {}
                _ => return Err(unsupported()),
            }
        }
        let end = end.ok_or_else(invalid)?;
        let primary = primary.ok_or_else(invalid)?;
        let root_record = &primary[156..190];
        let root_start = both32(&root_record[2..10])? * SECTOR;
        let dot = self.read(root_start, 256)?;
        let rr = dot.first().copied().unwrap_or(0) >= 41
            && dot
                .get(34..41)
                .is_some_and(|b| b.starts_with(b"SP\x07\x01\xbe\xef"));
        let unicode = !rr && joliet.is_some();
        let descriptor = if unicode {
            joliet.as_ref().unwrap()
        } else {
            &primary
        };
        let volume = both32(&descriptor[80..88])?
            .checked_mul(SECTOR)
            .ok_or_else(invalid)?;
        if !self.length.is_multiple_of(SECTOR)
            || volume > self.length
            || volume < end
            || both16(&descriptor[128..132])? != 2048
            || both16(&descriptor[120..124])? != 1
            || both16(&descriptor[124..128])? != 1
        {
            return Err(unsupported());
        }
        let root = &descriptor[156..190];
        if root[0] != 34
            || root[1] != 0
            || root[25] != 2
            || root[26] != 0
            || root[27] != 0
            || root[32] != 1
            || root[33] != 0
            || both16(&root[28..32])? != 1
        {
            return Err(unsupported());
        }
        let root_offset = both32(&root[2..10])? * SECTOR;
        let root_size = both32(&root[10..18])?;
        let mut ranges = BTreeMap::new();
        ranges.insert(0, end);
        // Path tables are metadata, never file content.
        let table_size = both32(&descriptor[132..140])?;
        for (at, little) in [(140, true), (144, true), (148, false), (152, false)] {
            let raw: [u8; 4] = descriptor[at..at + 4].try_into().unwrap();
            let lba = if little {
                u32::from_le_bytes(raw)
            } else {
                u32::from_be_bytes(raw)
            };
            if lba != 0 && table_size != 0 {
                reserve(&mut ranges, u64::from(lba) * SECTOR, table_size, volume)?;
            }
        }
        reserve(&mut ranges, root_offset, root_size, volume)?;
        let mut queue = VecDeque::from([(
            String::new(),
            root_offset,
            root_size,
            root_offset,
            root_size,
        )]);
        let mut members = Vec::new();
        let mut plan = output::Plan::default();
        let mut total = 0;
        let mut directory_bytes = 0_u64;
        while let Some((prefix, offset, size, parent_offset, parent_size)) = queue.pop_front() {
            directory_bytes = directory_bytes.saturating_add(size);
            if size < 68 {
                return Err(invalid());
            }
            if size > MAX_DIRECTORY {
                return Err(common::limit(format!("ISO directory {prefix:?} declares {size} bytes; the per-directory limit is 64 MiB.").as_str()));
            }
            if directory_bytes > MAX_DIRECTORY_TOTAL {
                return Err(common::limit(
                    "ISO directory metadata exceeds the 256 MiB total scanning budget.",
                ));
            }
            let mut dots = 0;
            // ISO directory data lengths need not include the last sector's
            // padding. Records still cannot cross logical-sector boundaries.
            for relative in (0..size).step_by(SECTOR as usize) {
                let bytes = self.read(offset + relative, (size - relative).min(SECTOR) as usize)?;
                let mut pos = 0;
                while pos < bytes.len() {
                    crate::tasks::check()?;
                    let len = bytes[pos] as usize;
                    if len == 0 {
                        let next = ((pos / 2048) + 1) * 2048;
                        if bytes[pos..next.min(bytes.len())].iter().any(|b| *b != 0) {
                            return Err(invalid());
                        }
                        pos = next;
                        continue;
                    }
                    if len < 34 || pos + len > bytes.len() || pos % 2048 + len > 2048 {
                        return Err(invalid());
                    }
                    let record = &bytes[pos..pos + len];
                    pos += len;
                    let n = record[32] as usize;
                    if n == 0
                        || 33 + n > len
                        || record[1] != 0
                        || record[26] != 0
                        || record[27] != 0
                        || record[25] & !3 != 0
                        || both16(&record[28..32])? != 1
                    {
                        return Err(unsupported());
                    }
                    let start = both32(&record[2..10])? * SECTOR;
                    let length = both32(&record[10..18])?;
                    if start.checked_add(length).is_none_or(|end| end > volume) {
                        return Err(invalid());
                    }
                    let kind = if record[25] & 2 != 0 {
                        EntryKind::Directory
                    } else {
                        EntryKind::File
                    };
                    let special = n == 1 && record[33] <= 1;
                    if special {
                        let (expected_start, expected_size) = if dots == 0 {
                            (offset, size)
                        } else {
                            (parent_offset, parent_size)
                        };
                        if dots > 1
                            || record[33] != dots
                            || kind != EntryKind::Directory
                            || start != expected_start
                            || length != expected_size
                        {
                            return Err(invalid());
                        }
                        dots += 1;
                        continue;
                    }
                    if dots != 2 {
                        return Err(invalid());
                    }
                    let raw = &record[33..33 + n];
                    let mut name = if unicode {
                        if !n.is_multiple_of(2) {
                            return Err(invalid());
                        }
                        String::from_utf16(
                            &raw.as_chunks::<2>()
                                .0
                                .iter()
                                .map(|p| u16::from_be_bytes([p[0], p[1]]))
                                .collect::<Vec<_>>(),
                        )
                        .map_err(|_| invalid())?
                    } else {
                        std::str::from_utf8(raw)
                            .map_err(|_| unsupported())?
                            .to_owned()
                    };
                    if rr {
                        let skip = dot.get(40).copied().unwrap_or(0) as usize;
                        let system = 33 + n + usize::from(n.is_multiple_of(2)) + skip;
                        if system > record.len() {
                            return Err(invalid());
                        }
                        if let Some(extended) = rock_ridge(&record[system..], kind)? {
                            name = extended;
                        }
                    }
                    if name.contains('/') || name.contains('\\') {
                        return Err(ArchiveError::new("unsafe_path", "Unsafe ISO filename."));
                    }
                    if let Some((stem, version)) = name.rsplit_once(';').filter(|_| !rr) {
                        if !version.is_empty() && version.bytes().all(|b| b.is_ascii_digit()) {
                            name = stem.to_owned();
                        }
                    }
                    if !rr && !unicode {
                        name = name.trim_end_matches('.').to_owned();
                    }
                    let path = if prefix.is_empty() {
                        name
                    } else {
                        format!("{prefix}/{name}")
                    };
                    let entry = ArchiveEntry {
                        path: path.clone(),
                        kind,
                        compressed_bytes: None,
                        uncompressed_bytes: if kind == EntryKind::File { length } else { 0 },
                        method: "Stored".into(),
                        modified: iso_time(&record[18..25])?,
                        issue: None,
                    };
                    common::add(&mut plan, &entry, members.len(), &mut total)?;
                    reserve(&mut ranges, start, length, volume)?;
                    if kind == EntryKind::Directory {
                        queue.push_back((path, start, length, offset, size));
                    }
                    members.push(Member {
                        entry,
                        offset: start,
                    });
                }
            }
            if dots != 2 {
                return Err(invalid());
            }
        }
        Ok((members, plan))
    }
    fn stream(&mut self, members: &[Member], mut out: Option<&mut output::Output>) -> Result<()> {
        let entries: Vec<_> = members.iter().map(|m| m.entry.clone()).collect();
        crate::tasks::phase(
            if out.is_some() {
                "running"
            } else {
                "verifying"
            },
            Some(&entries),
        );
        let mut buffer = vec![0; common::BUFFER];
        for member in members.iter().filter(|m| m.entry.kind == EntryKind::File) {
            self.file
                .seek(SeekFrom::Start(member.offset))
                .map_err(common::damaged)?;
            common::entry_data(
                &mut self.file,
                &member.entry,
                out.as_deref_mut(),
                &mut buffer,
            )?;
        }
        Ok(())
    }
}
fn iso_time(time: &[u8]) -> Result<Option<String>> {
    if time.iter().all(|b| *b == 0) {
        return Ok(None);
    }
    if !(1..=12).contains(&time[1])
        || !(1..=31).contains(&time[2])
        || time[3] > 23
        || time[4] > 59
        || time[5] > 59
        || !(-48..=52).contains(&(time[6] as i8))
    {
        return Err(invalid());
    }
    Ok(Some(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} (UTC offset {} min)",
        1900 + u16::from(time[0]),
        time[1],
        time[2],
        time[3],
        time[4],
        time[5],
        i16::from(time[6] as i8) * 15
    )))
}
fn reserve(ranges: &mut BTreeMap<u64, u64>, start: u64, size: u64, volume: u64) -> Result<()> {
    if size == 0 {
        return if start <= volume {
            Ok(())
        } else {
            Err(invalid())
        };
    }
    let end = start.checked_add(size).ok_or_else(invalid)?;
    if end > volume
        || ranges
            .range(..end)
            .next_back()
            .is_some_and(|(_, old_end)| *old_end > start)
    {
        return Err(invalid());
    }
    ranges.insert(start, end);
    Ok(())
}
fn rock_ridge(mut bytes: &[u8], kind: EntryKind) -> Result<Option<String>> {
    let mut name = Vec::new();
    let mut seen = false;
    let mut continuation = false;
    while !bytes.is_empty() && bytes.iter().any(|b| *b != 0) {
        if bytes.len() < 4 || bytes[2] < 4 || bytes[2] as usize > bytes.len() || bytes[3] != 1 {
            return Err(invalid());
        }
        let record = &bytes[..bytes[2] as usize];
        bytes = &bytes[record.len()..];
        match &record[..2] {
            b"NM" => {
                if record.len() < 5 || record[4] & !1 != 0 || (seen && !continuation) {
                    return Err(unsupported());
                }
                name.extend_from_slice(&record[5..]);
                seen = true;
                continuation = record[4] & 1 != 0;
                if name.len() > 1024 {
                    return Err(invalid());
                }
            }
            b"PX" => {
                if record.len() != 36 && record.len() != 44 {
                    return Err(invalid());
                }
                let mode = both32(&record[4..12])? as u32 & 0o170000;
                if mode
                    != if kind == EntryKind::Directory {
                        0o040000
                    } else {
                        0o100000
                    }
                {
                    return Err(ArchiveError::new(
                        "unsafe_entry",
                        "ISO links and special files are disabled.",
                    ));
                }
            }
            b"ST" => break,
            b"RR" | b"TF" | b"SP" | b"ER" | b"PD" => {}
            _ => return Err(unsupported()),
        }
    }
    if continuation {
        return Err(invalid());
    }
    if seen {
        Ok(Some(String::from_utf8(name).map_err(|_| invalid())?))
    } else {
        Ok(None)
    }
}
impl ArchiveBackend for IsoBackend {
    fn output_name(&self) -> String {
        output::output_folder(&self.name, &[".iso"])
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
        Ok(self.members()?.0.into_iter().map(|m| m.entry).collect())
    }
    fn extract(&mut self, destination: &Path, policy: OverwritePolicy) -> Result<OperationReport> {
        let (members, plan) = self.members()?;
        let (name, count) =
            output::extract_to(destination, &self.output_name(), policy, &plan, |out| {
                let count = plan.create_directories(out)?;
                self.stream(&members, Some(out))?;
                Ok(count)
            })?;
        Ok(common::report(
            &members.into_iter().map(|m| m.entry).collect::<Vec<_>>(),
            count,
            Some(destination.join(name).to_string_lossy().into_owned()),
        ))
    }
    fn test(&mut self) -> Result<OperationReport> {
        let (members, _) = self.members()?;
        self.stream(&members, None)?;
        let entries: Vec<_> = members.into_iter().map(|m| m.entry).collect();
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
        FormatCapabilities{format:"ISO",zip64:false,inspection:true,extraction:true,integrity:Integrity::Structure,limitations:"Single-volume ISO 9660, Joliet and basic inline Rock Ridge; sector-streamed directories (64 MiB each, 256 MiB total). No UDF, continuation/relocation, links, interleaving or multi-extent files. Structural/readability checks only; no file-content checksums.",compression_methods:&["Stored"],encryption:false,multipart:false}
    }
}
