// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  ArchiveInspection,
  CreationOptions,
  CreationFormat,
  ShellLaunch,
  MainRequest,
  OperationReport,
  OperationRequest,
  OverwritePolicy,
  Settings,
} from "./contracts";

export type UpdateReply = {
  state: "disabled" | "available" | "current";
  version: string | null;
  message: string;
};
export const desktop = {
  checkUpdates: () => invoke<UpdateReply>("check_updates"),
  installUpdate: () => invoke<void>("install_update"),
  updateFallback: () => invoke<void>("open_update_fallback"),
  legalDocument: (document: "license" | "notices") =>
    invoke<string>("legal_document", { document }),
  closeOperationWindow: () => getCurrentWindow().close(),
  applyWindowTheme: (theme: "dark" | "light" | "system") =>
    isTauri()
      ? getCurrentWindow().setTheme(theme === "system" ? null : theme)
      : Promise.resolve(),
  nextMainRequest: () => invoke<MainRequest | null>("next_main_request"),
  acknowledgeMainRequest: (id: string) =>
    invoke<void>("acknowledge_main_request", { id }),
  shellLaunch: () => invoke<ShellLaunch | null>("shell_launch"),
  sourceParent: (path: string) => invoke<string>("source_parent", { path }),
  suggestCreationName: (path: string, format: CreationFormat) =>
    invoke<string>("suggest_creation_name", { path, format }),
  normalizeCreationName: (filename: string, format: CreationFormat) =>
    invoke<string>("normalize_creation_name", { filename, format }),
  chooseSources: (directory: boolean) =>
    open({
      title: directory ? "Add source folders" : "Add source files",
      directory,
      multiple: true,
    }),
  creationDestinationExists: (
    destination: string,
    filename: string,
    format: CreationFormat,
  ) =>
    invoke<boolean>("creation_destination_exists", {
      destination,
      filename,
      format,
    }),
  createArchive: (options: CreationOptions, request: OperationRequest) =>
    invoke<OperationReport>("create_archive", { options, request }),
  closeSession: (sessionId: string) =>
    invoke<void>("close_archive_session", { sessionId }),
  cancelOperation: (operationId: string) =>
    invoke<boolean>("cancel_archive_operation", { operationId }),
  loadSettings: () => invoke<Settings>("load_settings"),
  saveSettings: (settings: Settings) =>
    invoke<Settings>("save_settings", { settings }),
  chooseFile: () => open({ title: "Open archive", multiple: false }),
  chooseFolder: (defaultPath?: string) =>
    open({
      title: "Choose an extraction folder",
      directory: true,
      multiple: false,
      defaultPath,
    }),
  inspectArchive: (path: string, request: OperationRequest) =>
    invoke<ArchiveInspection>("inspect_archive", { path, request }),
  testArchive: (path: string, request: OperationRequest) =>
    invoke<OperationReport>("test_archive", { path, request }),
  extractionDestinationExists: (
    path: string,
    destination: string,
    here = false,
  ) =>
    invoke<boolean>("extraction_destination_exists", {
      path,
      destination,
      here,
    }),
  extractArchive: (
    path: string,
    destination: string,
    policy: OverwritePolicy,
    request: OperationRequest,
  ) =>
    invoke<OperationReport>("extract_archive", {
      path,
      destination,
      policy,
      request,
    }),
};

export function errorMessage(error: unknown, fallback: string): string {
  if (
    typeof error === "object" &&
    error &&
    "message" in error &&
    typeof error.message === "string"
  ) {
    return error.message;
  }
  return fallback;
}
