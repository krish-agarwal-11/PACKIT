// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
import { useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { desktop, type UpdateReply } from "../desktop";
import policy from "../../src-tauri/update-policy.json";
import { Button, Dialog } from "./ui";
export type UpdateFailure = "check" | "verification" | "install" | null;
export type Progress = { stage: string; bytes: number; total: number | null };
export function UpdateManager({
  request,
  busy,
}: {
  request: number;
  busy: boolean;
}) {
  const [failure, setFailure] = useState<UpdateFailure>(null);
  const [reply, setReply] = useState<UpdateReply | null>(null);
  const [open, setOpen] = useState(false);
  const [working, setWorking] = useState(false);
  const [message, setMessage] = useState("");
  const [progress, setProgress] = useState<Progress | null>(null);
  const running = useRef(false);
  async function check(silent: boolean) {
    if (running.current) return;
    running.current = true;
    setWorking(true);
    setFailure(null);
    setMessage("Checking for updates…");
    setReply(null);
    setProgress(null);
    try {
      const result = await desktop.checkUpdates();
      setReply(result);
      setMessage(result.message);
      if (!silent || result.state === "available") setOpen(true);
    } catch {
      if (!silent) {
        setFailure("check");
        setMessage(
          "Could not check for updates. The release or signed manifest may not be published or accessible yet, or you may be offline. Nothing was installed. Try later or use the manual installer.",
        );
        setReply(null);
        setOpen(true);
      }
    } finally {
      running.current = false;
      setWorking(false);
    }
  }
  useEffect(() => {
    if (!isTauri()) return;
    const unlisten = listen<Progress>("updater-progress", (event) =>
      setProgress(event.payload),
    );
    if (policy.feedReady) queueMicrotask(() => void check(true));
    return () => {
      void unlisten.then((stop) => stop()).catch(() => {});
    };
  }, []);
  useEffect(() => {
    if (request > 0) queueMicrotask(() => void check(false));
  }, [request]);
  async function install() {
    if (busy || running.current) return;
    running.current = true;
    setWorking(true);
    setFailure(null);
    setProgress({ stage: "Preparing update", bytes: 0, total: null });
    try {
      await desktop.installUpdate();
    } catch (error) {
      const verification =
        typeof error === "string" && /signature|signed version/i.test(error);
      setFailure(verification ? "verification" : "install");
      setMessage(
        verification
          ? "PACKIT could not verify this update. Nothing was installed. Check again or use the manual installer."
          : typeof error === "string"
            ? error
            : "The update could not be started. Retry or use the manual installer.",
      );
      setProgress(null);
    } finally {
      running.current = false;
      setWorking(false);
    }
  }
  if (!open || busy) return null;
  return (
    <UpdateDialog
      reply={reply}
      failure={failure}
      working={working}
      message={message}
      progress={progress}
      onClose={() => setOpen(false)}
      onInstall={() => void install()}
      onRetry={() => void check(false)}
      onFallback={() =>
        void desktop
          .updateFallback()
          .catch(() =>
            setMessage(
              "Open https://github.com/krish-agarwal-11/PACKIT/releases in your browser.",
            ),
          )
      }
    />
  );
}

export function UpdateDialog({
  reply,
  failure,
  working,
  message,
  progress,
  onClose,
  onInstall,
  onRetry,
  onFallback,
}: {
  reply: UpdateReply | null;
  failure: UpdateFailure;
  working: boolean;
  message: string;
  progress: Progress | null;
  onClose: () => void;
  onInstall: () => void;
  onRetry: () => void;
  onFallback: () => void;
}) {
  const available = reply?.state === "available" && !failure;
  return (
    <Dialog
      title={
        failure === "verification"
          ? "Update verification failed"
          : failure === "check"
            ? "Could not check for updates"
            : failure
              ? "Update failed"
              : available
                ? `PACKIT ${reply.version} is available`
                : "PACKIT updates"
      }
      onClose={() => {
        if (!working) onClose();
      }}
      actions={
        <>
          {failure && (
            <Button primary disabled={working} onClick={onRetry}>
              Retry
            </Button>
          )}
          {available && (
            <Button primary disabled={working} onClick={onInstall}>
              {working ? "Updating…" : "Update Now"}
            </Button>
          )}
          <Button disabled={working} onClick={onClose}>
            {available ? "Later" : "Close"}
          </Button>
          <Button disabled={working} onClick={onFallback}>
            Manual installer
          </Button>
        </>
      }
    >
      <p role={failure ? "alert" : "status"}>{message}</p>
      {progress && (
        <div aria-live="polite">
          <p>{progress.stage}</p>
          <progress
            aria-label="Update download"
            max={progress.total ?? undefined}
            value={
              progress.total && progress.stage === "Downloading"
                ? progress.bytes
                : undefined
            }
          />
          {progress.bytes > 0 && (
            <p>
              {(progress.bytes / 1048576).toFixed(1)} MiB
              {progress.total
                ? ` of ${(progress.total / 1048576).toFixed(1)} MiB`
                : ""}
            </p>
          )}
        </div>
      )}
      {available && (
        <p className="muted">
          PACKIT verifies the signature before starting the installer. Settings
          and Explorer integration are preserved. Finish other PACKIT operation
          windows before installing.
        </p>
      )}
    </Dialog>
  );
}
