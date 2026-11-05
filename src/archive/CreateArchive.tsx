// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
import { useEffect, useEffectEvent, useRef, useState } from "react";
import { Button, Card, Dialog, Status } from "../components/ui";
import { desktop, errorMessage } from "../desktop";
import type {
  CreationFormat,
  CreationOptions,
  CreationSeed,
  OverwritePolicy,
} from "../contracts";
import type { useArchive } from "./useArchive";

export function CreateArchive({
  explorer,
  seed,
  compact = false,
}: {
  explorer: ReturnType<typeof useArchive>;
  seed?: CreationSeed | null;
  compact?: boolean;
}) {
  const [sources, setSources] = useState<string[]>(seed?.sources ?? []);
  const [destination, setDestination] = useState(seed?.destination ?? "");
  const [filename, setFilename] = useState(seed?.filename ?? "Archive.zip");
  const [format, setFormat] = useState<CreationFormat>(seed?.format ?? "zip");
  const [preset, setPreset] = useState<CreationOptions["preset"]>("balanced");
  const [pending, setPending] = useState<CreationOptions | null>(null);
  const [picking, setPicking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const edited = useRef(false);
  const namingVersion = useRef(0);
  const quickStarted = useRef(false);
  const firstSource = sources[0];
  useEffect(() => {
    if (!firstSource || edited.current) return;
    const version = ++namingVersion.current;
    void desktop
      .suggestCreationName(firstSource, format)
      .then((name) => {
        if (version === namingVersion.current) setFilename(name);
      })
      .catch((error: unknown) =>
        setError(errorMessage(error, "Could not suggest an archive name.")),
      );
  }, [firstSource, format]);
  function changeFormat(next: CreationFormat) {
    const version = ++namingVersion.current;
    setFormat(next);
    void desktop
      .normalizeCreationName(filename, next)
      .then((name) => {
        if (version === namingVersion.current) setFilename(name);
      })
      .catch((error: unknown) =>
        setError(errorMessage(error, "Could not update the extension.")),
      );
  }
  const busy = explorer.busy || picking;
  async function pick(directory: boolean) {
    setPicking(true);
    setError(null);
    try {
      const paths = await desktop.chooseSources(directory);
      if (paths)
        setSources((list) => [
          ...new Set([...list, ...(Array.isArray(paths) ? paths : [paths])]),
        ]);
    } catch (error) {
      setError(errorMessage(error, "Could not select sources."));
    } finally {
      setPicking(false);
    }
  }
  async function chooseDestination() {
    setPicking(true);
    setError(null);
    try {
      const path = await desktop.chooseFolder();
      if (typeof path === "string") setDestination(path);
    } catch (error) {
      setError(errorMessage(error, "Could not select the output folder."));
    } finally {
      setPicking(false);
    }
  }
  async function start() {
    setPicking(true);
    setError(null);
    const options: CreationOptions = {
      sources,
      destination,
      filename,
      format,
      preset,
      policy: "refuse",
    };
    try {
      if (
        await desktop.creationDestinationExists(destination, filename, format)
      )
        setPending(options);
      else explorer.create(options);
    } catch (error) {
      setError(errorMessage(error, "Could not prepare archive creation."));
    } finally {
      setPicking(false);
    }
  }
  function cancelConflict() {
    setPending(null);
    if (seed?.quick) explorer.cancelPrepared();
  }
  function confirm(policy: OverwritePolicy) {
    if (pending) explorer.create({ ...pending, policy });
    setPending(null);
  }
  const startQuick = useEffectEvent(async () => {
    await start();
  });
  useEffect(() => {
    if (!seed?.quick || quickStarted.current) return;
    quickStarted.current = true;
    void startQuick();
  }, [seed]);
  if (
    compact &&
    explorer.operation.state !== "idle" &&
    explorer.operation.state !== "inspecting" &&
    !pending
  )
    return (
      <Card>
        <h2>{filename}</h2>
        <p className="muted">{sources.length} selected source items</p>
      </Card>
    );

  return (
    <Card>
      {seed?.quick ? (
        <>
          <h2>{filename}</h2>
          <p className="muted">
            {sources.length} selected source items · Balanced compression
          </p>
        </>
      ) : (
        <>
          <div className="creation-intro">
            <h2>{compact ? "Create an archive" : "Source files"}</h2>
            <span className="muted">{sources.length} selected</span>
          </div>
          <p className="muted">
            Package files and folders locally. Every archive is verified before
            it is saved.
          </p>

          <div className="archive-location">
            <Button disabled={busy} onClick={() => void pick(false)}>
              Add files
            </Button>
            <Button disabled={busy} onClick={() => void pick(true)}>
              Add folders
            </Button>
            <Button
              disabled={busy || !sources.length}
              onClick={() => setSources([])}
            >
              Clear sources
            </Button>
          </div>
          {!sources.length && (
            <div className="source-empty">
              Add files or folders to get started. Their relative structure is
              preserved.
            </div>
          )}
          <ul className="creation-sources" aria-label="Archive sources">
            {sources.map((path) => (
              <li key={path}>
                <span dir="auto" title={path}>
                  {path.split(/[\\/]/).at(-1)}
                  <small>{path}</small>
                </span>
                <Button
                  disabled={busy}
                  aria-label={`Remove ${path.split(/[\\/]/).at(-1)}`}
                  onClick={() => setSources(sources.filter((p) => p !== path))}
                >
                  Remove
                </Button>
              </li>
            ))}
          </ul>
          <div className="creation-options" aria-label="Archive output options">
            <div className="setting output-destination">
              <div>
                <strong>Output folder</strong>
                <p className="muted">
                  {destination || "Choose where to save the archive"}
                </p>
              </div>
              <Button disabled={busy} onClick={() => void chooseDestination()}>
                Choose folder
              </Button>
            </div>
            <div className="setting output-name">
              <label htmlFor="archive-name">Archive filename</label>
              <input
                id="archive-name"
                value={filename}
                disabled={busy}
                onChange={(event) => {
                  edited.current = true;
                  namingVersion.current++;
                  setFilename(event.currentTarget.value);
                }}
              />
            </div>
            <div className="setting">
              <label htmlFor="creation-format">Format</label>
              <select
                id="creation-format"
                disabled={busy}
                value={format}
                onChange={(event) =>
                  changeFormat(event.currentTarget.value as CreationFormat)
                }
              >
                <option value="zip">ZIP (.zip)</option>
                <option value="seven_zip">7z · LZMA2 (.7z)</option>
                <option value="tar">TAR (.tar)</option>
                <option value="tar_gz">TAR.GZ (.tar.gz)</option>
              </select>
            </div>
            <div className="setting">
              <div>
                <label htmlFor="creation-preset">Compression preset</label>
                <p className="muted">
                  {format === "tar"
                    ? "TAR packages files without compression."
                    : "Maximum may take longer and does not guarantee a smaller archive."}
                </p>
              </div>
              <select
                id="creation-preset"
                disabled={busy || format === "tar"}
                value={preset}
                onChange={(event) =>
                  setPreset(
                    event.currentTarget.value as CreationOptions["preset"],
                  )
                }
              >
                <option value="fast">Fast</option>
                <option value="balanced">Balanced</option>
                <option value="maximum">Maximum</option>
              </select>
            </div>
          </div>
        </>
      )}
      {error && <Status error>{error}</Status>}
      {!seed?.quick && (
        <p className="form-hint">
          {!sources.length
            ? "Choose at least one source."
            : !destination
              ? "Choose an output folder to continue."
              : !filename.trim()
                ? "Enter an archive filename."
                : "Ready to create and verify your archive."}
        </p>
      )}
      {!seed?.quick && (
        <Button
          primary
          disabled={busy || !sources.length || !destination || !filename.trim()}
          onClick={() => void start()}
        >
          Create Archive
        </Button>
      )}
      {pending && (
        <Dialog
          title="Output archive already exists"
          onClose={cancelConflict}
          actions={
            <>
              <Button onClick={cancelConflict}>Cancel</Button>
              <Button onClick={() => confirm("keep_both")}>
                Save numbered copy
              </Button>
              <Button primary onClick={() => confirm("replace")}>
                Replace archive
              </Button>
            </>
          }
        >
          <p>
            Replace the matching archive only after the new archive passes
            verification, save a separate numbered archive, or cancel without
            changing files.
          </p>
        </Dialog>
      )}
    </Card>
  );
}
