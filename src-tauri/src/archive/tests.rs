// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use super::*;
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
};
use tempfile::TempDir;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

struct Fixture {
    root: TempDir,
    zip: PathBuf,
    destination: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        // Use a different extension to exercise structure-based detection.
        let zip = root.path().join("fixture.data");
        let destination = root.path().join("destination");
        fs::create_dir(&destination).unwrap();
        Self {
            root,
            zip,
            destination,
        }
    }

    fn write(&self, populate: impl FnOnce(&mut ZipWriter<File>)) {
        let mut zip = ZipWriter::new(File::create(&self.zip).unwrap());
        populate(&mut zip);
        zip.finish().unwrap();
    }

    fn mutate(&self, edit: impl FnOnce(&mut Vec<u8>)) {
        let mut bytes = fs::read(&self.zip).unwrap();
        edit(&mut bytes);
        fs::write(&self.zip, bytes).unwrap();
    }

    fn extract(&self) -> Result<OperationReport> {
        extract(&self.zip, &self.destination, OverwritePolicy::Refuse)
    }

    fn assert_no_output(&self) {
        assert_eq!(self.destination.read_dir().unwrap().count(), 0);
    }
}

fn stored() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(CompressionMethod::Stored)
}

fn add(zip: &mut ZipWriter<File>, name: &str, contents: &[u8]) {
    zip.start_file(name, stored()).unwrap();
    zip.write_all(contents).unwrap();
}

fn signature(bytes: &[u8], magic: &[u8]) -> usize {
    bytes
        .windows(magic.len())
        .position(|value| value == magic)
        .unwrap()
}

fn central(bytes: &[u8]) -> usize {
    signature(bytes, b"PK\x01\x02")
}
fn eocd(bytes: &[u8]) -> usize {
    signature(bytes, b"PK\x05\x06")
}

#[test]
fn zip_directory_inspection_and_extraction() {
    let f = Fixture::new();
    f.write(|zip| {
        zip.add_directory("a/empty/", stored()).unwrap();
        add(zip, "a/日本語/café.txt", "内容".as_bytes());
        zip.start_file(
            "compressed.txt",
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(&[b'x'; 4096]).unwrap();
    });

    let metadata = inspect(&f.zip).unwrap();
    assert_eq!(metadata.capabilities.format, "ZIP");
    assert_eq!(metadata.entries.len(), 3);
    assert_eq!(metadata.entries[0].kind, EntryKind::Directory);
    assert_eq!(metadata.entries[1].path, "a/日本語/café.txt");
    assert_eq!(metadata.entries[1].uncompressed_bytes, 6);
    assert_eq!(metadata.entries[2].method, "Deflated");
    assert!(metadata.entries.iter().all(|entry| entry.issue.is_none()));

    let tested = test(&f.zip).unwrap();
    assert_eq!((tested.files, tested.directories), (2, 1));
    assert_eq!(tested.processed_bytes, 4102);
    assert!(tested.output_path.is_none());
    f.assert_no_output();

    let report = f.extract().unwrap();
    assert_eq!((report.files, report.directories), (2, 3));
    let root = PathBuf::from(report.output_path.unwrap());
    assert!(root.join("a/empty").is_dir());
    assert_eq!(
        fs::read_to_string(root.join("a/日本語/café.txt")).unwrap(),
        "内容"
    );
    assert_eq!(
        fs::read(root.join("compressed.txt")).unwrap(),
        vec![b'x'; 4096]
    );
}

#[test]
fn zip64_entry_and_end_records() {
    let f = Fixture::new();
    f.write(|zip| {
        zip.start_file("zip64.txt", stored().large_file(true))
            .unwrap();
        zip.write_all(b"zip64 data").unwrap();
    });
    f.mutate(|bytes| {
        let end = eocd(bytes);
        let mut footer = bytes.split_off(end);
        let count = u16::from_le_bytes(footer[10..12].try_into().unwrap()) as u64;
        let size = u32::from_le_bytes(footer[12..16].try_into().unwrap()) as u64;
        let offset = u32::from_le_bytes(footer[16..20].try_into().unwrap()) as u64;
        bytes.extend_from_slice(b"PK\x06\x06");
        bytes.extend_from_slice(&44_u64.to_le_bytes());
        bytes.extend_from_slice(&45_u16.to_le_bytes());
        bytes.extend_from_slice(&45_u16.to_le_bytes());
        bytes.extend_from_slice(&[0; 8]);
        for value in [count, count, size, offset] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(b"PK\x06\x07");
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&(end as u64).to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        footer[8..20].fill(255);
        bytes.extend(footer);
    });
    assert_eq!(inspect(&f.zip).unwrap().entries[0].uncompressed_bytes, 10);
    assert_eq!(test(&f.zip).unwrap().files, 1);
    let output = PathBuf::from(f.extract().unwrap().output_path.unwrap());
    assert_eq!(fs::read(output.join("zip64.txt")).unwrap(), b"zip64 data");
}

#[test]
fn empty_archive_and_cp437_name() {
    let f = Fixture::new();
    f.write(|_| {});
    assert!(inspect(&f.zip).unwrap().entries.is_empty());
    assert_eq!(test(&f.zip).unwrap().files, 0);
    f.write(|zip| add(zip, "x.txt", b"legacy"));
    f.mutate(|bytes| {
        let index = central(bytes);
        bytes[30] = 0x82;
        bytes[index + 46] = 0x82;
        bytes[7] &= !8;
        bytes[index + 9] &= !8;
    });
    assert_eq!(inspect(&f.zip).unwrap().entries[0].path, "é.txt");
    let root = PathBuf::from(f.extract().unwrap().output_path.unwrap());
    assert_eq!(fs::read(root.join("é.txt")).unwrap(), b"legacy");
}

#[test]
fn large_stored_entry_streams_across_many_buffers() {
    const BLOCKS: usize = 256;
    let f = Fixture::new();
    let mut block = [0x42; 64 * 1024];
    f.write(|zip| {
        zip.start_file("large.dat", stored()).unwrap();
        for n in 0..BLOCKS {
            block[0] = n as u8;
            zip.write_all(&block).unwrap();
        }
    });
    let report = f.extract().unwrap();
    assert_eq!(report.processed_bytes, (BLOCKS * block.len()) as u64);
    let mut file =
        File::open(PathBuf::from(report.output_path.unwrap()).join("large.dat")).unwrap();
    for n in 0..BLOCKS {
        file.read_exact(&mut block).unwrap();
        assert_eq!(block[0], n as u8);
        assert!(block[1..].iter().all(|byte| *byte == 0x42));
    }
    assert_eq!(file.read(&mut block).unwrap(), 0);
}

#[test]
fn corrupt_structure_and_local_header_are_rejected() {
    let f = Fixture::new();
    fs::write(&f.zip, b"not a zip").unwrap();
    assert_eq!(inspect(&f.zip).unwrap_err().code, "unsupported_format");
    f.write(|zip| add(zip, "data", b"hello"));
    f.mutate(|bytes| bytes[0] = 0);
    assert_eq!(inspect(&f.zip).unwrap_err().code, "invalid_zip");
    f.assert_no_output();
}

#[test]
fn bad_crc_is_detected_and_extraction_rolls_back_created_output() {
    let f = Fixture::new();
    f.write(|zip| {
        add(zip, "first/good.txt", b"good");
        add(zip, "second/bad.txt", b"payload to corrupt");
    });
    f.mutate(|bytes| {
        let at = signature(bytes, b"payload to corrupt");
        bytes[at] ^= 1;
    });
    // Directory inspection intentionally does not read or verify the payload.
    assert_eq!(inspect(&f.zip).unwrap().entries.len(), 2);
    assert_eq!(test(&f.zip).unwrap_err().code, "corrupt_entry");
    assert_eq!(f.extract().unwrap_err().code, "corrupt_entry");
    f.assert_no_output();
}

#[test]
fn unsupported_and_encrypted_are_not_reported_as_corruption() {
    let f = Fixture::new();
    for (encrypted, issue, code) in [
        (
            false,
            IssueCode::UnsupportedCompression,
            "unsupported_compression",
        ),
        (true, IssueCode::Encrypted, "password_required"),
    ] {
        f.write(|zip| add(zip, "data", b"hello"));
        f.mutate(|bytes| {
            let index = central(bytes);
            if encrypted {
                bytes[6] |= 1;
                bytes[index + 8] |= 1;
            } else {
                bytes[8..10].copy_from_slice(&133_u16.to_le_bytes());
                bytes[index + 10..index + 12].copy_from_slice(&133_u16.to_le_bytes());
            }
        });
        if encrypted {
            assert_eq!(inspect(&f.zip).unwrap_err().code, "password_required");
        } else {
            assert_eq!(
                inspect(&f.zip).unwrap().entries[0]
                    .issue
                    .as_ref()
                    .unwrap()
                    .code,
                issue
            );
        }
        assert_eq!(test(&f.zip).unwrap_err().code, code);
        assert_eq!(f.extract().unwrap_err().code, code);
        f.assert_no_output();
    }
}

#[test]
fn unsafe_paths_fail_before_creating_output() {
    let f = Fixture::new();
    for name in [
        "../escape",
        "/absolute",
        "C:/drive",
        "x:stream",
        "a\\b",
        "a/../b",
        "a//b",
        "a./b",
        "NUL.txt",
        "LPT¹.txt",
    ] {
        f.write(|zip| add(zip, name, b"hello"));
        let inspected = inspect(&f.zip).unwrap();
        assert_eq!(
            inspected.entries[0].issue.as_ref().unwrap().code,
            IssueCode::UnsafePath,
            "{name}"
        );
        assert_eq!(f.extract().unwrap_err().code, "unsafe_path", "{name}");
        f.assert_no_output();
    }
    assert!(!f.root.path().join("escape").exists());
}

#[test]
fn links_and_special_entries_are_rejected() {
    let f = Fixture::new();
    f.write(|zip| zip.add_symlink("link", "../escape", stored()).unwrap());
    assert_eq!(inspect(&f.zip).unwrap().entries[0].kind, EntryKind::Link);
    assert_eq!(f.extract().unwrap_err().code, "unsafe_entry");
    f.assert_no_output();

    f.write(|zip| add(zip, "fifo", b"hello"));
    f.mutate(|bytes| {
        let index = central(bytes);
        bytes[index + 5] = 3; // Unix origin.
        bytes[index + 38..index + 42].copy_from_slice(&(0o010644_u32 << 16).to_le_bytes());
    });
    assert_eq!(inspect(&f.zip).unwrap().entries[0].kind, EntryKind::Special);
    assert_eq!(f.extract().unwrap_err().code, "unsafe_entry");
    f.assert_no_output();
}

#[test]
fn existing_output_files_and_folders_are_never_reused() {
    let f = Fixture::new();
    f.write(|zip| add(zip, "existing", b"new"));
    let root = f.destination.join(inspect(&f.zip).unwrap().output_folder);
    fs::create_dir(&root).unwrap();
    fs::write(root.join("existing"), b"original").unwrap();
    assert_eq!(f.extract().unwrap_err().code, "destination_exists");
    assert_eq!(fs::read(root.join("existing")).unwrap(), b"original");
    fs::remove_file(root.join("existing")).unwrap();
    fs::remove_dir(&root).unwrap();
    fs::write(&root, b"file at output path").unwrap();
    assert_eq!(f.extract().unwrap_err().code, "destination_exists");
    assert_eq!(fs::read(root).unwrap(), b"file at output path");
}

#[test]
fn conflicting_paths_and_duplicate_records_are_rejected() {
    let f = Fixture::new();
    for (one, two) in [
        ("Case/a", "case/b"),
        ("parent", "parent/a"),
        ("é/a", "e\u{301}/b"),
    ] {
        f.write(|zip| {
            add(zip, one, b"one");
            add(zip, two, b"two");
        });
        assert_eq!(f.extract().unwrap_err().code, "path_conflict");
        f.assert_no_output();
    }
    f.write(|zip| {
        add(zip, "first", b"one");
        add(zip, "other", b"two");
    });
    f.mutate(|bytes| {
        let second = central(bytes) + 51;
        assert_eq!(&bytes[second..second + 4], b"PK\x01\x02");
        bytes[second + 46..second + 51].copy_from_slice(b"first");
    });
    assert_eq!(inspect(&f.zip).unwrap_err().code, "path_conflict");
}

#[test]
fn entry_count_and_decompression_limits_are_enforced() {
    let f = Fixture::new();
    f.write(|zip| add(zip, "data", b"tiny"));
    f.mutate(|bytes| {
        let index = eocd(bytes);
        bytes[index + 8..index + 10].copy_from_slice(&20_001_u16.to_le_bytes());
        bytes[index + 10..index + 12].copy_from_slice(&20_001_u16.to_le_bytes());
    });
    assert_eq!(inspect(&f.zip).unwrap_err().code, "archive_limit");

    f.write(|zip| add(zip, "data", b"tiny"));
    f.mutate(|bytes| {
        let index = central(bytes);
        bytes[index + 24..index + 28].copy_from_slice(&2_000_000_u32.to_le_bytes());
    });
    assert_eq!(test(&f.zip).unwrap_err().code, "archive_limit");
    assert_eq!(f.extract().unwrap_err().code, "archive_limit");
    f.assert_no_output();
}

#[cfg(unix)]
#[test]
fn existing_destination_symlink_is_not_followed() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    f.write(|zip| add(zip, "data", b"hello"));
    let link = f.root.path().join("link");
    symlink(&f.destination, &link).unwrap();
    assert_eq!(
        extract(&f.zip, &link, OverwritePolicy::Refuse)
            .unwrap_err()
            .code,
        "invalid_destination"
    );
    f.assert_no_output();
}

#[test]
fn clean_names_preserve_spaces_unicode_and_periods() {
    for (name, suffixes, expected) in [
        ("Project_Backup.zip", &[".zip"][..], "Project_Backup"),
        ("Photos.2026.ZIP", &[".zip"][..], "Photos.2026"),
        ("日本語 archive.zip", &[".zip"][..], "日本語 archive"),
        (
            "Land Rover Range Rover 2022.rar",
            &[".rar"][..],
            "Land Rover Range Rover 2022",
        ),
        ("Backup.tar.gz", &[".gz", ".tar.gz"][..], "Backup"),
        ("CON.zip", &[".zip"][..], "_CON"),
        ("bad:name. .zip", &[".zip"][..], "bad_name"),
        (".zip", &[".zip"][..], "Archive"),
    ] {
        assert_eq!(output::output_folder(name, suffixes), expected, "{name}");
    }
}

#[test]
fn checking_conflicts_and_cancelling_changes_nothing() {
    let f = Fixture::new();
    f.write(|zip| add(zip, "same.txt", b"new"));
    let root = f.destination.join("fixture");
    assert!(!destination_exists(&f.zip, &f.destination).unwrap());
    f.assert_no_output();
    fs::create_dir(&root).unwrap();
    fs::write(root.join("same.txt"), b"original").unwrap();
    assert!(destination_exists(&f.zip, &f.destination).unwrap());
    // Cancel dismisses the frontend prompt without invoking extraction.
    assert_eq!(f.destination.read_dir().unwrap().count(), 1);
    assert_eq!(fs::read(root.join("same.txt")).unwrap(), b"original");
    assert_eq!(root.read_dir().unwrap().count(), 1);
}

#[test]
fn replacement_merges_only_archive_paths() {
    let f = Fixture::new();
    f.write(|zip| {
        add(zip, "nested/same.txt", b"new");
        add(zip, "nested/added.txt", b"added");
        zip.add_directory("empty/", stored()).unwrap();
    });
    let root = f.destination.join("fixture");
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(root.join("nested/same.txt"), b"original").unwrap();
    fs::write(root.join("nested/unrelated.txt"), b"keep").unwrap();
    let result = extract(&f.zip, &f.destination, OverwritePolicy::Replace).unwrap();
    assert_eq!(PathBuf::from(result.output_path.unwrap()), root);
    assert_eq!(fs::read(root.join("nested/same.txt")).unwrap(), b"new");
    assert_eq!(fs::read(root.join("nested/added.txt")).unwrap(), b"added");
    assert_eq!(
        fs::read(root.join("nested/unrelated.txt")).unwrap(),
        b"keep"
    );
    assert!(root.join("empty").is_dir());
    assert_eq!(f.destination.read_dir().unwrap().count(), 1);
}

#[test]
fn keep_both_fills_first_gap_and_does_not_merge() {
    let f = Fixture::new();
    f.write(|zip| add(zip, "same.txt", b"new"));
    for name in ["fixture", "fixture (1)", "fixture (3)"] {
        let dir = f.destination.join(name);
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("same.txt"), b"original").unwrap();
    }
    for expected in ["fixture (2)", "fixture (4)"] {
        let result = extract(&f.zip, &f.destination, OverwritePolicy::KeepBoth).unwrap();
        assert_eq!(
            PathBuf::from(result.output_path.unwrap()),
            f.destination.join(expected)
        );
        assert_eq!(
            fs::read(f.destination.join(expected).join("same.txt")).unwrap(),
            b"new"
        );
    }
    for name in ["fixture", "fixture (1)", "fixture (3)"] {
        assert_eq!(
            fs::read(f.destination.join(name).join("same.txt")).unwrap(),
            b"original"
        );
    }
}

#[test]
fn concurrent_keep_both_requests_reserve_different_folders() {
    let f = Fixture::new();
    f.write(|zip| add(zip, "file", b"content"));
    fs::create_dir(f.destination.join("fixture")).unwrap();
    std::thread::scope(|scope| {
        let first =
            scope.spawn(|| extract(&f.zip, &f.destination, OverwritePolicy::KeepBoth).unwrap());
        let second =
            scope.spawn(|| extract(&f.zip, &f.destination, OverwritePolicy::KeepBoth).unwrap());
        let mut paths = [
            first.join().unwrap().output_path.unwrap(),
            second.join().unwrap().output_path.unwrap(),
        ];
        paths.sort();
        assert_eq!(PathBuf::from(&paths[0]), f.destination.join("fixture (1)"));
        assert_eq!(PathBuf::from(&paths[1]), f.destination.join("fixture (2)"));
    });
}

#[test]
fn failed_replacement_does_not_modify_existing_files_or_leave_staging() {
    let f = Fixture::new();
    f.write(|zip| {
        add(zip, "a.txt", b"replacement");
        add(zip, "b.txt", b"damage-this-payload");
    });
    let root = f.destination.join("fixture");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.txt"), b"original").unwrap();
    f.mutate(|bytes| {
        let at = signature(bytes, b"damage-this-payload");
        bytes[at] ^= 1;
    });
    assert_eq!(
        extract(&f.zip, &f.destination, OverwritePolicy::Replace)
            .unwrap_err()
            .code,
        "corrupt_entry"
    );
    assert_eq!(fs::read(root.join("a.txt")).unwrap(), b"original");
    assert!(!root.join("b.txt").exists());
    assert_eq!(f.destination.read_dir().unwrap().count(), 1);
}

#[test]
fn replacement_refuses_file_folder_conflicts_and_rolls_back_new_directories() {
    let f = Fixture::new();
    f.write(|zip| {
        add(zip, "a.txt", b"replacement");
        add(zip, "created/folder/new.txt", b"new");
        add(zip, "z.txt", b"blocked");
    });
    let root = f.destination.join("fixture");
    fs::create_dir_all(root.join("z.txt")).unwrap();
    fs::write(root.join("a.txt"), b"original").unwrap();
    fs::write(root.join("z.txt/unrelated.txt"), b"keep").unwrap();
    assert_eq!(
        extract(&f.zip, &f.destination, OverwritePolicy::Replace)
            .unwrap_err()
            .code,
        "destination_conflict"
    );
    assert_eq!(fs::read(root.join("a.txt")).unwrap(), b"original");
    assert_eq!(fs::read(root.join("z.txt/unrelated.txt")).unwrap(), b"keep");
    assert!(!root.join("created").exists());
    assert_eq!(f.destination.read_dir().unwrap().count(), 1);
}

#[test]
fn replacement_and_keep_both_do_not_bypass_archive_path_checks() {
    let f = Fixture::new();
    let root = f.destination.join("fixture");
    fs::create_dir(&root).unwrap();
    for policy in [OverwritePolicy::Replace, OverwritePolicy::KeepBoth] {
        f.write(|zip| add(zip, "../escape", b"unsafe"));
        assert_eq!(
            extract(&f.zip, &f.destination, policy).unwrap_err().code,
            "unsafe_path"
        );
    }
    for policy in [OverwritePolicy::Replace, OverwritePolicy::KeepBoth] {
        f.write(|zip| zip.add_symlink("link", "../escape", stored()).unwrap());
        assert_eq!(
            extract(&f.zip, &f.destination, policy).unwrap_err().code,
            "unsafe_entry"
        );
    }
    assert_eq!(f.destination.read_dir().unwrap().count(), 1);
    assert_eq!(root.read_dir().unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn replacement_refuses_links_in_existing_output() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    f.write(|zip| add(zip, "linked/file", b"new"));
    let root = f.destination.join("fixture");
    let outside = f.root.path().join("outside");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("file"), b"original").unwrap();
    symlink(&outside, root.join("linked")).unwrap();
    assert!(extract(&f.zip, &f.destination, OverwritePolicy::Replace).is_err());
    assert_eq!(fs::read(outside.join("file")).unwrap(), b"original");
    assert_eq!(f.destination.read_dir().unwrap().count(), 1);
}

#[cfg(windows)]
#[test]
fn locked_replacement_preserves_original_files() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = Fixture::new();
    f.write(|zip| {
        add(zip, "a.txt", b"new-a");
        add(zip, "z.txt", b"new-z");
    });
    let root = f.destination.join("fixture");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.txt"), b"old-a").unwrap();
    fs::write(root.join("z.txt"), b"old-z").unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(root.join("z.txt"))
        .unwrap();
    assert!(extract(&f.zip, &f.destination, OverwritePolicy::Replace).is_err());
    drop(lock);
    assert_eq!(fs::read(root.join("a.txt")).unwrap(), b"old-a");
    assert_eq!(fs::read(root.join("z.txt")).unwrap(), b"old-z");
    assert_eq!(f.destination.read_dir().unwrap().count(), 1);
}

#[cfg(windows)]
#[test]
fn destination_junctions_are_rejected_without_touching_the_target() {
    let f = Fixture::new();
    let outside = f.root.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("valuable.txt"), b"original").unwrap();
    f.write(|zip| add(zip, "nested/valuable.txt", b"new"));
    let target = f.destination.join("fixture");
    fs::create_dir(&target).unwrap();
    let link = target.join("nested");
    let command = std::process::Command::new("cmd.exe")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&outside)
        .output()
        .unwrap();
    assert!(command.status.success());
    assert!(extract(&f.zip, &f.destination, OverwritePolicy::Replace).is_err());
    assert_eq!(fs::read(outside.join("valuable.txt")).unwrap(), b"original");
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
    fs::remove_dir(&link).unwrap();
    fs::remove_dir(&target).unwrap();
    let command = std::process::Command::new("cmd.exe")
        .args(["/C", "mklink", "/J"])
        .arg(&target)
        .arg(&outside)
        .output()
        .unwrap();
    assert!(command.status.success());
    assert!(extract(&f.zip, &f.destination, OverwritePolicy::Replace).is_err());
    assert_eq!(fs::read(outside.join("valuable.txt")).unwrap(), b"original");
    fs::remove_dir(&target).unwrap();
}
