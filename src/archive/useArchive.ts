// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
import { useEffect, useEffectEvent, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type {
  ArchiveInspection,
  CreationOptions,
  ShellLaunch,
  ArchiveOperation,
  OverwritePolicy,
  OperationProgress,
  OperationRequest,
} from "../contracts";
import { desktop, errorMessage } from "../desktop";

export type OpenArchive = ArchiveInspection & { source: string };
const formatter = new Intl.NumberFormat();
export const bytes = (value: string) => `${formatter.format(BigInt(value))} B`;

export function useArchive(
  onOpenExplorer: () => void,
  defaultDestination?: string,
) {
  const [archive, setArchive] = useState<OpenArchive | null>(null);
  const [operation, setOperation] = useState<ArchiveOperation>({
    state: "idle",
  });
  const [progress, setProgress] = useState<OperationProgress | null>(null);
  const [destination, setDestination] = useState<{
    path: string;
    exists: boolean;
    selection?: string[];
    here?: boolean;
  } | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [picking, setPicking] = useState(false);
  const [passwordPrompt, setPasswordPrompt] = useState<string | null>(null);
  const passwordAnswer = useRef<((value: string | null) => void) | null>(null);
  const sessionId = useRef<string | null>(null);
  const eventReady = useRef<Promise<boolean>>(Promise.resolve(false));
  const activeId = useRef<string | null>(null);
  const lock = useRef(false);
  const busy =
    picking ||
    operation.state === "inspecting" ||
    operation.state === "testing" ||
    operation.state === "extracting" ||
    operation.state === "creating";

  useEffect(() => {
    if (!isTauri()) return;
    const lifetime = new AbortController();
    let cleanup: (() => void) | undefined;
    eventReady.current = listen<OperationProgress>(
      "archive-progress",
      ({ payload }) => {
        if (
          !lifetime.signal.aborted &&
          payload.operationId === activeId.current
        )
          setProgress(payload);
      },
    )
      .then((unlisten) => {
        if (!lifetime.signal.aborted) cleanup = unlisten;
        else unlisten();
        return !lifetime.signal.aborted;
      })
      .catch(() => false);
    return () => {
      lifetime.abort();
      cleanup?.();
      passwordAnswer.current?.(null);
      if (sessionId.current) void desktop.closeSession(sessionId.current);
    };
  }, []);

  function fail(error: unknown, fallback: string) {
    const cancelled =
      typeof error === "object" &&
      error &&
      "code" in error &&
      error.code === "cancelled";
    setOperation({
      state: cancelled ? "cancelled" : "failed",
      message: errorMessage(error, fallback),
    });
  }

  async function exclusive(action: () => Promise<unknown>) {
    if (lock.current) return false;
    lock.current = true;
    try {
      await action();
      return true;
    } finally {
      lock.current = false;
    }
  }

  function answerPassword(value: string | null) {
    setPasswordPrompt(null);
    const answer = passwordAnswer.current;
    passwordAnswer.current = null;
    answer?.(value);
  }

  async function request<T>(
    action: (request: OperationRequest) => Promise<T>,
    selection?: string[],
    here?: boolean,
  ): Promise<T> {
    let password: string | undefined;
    try {
      if (!(await eventReady.current)) {
        throw {
          code: "progress_unavailable",
          message:
            "Operation progress could not be connected. Restart PACKIT and try again.",
        };
      }
      for (;;) {
        const operationId = crypto.randomUUID();
        activeId.current = operationId;
        setProgress({
          operationId,
          state: "preparing",
          currentFile: null,
          processedBytes: "0",
          totalBytes: null,
          completedFiles: 0,
          totalFiles: null,
        });
        try {
          return await action({
            operationId,
            sessionId: sessionId.current ?? undefined,
            selection,
            destinationMode: here ? "here" : undefined,
            password,
          });
        } catch (error) {
          if (
            typeof error !== "object" ||
            !error ||
            !("code" in error) ||
            (error.code !== "password_required" &&
              error.code !== "incorrect_password")
          )
            throw error;
          activeId.current = null;
          password = undefined;
          setPasswordPrompt(
            error.code === "incorrect_password"
              ? "Incorrect password. Try again."
              : "Enter this ZIP archive’s password.",
          );
          const answer = await new Promise<string | null>((resolve) => {
            passwordAnswer.current = resolve;
          });
          if (answer === null)
            throw { code: "cancelled", message: "Password entry cancelled." };
          password = answer;
        }
      }
    } finally {
      // Credentials are only held for this request/retry loop, never in settings,
      // archive history, command arguments, or a saved archive model.
      password = undefined;
      activeId.current = null;
    }
  }

  async function inspect(path: string) {
    setArchive(null);
    setSelected([]);
    setOperation({ state: "inspecting" });
    try {
      if (sessionId.current) await desktop.closeSession(sessionId.current);
      sessionId.current = crypto.randomUUID();
      setProgress(null);
      const info = await request((options) =>
        desktop.inspectArchive(path, options),
      );
      setArchive({ ...info, source: path });
      setOperation({
        state: "completed",
        message: `${info.capabilities.format} archive loaded.`,
      });
      return info;
    } catch (error) {
      fail(error, "Could not inspect this archive.");
      if (sessionId.current) void desktop.closeSession(sessionId.current);
      sessionId.current = null;
    }
  }

  const handleDrop = useEffectEvent((paths: string[]) => {
    if (lock.current || destination) return;
    onOpenExplorer();
    if (paths.length !== 1 || !paths[0]) {
      setOperation({
        state: "failed",
        message: "Drop one archive or one volume from a supported set.",
      });
      return;
    }
    const path = paths[0];
    void exclusive(() => inspect(path));
  });

  const [dropAvailable, setDropAvailable] = useState(false);
  useEffect(() => {
    if (!isTauri()) return;
    let active = true;
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent(({ payload }) => {
        if (active && payload.type === "drop") handleDrop(payload.paths);
      })
      .then((cleanup) => {
        if (!active) cleanup();
        else {
          unlisten = cleanup;
          setDropAvailable(true);
        }
      })
      .catch((error: unknown) => {
        console.warn("Could not register PACKIT's native drop listener", error);
      });
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  function openShell(launch: ShellLaunch, quick = false) {
    const path = launch.paths[0];
    if (!path) return;
    onOpenExplorer();
    return exclusive(async () => {
      try {
        const info = await inspect(path);
        if (!info || launch.action === "open") return;
        const parent = await desktop.sourceParent(path);
        const target =
          launch.action === "extract_files"
            ? await desktop.chooseFolder(defaultDestination)
            : parent;
        if (typeof target !== "string") {
          setOperation({
            state: "cancelled",
            message: "Extraction cancelled. No output was changed.",
          });
          return;
        }
        const here = launch.action === "extract_here";
        const exists = await desktop.extractionDestinationExists(
          path,
          target,
          here,
        );
        const targetInfo = { path: target, exists, here };
        if (quick && !exists) {
          setProgress(null);
          setOperation({ state: "extracting" });
          try {
            const result = await request(
              (options) =>
                desktop.extractArchive(path, target, "refuse", options),
              undefined,
              here,
            );
            setOperation({
              state: "completed",
              message: `Extracted ${result.files} files (${bytes(result.processedBytes)}) to ${result.outputPath}.`,
            });
          } catch (error) {
            if (
              typeof error === "object" &&
              error &&
              "code" in error &&
              error.code === "destination_exists"
            ) {
              setDestination({ ...targetInfo, exists: true });
              setOperation({ state: "idle" });
            } else fail(error, "Extraction failed.");
          }
        } else setDestination(targetInfo);
      } catch (error) {
        fail(error, "Could not prepare the Explorer operation.");
      }
    });
  }

  function chooseArchive() {
    if (destination) return;
    void exclusive(async () => {
      setPicking(true);
      try {
        const path = await desktop.chooseFile();
        if (typeof path === "string") {
          onOpenExplorer();
          await inspect(path);
        }
      } catch (error) {
        onOpenExplorer();
        fail(error, "Could not open the native file picker.");
      } finally {
        setPicking(false);
      }
    });
  }

  function chooseDestination(selection?: string[]) {
    if (!archive) return;
    void exclusive(async () => {
      setPicking(true);
      try {
        const path = await desktop.chooseFolder(defaultDestination);
        if (typeof path === "string") {
          const exists = await desktop.extractionDestinationExists(
            archive.source,
            path,
          );
          setDestination({ path, exists, selection });
        }
      } catch (error) {
        fail(error, "Could not open the native folder picker.");
      } finally {
        setPicking(false);
      }
    });
  }

  function test() {
    if (!archive || destination) return;
    void exclusive(async () => {
      setProgress(null);
      setOperation({ state: "testing" });
      try {
        const result = await request((options) =>
          desktop.testArchive(archive.source, options),
        );
        setOperation({
          state: "completed",
          message:
            archive.capabilities.integrity === "structure"
              ? `Structure validation passed. Read ${result.files} files (${bytes(result.processedBytes)}); this format has no file-content checksums.`
              : `Checksum test passed. Checked ${result.files} files (${bytes(result.processedBytes)}).`,
        });
      } catch (error) {
        fail(error, "Could not verify this archive.");
      }
    });
  }

  function extract(policy: OverwritePolicy) {
    if (!archive || !destination) return;
    const target = destination;
    void exclusive(async () => {
      setDestination(null);
      setProgress(null);
      setOperation({ state: "extracting" });
      try {
        const result = await request(
          (options) =>
            desktop.extractArchive(
              archive.source,
              target.path,
              policy,
              options,
            ),
          target.selection,
          target.here,
        );
        setOperation({
          state: "completed",
          message: `Extracted ${result.files} files and ${result.directories} folders (${bytes(result.processedBytes)}) to ${result.outputPath}.`,
        });
      } catch (error) {
        if (
          policy === "refuse" &&
          typeof error === "object" &&
          error &&
          "code" in error &&
          error.code === "destination_exists"
        ) {
          setOperation({ state: "idle" });
          setDestination({ ...target, exists: true });
        } else {
          fail(error, "Could not extract this archive.");
        }
      }
    });
  }

  function cancelActive() {
    if (passwordAnswer.current) {
      answerPassword(null);
      return;
    }
    if (activeId.current)
      void desktop
        .cancelOperation(activeId.current)
        .catch((error: unknown) =>
          fail(error, "Could not cancel this operation."),
        );
  }

  function create(options: CreationOptions) {
    void exclusive(async () => {
      setProgress(null);
      setOperation({ state: "creating" });
      try {
        const result = await request((request) =>
          desktop.createArchive(options, request),
        );
        setOperation({
          state: "completed",
          message: `Created and verified ${result.outputPath} (${result.files} files, ${bytes(result.processedBytes)} source data).`,
        });
      } catch (error) {
        fail(error, "Could not create the archive.");
      }
    });
  }

  function closeArchive() {
    if (lock.current) return;
    if (sessionId.current) void desktop.closeSession(sessionId.current);
    sessionId.current = null;
    setArchive(null);
    setSelected([]);
    setOperation({ state: "idle" });
    setProgress(null);
  }

  return {
    archive,
    create,
    operation,
    progress,
    destination,
    busy,
    dropAvailable,
    selected,
    passwordPrompt,
    answerPassword,
    cancelActive,
    closeArchive,
    dismissOperation: () => {
      if (!lock.current) {
        setOperation({ state: "idle" });
        setProgress(null);
      }
    },
    toggle: (path: string) =>
      setSelected((list) =>
        list.includes(path)
          ? list.filter((item) => item !== path)
          : [...list, path],
      ),
    clearSelection: () => setSelected([]),
    chooseArchive,
    openShell,
    chooseDestination,
    test,
    extract,
    cancelPrepared: () =>
      setOperation({
        state: "cancelled",
        message: "Operation cancelled. No output was changed.",
      }),
    cancel: () => {
      setDestination(null);
      setOperation({
        state: "cancelled",
        message: "Extraction cancelled. No output was changed.",
      });
    },
  };
}
