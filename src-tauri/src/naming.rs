// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use unicode_normalization::UnicodeNormalization;
fn safe_component(part: &str) -> bool {
    if part.is_empty()
        || part.len() > 255
        || part.ends_with(['.', ' '])
        || part
            .chars()
            .any(|ch| ch.is_control() || "<>:\"\\|?*".contains(ch))
    {
        return false;
    }
    let canonical: String = part.nfkc().collect();
    let device = canonical
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    !matches!(
        device.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$"
    ) && !((device.starts_with("COM") || device.starts_with("LPT"))
        && device.len() == 4
        && device.as_bytes()[3].is_ascii_digit())
}

pub fn output_folder(name: &str, suffixes: &[&str]) -> String {
    let lower = name.to_ascii_lowercase();
    let stem = suffixes
        .iter()
        .filter(|suffix| lower.ends_with(**suffix))
        .max_by_key(|suffix| suffix.len())
        .map(|suffix| &name[..name.len() - suffix.len()])
        .or_else(|| {
            name.rsplit_once('.')
                .filter(|(stem, _)| !stem.is_empty())
                .map(|(stem, _)| stem)
        })
        .unwrap_or(name);
    let mut safe = String::new();
    for ch in stem.chars() {
        if safe.len() + ch.len_utf8() > 240 {
            break;
        }
        safe.push(if ch.is_control() || "/<>:\"\\|?*".contains(ch) {
            '_'
        } else {
            ch
        });
    }
    let safe = safe.trim().trim_end_matches(['.', ' ']);
    if safe.is_empty() {
        return "Archive".into();
    }
    if safe_component(safe) {
        safe.into()
    } else {
        format!("_{safe}")
    }
}

pub const ACTION_NAMES: [&str; 7] = [
    "open",
    "extract_files",
    "extract_here",
    "extract_to",
    "add",
    "zip",
    "seven_zip",
];
pub const DYNAMIC_BASE: u128 = 0x7686c014_ef9b_40d5_b2af_f87ea6701100;
pub fn normalize(name: &str, extension: &str) -> String {
    let mut base = name;
    loop {
        let lower = base.to_ascii_lowercase();
        if let Some(suffix) = [".tar.gz", ".zip", ".7z", ".tar", ".tgz"]
            .iter()
            .find(|suffix| lower.ends_with(**suffix))
        {
            base = &base[..base.len() - suffix.len()];
        } else {
            break;
        }
    }
    format!("{base}{extension}")
}
pub fn creation_name(name: &str, directory: bool, extension: &str) -> String {
    let base = if directory {
        name
    } else {
        name.rsplit_once('.')
            .filter(|(base, _)| !base.is_empty())
            .map_or(name, |(base, _)| base)
    };
    normalize(&format!("{base}{extension}"), extension)
}
#[allow(
    dead_code,
    reason = "Shared with the separately compiled COM extension"
)]
pub fn extraction_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if let Some((prefix, part)) = lower.rsplit_once(".part") {
        if let Some(number) = part.strip_suffix(".rar") {
            if !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) {
                return output_folder(&format!("{}.rar", &name[..prefix.len()]), &[".rar"]);
            }
        }
    }
    if let Some((prefix, number)) = lower.rsplit_once(".7z.") {
        if number.len() >= 3 && number.bytes().all(|b| b.is_ascii_digit()) {
            return output_folder(&format!("{}.7z", &name[..prefix.len()]), &[".7z"]);
        }
    }
    output_folder(
        name,
        &[
            ".tar.gz", ".tgz", ".zip", ".7z", ".rar", ".tar", ".iso", ".udf",
        ],
    )
}
#[allow(
    dead_code,
    reason = "Shared with the separately compiled COM extension"
)]
pub fn menu_title(action: &str, name: &str, directory: bool) -> String {
    match action {
        "open" => "Open with PACKIT".into(),
        "extract_files" => "Extract files...".into(),
        "extract_here" => "Extract Here".into(),
        "extract_to" => format!("Extract to \"{}\"", extraction_name(name)),
        "add" => "Add to archive...".into(),
        "zip" => format!("Add to \"{}\"", creation_name(name, directory, ".zip")),
        "seven_zip" => format!("Add to \"{}\"", creation_name(name, directory, ".7z")),
        _ => String::new(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dynamic_titles_use_real_unicode_names() {
        assert_eq!(menu_title("zip", "Photos", true), "Add to \"Photos.zip\"");
        assert_eq!(
            menu_title("seven_zip", "Movie.2026.mp4", false),
            "Add to \"Movie.2026.7z\""
        );
        assert_eq!(
            menu_title("extract_to", "日本 Backup.tar.gz", false),
            "Extract to \"日本 Backup\""
        );
        assert_eq!(extraction_name("Photos.2026.part01.rar"), "Photos.2026");
        assert_eq!(extraction_name("Photos.2026.7z.003"), "Photos.2026");
        assert_eq!(
            menu_title("zip", "Project.Files", true),
            "Add to \"Project.Files.zip\""
        );
    }
}

/// Shell requests contain filesystem paths, never Win32 devices or ADS names.
/// Extended drive/UNC spellings from Windows pickers remain supported.
#[allow(dead_code)] // Shared with the Windows-only COM extension.
pub fn safe_shell_path(path: &str) -> bool {
    let normalized = path.replace('/', "\\");
    let normalized = if let Some(rest) = normalized.strip_prefix("\\\\?\\") {
        if rest
            .get(..4)
            .is_some_and(|p| p.eq_ignore_ascii_case("UNC\\"))
        {
            format!("\\\\{}", &rest[4..])
        } else {
            rest.to_owned()
        }
    } else {
        normalized
    };
    let bytes = normalized.as_bytes();
    let rest = if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes[2] == b'\\'
    {
        &normalized[3..]
    } else if let Some(rest) = normalized.strip_prefix("\\\\") {
        if rest.split('\\').filter(|p| !p.is_empty()).count() < 2 {
            return false;
        }
        rest
    } else {
        return false;
    };
    rest.split('\\').all(|p| {
        !p.is_empty()
            && p != "."
            && p != ".."
            && !p.ends_with(['.', ' '])
            && !p.chars().any(|c| c.is_control() || "<>:\"|?*".contains(c))
    })
}
#[cfg(test)]
mod shell_security_tests {
    #[test]
    fn device_ads_and_relative_spellings_are_not_shell_files() {
        for value in [
            r"C:\Photos\café & 日本.txt",
            r"\\server\share\Photos.2026",
            r"\\?\C:\Photos\file.txt",
            r"\\?\UNC\server\share\file",
        ] {
            assert!(super::safe_shell_path(value), "{value}");
        }
        for value in [
            r"\\.\pipe\attack",
            r"\\?\GLOBALROOT\Device\HarddiskVolume1\file",
            r"C:\file:stream",
            r"C:relative",
            r"C:\folder\..\other",
            r"C:\bad.\file",
        ] {
            assert!(!super::safe_shell_path(value), "{value}");
        }
    }
}
