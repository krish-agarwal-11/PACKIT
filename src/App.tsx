// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
import { UpdateManager } from "./components/UpdateManager";
import { About } from "./components/About";
import { appVersion } from "./version";
import { HomeWorkspace } from "./archive/HomeWorkspace";
import packitLogo from "../branding/packit-logo.png";
import { CreateArchive } from "./archive/CreateArchive";
import { useEffect, useEffectEvent, useRef, useState } from "react";
import {
  Activity,
  FolderOpen,
  Home,
  Info,
  Plus,
  Settings2,
} from "lucide-react";
import { Button, Card, EmptyState, Navigation, Status } from "./components/ui";
import { ArchiveExplorer, OperationPanel } from "./archive/ArchiveExplorer";
import { useArchive } from "./archive/useArchive";
import { desktop, errorMessage } from "./desktop";
import type { CreationSeed, ShellLaunch, Settings, Theme } from "./contracts";

const pages = [
  { label: "Home", icon: Home },
  { label: "Archive Explorer", icon: FolderOpen },
  { label: "Create Archive", icon: Plus },
  { label: "Tasks", icon: Activity },
  { label: "Settings", icon: Settings2 },
] as const;
type Page = (typeof pages)[number]["label"];

const popup = new URLSearchParams(window.location.search).has("operation");
export default function App() {
  const [popupSource, setPopupSource] = useState("");
  const [creationSeed, setCreationSeed] = useState<CreationSeed | null>(null);
  const shellHandled = useRef(false);
  const [page, setPage] = useState<Page>("Home");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [status, setStatus] = useState<{
    text: string;
    error?: boolean;
  } | null>(null);
  const [busy, setBusy] = useState(false);
  const [about, setAbout] = useState(false);
  const [updateRequest, setUpdateRequest] = useState(0);
  const explorer = useArchive(() => {
    setCreationSeed(null);
    setStatus(null);
    setPage("Archive Explorer");
  }, settings?.extractionDirectory ?? undefined);

  useEffect(() => {
    let active = true;
    desktop.loadSettings().then(
      (value) => {
        if (active) setSettings(value);
      },
      (error: unknown) => {
        if (active)
          setStatus({
            error: true,
            text: errorMessage(
              error,
              "Settings are available in the desktop app.",
            ),
          });
      },
    );
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    const theme = settings?.theme ?? "dark";
    document.documentElement.dataset.theme = theme;
    void desktop.applyWindowTheme(theme).catch(() => {});
  }, [settings?.theme]);

  const handleShellLaunch = useEffectEvent(
    async (launch: ShellLaunch | null) => {
      if (!launch || shellHandled.current) return;
      shellHandled.current = true;
      setPopupSource(launch.paths[0]?.split(/[\\/]/).at(-1) ?? "");
      if (
        launch.action === "add" ||
        launch.action === "zip" ||
        launch.action === "seven_zip"
      ) {
        const format = launch.action === "seven_zip" ? "seven_zip" : "zip";
        const first = launch.paths[0];
        if (!first) return;
        const [filename, destination] = await Promise.all([
          desktop.suggestCreationName(first, format),
          desktop.sourceParent(first),
        ]);
        setCreationSeed({
          sources: launch.paths,
          filename,
          destination,
          format,
          quick: launch.action !== "add",
          id: crypto.randomUUID(),
        });
        setPage("Create Archive");
      } else
        await explorer.openShell(
          launch,
          popup && launch.action !== "extract_files",
        );
    },
  );
  useEffect(() => {
    if (!popup) return;
    void desktop
      .shellLaunch()
      .then((launch) => handleShellLaunch(launch))
      .catch((error: unknown) =>
        setStatus({
          error: true,
          text: errorMessage(error, "Could not process the Explorer request."),
        }),
      );
  }, []);

  const queueWorking = useRef(false);
  const processQueue = useEffectEvent(async () => {
    if (
      popup ||
      queueWorking.current ||
      explorer.busy ||
      explorer.destination ||
      explorer.passwordPrompt
    )
      return;
    queueWorking.current = true;
    try {
      const request = await desktop.nextMainRequest();
      if (!request || explorer.busy) return;
      if (!request.launch) await desktop.acknowledgeMainRequest(request.id);
      else {
        const handled = await explorer.openShell(request.launch);
        if (handled) await desktop.acknowledgeMainRequest(request.id);
      }
    } catch (error) {
      setStatus({
        error: true,
        text: errorMessage(
          error,
          "Could not process the queued archive request.",
        ),
      });
    } finally {
      queueWorking.current = false;
    }
  });
  useEffect(() => {
    if (popup) return;
    const first = window.setTimeout(() => void processQueue(), 0);
    const timer = window.setInterval(() => void processQueue(), 400);
    return () => {
      window.clearTimeout(first);
      window.clearInterval(timer);
    };
  }, []);

  async function run(action: () => Promise<void>, fallback: string) {
    setBusy(true);
    setStatus(null);
    try {
      await action();
    } catch (error) {
      setStatus({ error: true, text: errorMessage(error, fallback) });
    } finally {
      setBusy(false);
    }
  }

  async function save(next: Settings) {
    setSettings(await desktop.saveSettings(next));
    setStatus({ text: "Preferences saved." });
  }

  if (popup)
    return (
      <main className="operation-workspace">
        <header className="popup-brand">
          <img src={packitLogo} alt="" />
          <div>
            <strong>PACKIT</strong>
            <p className="muted" dir="auto">
              {popupSource || "Local archive operation"}
            </p>
          </div>
        </header>
        {status && <Status error={status.error}>{status.text}</Status>}
        <OperationPanel explorer={explorer} compact />
        {creationSeed && (
          <CreateArchive
            key={creationSeed.id}
            seed={creationSeed}
            explorer={explorer}
            compact
          />
        )}
        <ArchiveExplorer explorer={explorer} compact />
        {!creationSeed &&
          explorer.operation.state === "idle" &&
          !explorer.destination &&
          !status?.error && (
            <p className="muted">Preparing the selected archive operation…</p>
          )}
      </main>
    );

  return (
    <div className="shell">
      {page !== "Home" && <OperationPanel explorer={explorer} />}
      <a className="skip" href="#workspace">
        Skip to workspace
      </a>
      <aside className="sidebar">
        <button
          type="button"
          className="brand"
          aria-label="PACKIT — Home"
          onClick={() => setPage("Home")}
        >
          <img className="brand-logo" src={packitLogo} alt="" />
          <div>
            PACKIT<span>ARCHIVE MANAGER</span>
          </div>
        </button>
        <div className="nav-heading">Workspace</div>
        <Navigation
          items={pages}
          current={page}
          onSelect={(next) => {
            if (next !== "Create Archive") setCreationSeed(null);
            setPage(next);
          }}
        />
        <button type="button" className="about" onClick={() => setAbout(true)}>
          <Info size={14} aria-hidden="true" /> About PACKIT{" "}
          <span className="muted">v{appVersion}</span>
        </button>
      </aside>
      <main id="workspace" tabIndex={-1}>
        <header className="topbar">
          <span className="muted">
            Workspace <span aria-hidden="true">/</span>
          </span>{" "}
          {page}
          <span className="topbar-context ml-auto">Local workspace</span>
        </header>
        <div
          className={`content ${page === "Home" ? "content-home" : ""}`}
          key={page}
        >
          {status && <Status error={status.error}>{status.text}</Status>}
          {page === "Home" ? (
            <HomeWorkspace
              explorer={explorer}
              disabled={busy || explorer.busy}
              onCreate={() => setPage("Create Archive")}
              onBrowse={() => setPage("Archive Explorer")}
              onTasks={() => setPage("Tasks")}
            />
          ) : (
            <>
              <p className="page-description">
                {page === "Archive Explorer"
                  ? "Browse contents. Extract exactly what you need."
                  : page === "Create Archive"
                    ? "Choose your sources. We’ll take care of the package."
                    : page === "Tasks"
                      ? "The current operation, without the guesswork."
                      : "A few essentials, just how you like them."}
              </p>
              <h1>{page}</h1>
              {page === "Archive Explorer" && (
                <ArchiveExplorer explorer={explorer} />
              )}
              {page === "Create Archive" && (
                <CreateArchive
                  key={creationSeed?.id ?? "picker"}
                  seed={creationSeed}
                  explorer={explorer}
                />
              )}
              {page === "Tasks" && (
                <Card>
                  {explorer.operation.state === "idle" ? (
                    <EmptyState icon={Activity} title="All clear">
                      No operation is running. Your next extraction or creation
                      will appear here.
                    </EmptyState>
                  ) : (
                    <div className="task-summary">
                      <Activity size={22} aria-hidden="true" />
                      <div>
                        <h2>
                          {explorer.busy
                            ? (explorer.progress?.state ??
                              explorer.operation.state)
                            : explorer.operation.state}
                        </h2>
                        <p>
                          {("message" in explorer.operation
                            ? explorer.operation.message
                            : undefined) ??
                            explorer.progress?.currentFile ??
                            "Operation in progress"}
                        </p>
                      </div>
                      {explorer.busy && (
                        <Button onClick={explorer.cancelActive}>
                          Cancel operation
                        </Button>
                      )}
                    </div>
                  )}
                </Card>
              )}
              {page === "Settings" && (
                <Card>
                  <h2>Preferences</h2>
                  <div className="setting">
                    <div>
                      <label htmlFor="theme">Appearance</label>
                      <p>Choose how PACKIT looks on this device.</p>
                    </div>
                    <select
                      id="theme"
                      disabled={!settings || busy}
                      value={settings?.theme ?? "dark"}
                      onChange={(event) => {
                        const theme = event.currentTarget.value as Theme;
                        if (settings)
                          void run(
                            () => save({ ...settings, theme }),
                            "Could not save the appearance.",
                          );
                      }}
                    >
                      <option value="dark">Dark</option>
                      <option value="light">Light</option>
                      <option value="system">System</option>
                    </select>
                  </div>
                  <div className="setting">
                    <div>
                      <strong>Default extraction folder</strong>
                      <p>
                        {settings?.extractionDirectory ?? "No folder selected"}
                      </p>
                    </div>
                    <div className="flex flex-wrap gap-2">
                      <Button
                        disabled={!settings || busy}
                        onClick={() => {
                          if (settings)
                            void run(async () => {
                              const path = await desktop.chooseFolder();
                              if (typeof path === "string")
                                await save({
                                  ...settings,
                                  extractionDirectory: path,
                                });
                            }, "Could not choose an extraction folder.");
                        }}
                      >
                        Choose folder
                      </Button>
                      {settings?.extractionDirectory && (
                        <Button
                          disabled={busy}
                          onClick={() =>
                            void run(
                              () =>
                                save({
                                  ...settings,
                                  extractionDirectory: null,
                                }),
                              "Could not clear the extraction folder.",
                            )
                          }
                        >
                          Clear
                        </Button>
                      )}
                    </div>
                  </div>
                </Card>
              )}
            </>
          )}
        </div>
      </main>
      {about && (
        <About
          onClose={() => setAbout(false)}
          onCheckUpdates={() => {
            setAbout(false);
            setUpdateRequest((value) => value + 1);
          }}
        />
      )}
      <UpdateManager request={updateRequest} busy={busy || explorer.busy} />
    </div>
  );
}
