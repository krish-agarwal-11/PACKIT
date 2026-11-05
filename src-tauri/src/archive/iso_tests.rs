// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use super::*;
use std::{fs, path::PathBuf};
#[test]
fn real_iso9660_joliet_rockridge_browse_select_extract_and_replace() {
    for (name, bytes, entry) in [
        (
            "plain.iso",
            &include_bytes!("../../tests/fixtures/iso9660.iso")[..],
            "NESTED/FILE.TXT",
        ),
        (
            "joliet.iso",
            &include_bytes!("../../tests/fixtures/joliet.iso")[..],
            "nested/café.txt",
        ),
        (
            "rockridge.iso",
            &include_bytes!("../../tests/fixtures/rockridge.iso")[..],
            "nested/café.txt",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join(name);
        fs::write(&source, bytes).unwrap();
        let info = inspect(&source).unwrap();
        assert_eq!(info.capabilities.format, "ISO");
        assert!(info.entries.iter().any(|e| e.path == entry));
        assert_eq!(test(&source).unwrap().processed_bytes, 24);
        let out = extract(&source, dir.path(), OverwritePolicy::Refuse).unwrap();
        let root = PathBuf::from(out.output_path.unwrap());
        assert_eq!(
            fs::read(root.join(entry)).unwrap(),
            b"PACKIT ISO real payload\n"
        );
        fs::write(root.join(entry), b"original").unwrap();
        fs::write(root.join("unrelated"), b"keep").unwrap();
        assert_eq!(
            extract(&source, dir.path(), OverwritePolicy::Refuse)
                .unwrap_err()
                .code,
            "destination_exists"
        );
        crate::tasks::test_context(Some(vec![entry.into()]), None, |_| {
            let selected = extract(&source, dir.path(), OverwritePolicy::KeepBoth).unwrap();
            assert_eq!(selected.files, 1);
        });
        extract(&source, dir.path(), OverwritePolicy::Replace).unwrap();
        assert_eq!(fs::read(root.join("unrelated")).unwrap(), b"keep");
        assert_eq!(
            fs::read(root.join(entry)).unwrap(),
            b"PACKIT ISO real payload\n"
        );
    }
}
#[test]
fn iso_rejects_truncation_extents_path_overrides_links_and_udf() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("bad.iso");
    let valid = include_bytes!("../../tests/fixtures/iso9660.iso");
    for bytes in [valid[..valid.len() - 2048].to_vec(), {
        let mut bad = valid.to_vec();
        bad[16 * 2048 + 84] ^= 1;
        bad
    }] {
        fs::write(&source, bytes).unwrap();
        assert!(inspect(&source).is_err());
        assert!(!dir.path().join("bad").exists());
    }
    let rr = include_bytes!("../../tests/fixtures/rockridge.iso");
    for attack in [b"../escape".as_slice(), b"/absolute".as_slice()] {
        let mut bad = rr.to_vec();
        let needle = "café.txt".as_bytes();
        let at = bad.windows(needle.len()).position(|p| p == needle).unwrap();
        // Same-length path override keeps the actual NM framing intact.
        let replacement = if attack[0] == b'.' {
            b"../xx.txt".as_slice()
        } else {
            b"/badd.txt".as_slice()
        };
        bad[at..at + needle.len()].copy_from_slice(replacement);
        fs::write(&source, bad).unwrap();
        assert!(inspect(&source).is_err());
    }
    let mut udf = vec![0; 18 * 2048];
    udf[16 * 2048 + 1..16 * 2048 + 6].copy_from_slice(b"NSR03");
    udf[16 * 2048 + 6] = 1;
    fs::write(&source, udf).unwrap();
    assert_eq!(inspect(&source).unwrap_err().code, "invalid_udf");
}

#[test]
fn iso_rejects_overlapping_metadata_bad_records_and_rockridge_special_files() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("attack.iso");
    let valid = include_bytes!("../../tests/fixtures/iso9660.iso");
    let filename_at = valid
        .windows(b"FILE.TXT;1".len())
        .position(|b| b == b"FILE.TXT;1")
        .unwrap();
    let record_at = filename_at - 33;
    for kind in 0..4 {
        let mut bytes = valid.to_vec();
        match kind {
            0 => {
                bytes[record_at + 2..record_at + 6].copy_from_slice(&16_u32.to_le_bytes());
                bytes[record_at + 6..record_at + 10].copy_from_slice(&16_u32.to_be_bytes());
            }
            1 => bytes[record_at] = 33,
            2 => bytes[record_at + 25] |= 0x80,
            _ => bytes[record_at + 10..record_at + 14].copy_from_slice(&u32::MAX.to_le_bytes()),
        }
        fs::write(&source, bytes).unwrap();
        assert!(extract(&source, dir.path(), OverwritePolicy::Refuse).is_err());
        assert!(!dir.path().join("attack").exists());
    }
    let mut rr = include_bytes!("../../tests/fixtures/rockridge.iso").to_vec();
    let px = rr
        .windows(4)
        .rposition(|b| b == b"PX\x24\x01" || b == b"PX\x2c\x01")
        .unwrap();
    let mode = 0o120777_u32;
    rr[px + 4..px + 8].copy_from_slice(&mode.to_le_bytes());
    rr[px + 8..px + 12].copy_from_slice(&mode.to_be_bytes());
    fs::write(&source, rr).unwrap();
    assert_eq!(inspect(&source).unwrap_err().code, "unsafe_entry");
}

fn set_both(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    data[offset + 4..offset + 8].copy_from_slice(&value.to_be_bytes());
}
fn directory_records(block: &[u8]) -> Vec<usize> {
    let mut records = Vec::new();
    let mut at = 0;
    while at < block.len() && block[at] != 0 {
        records.push(at);
        at += block[at] as usize;
    }
    records
}
#[test]
fn iso_partial_final_directory_sector_and_large_sparse_directory_regression() {
    use std::io::{Seek, SeekFrom, Write};
    for large in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("compatible.iso");
        let mut original = include_bytes!("../../tests/fixtures/iso9660.iso").to_vec();
        let old_lba = u32::from_le_bytes(
            original[16 * 2048 + 158..16 * 2048 + 162]
                .try_into()
                .unwrap(),
        );
        let old = old_lba as usize * 2048;
        let mut root = original[old..old + 2048].to_vec();
        let records = directory_records(&root);
        let used = records.last().unwrap() + root[*records.last().unwrap()] as usize;
        let size = if large { 17 * 1024 * 1024 } else { used as u32 };
        let lba = if large {
            (original.len() / 2048) as u32
        } else {
            old_lba
        };
        for at in &records {
            if root[at + 25] & 2 != 0 && root[at + 33] > 1 {
                let child =
                    u32::from_le_bytes(root[at + 2..at + 6].try_into().unwrap()) as usize * 2048;
                let parent = child + original[child] as usize;
                set_both(&mut original, parent + 2, lba);
                set_both(&mut original, parent + 10, size);
            }
        }
        for at in &records[..2] {
            set_both(&mut root, at + 2, lba);
            set_both(&mut root, at + 10, size);
        }
        set_both(&mut original, 16 * 2048 + 158, lba);
        set_both(&mut original, 16 * 2048 + 166, size);
        // Update both path table root extents when relocating the directory.
        for (at, little) in [(140, true), (148, false)] {
            let raw: [u8; 4] = original[16 * 2048 + at..16 * 2048 + at + 4]
                .try_into()
                .unwrap();
            let table = if little {
                u32::from_le_bytes(raw)
            } else {
                u32::from_be_bytes(raw)
            } as usize
                * 2048;
            let bytes = if little {
                lba.to_le_bytes()
            } else {
                lba.to_be_bytes()
            };
            original[table + 2..table + 6].copy_from_slice(&bytes);
        }
        let mut file = fs::File::create(&source).unwrap();
        if large {
            set_both(&mut original, 16 * 2048 + 80, lba + size / 2048);
            file.write_all(&original).unwrap();
            file.set_len(u64::from(lba) * 2048 + u64::from(size))
                .unwrap();
            file.seek(SeekFrom::Start(u64::from(lba) * 2048)).unwrap();
            file.write_all(&root).unwrap();
        } else {
            original[old..old + 2048].copy_from_slice(&root);
            file.write_all(&original).unwrap();
        }
        drop(file);
        let info = inspect(&source).unwrap();
        assert!(info.entries.iter().any(|e| e.path == "NESTED/FILE.TXT"));
        let report = extract(&source, dir.path(), OverwritePolicy::Refuse).unwrap();
        assert_eq!(
            fs::read(PathBuf::from(report.output_path.unwrap()).join("NESTED/FILE.TXT")).unwrap(),
            b"PACKIT ISO real payload\n"
        );
    }
}

#[test]
fn udf_bridge_after_iso_terminator_is_not_presented_as_complete_iso_contents() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("udf-bridge.iso");
    let mut data = include_bytes!("../../tests/fixtures/iso9660.iso").to_vec();
    for (sector, signature) in [(18, b"BEA01"), (19, b"NSR02"), (20, b"TEA01")] {
        data[sector * 2048] = 0;
        data[sector * 2048 + 1..sector * 2048 + 6].copy_from_slice(signature);
        data[sector * 2048 + 6] = 1;
    }
    fs::write(&source, data).unwrap();
    assert_eq!(inspect(&source).unwrap_err().code, "invalid_udf");
    assert!(extract(&source, root.path(), OverwritePolicy::Refuse).is_err());
    assert!(!root.path().join("udf-bridge").exists());
}
#[test]
#[ignore = "Requires the user's external UDF bridge image via PACKIT_ACCEPTANCE_ISO; never modifies it"]
fn supplied_acceptance_iso_browses_and_extracts_udf() {
    let path = std::env::var_os("PACKIT_ACCEPTANCE_ISO")
        .expect("Set PACKIT_ACCEPTANCE_ISO to the supplied image");
    let source = PathBuf::from(path);
    let before = fs::metadata(&source).unwrap();
    let info = inspect(&source).unwrap();
    assert_eq!(info.capabilities.format, "UDF 1.02");
    assert!(
        info.entries
            .iter()
            .filter(|e| e.kind == EntryKind::File)
            .count()
            > 1
    );
    let small = info
        .entries
        .iter()
        .find(|e| {
            e.kind == EntryKind::File
                && e.uncompressed_bytes > 0
                && e.uncompressed_bytes < 64 * 1024
        })
        .unwrap()
        .clone();
    let destination = tempfile::tempdir().unwrap();
    crate::tasks::test_context(Some(vec![small.path.clone()]), None, |_| {
        let result = extract(&source, destination.path(), OverwritePolicy::Refuse).unwrap();
        assert_eq!(result.files, 1);
        assert_eq!(
            fs::metadata(PathBuf::from(result.output_path.unwrap()).join(&small.path))
                .unwrap()
                .len(),
            small.uncompressed_bytes
        );
    });
    let after = fs::metadata(&source).unwrap();
    assert_eq!(before.len(), after.len());
    assert_eq!(before.modified().unwrap(), after.modified().unwrap());
}
