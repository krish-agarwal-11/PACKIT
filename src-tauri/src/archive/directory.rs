// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use super::{ArchiveError, Result};
use std::{
    cell::Cell,
    io::{self, Read, Seek, SeekFrom},
    rc::Rc,
};

const MAX_ENTRIES: u64 = 20_000;
const MAX_DIRECTORY_BYTES: u64 = 32 * 1024 * 1024;
const END_SIZE: usize = 22;

pub(super) struct Directory {
    pub start: u64,
    pub count: usize,
    metadata: Vec<u8>,
}

fn corrupt() -> ArchiveError {
    ArchiveError::new("invalid_zip", "Invalid or damaged ZIP directory.")
}

fn read_at(reader: &mut (impl Read + Seek), offset: u64, bytes: &mut [u8]) -> Result<()> {
    reader
        .seek(SeekFrom::Start(offset))
        .and_then(|_| reader.read_exact(bytes))
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::UnexpectedEof {
                corrupt()
            } else {
                ArchiveError::io("Could not read ZIP directory", error)
            }
        })
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn multipart() -> ArchiveError {
    ArchiveError::new(
        "unsupported_zip",
        "Split or multipart ZIP archives are unsupported.",
    )
}

/// Bound ZIP metadata before the decoder allocates its central directory index.
/// Only single-volume, unwrapped ZIP layouts are accepted.
pub(super) fn preflight(reader: &mut (impl Read + Seek), length: u64) -> Result<Directory> {
    let tail_len = length.min((END_SIZE + u16::MAX as usize) as u64) as usize;
    let tail_start = length - tail_len as u64;
    let mut tail = vec![0; tail_len];
    read_at(reader, tail_start, &mut tail)?;
    let i = (0..=tail_len.saturating_sub(END_SIZE))
        .rev()
        .find(|&i| {
            tail.get(i..i + 4) == Some(b"PK\x05\x06")
                && tail_len >= i + END_SIZE
                && i + END_SIZE + u16_at(&tail, i + 20) as usize == tail_len
        })
        .ok_or_else(corrupt)?;
    let end = &tail[i..];
    let end_pos = tail_start + i as u64;
    if u16_at(end, 4) != 0 || u16_at(end, 6) != 0 || u16_at(end, 8) != u16_at(end, 10) {
        return Err(multipart());
    }
    let mut locator = [0; 20];
    if let Some(position) = end_pos.checked_sub(20) {
        read_at(reader, position, &mut locator)?;
    }
    let (count, bytes, start, boundary) = if &locator[..4] == b"PK\x06\x07" {
        if u32_at(&locator, 4) != 0 || u32_at(&locator, 16) != 1 {
            return Err(multipart());
        }
        let position = u64_at(&locator, 8);
        let mut record = [0; 56];
        if position.checked_add(56).is_none_or(|p| p > end_pos - 20) {
            return Err(corrupt());
        }
        read_at(reader, position, &mut record)?;
        let record_size = u64_at(&record, 4);
        if &record[..4] != b"PK\x06\x06"
            || !(44..=4096).contains(&record_size)
            || position.checked_add(12 + record_size) != Some(end_pos - 20)
        {
            return Err(corrupt());
        }
        if u32_at(&record, 16) != 0
            || u32_at(&record, 20) != 0
            || u64_at(&record, 24) != u64_at(&record, 32)
        {
            return Err(multipart());
        }
        let count = u64_at(&record, 32);
        let bytes = u64_at(&record, 40);
        let start = u64_at(&record, 48);
        if (u16_at(end, 10) != u16::MAX && u64::from(u16_at(end, 10)) != count)
            || (u32_at(end, 12) != u32::MAX && u64::from(u32_at(end, 12)) != bytes)
            || (u32_at(end, 16) != u32::MAX && u64::from(u32_at(end, 16)) != start)
        {
            return Err(corrupt());
        }
        (count, bytes, start, position)
    } else {
        (
            u64::from(u16_at(end, 10)),
            u64::from(u32_at(end, 12)),
            u64::from(u32_at(end, 16)),
            end_pos,
        )
    };
    if count > MAX_ENTRIES || bytes > MAX_DIRECTORY_BYTES {
        return Err(ArchiveError::new(
            "archive_limit",
            "ZIP exceeds the limit of 20,000 entries or 32 MiB of directory metadata.",
        ));
    }
    if start.checked_add(bytes) != Some(boundary) || count * 46 > bytes {
        return Err(corrupt());
    }

    // Freeze the validated metadata for zip-rs. Its recovery scanner must not
    // search arbitrary payload bytes for alternative, unbounded end records.
    let mut metadata = vec![0; (length - start) as usize];
    read_at(reader, start, &mut metadata)?;
    if metadata.get((end_pos - start) as usize..) != Some(end) {
        return Err(corrupt());
    }
    for (index, signature) in metadata.windows(4).enumerate() {
        let position = start + index as u64;
        if (signature == b"PK\x05\x06" && position != end_pos)
            || (signature == b"PK\x06\x06" && (position != boundary || boundary == end_pos))
        {
            return Err(ArchiveError::new(
                "invalid_zip",
                "ZIP contains ambiguous directory end records.",
            ));
        }
    }
    if boundary != end_pos {
        let record = &metadata[bytes as usize..];
        let locator_start = (end_pos - start - 20) as usize;
        if u64_at(record, 32) != count
            || u64_at(record, 40) != bytes
            || u64_at(record, 48) != start
            || u64_at(record, 24) != count
            || u32_at(record, 16) != 0
            || u32_at(record, 20) != 0
            || &metadata[locator_start..locator_start + 20] != locator.as_slice()
            || u64_at(record, 4).checked_add(12) != Some(end_pos - 20 - boundary)
        {
            return Err(corrupt());
        }
    }

    // Count physical records independently. The ZIP decoder indexes by raw name
    // and otherwise collapses duplicate records.
    let mut offset = start;
    for _ in 0..count {
        if boundary - offset < 46 {
            return Err(corrupt());
        }
        let index = (offset - start) as usize;
        let header = &metadata[index..index + 46];
        if &header[..4] != b"PK\x01\x02" {
            return Err(corrupt());
        }
        if u16_at(header, 34) != 0 && u16_at(header, 34) != u16::MAX {
            return Err(multipart());
        }
        offset += 46
            + u64::from(u16_at(header, 28))
            + u64::from(u16_at(header, 30))
            + u64::from(u16_at(header, 32));
        if offset > boundary {
            return Err(corrupt());
        }
    }
    if offset != boundary {
        return Err(corrupt());
    }
    Ok(Directory {
        start,
        count: count as usize,
        metadata,
    })
}

/// Serve only the bounded directory snapshot while zip-rs builds its index.
/// Afterward the same reader switches to the original file for local data.
pub(super) struct MetadataReader<R> {
    input: R,
    directory: Directory,
    position: u64,
    payload_enabled: Rc<Cell<bool>>,
}

impl<R> MetadataReader<R> {
    pub fn new(input: R, directory: Directory) -> (Self, Rc<Cell<bool>>) {
        let payload_enabled = Rc::new(Cell::new(false));
        (
            Self {
                input,
                directory,
                position: 0,
                payload_enabled: payload_enabled.clone(),
            },
            payload_enabled,
        )
    }
}

impl<R: Read + Seek> Read for MetadataReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.payload_enabled.get() {
            if !self.directory.metadata.is_empty() {
                self.input.seek(SeekFrom::Start(self.position))?;
                self.directory.metadata = Vec::new();
            }
            return self.input.read(buffer);
        }
        let start = self.directory.start;
        if self.position < start {
            // The decoder scans in blocks and can begin just before the
            // directory. Expose bounded zero padding instead of payload bytes.
            let gap = start - self.position;
            if gap > 2048 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "ZIP directory scan exceeded its bounds",
                ));
            }
            let count = buffer.len().min(gap as usize);
            buffer[..count].fill(0);
            self.position += count as u64;
            return Ok(count);
        }
        let offset = self.position - start;
        let Some(bytes) = usize::try_from(offset)
            .ok()
            .and_then(|offset| self.directory.metadata.get(offset..))
        else {
            return Ok(0);
        };
        let count = buffer.len().min(bytes.len());
        buffer[..count].copy_from_slice(&bytes[..count]);
        self.position += count as u64;
        Ok(count)
    }
}

impl<R: Seek> Seek for MetadataReader<R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        if self.payload_enabled.get() {
            if !self.directory.metadata.is_empty() {
                self.input.seek(SeekFrom::Start(self.position))?;
                self.directory.metadata = Vec::new();
            }
            return self.input.seek(position);
        }
        self.position = match position {
            SeekFrom::Start(position) => Some(position),
            SeekFrom::Current(offset) => self.position.checked_add_signed(offset),
            SeekFrom::End(offset) => (self.directory.start + self.directory.metadata.len() as u64)
                .checked_add_signed(offset),
        }
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid ZIP directory seek"))?;
        Ok(self.position)
    }
}
