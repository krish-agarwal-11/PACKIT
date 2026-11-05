// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use super::*;
use std::{fs, path::PathBuf};
#[test]
fn real_udf_physical_partition_browses_unicode_and_selectively_extracts() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("sample.iso");
    fs::write(&source, include_bytes!("../../tests/fixtures/udf260.iso")).unwrap();
    let info = inspect(&source).unwrap();
    assert_eq!(info.capabilities.format, "UDF 1.02");
    assert!(info
        .entries
        .iter()
        .any(|e| e.path == "nested/café-日本.txt"));
    assert!(info.entries.iter().any(|e| e.path == "empty"));
    let report = test(&source).unwrap();
    assert_eq!(report.files, 2);
    crate::tasks::test_context(Some(vec!["nested".into()]), None, |_| {
        let result = extract(&source, root.path(), OverwritePolicy::Refuse).unwrap();
        assert_eq!(result.files, 1);
        let folder = PathBuf::from(result.output_path.unwrap());
        assert_eq!(
            fs::read(folder.join("nested/café-日本.txt")).unwrap(),
            b"PACKIT original UDF payload\n"
        );
        assert!(!folder.join("zero.txt").exists());
    });
    let out = extract(&source, root.path(), OverwritePolicy::KeepBoth).unwrap();
    let folder = PathBuf::from(out.output_path.unwrap());
    assert!(folder.join("empty").is_dir());
    assert_eq!(fs::metadata(folder.join("zero.txt")).unwrap().len(), 0);
}
