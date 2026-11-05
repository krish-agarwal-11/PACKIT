// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
//! Conservative read-only UDF: physical type-1 partition maps, FE/EFE and
//! recorded short/long/embedded allocation descriptors. No mounting or codecs.
use super::{
    output, streaming as common, ArchiveBackend, ArchiveEntry, ArchiveError, ArchiveInspection,
    EntryKind, FormatCapabilities, Integrity, OperationReport, OverwritePolicy, Result,
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};
const BLOCK: u64 = 2048;
const MAX_META: u64 = 128 << 20;
const MAX_DIR: u64 = 32 << 20;
fn invalid() -> ArchiveError {
    ArchiveError::new(
        "invalid_udf",
        "Malformed, truncated or inconsistent UDF metadata or extent.",
    )
}
fn unsupported() -> ArchiveError {
    ArchiveError::new("unsupported_udf","Unsupported UDF layout: only physical single-volume partitions with recorded short/long or embedded extents are enabled; no VAT, sparing, metadata partitions, links, streams or indirect allocation.")
}
fn u16le(b: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        b.get(at..at + 2).ok_or_else(invalid)?.try_into().unwrap(),
    ))
}
fn u32le(b: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        b.get(at..at + 4).ok_or_else(invalid)?.try_into().unwrap(),
    ))
}
fn u64le(b: &[u8], at: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(
        b.get(at..at + 8).ok_or_else(invalid)?.try_into().unwrap(),
    ))
}
fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0_u16;
    for b in bytes {
        crc ^= u16::from(*b) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}
fn tag(b: &[u8], location: u32, id: u16) -> Result<()> {
    if b.len() < 16
        || u16le(b, 0)? != id
        || !matches!(u16le(b, 2)?, 2 | 3)
        || b[5] != 0
        || u32le(b, 12)? != location
    {
        return Err(invalid());
    }
    let checksum = b[..16]
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 4)
        .fold(0_u8, |s, (_, b)| s.wrapping_add(*b));
    let length = u16le(b, 10)? as usize;
    if b[4] != checksum
        || length == 0
        || length > b.len() - 16
        || crc16(&b[16..16 + length]) != u16le(b, 8)?
    {
        return Err(invalid());
    }
    Ok(())
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Icb {
    part: u16,
    block: u32,
}
fn long_ad(bytes: &[u8], at: usize) -> Result<(Icb, u32)> {
    let length = u32le(bytes, at)?;
    if length >> 30 != 0 {
        return Err(unsupported());
    }
    Ok((
        Icb {
            block: u32le(bytes, at + 4)?,
            part: u16le(bytes, at + 8)?,
        },
        length,
    ))
}
#[derive(Clone)]
struct Extent {
    offset: u64,
    length: u64,
    logical: u32,
}
#[derive(Clone)]
struct Node {
    kind: EntryKind,
    size: u64,
    extents: Vec<Extent>,
    embedded: Vec<u8>,
    icb: Icb,
    links: u16,
}
pub(super) fn probe(file: &mut File) -> Result<bool> {
    let length = file.metadata().map_err(common::damaged)?.len();
    for sector in 16..80 {
        if sector * BLOCK + 7 > length {
            break;
        }
        file.seek(SeekFrom::Start(sector * BLOCK))
            .map_err(common::damaged)?;
        let mut b = [0; 7];
        file.read_exact(&mut b).map_err(common::damaged)?;
        if b[0] == 0 && b[6] == 1 && matches!(&b[1..6], b"NSR02" | b"NSR03") {
            file.rewind().map_err(common::damaged)?;
            return Ok(true);
        }
    }
    file.rewind().map_err(common::damaged)?;
    Ok(false)
}
struct Member {
    entry: ArchiveEntry,
    node: Node,
}
struct Volume {
    maps: Vec<(u64, u64)>,
    root: Icb,
    fsd: Icb,
    fsd_length: u32,
    revision: u16,
}
pub(super) struct UdfBackend {
    file: File,
    name: String,
    length: u64,
    metadata: u64,
}
impl UdfBackend {
    pub fn new(file: File, name: String, length: u64) -> Self {
        Self {
            file,
            name,
            length,
            metadata: 0,
        }
    }
    fn read(&mut self, offset: u64, length: usize) -> Result<Vec<u8>> {
        crate::tasks::check()?;
        self.metadata = self.metadata.saturating_add(length as u64);
        if self.metadata > MAX_META {
            return Err(common::limit(
                "UDF metadata exceeds its 128 MiB scanning budget.",
            ));
        }
        if offset
            .checked_add(length as u64)
            .is_none_or(|end| end > self.length)
        {
            return Err(invalid());
        }
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(common::damaged)?;
        let mut b = vec![0; length];
        self.file.read_exact(&mut b).map_err(common::damaged)?;
        Ok(b)
    }
    fn volume(&mut self) -> Result<Volume> {
        let a = self.read(256 * BLOCK, BLOCK as usize)?;
        tag(&a, 256, 2)?;
        if 16 + usize::from(u16le(&a, 10)?) < 512 {
            return Err(invalid());
        }
        let length = u32le(&a, 16)? as u64;
        let start = u32le(&a, 20)?;
        if length == 0 || length > 4 << 20 || !length.is_multiple_of(BLOCK) || u64::from(start) < 16
        {
            return Err(unsupported());
        }
        let mut logical = None;
        let mut partitions = BTreeMap::new();
        let mut terminated = false;
        for index in 0..length / BLOCK {
            let sector = u64::from(start) + index;
            let b = self.read(sector * BLOCK, BLOCK as usize)?;
            let id = u16le(&b, 0)?;
            tag(&b, sector as u32, id)?;
            match id {
                5 => {
                    let number = u16le(&b, 22)?;
                    let begin = u32le(&b, 188)? as u64;
                    let count = u32le(&b, 192)? as u64;
                    if 16 + usize::from(u16le(&b, 10)?) < 196 {
                        return Err(invalid());
                    }
                    if count == 0
                        || begin
                            .checked_add(count)
                            .is_none_or(|end| end > self.length / BLOCK)
                        || u16le(&b, 20)? & 1 == 0
                        || partitions.insert(number, (begin, count)).is_some()
                    {
                        return Err(invalid());
                    }
                }
                6 => {
                    if logical.replace(b).is_some() {
                        return Err(unsupported());
                    }
                }
                8 => {
                    terminated = true;
                    break;
                }
                1 | 4 | 7 => {}
                _ => return Err(unsupported()),
            }
        }
        if !terminated {
            return Err(invalid());
        }
        let l = logical.ok_or_else(invalid)?;
        if u32le(&l, 212)? != BLOCK as u32 || &l[217..236] != b"*OSTA UDF Compliant" {
            return Err(unsupported());
        }
        let revision = u16le(&l, 240)?;
        if !matches!(revision, 0x0102) {
            return Err(ArchiveError::new(
                "unsupported_udf",
                "This UDF revision is not enabled.",
            ));
        }
        let map_length = u32le(&l, 264)? as usize;
        let count = u32le(&l, 268)? as usize;
        if count == 0 || count > 16 || 440 + map_length > l.len() {
            return Err(unsupported());
        }
        let mut maps = Vec::new();
        let mut at = 440;
        for _ in 0..count {
            if at + 6 > 440 + map_length || l[at] != 1 || l[at + 1] != 6 || u16le(&l, at + 2)? != 1
            {
                return Err(unsupported());
            }
            maps.push(*partitions.get(&u16le(&l, at + 4)?).ok_or_else(invalid)?);
            at += 6;
        }
        if at != 440 + map_length {
            return Err(invalid());
        }
        if 16 + usize::from(u16le(&l, 10)?) < 440 + map_length {
            return Err(invalid());
        }
        let metadata_end = (u64::from(start) + length / BLOCK).max(257);
        if maps.iter().any(|(start, _)| *start < metadata_end) {
            return Err(unsupported());
        }
        let (fsd, fsd_length) = long_ad(&l, 248)?;
        if fsd_length < BLOCK as u32 {
            return Err(invalid());
        }
        let mut volume = Volume {
            maps,
            root: fsd,
            fsd,
            fsd_length,
            revision,
        };
        let address = volume.address(fsd, BLOCK)?;
        let descriptor = self.read(address, BLOCK as usize)?;
        tag(&descriptor, fsd.block, 256)?;
        if 16 + usize::from(u16le(&descriptor, 10)?) < 480
            || u32le(&descriptor, 40)? != 0
            || u32le(&descriptor, 44)? != 0
            || u32le(&descriptor, 448)? != 0
            || u32le(&descriptor, 464)? != 0
        {
            return Err(unsupported());
        }
        let (root, length) = long_ad(&descriptor, 400)?;
        if length != BLOCK as u32 {
            return Err(invalid());
        }
        volume.root = root;
        Ok(volume)
    }
    fn node(&mut self, volume: &Volume, icb: Icb, ranges: &mut BTreeMap<u64, u64>) -> Result<Node> {
        let offset = volume.address(icb, BLOCK)?;
        reserve(ranges, offset, BLOCK)?;
        let b = self.read(offset, BLOCK as usize)?;
        let id = u16le(&b, 0)?;
        if !matches!(id, 261 | 266) {
            return Err(unsupported());
        }
        tag(&b, icb.block, id)?;
        if u16le(&b, 20)? != 4 || u16le(&b, 24)? != 1 {
            return Err(unsupported());
        }
        let kind = match b[27] {
            4 => EntryKind::Directory,
            5 => EntryKind::File,
            _ => {
                return Err(ArchiveError::new(
                    "unsafe_entry",
                    "UDF links, devices and special files are disabled.",
                ))
            }
        };
        let links = u16le(&b, 48)?;
        if links == 0 {
            return Err(invalid());
        }
        let size = u64le(&b, 56)?;
        if size > common::MAX_FILE {
            return Err(common::limit("UDF file exceeds the 8 GiB per-file limit."));
        }
        let (base, ea, alloc) = if id == 261 {
            (176, u32le(&b, 168)? as usize, u32le(&b, 172)? as usize)
        } else {
            if u32le(&b, 152)? != 0 {
                return Err(unsupported());
            }
            (216, u32le(&b, 208)? as usize, u32le(&b, 212)? as usize)
        };
        let start = base + ea;
        let end = start.checked_add(alloc).ok_or_else(invalid)?;
        if end > b.len() || 16 + (u16le(&b, 10)? as usize) < end {
            return Err(invalid());
        }
        let flags = u16le(&b, 34)?;
        if flags & !0x07ff != 0 {
            return Err(unsupported());
        }
        let mode = flags & 7;
        let mut extents = Vec::new();
        let mut embedded = Vec::new();
        let mut total = 0_u64;
        if mode == 3 {
            if size != alloc as u64 {
                return Err(invalid());
            }
            embedded.extend_from_slice(&b[start..end]);
        } else {
            let step = match mode {
                0 => 8,
                1 => 16,
                _ => return Err(unsupported()),
            };
            if !alloc.is_multiple_of(step) {
                return Err(invalid());
            }
            for at in (start..end).step_by(step) {
                let length = u32le(&b, at)?;
                if length == 0 {
                    continue;
                }
                if length >> 30 != 0 {
                    return Err(unsupported());
                }
                let location = Icb {
                    block: u32le(&b, at + 4)?,
                    part: if mode == 0 {
                        icb.part
                    } else {
                        u16le(&b, at + 8)?
                    },
                };
                let address = volume.address(location, length as u64)?;
                reserve(ranges, address, length as u64)?;
                total = total.checked_add(length as u64).ok_or_else(invalid)?;
                extents.push(Extent {
                    offset: address,
                    length: length as u64,
                    logical: location.block,
                });
            }
            if total < size || total - size >= BLOCK || extents.len() > 256 {
                return Err(invalid());
            }
        }
        let recorded = u64le(&b, if id == 261 { 64 } else { 72 })?;
        let expected = if mode == 3 {
            0
        } else {
            extents
                .iter()
                .map(|extent| extent.length.div_ceil(BLOCK))
                .sum()
        };
        if recorded != expected {
            return Err(invalid());
        }
        Ok(Node {
            kind,
            size,
            extents,
            embedded,
            icb,
            links,
        })
    }
    fn members(&mut self) -> Result<(Vec<Member>, output::Plan, Volume)> {
        self.metadata = 0;
        let volume = self.volume()?;
        let mut ranges = BTreeMap::new();
        reserve(
            &mut ranges,
            volume.address(volume.fsd, u64::from(volume.fsd_length))?,
            u64::from(volume.fsd_length),
        )?;
        let mut seen = BTreeSet::new();
        let mut files: BTreeMap<Icb, (Node, u16)> = BTreeMap::new();
        let root = self.node(&volume, volume.root, &mut ranges)?;
        if root.kind != EntryKind::Directory {
            return Err(invalid());
        }
        seen.insert(volume.root);
        let mut queue = VecDeque::from([(String::new(), root, volume.root)]);
        let mut members = Vec::new();
        let mut plan = output::Plan::default();
        let mut total = 0_u64;
        while let Some((prefix, directory, parent)) = queue.pop_front() {
            if directory.size > MAX_DIR {
                return Err(common::limit("UDF directory exceeds its 32 MiB limit."));
            }
            let mut at = 0_u64;
            while at < directory.size {
                crate::tasks::check()?;
                if directory.size - at < 38 {
                    return Err(invalid());
                }
                let mut header = self.node_bytes(&directory, at, 38)?;
                let implementation = u16le(&header, 36)? as usize;
                let name_length = header[19] as usize;
                let length = (38 + implementation + name_length).next_multiple_of(4);
                if length > BLOCK as usize || at + length as u64 > directory.size {
                    return Err(invalid());
                }
                header.extend(self.node_bytes(&directory, at + 38, length - 38)?);
                let location = directory.logical(at)?;
                tag(&header, location, 257)?;
                if usize::from(u16le(&header, 10)?) + 16 != length {
                    return Err(invalid());
                }
                let flags = header[18];
                if flags & !0x0b != 0 || u16le(&header, 16)? != 1 {
                    return Err(unsupported());
                }
                let (child, extent_length) = long_ad(&header, 20)?;
                if extent_length != BLOCK as u32 {
                    return Err(invalid());
                }
                if flags & 8 != 0 {
                    if name_length != 0 || child != parent {
                        return Err(invalid());
                    }
                    at += length as u64;
                    continue;
                }
                let name =
                    decode_name(&header[38 + implementation..38 + implementation + name_length])?;
                if name.contains('/') || name.contains('\\') {
                    return Err(ArchiveError::new("unsafe_path", "Unsafe UDF filename."));
                }
                let path = if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                };
                let node = if let Some((node, references)) = files.get_mut(&child) {
                    *references = references.checked_add(1).ok_or_else(invalid)?;
                    if *references > node.links {
                        return Err(invalid());
                    }
                    node.clone()
                } else {
                    if !seen.insert(child) {
                        return Err(ArchiveError::new(
                            "unsafe_entry",
                            "UDF directory cycles or aliases are disabled.",
                        ));
                    }
                    let node = self.node(&volume, child, &mut ranges)?;
                    if node.kind == EntryKind::File {
                        files.insert(child, (node.clone(), 1));
                    }
                    node
                };
                if (flags & 2 != 0) != (node.kind == EntryKind::Directory) {
                    return Err(invalid());
                }
                let entry = ArchiveEntry {
                    path: path.clone(),
                    kind: node.kind,
                    compressed_bytes: None,
                    uncompressed_bytes: if node.kind == EntryKind::File {
                        node.size
                    } else {
                        0
                    },
                    method: "Stored".into(),
                    modified: None,
                    issue: None,
                };
                // Format-specific bounded budget for ordinary software discs.
                // Existing ZIP/TAR/ISO limits remain unchanged.
                if members.len() >= 32_000
                    || total.saturating_add(entry.uncompressed_bytes) > common::MAX_TOTAL
                {
                    return Err(common::limit("UDF exceeds 32,000 entries or 16 GiB total output, including regular-file aliases."));
                }
                total += entry.uncompressed_bytes;
                plan.add(&entry.path, entry.kind)?;
                if node.kind == EntryKind::Directory {
                    queue.push_back((path, node.clone(), directory.icb));
                }
                members.push(Member { entry, node });
                at += length as u64;
            }
        }
        if files.values().any(|(node, count)| node.links != *count) {
            return Err(ArchiveError::new(
                "invalid_udf",
                "UDF regular-file reference counts are inconsistent.",
            ));
        }
        Ok((members, plan, volume))
    }
    fn node_bytes(&mut self, node: &Node, offset: u64, length: usize) -> Result<Vec<u8>> {
        if offset + length as u64 > node.size {
            return Err(invalid());
        }
        self.metadata = self.metadata.saturating_add(length as u64);
        if self.metadata > MAX_META {
            return Err(common::limit("UDF metadata scanning limit exceeded."));
        }
        let mut reader = NodeReader::new(&mut self.file, node, offset);
        let mut bytes = vec![0; length];
        reader.read_exact(&mut bytes).map_err(common::damaged)?;
        Ok(bytes)
    }
    fn stream(&mut self, members: &[Member], mut out: Option<&mut output::Output>) -> Result<()> {
        let entries: Vec<_> = members
            .iter()
            .filter(|m| {
                out.is_none()
                    || crate::tasks::selected(&m.entry.path, m.entry.kind == EntryKind::Directory)
            })
            .map(|m| m.entry.clone())
            .collect();
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
            let mut reader = NodeReader::new(&mut self.file, &member.node, 0);
            if out.is_some() && !crate::tasks::selected(&member.entry.path, false) {
                continue;
            }
            common::entry_data(&mut reader, &member.entry, out.as_deref_mut(), &mut buffer)
                .map_err(|error| crate::tasks::check().err().unwrap_or(error))?;
        }
        Ok(())
    }
}
impl Volume {
    fn address(&self, location: Icb, length: u64) -> Result<u64> {
        let (start, blocks) = *self.maps.get(location.part as usize).ok_or_else(invalid)?;
        let relative = u64::from(location.block) * BLOCK;
        if relative
            .checked_add(length)
            .is_none_or(|end| end > blocks * BLOCK)
        {
            return Err(invalid());
        }
        Ok(start * BLOCK + relative)
    }
}
impl Node {
    fn logical(&self, offset: u64) -> Result<u32> {
        if !self.embedded.is_empty() {
            return Ok(self.icb.block);
        }
        let mut position = offset;
        for extent in &self.extents {
            if position < extent.length {
                return Ok(extent.logical + (position / BLOCK) as u32);
            }
            position -= extent.length;
        }
        Err(invalid())
    }
}
struct NodeReader<'a> {
    file: &'a mut File,
    node: &'a Node,
    position: u64,
}
impl<'a> NodeReader<'a> {
    fn new(file: &'a mut File, node: &'a Node, position: u64) -> Self {
        Self {
            file,
            node,
            position,
        }
    }
}
impl Read for NodeReader<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if crate::tasks::check().is_err() {
            return Err(io::Error::other("Operation cancelled"));
        }
        let n = bytes
            .len()
            .min(self.node.size.saturating_sub(self.position) as usize);
        if n == 0 {
            return Ok(0);
        }
        if !self.node.embedded.is_empty() {
            bytes[..n].copy_from_slice(
                &self.node.embedded[self.position as usize..self.position as usize + n],
            );
            self.position += n as u64;
            return Ok(n);
        }
        let mut relative = self.position;
        for extent in &self.node.extents {
            if relative < extent.length {
                let amount = n.min((extent.length - relative) as usize);
                self.file.seek(SeekFrom::Start(extent.offset + relative))?;
                self.file.read_exact(&mut bytes[..amount])?;
                self.position += amount as u64;
                return Ok(amount);
            }
            relative -= extent.length;
        }
        Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "Invalid UDF allocation",
        ))
    }
}
fn reserve(ranges: &mut BTreeMap<u64, u64>, start: u64, length: u64) -> Result<()> {
    if length == 0 {
        return Ok(());
    }
    let end = start.checked_add(length).ok_or_else(invalid)?;
    if ranges
        .range(..end)
        .next_back()
        .is_some_and(|(_, old)| *old > start)
    {
        return Err(ArchiveError::new(
            "invalid_udf",
            "Overlapping UDF data or metadata extents are disabled.",
        ));
    }
    ranges.insert(start, end);
    Ok(())
}
fn decode_name(bytes: &[u8]) -> Result<String> {
    match bytes.first() {
        Some(8) => Ok(bytes[1..].iter().map(|b| char::from(*b)).collect()),
        Some(16) if (bytes.len() - 1).is_multiple_of(2) => String::from_utf16(
            &bytes[1..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_be_bytes(*b))
                .collect::<Vec<_>>(),
        )
        .map_err(|_| invalid()),
        _ => Err(invalid()),
    }
}
impl ArchiveBackend for UdfBackend {
    fn output_name(&self) -> String {
        output::output_folder(&self.name, &[".iso", ".udf"])
    }
    fn inspect(&mut self) -> Result<ArchiveInspection> {
        let (members, _, volume) = self.members()?;
        let entries = members.into_iter().map(|m| m.entry).collect();
        let mut caps = self.capabilities();
        caps.format = match volume.revision {
            0x0102 => "UDF 1.02",
            0x0150 => "UDF 1.50",
            0x0200 => "UDF 2.00",
            0x0201 => "UDF 2.01",
            _ => "UDF 2.60 (physical partition)",
        };
        Ok(common::inspection(
            &self.name,
            self.output_name(),
            self.length,
            entries,
            caps,
        ))
    }
    fn list_entries(&mut self) -> Result<Vec<ArchiveEntry>> {
        Ok(self.members()?.0.into_iter().map(|m| m.entry).collect())
    }
    fn extract(&mut self, destination: &Path, policy: OverwritePolicy) -> Result<OperationReport> {
        let (members, plan, _) = self.members()?;
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
        let (members, _, _) = self.members()?;
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
        FormatCapabilities{format:"UDF",zip64:false,inspection:true,extraction:true,integrity:Integrity::Structure,limitations:"Read-only physical single-volume UDF. Metadata tag checksums/CRCs and readable file extents, not file-content checksums. No VAT, sparing, metadata partitions, links, sparse/indirect allocations or alternate streams.",compression_methods:&["Stored"],encryption:false,multipart:false}
    }
}
#[cfg(test)]
mod security_tests {
    use super::*;
    fn repair(bytes: &mut [u8], at: usize) {
        let length = u16le(bytes, at + 10).unwrap() as usize;
        let crc = crc16(&bytes[at + 16..at + 16 + length]);
        bytes[at + 8..at + 10].copy_from_slice(&crc.to_le_bytes());
        bytes[at + 4] = 0;
        bytes[at + 4] = bytes[at..at + 16]
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != 4)
            .fold(0_u8, |sum, (_, b)| sum.wrapping_add(*b));
    }
    fn fixture() -> (tempfile::TempDir, std::path::PathBuf, UdfBackend, Vec<u8>) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.iso");
        let bytes = include_bytes!("../../tests/fixtures/udf260.iso").to_vec();
        std::fs::write(&path, &bytes).unwrap();
        let backend = UdfBackend::new(
            File::open(&path).unwrap(),
            "image.iso".into(),
            bytes.len() as u64,
        );
        (dir, path, backend, bytes)
    }
    #[test]
    fn malformed_udf_checksums_extent_bounds_special_files_and_limits_fail_before_output() {
        for attack in 0..4 {
            let (dir, path, mut backend, mut bytes) = fixture();
            let (members, _, volume) = backend.members().unwrap();
            let member = members
                .iter()
                .find(|m| m.entry.path == "nested/café-日本.txt")
                .unwrap();
            let at = volume.address(member.node.icb, BLOCK).unwrap() as usize;
            match attack {
                0 => bytes[at + 56] ^= 1,
                1 => {
                    let ad = at + 176 + u32le(&bytes, at + 168).unwrap() as usize;
                    bytes[ad + 4..ad + 8].copy_from_slice(&u32::MAX.to_le_bytes());
                    repair(&mut bytes, at);
                }
                2 => {
                    bytes[at + 27] = 12;
                    repair(&mut bytes, at);
                }
                _ => {
                    bytes[at + 56..at + 64].copy_from_slice(&(common::MAX_FILE + 1).to_le_bytes());
                    repair(&mut bytes, at);
                }
            }
            drop(backend);
            std::fs::write(&path, bytes).unwrap();
            assert!(super::super::extract(&path, dir.path(), OverwritePolicy::Refuse).is_err());
            assert!(!dir.path().join("image").exists());
        }
    }
    #[test]
    fn malicious_udf_filename_override_and_partition_map_are_rejected() {
        let (dir, path, mut backend, mut bytes) = fixture();
        let (members, _, _) = backend.members().unwrap();
        let directory = &members
            .iter()
            .find(|m| m.entry.path == "nested")
            .unwrap()
            .node;
        let data = backend
            .node_bytes(directory, 0, directory.size as usize)
            .unwrap();
        let mut offset = 0;
        while data[offset + 18] & 8 != 0 {
            let length =
                (38 + u16le(&data, offset + 36).unwrap() as usize + data[offset + 19] as usize)
                    .next_multiple_of(4);
            offset += length;
        }
        let physical = directory.extents[0].offset as usize + offset;
        let start = physical + 38 + u16le(&bytes, physical + 36).unwrap() as usize;
        let length = bytes[physical + 19] as usize;
        assert_eq!(bytes[start], 16);
        let chars = (length - 1) / 2;
        let name = format!("../{}", "x".repeat(chars - 3));
        let encoded: Vec<_> = name.encode_utf16().flat_map(u16::to_be_bytes).collect();
        bytes[start + 1..start + length].copy_from_slice(&encoded);
        repair(&mut bytes, physical);
        drop(backend);
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(
            super::super::inspect(&path).unwrap_err().code,
            "unsafe_path"
        );
        assert!(!dir.path().join("image").exists());
        let (_second_dir, path, _, mut bytes) = fixture();
        let anchor = &bytes[256 * 2048..257 * 2048];
        let start = u32le(anchor, 20).unwrap() as usize;
        for block in start..start + 16 {
            if u16le(&bytes, block * 2048).unwrap() == 6 {
                bytes[block * 2048 + 440] = 2;
                repair(&mut bytes, block * 2048);
                break;
            }
        }
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(
            super::super::inspect(&path).unwrap_err().code,
            "unsupported_udf"
        );
    }
    #[test]
    fn udf_replace_numbering_cancellation_and_unrelated_files_use_shared_output() {
        let (dir, path, _, _) = fixture();
        let first = super::super::extract(&path, dir.path(), OverwritePolicy::Refuse).unwrap();
        let root = std::path::PathBuf::from(first.output_path.unwrap());
        std::fs::write(root.join("unrelated"), b"keep").unwrap();
        assert_eq!(
            super::super::extract(&path, dir.path(), OverwritePolicy::Refuse)
                .unwrap_err()
                .code,
            "destination_exists"
        );
        let second = super::super::extract(&path, dir.path(), OverwritePolicy::KeepBoth).unwrap();
        assert!(second.output_path.unwrap().ends_with("image (1)"));
        crate::tasks::test_context(None, None, |control| {
            control
                .cancel_after_commit
                .store(true, std::sync::atomic::Ordering::Release);
            assert_eq!(
                super::super::extract(&path, dir.path(), OverwritePolicy::Replace)
                    .unwrap_err()
                    .code,
                "cancelled"
            );
        });
        assert_eq!(std::fs::read(root.join("unrelated")).unwrap(), b"keep");
    }
}
