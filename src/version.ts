// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
import metadata from "../src-tauri/tauri.conf.json";

// Tauri uses this metadata for the native application version as well.
export const appVersion: string = metadata.version;
