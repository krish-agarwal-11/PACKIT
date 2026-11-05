// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
export type Theme = "dark" | "light" | "system";
export interface Settings {
  theme: Theme;
  extractionDirectory: string | null;
}
export interface AppError {
  code: string;
  message: string;
}
export interface ArchiveEntry {
  path: string;
  kind: "file" | "directory" | "link" | "special";
  compressedBytes: string | null;
  uncompressedBytes: string;
  method: string;
  modified: string | null;
  issue: {
    code:
      "encrypted" | "unsupported_compression" | "unsafe_entry" | "unsafe_path";
    message: string;
  } | null;
}
export interface FormatCapabilities {
  format: string;
  zip64: boolean;
  inspection: boolean;
  extraction: boolean;
  integrity: "checksums" | "structure";
  limitations: string;
  compressionMethods: string[];
  encryption: boolean;
  multipart: boolean;
}
export interface ArchiveInspection {
  name: string;
  outputFolder: string;
  archiveBytes: string;
  totalUncompressedBytes: string;
  entries: ArchiveEntry[];
  capabilities: FormatCapabilities;
}
export interface OperationReport {
  files: number;
  directories: number;
  processedBytes: string;
  outputPath: string | null;
}
export type ArchiveOperation =
  | { state: "idle" | "inspecting" | "extracting" | "testing" | "creating" }
  | { state: "completed" | "failed" | "cancelled"; message: string };

export type TaskState =
  "queued" | "running" | "completed" | "failed" | "cancelled";

export type OverwritePolicy = "refuse" | "replace" | "keep_both";

export interface OperationRequest {
  operationId: string;
  sessionId?: string;
  destinationMode?: "folder" | "here";
  selection?: string[];
  password?: string;
}
export interface OperationProgress {
  operationId: string;
  state:
    | "compressing"
    | "packaging"
    | "preparing"
    | "running"
    | "verifying"
    | "committing"
    | "completed"
    | "failed"
    | "cancelled";
  currentFile: string | null;
  processedBytes: string;
  totalBytes: string | null;
  completedFiles: number;
  totalFiles: number | null;
}

export type CreationFormat = "zip" | "seven_zip" | "tar" | "tar_gz";
export interface CreationOptions {
  sources: string[];
  destination: string;
  filename: string;
  format: CreationFormat;
  preset: "fast" | "balanced" | "maximum";
  policy: OverwritePolicy;
}

export interface ShellLaunch {
  action:
    | "open"
    | "extract_files"
    | "extract_here"
    | "extract_to"
    | "add"
    | "zip"
    | "seven_zip";
  paths: string[];
}
export interface CreationSeed {
  sources: string[];
  destination: string;
  filename: string;
  format: CreationFormat;
  quick: boolean;
  id: string;
}

export interface MainRequest {
  id: string;
  launch: ShellLaunch | null;
}
