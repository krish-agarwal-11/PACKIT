// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use super::{creation::*, *};
use std::{fs, path::PathBuf};
fn options(root: &std::path::Path, format: Format, policy: OverwritePolicy) -> Options {
    Options {
        sources: vec![root.join("source")],
        destination: root.into(),
        filename: "Backup.tar.gz.zip".into(),
        format,
        preset: Preset::Balanced,
        policy,
    }
}
#[test]
fn creation_roundtrips_unicode_nested_empty_entries_and_protects_existing_outputs() {
    let formats = [Format::Zip, Format::Tar, Format::TarGz, Format::SevenZip];
    for format in formats {
        if format == Format::SevenZip && !cfg!(all(windows, target_arch = "x86_64")) {
            continue;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("source/nested/empty")).unwrap();
        fs::write(root.join("source/nested/café.txt"), b"original payload").unwrap();
        fs::write(root.join("source/empty.txt"), []).unwrap();
        let report = create(options(root, format, OverwritePolicy::Refuse)).unwrap();
        let archive = PathBuf::from(report.output_path.unwrap());
        assert!(archive
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("Backup."));
        let before = fs::read(&archive).unwrap();
        assert_eq!(
            create(options(root, format, OverwritePolicy::Refuse))
                .unwrap_err()
                .code,
            "destination_exists"
        );
        assert_eq!(fs::read(&archive).unwrap(), before);
        let info = inspect(&archive).unwrap();
        assert!(info
            .entries
            .iter()
            .any(|e| e.path.trim_end_matches('/') == "source/nested/empty"));
        assert_eq!(test(&archive).unwrap().files, 2);
        let out = extract(&archive, root, OverwritePolicy::KeepBoth).unwrap();
        assert_eq!(
            fs::read(PathBuf::from(out.output_path.unwrap()).join("source/nested/café.txt"))
                .unwrap(),
            b"original payload"
        );
        let copy = create(options(root, format, OverwritePolicy::KeepBoth)).unwrap();
        assert!(copy.output_path.unwrap().contains("Backup (1)."));
        fs::write(root.join("source/nested/café.txt"), b"replacement").unwrap();
        create(options(root, format, OverwritePolicy::Replace)).unwrap();
        assert_ne!(fs::read(&archive).unwrap(), before);
    }
}
#[test]
fn creation_presets_and_naming_are_real_and_portable() {
    assert_eq!(
        filename("Photos.2026.zip.zip", Format::Zip).unwrap(),
        "Photos.2026.zip"
    );
    assert_eq!(
        filename("Backup.tar.gz", Format::TarGz).unwrap(),
        "Backup.tar.gz"
    );
    assert!(filename("../escape", Format::Tar).is_err());
    for format in [Format::Zip, Format::TarGz, Format::SevenZip] {
        if format == Format::SevenZip && !cfg!(all(windows, target_arch = "x86_64")) {
            continue;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("source")).unwrap();
        fs::write(
            root.join("source/payload"),
            b"repeated content for compression ".repeat(2048),
        )
        .unwrap();
        for preset in [Preset::Fast, Preset::Balanced, Preset::Maximum] {
            let mut config = options(root, format, OverwritePolicy::KeepBoth);
            config.preset = preset;
            let result = create(config).unwrap();
            let output = PathBuf::from(result.output_path.unwrap());
            assert!(output.metadata().unwrap().len() < 33 * 2048);
            assert_eq!(test(&output).unwrap().processed_bytes, 33 * 2048);
        }
    }
}
#[test]
fn creation_cancellation_and_source_links_preserve_original_archive() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("source")).unwrap();
    fs::write(root.join("source/file"), b"payload").unwrap();
    fs::write(root.join("Backup.zip"), b"original").unwrap();
    crate::tasks::test_context(None, None, |control| {
        control
            .cancelled
            .store(true, std::sync::atomic::Ordering::Release);
        assert_eq!(
            create(options(root, Format::Zip, OverwritePolicy::Replace))
                .unwrap_err()
                .code,
            "cancelled"
        );
    });
    assert_eq!(fs::read(root.join("Backup.zip")).unwrap(), b"original");
    #[cfg(windows)]
    {
        let status = std::process::Command::new("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(root.join("source").join("link"))
            .arg(root.join("source"))
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "Could not create the unprivileged junction fixture: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("source/file"), root.join("source").join("link"))
            .unwrap();
    }
    assert_eq!(
        create(options(root, Format::Zip, OverwritePolicy::Replace))
            .unwrap_err()
            .code,
        "unsafe_source"
    );
    assert_eq!(fs::read(root.join("Backup.zip")).unwrap(), b"original");
}
#[test]
fn creation_excludes_output_from_source_tree() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("source")).unwrap();
    fs::write(root.join("source/file"), b"content").unwrap();
    fs::write(root.join("source/Backup.zip"), b"old").unwrap();
    let mut config = options(root, Format::Zip, OverwritePolicy::Replace);
    config.destination = root.join("source");
    let result = create(config).unwrap();
    let info = inspect(&PathBuf::from(result.output_path.unwrap())).unwrap();
    assert!(!info.entries.iter().any(|e| e.path.ends_with("Backup.zip")));
    assert_eq!(
        info.entries
            .iter()
            .filter(|e| e.kind == EntryKind::File)
            .count(),
        1
    );
}

#[test]
fn cancellation_during_snapshot_compression_and_commit_preserves_output() {
    for (format, phase) in [
        (Format::Zip, "preparing"),
        (Format::Zip, "compressing"),
        (Format::Tar, "packaging"),
        (Format::TarGz, "compressing"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("source")).unwrap();
        fs::write(root.join("source/file"), vec![7; 128 * 1024]).unwrap();
        let target = root.join(filename("Backup", format).unwrap());
        fs::write(&target, b"original").unwrap();
        crate::tasks::test_context(None, None, |control| {
            *control.cancel_byte_phase.lock().unwrap() = Some(phase.into());
            assert_eq!(
                create(options(root, format, OverwritePolicy::Replace))
                    .unwrap_err()
                    .code,
                "cancelled"
            );
        });
        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert_eq!(fs::read_dir(root).unwrap().count(), 2);
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("source")).unwrap();
    fs::write(root.join("source/file"), b"data").unwrap();
    fs::write(root.join("Backup.zip"), b"original").unwrap();
    crate::tasks::test_context(None, None, |control| {
        control
            .cancel_after_commit
            .store(true, std::sync::atomic::Ordering::Release);
        assert_eq!(
            create(options(root, Format::Zip, OverwritePolicy::Replace))
                .unwrap_err()
                .code,
            "cancelled"
        );
    });
    assert_eq!(fs::read(root.join("Backup.zip")).unwrap(), b"original");
    assert_eq!(fs::read_dir(root).unwrap().count(), 2);
}

#[test]
fn naming_suggestions_preserve_folders_unicode_custom_names_and_selection_order() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Project Files.2026");
    fs::create_dir(&folder).unwrap();
    let file = dir.path().join("café movie.mp4");
    fs::write(&file, []).unwrap();
    assert_eq!(
        suggest_name(&folder, Format::Zip).unwrap(),
        "Project Files.2026.zip"
    );
    assert_eq!(
        suggest_name(&file, Format::SevenZip).unwrap(),
        "café movie.7z"
    );
    assert_eq!(
        filename("My custom.backup.zip", Format::SevenZip).unwrap(),
        "My custom.backup.7z"
    );
    assert_eq!(
        filename("日本 Files.tar.gz.zip", Format::TarGz).unwrap(),
        "日本 Files.tar.gz"
    );
    let received = [file.clone(), folder.clone()];
    assert_eq!(
        suggest_name(&received[0], Format::Zip).unwrap(),
        "café movie.zip"
    );
    let received = [folder, file];
    assert_eq!(
        suggest_name(&received[0], Format::SevenZip).unwrap(),
        "Project Files.2026.7z"
    );
}
