// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
//! Audit-only extraction through the existing contained decoder; never executes the installer.
use super::*;
#[test]
#[ignore = "requires official release archive and Windows LPAC permission"]
fn official_release_binary_provenance() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let input = workspace.join("experiments/license-audit/stage3b/7z2605-x64.exe");
    let output = workspace.join("experiments/license-audit/stage3b/official-binaries");
    std::fs::create_dir_all(&output).unwrap();
    let source = File::open(input).unwrap();
    let length = source.metadata().unwrap().len();
    let decoder = SevenZip::open(source, "official-26.05-installer".into(), length, "*").unwrap();
    for name in ["7z.exe", "7z.dll"] {
        let bytes = decoder
            .run("x", &["-so", "-bso0", "-bse2"], Some(name), |reader| {
                let mut bytes = Vec::new();
                reader
                    .take(16 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(common::damaged)?;
                assert!(bytes.len() <= 16 * 1024 * 1024);
                Ok(bytes)
            })
            .unwrap();
        assert!(!bytes.is_empty());
        assert_eq!(
            bytes,
            std::fs::read(workspace.join("src-tauri/vendor/7zip").join(name)).unwrap()
        );
        std::fs::write(output.join(name), bytes).unwrap();
    }
}

#[test]
#[ignore = "audit-only: requires the built private NSIS installer and Windows LPAC permission"]
fn private_installer_payload_matches_build_inputs() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let config: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("src-tauri/tauri.conf.json")).unwrap(),
    )
    .unwrap();
    let version = config["version"].as_str().unwrap();
    let path = root.join(format!("src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/PACKIT_{version}_x64-setup.exe"));
    set_resource_dir(root.join("src-tauri/vendor/7zip"));
    let source = File::open(path).unwrap();
    let length = source.metadata().unwrap().len();
    let decoder = SevenZip::open(source, "private-installer".into(), length, "*").unwrap();
    let listing = decoder
        .run("l", &["-slt"], None, |reader| {
            let mut bytes = Vec::new();
            reader
                .take(4 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(common::damaged)?;
            assert!(bytes.len() <= 4 * 1024 * 1024);
            Ok(String::from_utf8(bytes).unwrap())
        })
        .unwrap();
    std::fs::write(
        root.join("experiments/license-audit/stage4/installer-listing.txt"),
        &listing,
    )
    .unwrap();
    let names: Vec<_> = listing
        .lines()
        .filter_map(|line| line.strip_prefix("Path = "))
        .collect();
    let evidence: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("legal/evidence-manifest.json")).unwrap(),
    )
    .unwrap();
    for file in evidence["files"].as_array().unwrap() {
        let expected = file["file"].as_str().unwrap();
        assert!(
            names.iter().any(|name| {
                let name = name.replace('\\', "/");
                name == expected || name.ends_with(&format!("/{expected}"))
            }),
            "Missing legal notice: {expected}"
        );
    }
    decoder
        .run("t", &["-bso0", "-bse2"], None, |reader| {
            std::io::copy(&mut reader.take(1024 * 1024), &mut std::io::sink())
                .map_err(common::damaged)?;
            Ok(())
        })
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("src-tauri/vendor/shell/manifest.json")).unwrap(),
    )
    .unwrap();
    let shell = manifest["filename"].as_str().unwrap();
    let pairs = vec![
        (
            "vendor/7zip/7z.exe".to_string(),
            "src-tauri/vendor/7zip/7z.exe".to_string(),
        ),
        (
            "vendor/7zip/7z.dll".to_string(),
            "src-tauri/vendor/7zip/7z.dll".to_string(),
        ),
        (shell.to_string(), format!("src-tauri/vendor/shell/{shell}")),
        ("LICENSE".into(), "LICENSE".into()),
        (
            "THIRD_PARTY_NOTICES.md".into(),
            "THIRD_PARTY_NOTICES.md".into(),
        ),
        (
            "legal/SOURCE_DISTRIBUTION.md".into(),
            "legal/SOURCE_DISTRIBUTION.md".into(),
        ),
        (
            "packit.exe".into(),
            "src-tauri/target/x86_64-pc-windows-msvc/release/packit.exe".into(),
        ),
    ];
    for (suffix, expected) in pairs {
        let name = names
            .iter()
            .find(|name| {
                let name = name.replace('\\', "/");
                name == suffix || name.ends_with(&format!("/{suffix}"))
            })
            .unwrap_or_else(|| panic!("Missing installer member: {suffix}\n{listing}"));
        let mut expected = std::fs::read(root.join(expected)).unwrap();
        // Tauri intentionally patches its bundle marker in the packaged copy,
        // then restores the original build executable. No other byte may differ.
        if suffix == "packit.exe" {
            let before = b"__TAURI_BUNDLE_TYPE_VAR_UNK";
            let after = b"__TAURI_BUNDLE_TYPE_VAR_NSS";
            if let Some(offset) = expected
                .windows(before.len())
                .position(|bytes| bytes == before)
            {
                expected[offset..offset + before.len()].copy_from_slice(after);
            }
        }
        eprintln!("Checking packed member {suffix}: {name}");
        let actual = decoder
            .run("x", &["-so", "-bso0", "-bse2"], Some(name), |reader| {
                let mut bytes = Vec::new();
                reader
                    .take(expected.len() as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(common::damaged)?;
                Ok(bytes)
            })
            .unwrap();
        if suffix == "packit.exe" {
            std::fs::write(
                root.join("experiments/license-audit/stage4/packit-packaged.exe"),
                &actual,
            )
            .unwrap();
        }
        if actual != expected {
            let file = root.join("experiments/license-audit/stage4/mismatch-payload.bin");
            std::fs::write(file, &actual).unwrap();
            let differences: Vec<_> = actual
                .iter()
                .zip(&expected)
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .take(12)
                .map(|(i, _)| i)
                .collect();
            panic!("Installer payload differs: {suffix}; actual length {}, expected {}; first difference offsets {differences:?}",actual.len(),expected.len());
        }
    }
    let output = root.join("experiments/license-audit/stage4");
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(output.join("installer-listing.txt"), listing).unwrap();
}
