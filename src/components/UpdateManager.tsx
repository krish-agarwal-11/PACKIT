// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
import { useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { desktop, type UpdateReply } from "../desktop";
import policy from "../../src-tauri/update-policy.json";
import { Button, Dialog } from "./ui";
type Progress = { stage: string; bytes: number; total: number | null };
export function UpdateManager({
  request,
  busy,
}: {
  request: number;
  busy: boolean;
}) {
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
    try {
      const result = await desktop.checkUpdates();
      setReply(result);
      setMessage(result.message);
      if (!silent || result.state === "available") setOpen(true);
    } catch {
      if (!silent) {
        setMessage(
          "Could not check for updates. You may be offline, or no release is available. Try later or use the manual installer.",
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
    setProgress({ stage: "Preparing update", bytes: 0, total: null });
    try {
      await desktop.installUpdate();
    } catch (error) {
      setMessage(
        typeof error === "string"
          ? error
          : "The update could not be installed. You can retry or use the manual installer.",
      );
      setProgress(null);
    } finally {
      running.current = false;
      setWorking(false);
    }
  }
  if (!open || busy) return null;
  const available = reply?.state === "available";
  return (
    <Dialog
      title={
        available ? `PACKIT ${reply.version} is available` : "PACKIT updates"
      }
      onClose={() => {
        if (!working) setOpen(false);
      }}
      actions={
        <>
          {available && (
            <Button
              primary
              disabled={working || busy}
              onClick={() => void install()}
            >
              {working ? "Updating…" : "Update Now"}
            </Button>
          )}
          <Button disabled={working} onClick={() => setOpen(false)}>
            {available ? "Later" : "Close"}
          </Button>
          <Button
            disabled={working}
            onClick={() =>
              void desktop
                .updateFallback()
                .catch(() =>
                  setMessage(
                    "Open https://github.com/krish-agarwal-11/PACKIT/releases in your browser.",
                  ),
                )
            }
          >
            Manual installer
          </Button>
        </>
      }
    >
      <p role="status">{message}</p>
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
