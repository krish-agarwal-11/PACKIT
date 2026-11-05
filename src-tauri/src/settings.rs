// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
use serde::{Deserialize, Serialize};
use std::{fs, io::ErrorKind, path::Path};

#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    System,
}

#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub theme: Theme,
    pub extraction_directory: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AppError {
    code: &'static str,
    message: &'static str,
}

impl AppError {
    pub fn from_error(error: impl std::fmt::Display) -> Self {
        tracing::error!(%error, "Settings operation failed");
        Self {
            code: "settings_error",
            message: "PACKIT could not read or save your preferences.",
        }
    }
}

pub fn load(directory: &Path) -> Result<Settings, AppError> {
    match fs::read(directory.join("settings.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(AppError::from_error),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Settings::default()),
        Err(error) => Err(AppError::from_error(error)),
    }
}

pub fn save(directory: &Path, settings: Settings) -> Result<Settings, AppError> {
    fs::create_dir_all(directory).map_err(AppError::from_error)?;
    let bytes = serde_json::to_vec_pretty(&settings).map_err(AppError::from_error)?;
    let temp = directory.join("settings.json.tmp");
    fs::write(&temp, bytes).map_err(AppError::from_error)?;
    fs::rename(temp, directory.join("settings.json")).map_err(AppError::from_error)?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_replacement_survive_reload() {
        let dir = std::env::temp_dir().join(format!("packit-settings-{}", std::process::id()));
        assert_eq!(load(&dir).unwrap(), Settings::default());
        save(&dir, Settings::default()).unwrap();
        let settings = Settings {
            theme: Theme::Light,
            extraction_directory: Some("chosen".into()),
        };
        assert_eq!(load_after_save(&dir, settings), Theme::Light);
        assert_eq!(
            load(&dir).unwrap().extraction_directory.as_deref(),
            Some("chosen")
        );
        fs::remove_dir_all(dir).unwrap();
    }

    fn load_after_save(dir: &Path, settings: Settings) -> Theme {
        save(dir, settings).unwrap();
        load(dir).unwrap().theme
    }
}
