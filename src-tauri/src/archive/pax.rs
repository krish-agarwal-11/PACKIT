// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use super::{streaming, ArchiveError, Result};
use std::collections::BTreeMap;

pub(super) const MAX_HEADER: u64 = 64 * 1024;
pub(super) const MAX_METADATA: u64 = 4 * 1024 * 1024;
pub(super) const MAX_RECORDS: usize = 8192;
pub(super) type Fields = BTreeMap<String, String>;

fn invalid() -> ArchiveError {
    ArchiveError::new(
        "invalid_pax",
        "Malformed, conflicting or unsupported PAX metadata.",
    )
}

pub(super) fn parse(bytes: &[u8]) -> Result<Fields> {
    let mut result = Fields::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if result.len() >= 256 {
            return Err(streaming::limit("PAX header exceeds 256 records."));
        }
        let rest = &bytes[offset..];
        let space = rest.iter().position(|b| *b == b' ').ok_or_else(invalid)?;
        if space == 0
            || space > 6
            || rest[0] == b'0'
            || !rest[..space].iter().all(u8::is_ascii_digit)
        {
            return Err(invalid());
        }
        let length = std::str::from_utf8(&rest[..space])
            .map_err(|_| invalid())?
            .parse::<usize>()
            .map_err(|_| invalid())?;
        if length <= space + 3 || length > rest.len() || rest[length - 1] != b'\n' {
            return Err(invalid());
        }
        let record = std::str::from_utf8(&rest[space + 1..length - 1]).map_err(|_| invalid())?;
        let (key, value) = record.split_once('=').ok_or_else(invalid)?;
        if value.len() > 4096 || value.chars().any(char::is_control) || result.contains_key(key) {
            return Err(invalid());
        }
        match key {
            "path" => { if value.len() > 1024 { return Err(invalid()); } }
            "size" | "uid" | "gid" if !value.is_empty() => {
                if !value.bytes().all(|b| b.is_ascii_digit()) || value.parse::<u64>().is_err() { return Err(invalid()); }
            }
            "mtime" | "atime" | "ctime" if !value.is_empty() => {
                let number = value.strip_prefix('-').unwrap_or(value);
                let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
                if whole.is_empty() || whole.len() > 20 || !whole.bytes().all(|b| b.is_ascii_digit())
                    || fraction.len() > 18 || !fraction.bytes().all(|b| b.is_ascii_digit())
                    || (number.contains('.') && fraction.is_empty()) { return Err(invalid()); }
            }
            "hdrcharset" | "charset" if !value.is_empty() => {
                if !matches!(value, "UTF-8" | "ISO-IR 10646 2000 UTF-8") { return Err(invalid()); }
            }
            "size" | "uid" | "gid" | "mtime" | "atime" | "ctime" | "hdrcharset" | "charset" |
            "uname" | "gname" | "comment" => {}
            _ => return Err(ArchiveError::new("unsupported_variant", format!("Unsupported PAX metadata key {key:?}; links and sparse extensions remain disabled."))),
        }
        result.insert(key.into(), value.into());
        offset += length;
    }
    Ok(result)
}

pub(super) fn effective<'a>(local: &'a Fields, global: &'a Fields, key: &str) -> Option<&'a str> {
    local
        .get(key)
        .or_else(|| global.get(key))
        .filter(|value| !value.is_empty())
        .map(String::as_str)
}
