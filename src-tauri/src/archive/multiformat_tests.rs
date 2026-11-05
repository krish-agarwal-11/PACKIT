// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use super::*;
use std::{fs, io::Write, path::PathBuf};

fn tar_bytes(name: &str, data: &[u8], kind: u8) -> Vec<u8> {
    let mut header = tar::Header::new_ustar();
    header.set_mode(0o644);
    header.set_size(data.len() as u64);
    header.set_mtime(1);
    header.set_entry_type(tar::EntryType::new(kind));
    // Fixture paths include deliberate traversal and link types.
    header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
    header.set_cksum();
    let mut bytes = header.as_bytes().to_vec();
    bytes.extend_from_slice(data);
    bytes.resize(bytes.len().next_multiple_of(512) + 1024, 0);
    bytes
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut writer = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    writer.write_all(bytes).unwrap();
    writer.finish().unwrap()
}

fn rar_block(kind: u8, flags: u16, payload: &[u8]) -> Vec<u8> {
    // Same original stored RAR4 fixture layout as Phase 1B-R probe.py.
    let mut record = vec![0, 0, kind];
    record.extend(flags.to_le_bytes());
    record.extend((7 + payload.len() as u16).to_le_bytes());
    record.extend_from_slice(payload);
    let crc = crc32fast::hash(&record[2..]) as u16;
    record[..2].copy_from_slice(&crc.to_le_bytes());
    record
}

fn rar_bytes(name: &str, data: &[u8], method: u8, flags: u16, os: u8, attributes: u32) -> Vec<u8> {
    let mut file = rar_backend::SIGNATURE.to_vec();
    file.extend(rar_block(0x73, 0, &[0; 6]));
    let mut entry = Vec::new();
    entry.extend((data.len() as u32).to_le_bytes());
    entry.extend((data.len() as u32).to_le_bytes());
    entry.push(os);
    entry.extend(crc32fast::hash(data).to_le_bytes());
    entry.extend(0x50210000_u32.to_le_bytes());
    entry.extend([20, method]);
    entry.extend((name.len() as u16).to_le_bytes());
    entry.extend(attributes.to_le_bytes());
    entry.extend_from_slice(name.as_bytes());
    file.extend(rar_block(0x74, flags | 0x8000, &entry));
    file.extend_from_slice(data);
    file.extend(rar_block(0x7b, 0, &[]));
    file
}

#[test]
fn tar_and_gzip_detect_structure_list_unicode_and_extract() {
    let dir = tempfile::tempdir().unwrap();
    let raw = tar_bytes("PKnested/café.txt", b"real file", b'0');
    for (name, bytes, format, folder) in [
        ("plain.data", raw.clone(), "TAR", "plain"),
        ("Source.2026.tar.gz", gzip(&raw), "TAR.GZ", "Source.2026"),
    ] {
        let source = dir.path().join(name);
        fs::write(&source, bytes).unwrap();
        let info = inspect(&source).unwrap();
        assert_eq!(info.capabilities.format, format);
        assert_eq!(info.output_folder, folder);
        assert_eq!(info.entries[0].path, "PKnested/café.txt");
        assert_eq!(info.entries[0].uncompressed_bytes, 9);
        assert!(info.entries[0].compressed_bytes.is_none());
        assert_eq!(test(&source).unwrap().processed_bytes, 9);
        let result = extract(&source, dir.path(), OverwritePolicy::Refuse).unwrap();
        let root = PathBuf::from(result.output_path.unwrap());
        assert_eq!(root, dir.path().join(folder));
        assert_eq!(
            fs::read(root.join("PKnested/café.txt")).unwrap(),
            b"real file"
        );
    }
}

#[test]
fn gzip_footer_and_tar_truncation_fail_before_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let raw = tar_bytes("a.txt", b"new", b'0');
    let source = dir.path().join("source.tar.gz");
    let root = dir.path().join("source");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.txt"), b"old").unwrap();
    let valid = gzip(&raw);
    for from_end in [8, 4] {
        let mut bad = valid.clone();
        let index = bad.len() - from_end;
        bad[index] ^= 1;
        fs::write(&source, bad).unwrap();
        assert_eq!(test(&source).unwrap_err().code, "corrupt_entry");
        assert_eq!(
            extract(&source, dir.path(), OverwritePolicy::Replace)
                .unwrap_err()
                .code,
            "corrupt_entry"
        );
        assert_eq!(fs::read(root.join("a.txt")).unwrap(), b"old");
    }
    fs::write(&source, &valid[..valid.len() - 2]).unwrap();
    assert_eq!(inspect(&source).unwrap_err().code, "corrupt_entry");
    let tar = dir.path().join("truncated.tar");
    fs::write(&tar, &raw[..raw.len() - 512]).unwrap();
    assert_eq!(inspect(&tar).unwrap_err().code, "corrupt_entry");
}

#[test]
fn tar_dangerous_entries_and_header_crc_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("input.tar");
    for (name, kind, code) in [
        ("../escape", b'0', "unsafe_path"),
        ("/absolute", b'0', "unsafe_path"),
        ("C:/absolute", b'0', "unsafe_path"),
        ("dir/name:stream", b'0', "unsafe_path"),
        ("hard-link", b'1', "unsafe_entry"),
        ("sym-link", b'2', "unsafe_entry"),
        ("pax", b'x', "invalid_pax"),
    ] {
        fs::write(&source, tar_bytes(name, b"", kind)).unwrap();
        assert_eq!(
            extract(&source, dir.path(), OverwritePolicy::Refuse)
                .unwrap_err()
                .code,
            code
        );
        assert!(!dir.path().join("input").exists());
    }
    let mut damaged = tar_bytes("a.txt", b"data", b'0');
    damaged[0] ^= 1;
    fs::write(&source, damaged).unwrap();
    assert_eq!(inspect(&source).unwrap_err().code, "corrupt_entry");
}

#[test]
fn all_new_formats_use_shared_conflicts_and_clean_names() {
    for (filename, archive) in [
        ("Backup.tar", tar_bytes("dir/a.txt", b"new", b'0')),
        ("Backup.tar.gz", gzip(&tar_bytes("dir/a.txt", b"new", b'0'))),
        (
            "Backup.rar",
            rar_bytes("dir/a.txt", b"new", 0x30, 0, 2, 0x20),
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join(filename);
        fs::write(&source, archive).unwrap();
        let base = dir.path().join("Backup");
        fs::create_dir_all(base.join("dir")).unwrap();
        fs::write(base.join("dir/a.txt"), b"old").unwrap();
        fs::write(base.join("unrelated"), b"keep").unwrap();
        fs::create_dir(dir.path().join("Backup (1)")).unwrap();
        fs::create_dir(dir.path().join("Backup (3)")).unwrap();
        assert!(destination_exists(&source, dir.path()).unwrap());
        assert_eq!(
            extract(&source, dir.path(), OverwritePolicy::Refuse)
                .unwrap_err()
                .code,
            "destination_exists"
        );
        let copy = extract(&source, dir.path(), OverwritePolicy::KeepBoth).unwrap();
        assert_eq!(
            PathBuf::from(copy.output_path.unwrap()),
            dir.path().join("Backup (2)")
        );
        assert_eq!(fs::read(base.join("dir/a.txt")).unwrap(), b"old");
        extract(&source, dir.path(), OverwritePolicy::Replace).unwrap();
        assert_eq!(fs::read(base.join("dir/a.txt")).unwrap(), b"new");
        assert_eq!(fs::read(base.join("unrelated")).unwrap(), b"keep");
    }
}

#[test]
fn stored_rar_fixture_lists_streams_and_checks_payload_crc() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("fixture.data");
    let content = vec![0x34; streaming::BUFFER * 3 + 1];
    let mut bytes = rar_bytes("nested/sample.txt", &content, 0x30, 0, 2, 0x20);
    fs::write(&source, &bytes).unwrap();
    let info = inspect(&source).unwrap();
    assert_eq!(info.capabilities.format, "RAR4 (Stored)");
    assert_eq!(info.entries[0].path, "nested/sample.txt");
    assert_eq!(info.entries[0].compressed_bytes, Some(content.len() as u64));
    assert_eq!(test(&source).unwrap().processed_bytes, content.len() as u64);
    let result = extract(&source, dir.path(), OverwritePolicy::Refuse).unwrap();
    let path = PathBuf::from(result.output_path.unwrap()).join("nested/sample.txt");
    assert_eq!(fs::read(&path).unwrap(), content);
    bytes[7 + 13 + 32 + "nested/sample.txt".len()] ^= 1;
    fs::write(&source, bytes).unwrap();
    assert_eq!(test(&source).unwrap_err().code, "corrupt_entry");
    assert_eq!(
        extract(&source, dir.path(), OverwritePolicy::Replace)
            .unwrap_err()
            .code,
        "corrupt_entry"
    );
    assert_eq!(fs::read(path).unwrap(), content);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn rar_rejects_unsupported_variants_and_unsafe_entries() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("input.rar");
    for (bytes, code) in [
        (
            rar_bytes("a", b"data", 0x33, 0, 2, 0x20),
            "unsupported_compression",
        ),
        (
            rar_bytes("a", b"data", 0x30, 4, 2, 0x20),
            "unsupported_encryption",
        ),
        (
            rar_bytes("a", b"data", 0x30, 1, 2, 0x20),
            "unsupported_variant",
        ),
        (
            rar_bytes("..\\escape", b"data", 0x30, 0, 2, 0x20),
            "unsafe_path",
        ),
        (
            rar_bytes("link", b"target", 0x30, 0, 3, 0o120777),
            "unsafe_entry",
        ),
    ] {
        fs::write(&source, &bytes).unwrap();
        let mut backend = rar_backend::RarBackend::new(
            fs::File::open(&source).unwrap(),
            "input.rar".into(),
            bytes.len() as u64,
        );
        assert_eq!(backend.inspect().unwrap_err().code, code);
        assert!(!dir.path().join("input").exists());
    }
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
fn native_compressed_fixtures_list_test_and_extract() {
    for (name, bytes, format, entry, size, checksum) in [
        (
            "one.7z",
            &include_bytes!("../../tests/fixtures/lzma.7z")[..],
            "7z",
            "nested/café.txt",
            5200,
            0xf4b2381d,
        ),
        (
            "two.7z",
            &include_bytes!("../../tests/fixtures/lzma2.7z")[..],
            "7z",
            "nested/café.txt",
            5200,
            0xf4b2381d,
        ),
        (
            "four.rar",
            &include_bytes!("../../tests/fixtures/compressed-rar4.rar")[..],
            "RAR4",
            "testdir/test.txt",
            20,
            0xbec8a242,
        ),
        (
            "five.rar",
            &include_bytes!("../../tests/fixtures/compressed-rar5.rar")[..],
            "RAR5",
            "test.bin",
            1200,
            0x7cca70cd,
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join(name);
        fs::write(&source, bytes).unwrap();
        let info = inspect(&source).unwrap();
        assert_eq!(info.capabilities.format, format);
        assert!(info
            .entries
            .iter()
            .any(|e| e.path == entry && e.uncompressed_bytes == size));
        assert_eq!(test(&source).unwrap().processed_bytes, size);
        let output = PathBuf::from(
            extract(&source, dir.path(), OverwritePolicy::Refuse)
                .unwrap()
                .output_path
                .unwrap(),
        );
        let target = output.join(entry);
        assert_eq!(crc32fast::hash(&fs::read(&target).unwrap()), checksum);
        fs::write(&target, b"original").unwrap();
        fs::write(output.join("unrelated"), b"keep").unwrap();
        assert!(destination_exists(&source, dir.path()).unwrap());
        assert_eq!(
            extract(&source, dir.path(), OverwritePolicy::Refuse)
                .unwrap_err()
                .code,
            "destination_exists"
        );
        let copy = PathBuf::from(
            extract(&source, dir.path(), OverwritePolicy::KeepBoth)
                .unwrap()
                .output_path
                .unwrap(),
        );
        assert!(copy.ends_with(format!("{} (1)", info.output_folder)));
        assert_eq!(fs::read(&target).unwrap(), b"original");
        extract(&source, dir.path(), OverwritePolicy::Replace).unwrap();
        assert_eq!(crc32fast::hash(&fs::read(&target).unwrap()), checksum);
        assert_eq!(fs::read(output.join("unrelated")).unwrap(), b"keep");
        fs::write(&source, &bytes[..bytes.len() / 2]).unwrap();
        assert!(test(&source).is_err());
        assert!(extract(&source, dir.path(), OverwritePolicy::Replace).is_err());
        assert_eq!(crc32fast::hash(&fs::read(target).unwrap()), checksum);
    }
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
fn native_rejects_unsupported_methods_and_passwords() {
    let dir = tempfile::tempdir().unwrap();
    for (bytes, code) in [
        (
            &include_bytes!("../../tests/fixtures/unsupported-bzip2.7z")[..],
            "unsupported_compression",
        ),
        (
            &include_bytes!("../../tests/fixtures/encrypted-header.7z")[..],
            "unsupported_encryption",
        ),
    ] {
        let source = dir.path().join("negative.7z");
        fs::write(&source, bytes).unwrap();
        assert_eq!(inspect(&source).unwrap_err().code, code);
        assert!(!dir.path().join("negative").exists());
    }
}

#[test]
fn zip_passwords_use_memory_only_and_retry() {
    for bytes in [
        &include_bytes!("../../tests/fixtures/encrypted-aes.zip")[..],
        &include_bytes!("../../tests/fixtures/encrypted-zipcrypto.zip")[..],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("secret.zip");
        fs::write(&source, bytes).unwrap();
        assert_eq!(inspect(&source).unwrap_err().code, "password_required");
        crate::tasks::test_context(None, Some("incorrect".into()), |_| {
            assert_eq!(test(&source).unwrap_err().code, "incorrect_password");
        });
        crate::tasks::test_context(None, Some("fixture-only".into()), |_| {
            assert_eq!(inspect(&source).unwrap().entries[0].path, "secret.txt");
            assert_eq!(test(&source).unwrap().files, 1);
            let output = extract(&source, dir.path(), OverwritePolicy::Refuse).unwrap();
            assert_eq!(
                fs::metadata(PathBuf::from(output.output_path.unwrap()).join("secret.txt"))
                    .unwrap()
                    .len(),
                1700
            );
        });
        assert!(!crate::tasks::has_password());
    }
}

#[test]
fn selections_extract_only_chosen_files_and_folders_for_all_readers() {
    let dir = tempfile::tempdir().unwrap();
    let raw = tar_bytes("nested/keep.txt", b"keep", b'0');
    let extra = tar_bytes("unselected.txt", b"skip", b'0');
    let mut tar = raw[..raw.len() - 1024].to_vec();
    tar.extend(extra);
    let first = rar_bytes("nested/keep.txt", b"keep", 0x30, 0, 2, 0x20);
    let second = rar_bytes("unselected.txt", b"skip", 0x30, 0, 2, 0x20);
    let mut rar = first[..first.len() - 7].to_vec();
    rar.extend_from_slice(&second[20..]);
    for (name, content) in [
        ("selection.rar", rar),
        ("selection.tar", tar.clone()),
        ("selection.tar.gz", gzip(&tar)),
    ] {
        let source = dir.path().join(name);
        fs::write(&source, content).unwrap();
        crate::tasks::test_context(Some(vec!["nested".into()]), None, |_| {
            let output = extract(&source, dir.path(), OverwritePolicy::KeepBoth).unwrap();
            assert_eq!(output.files, 1);
            let path = PathBuf::from(output.output_path.unwrap());
            assert_eq!(fs::read(path.join("nested/keep.txt")).unwrap(), b"keep");
            assert!(!path.join("unselected.txt").exists());
        });
    }
    let source = dir.path().join("selection.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&source).unwrap());
    zip.start_file("nested/keep.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"keep").unwrap();
    zip.start_file("unselected.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"skip").unwrap();
    zip.finish().unwrap();
    crate::tasks::test_context(Some(vec!["nested".into()]), None, |_| {
        let output = extract(&source, dir.path(), OverwritePolicy::Refuse).unwrap();
        assert_eq!(output.files, 1);
        let path = PathBuf::from(output.output_path.unwrap());
        assert!(path.join("nested/keep.txt").exists());
        assert!(!path.join("unselected.txt").exists());
    });
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
fn multipart_rar_and_byte_split_7z_roundtrip_and_missing_middle() {
    for (names, last, expected) in [
        (
            vec!["sample.part1.rar", "sample.part2.rar", "sample.part3.rar"],
            "sample.part3.rar",
            "LibarchiveAddingTest.html",
        ),
        (
            vec!["sample.7z.001", "sample.7z.002", "sample.7z.003"],
            "sample.7z.003",
            "nested/café.txt",
        ),
        (
            vec![
                "rar5.part01.rar",
                "rar5.part02.rar",
                "rar5.part03.rar",
                "rar5.part04.rar",
                "rar5.part05.rar",
                "rar5.part06.rar",
                "rar5.part07.rar",
                "rar5.part08.rar",
            ],
            "rar5.part08.rar",
            "home/antek/temp/build/unrar5/libarchive/bin/bsdcat_test",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        for name in &names {
            fs::copy(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/volumes")
                    .join(name),
                dir.path().join(name),
            )
            .unwrap();
        }
        let selected = dir.path().join(last);
        let info = inspect(&selected).unwrap();
        assert_eq!(
            info.output_folder,
            if last.starts_with("rar5") {
                "rar5"
            } else {
                "sample"
            }
        );
        assert!(info.entries.iter().any(|entry| entry.path == expected));
        assert_eq!(
            test(&selected).unwrap().files,
            info.entries
                .iter()
                .filter(|entry| entry.kind == EntryKind::File)
                .count()
        );
        let output = extract(&selected, dir.path(), OverwritePolicy::Refuse).unwrap();
        assert!(PathBuf::from(output.output_path.unwrap())
            .join(expected)
            .is_file());
        crate::tasks::test_context(Some(vec![expected.into()]), None, |_| {
            let result = extract(&selected, dir.path(), OverwritePolicy::KeepBoth).unwrap();
            assert_eq!(result.files, 1);
            let root = PathBuf::from(result.output_path.unwrap());
            assert!(root.join(expected).is_file());
            for entry in info
                .entries
                .iter()
                .filter(|e| e.kind == EntryKind::File && e.path != expected)
            {
                assert!(!root.join(&entry.path).exists());
            }
        });
        fs::remove_file(dir.path().join(names[1])).unwrap();
        assert_eq!(inspect(&selected).unwrap_err().code, "missing_volume");
    }
}

#[test]
fn cancellation_before_commit_preserves_existing_destination() {
    use std::sync::atomic::Ordering;
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("output")).unwrap();
    fs::write(dir.path().join("output/original"), b"keep").unwrap();
    crate::tasks::test_context(None, None, |control| {
        let mut plan = output::Plan::default();
        plan.add("new.txt", EntryKind::File).unwrap();
        let result = output::extract_to(
            dir.path(),
            "output",
            OverwritePolicy::Replace,
            &plan,
            |out| {
                out.new_file("new.txt")?.write_all(b"partial").unwrap();
                control.cancelled.store(true, Ordering::Release);
                Ok(())
            },
        );
        assert_eq!(result.unwrap_err().code, "cancelled");
    });
    assert_eq!(
        fs::read(dir.path().join("output/original")).unwrap(),
        b"keep"
    );
    assert!(!dir.path().join("output/new.txt").exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn cancellation_during_replacement_rolls_back_committed_files() {
    use std::sync::atomic::Ordering;
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("replace.tar");
    fs::write(&archive, tar_bytes("same.txt", b"new", b'0')).unwrap();
    let output = dir.path().join("replace");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("same.txt"), b"original").unwrap();
    fs::write(output.join("unrelated"), b"keep").unwrap();
    crate::tasks::test_context(None, None, |control| {
        control.cancel_after_commit.store(true, Ordering::Release);
        assert_eq!(
            extract(&archive, dir.path(), OverwritePolicy::Replace)
                .unwrap_err()
                .code,
            "cancelled"
        );
    });
    assert_eq!(fs::read(output.join("same.txt")).unwrap(), b"original");
    assert_eq!(fs::read(output.join("unrelated")).unwrap(), b"keep");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn python_default_pax_tar_and_gzip_extract_unicode_nested_paths() {
    let dir = tempfile::tempdir().unwrap();
    for (name, bytes) in [
        (
            "pax.tar",
            &include_bytes!("../../tests/fixtures/python-pax.tar")[..],
        ),
        (
            "pax.tar.gz",
            &include_bytes!("../../tests/fixtures/python-pax.tar.gz")[..],
        ),
    ] {
        let source = dir.path().join(name);
        fs::write(&source, bytes).unwrap();
        let info = inspect(&source).unwrap();
        let entry = info
            .entries
            .iter()
            .find(|e| e.kind == EntryKind::File)
            .unwrap();
        assert!(entry.path.ends_with("café-日本.txt"));
        assert!(entry.path.len() > 100);
        assert_eq!(
            entry.modified.as_deref(),
            Some("1700000000.125 (Unix seconds)")
        );
        test(&source).unwrap();
        let result = extract(&source, dir.path(), OverwritePolicy::KeepBoth).unwrap();
        assert_eq!(
            fs::read(PathBuf::from(result.output_path.unwrap()).join(&entry.path)).unwrap(),
            b"real Python PAX payload\n"
        );
    }
}

fn pax_record(key: &str, value: &str) -> Vec<u8> {
    let body = format!(" {key}={value}\n");
    let mut length = body.len() + 1;
    loop {
        let next = body.len() + length.to_string().len();
        if length == next {
            return format!("{length}{body}").into_bytes();
        }
        length = next;
    }
}

#[test]
fn pax_overrides_cannot_escape_limits_or_hide_unsafe_entries() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("attack.tar");
    for (records, kind, code) in [
        (pax_record("path", "../escape"), b'0', "unsafe_path"),
        (pax_record("path", "C:/escape"), b'0', "unsafe_path"),
        (pax_record("path", "/escape"), b'0', "unsafe_path"),
        (pax_record("path", "dir/file:ads"), b'0', "unsafe_path"),
        (pax_record("path", &"d/".repeat(33)), b'0', "unsafe_path"),
        (pax_record("size", "8589934593"), b'0', "archive_limit"),
        (pax_record("path", "safe"), b'2', "unsafe_entry"),
        (
            pax_record("GNU.sparse.size", "1"),
            b'0',
            "unsupported_variant",
        ),
        (b"99 path=invalid\n".to_vec(), b'0', "invalid_pax"),
        (
            [pax_record("path", "one"), pax_record("path", "two")].concat(),
            b'0',
            "invalid_pax",
        ),
    ] {
        let header = tar_bytes("././@PaxHeader", &records, b'x');
        let mut archive = header[..header.len() - 1024].to_vec();
        archive.extend(tar_bytes("raw", b"", kind));
        fs::write(&source, archive).unwrap();
        assert_eq!(
            extract(&source, dir.path(), OverwritePolicy::Refuse)
                .unwrap_err()
                .code,
            code
        );
        assert!(!dir.path().join("attack").exists());
    }
}

#[test]
fn session_password_reused_then_invalidated_on_close_change_and_wrong_password() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("secret.zip");
    let other = dir.path().join("other.zip");
    let fixture = include_bytes!("../../tests/fixtures/encrypted-aes.zip");
    fs::write(&source, fixture).unwrap();
    fs::write(&other, fixture).unwrap();
    let run = |path: &std::path::Path, password: Option<&str>| {
        crate::tasks::test_context(None, password.map(str::to_owned), |_| {
            crate::tasks::set_session_id("session-reuse-test");
            crate::sessions::with_archive(path, || inspect(path))
        })
    };
    run(&source, Some("fixture-only")).unwrap();
    run(&source, None).unwrap();
    crate::tasks::test_context(Some(vec!["secret.txt".into()]), None, |_| {
        crate::tasks::set_session_id("session-reuse-test");
        crate::sessions::with_archive(&source, || test(&source)).unwrap();
        crate::sessions::with_archive(&source, || {
            extract(&source, dir.path(), OverwritePolicy::Refuse)
        })
        .unwrap();
    });
    assert_eq!(
        run(&source, Some("wrong")).unwrap_err().code,
        "incorrect_password"
    );
    assert_eq!(run(&source, None).unwrap_err().code, "password_required");
    run(&source, Some("fixture-only")).unwrap();
    assert_eq!(run(&other, None).unwrap_err().code, "password_required");
    run(&source, Some("fixture-only")).unwrap();
    crate::sessions::close("session-reuse-test");
    assert_eq!(run(&source, None).unwrap_err().code, "password_required");
    run(&source, Some("fixture-only")).unwrap();
    fs::write(
        &source,
        include_bytes!("../../tests/fixtures/encrypted-zipcrypto.zip"),
    )
    .unwrap();
    assert_eq!(run(&source, None).unwrap_err().code, "password_required");
    crate::sessions::close("session-reuse-test");
}

#[test]
fn extract_here_routes_through_shared_conflicts_numbering_and_rollback() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.tar");
    fs::write(&source, tar_bytes("nested/file.txt", b"new", b'0')).unwrap();
    fs::create_dir(dir.path().join("nested")).unwrap();
    fs::write(dir.path().join("nested/file.txt"), b"old").unwrap();
    fs::write(dir.path().join("unrelated"), b"keep").unwrap();
    assert!(here_conflict(&source, dir.path()).unwrap());
    crate::tasks::test_context(None, None, |_| {
        crate::tasks::set_extract_here();
        assert_eq!(
            extract(&source, dir.path(), OverwritePolicy::Refuse)
                .unwrap_err()
                .code,
            "destination_exists"
        );
        assert_eq!(
            fs::read(dir.path().join("nested/file.txt")).unwrap(),
            b"old"
        );
        let copy = extract(&source, dir.path(), OverwritePolicy::KeepBoth).unwrap();
        assert_eq!(
            PathBuf::from(copy.output_path.unwrap()),
            dir.path().join("source (1)")
        );
        extract(&source, dir.path(), OverwritePolicy::Replace).unwrap();
        assert_eq!(
            fs::read(dir.path().join("nested/file.txt")).unwrap(),
            b"new"
        );
        assert_eq!(fs::read(dir.path().join("unrelated")).unwrap(), b"keep");
    });
    fs::write(dir.path().join("nested/file.txt"), b"before cancel").unwrap();
    crate::tasks::test_context(None, None, |control| {
        crate::tasks::set_extract_here();
        control
            .cancel_after_commit
            .store(true, std::sync::atomic::Ordering::Release);
        assert_eq!(
            extract(&source, dir.path(), OverwritePolicy::Replace)
                .unwrap_err()
                .code,
            "cancelled"
        );
    });
    assert_eq!(
        fs::read(dir.path().join("nested/file.txt")).unwrap(),
        b"before cancel"
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 4);
}
#[test]
fn extract_here_without_conflicts_and_source_archive_protection() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("input.tar");
    fs::write(&source, tar_bytes("file.txt", b"actual", b'0')).unwrap();
    assert!(!here_conflict(&source, dir.path()).unwrap());
    crate::tasks::test_context(None, None, |_| {
        crate::tasks::set_extract_here();
        let result = extract(&source, dir.path(), OverwritePolicy::Refuse).unwrap();
        assert_eq!(PathBuf::from(result.output_path.unwrap()), dir.path());
    });
    fs::write(
        &source,
        tar_bytes("input.tar", b"cannot replace source", b'0'),
    )
    .unwrap();
    let original = fs::read(&source).unwrap();
    crate::tasks::test_context(None, None, |_| {
        crate::tasks::set_extract_here();
        assert_eq!(
            extract(&source, dir.path(), OverwritePolicy::Replace)
                .unwrap_err()
                .code,
            "source_conflict"
        );
    });
    assert_eq!(fs::read(&source).unwrap(), original);
}
