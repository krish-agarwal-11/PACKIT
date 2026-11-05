// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
import { useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { Archive, File, Folder, FolderOpen } from "lucide-react";
import {
  Button,
  Card,
  Dialog,
  EmptyState,
  FileRow,
  Status,
} from "../components/ui";
import type { ArchiveEntry } from "../contracts";
import { desktop } from "../desktop";
import { bytes, type OpenArchive, type useArchive } from "./useArchive";

type Explorer = ReturnType<typeof useArchive>;
type Row = {
  id: string;
  name: string;
  path: string;
  entry?: ArchiveEntry;
  folder?: string;
};
const PAGE_SIZE = 100;

function buildTree(entries: ArchiveEntry[]): Map<string, Row[]> {
  const tree = new Map<string, Row[]>();
  const folders = new Map<string, Row>();
  const push = (parent: string, row: Row) => {
    const rows = tree.get(parent);
    if (rows) rows.push(row);
    else tree.set(parent, [row]);
  };
  for (const [index, entry] of entries.entries()) {
    const directory = entry.kind === "directory";
    const path =
      directory && entry.path.endsWith("/")
        ? entry.path.slice(0, -1)
        : entry.path;
    const parts = path.split("/");
    if (
      parts.length > 32 ||
      parts.some((part) => !part || part === "." || part === "..") ||
      entry.issue?.code === "unsafe_path"
    ) {
      push("", {
        id: `entry:${index}`,
        name: entry.path,
        path: entry.path,
        entry,
      });
      continue;
    }
    let parent = "";
    for (const [i, name] of parts.entries()) {
      const last = i === parts.length - 1;
      const path = parent ? `${parent}/${name}` : name;
      if (!last || directory) {
        let row = folders.get(path);
        if (!row) {
          row = { id: `folder:${path}`, name, path, folder: path };
          folders.set(path, row);
          push(parent, row);
        }
        if (last) {
          // Preserve multiple records for the same display path when the
          // underlying encoded names differ.
          if (row.entry)
            push(parent, { id: `entry:${index}`, name, path, entry });
          else row.entry = entry;
        }
      } else {
        push(parent, { id: `entry:${index}`, name, path, entry });
      }
      parent = path;
    }
  }
  for (const rows of tree.values()) {
    rows.sort(
      (a, b) =>
        Number(Boolean(b.folder)) - Number(Boolean(a.folder)) ||
        a.name.localeCompare(b.name),
    );
  }
  return tree;
}

function ArchiveTable({
  archive,
  selected,
  toggle,
  busy,
}: {
  archive: OpenArchive;
  selected: string[];
  toggle: (path: string) => void;
  busy: boolean;
}) {
  const tree = useMemo(() => buildTree(archive.entries), [archive.entries]);
  const [folder, setFolder] = useState("");
  const [page, setPage] = useState(0);
  const rows = tree.get(folder) ?? [];
  const parts = folder ? folder.split("/") : [];
  const pageCount = Math.ceil(rows.length / PAGE_SIZE);
  function go(path: string) {
    setFolder(path);
    setPage(0);
  }

  return (
    <>
      <div className="archive-location" aria-label="Archive location">
        <Button onClick={() => go("")} disabled={!folder}>
          Archive root
        </Button>
        {folder && (
          <Button onClick={() => go(parts.slice(0, -1).join("/"))}>
            Up one folder
          </Button>
        )}
        {parts.map((part, index) => (
          <span className="breadcrumb" key={index}>
            <span aria-hidden="true">/</span>
            <button
              onClick={() => go(parts.slice(0, index + 1).join("/"))}
              title={parts.slice(0, index + 1).join("/")}
            >
              {part}
            </button>
          </span>
        ))}
      </div>
      {rows.length === 0 ? (
        <EmptyState
          icon={Folder}
          title={folder ? "Empty folder" : "Empty archive"}
        >
          This location contains no entries.
        </EmptyState>
      ) : (
        <div className="archive-table-wrap">
          <table className="archive-table">
            <caption className="sr-only">
              Entries in {folder || "the archive root"}
            </caption>
            <thead>
              <tr>
                <th scope="col">Name</th>
                <th scope="col">Type</th>
                <th scope="col">Compressed</th>
                <th scope="col">Size</th>
              </tr>
            </thead>
            <tbody>
              {rows
                .slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE)
                .map(({ id, name, path, entry, folder }) => (
                  <tr
                    key={id}
                    data-selected={selected.includes(
                      (entry?.path ?? path).replace(/\/$/, ""),
                    )}
                  >
                    <td>
                      <input
                        type="checkbox"
                        aria-label={`Select ${name}`}
                        checked={selected.includes(
                          (entry?.path ?? path).replace(/\/$/, ""),
                        )}
                        disabled={
                          busy ||
                          Boolean(
                            entry?.issue && entry.issue.code !== "encrypted",
                          )
                        }
                        onChange={() =>
                          toggle((entry?.path ?? path).replace(/\/$/, ""))
                        }
                        className="mr-2"
                      />
                      {folder !== undefined ? (
                        <Button
                          className="archive-folder"
                          onClick={() => go(folder)}
                        >
                          <Folder size={16} aria-hidden="true" />
                          <bdi title={entry?.path ?? path}>{name}</bdi>
                        </Button>
                      ) : (
                        <span className="archive-file">
                          <File size={16} aria-hidden="true" />
                          <bdi title={entry?.path ?? path}>{name}</bdi>
                        </span>
                      )}

                      {entry?.issue && (
                        <small className="archive-issue">
                          {entry.issue.message}
                        </small>
                      )}
                    </td>
                    <td>
                      {entry
                        ? {
                            file: "File",
                            directory: "Folder",
                            link: "Link",
                            special: "Special",
                          }[entry.kind]
                        : "Folder"}
                      {entry?.kind === "file" && entry.method && (
                        <small className="muted">{entry.method}</small>
                      )}
                      {entry?.modified && (
                        <small className="muted">{entry.modified}</small>
                      )}
                    </td>
                    <td className="archive-size">
                      {entry?.compressedBytes != null
                        ? bytes(entry.compressedBytes)
                        : "—"}
                    </td>
                    <td className="archive-size">
                      {entry ? bytes(entry.uncompressedBytes) : "—"}
                    </td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
      )}
      {pageCount > 1 && (
        <div className="archive-location">
          <Button disabled={page === 0} onClick={() => setPage(page - 1)}>
            Previous
          </Button>
          <span role="status">
            Page {page + 1} of {pageCount}
          </span>
          <Button
            disabled={page + 1 >= pageCount}
            onClick={() => setPage(page + 1)}
          >
            Next
          </Button>
        </div>
      )}
    </>
  );
}

export function OperationPanel({
  explorer,
  compact = false,
  inline = false,
}: {
  explorer: Explorer;
  compact?: boolean;
  inline?: boolean;
}) {
  const { operation, progress } = explorer;
  if (operation.state === "idle") return null;
  const terminal =
    operation.state === "completed" ||
    operation.state === "failed" ||
    operation.state === "cancelled";
  const labels: Record<string, string> = {
    preparing: "Preparing",
    compressing: "Compressing archive",
    packaging: "Packaging archive",
    creating: "Creating archive",
    running: "Processing files",
    verifying: "Verifying archive",
    committing: "Committing output",
    completed: "Completed",
    failed: "Failed",
    cancelled: "Cancelled",
    inspecting: "Opening archive",
    testing: "Checking archive",
    extracting: "Extracting archive",
  };
  const stage = terminal
    ? operation.state
    : (progress?.state ?? operation.state);
  const total = progress?.totalBytes ? Number(progress.totalBytes) : 0;
  const processed = progress ? Number(progress.processedBytes) : 0;
  const panel = (
    <section
      className={`operation-panel ${inline ? "operation-inline" : ""} operation-${stage}`}
      aria-label="Archive operation"
      aria-busy={!terminal}
    >
      <div className="operation-panel-heading">
        <h2>{labels[stage] ?? stage}</h2>
        {terminal ? (
          <Button
            onClick={
              compact
                ? () => void desktop.closeOperationWindow()
                : explorer.dismissOperation
            }
          >
            {compact ? "Close" : "Dismiss"}
          </Button>
        ) : (
          <Button primary onClick={explorer.cancelActive}>
            Cancel operation
          </Button>
        )}
      </div>
      {terminal && (
        <Status error={operation.state === "failed"}>
          {operation.message}
        </Status>
      )}
      {progress?.currentFile && (
        <p className="operation-file" dir="auto">
          {progress.currentFile}
        </p>
      )}
      {progress &&
        (progress.totalBytes !== null ||
          progress.processedBytes !== "0" ||
          progress.totalFiles !== null) && (
          <p className="muted">
            {bytes(progress.processedBytes)}
            {progress.totalBytes && ` / ${bytes(progress.totalBytes)}`}
            {progress.totalFiles != null &&
              ` · ${progress.completedFiles} / ${progress.totalFiles} files`}
          </p>
        )}
      {!terminal &&
        (total > 0 ? (
          <progress
            aria-label={labels[stage] ?? stage}
            max={total}
            value={Math.min(processed, total - 1)}
          />
        ) : (
          <progress aria-label={labels[stage] ?? stage} />
        ))}
    </section>
  );
  return inline ? panel : createPortal(panel, document.body);
}

function PasswordPrompt({
  message,
  answer,
}: {
  message: string;
  answer: (password: string | null) => void;
}) {
  const [password, setPassword] = useState("");
  return (
    <Dialog
      title="Archive password"
      onClose={() => {
        setPassword("");
        answer(null);
      }}
      actions={<></>}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          const value = password;
          setPassword("");
          answer(value);
        }}
      >
        <p>{message}</p>
        <label htmlFor="archive-password" className="block mt-3">
          Password
        </label>
        <input
          id="archive-password"
          type="password"
          autoComplete="off"
          autoFocus
          value={password}
          onChange={(event) => setPassword(event.currentTarget.value)}
          className="w-full my-3 p-2 rounded border border-current"
        />
        <div className="flex gap-2">
          <Button
            onClick={() => {
              setPassword("");
              answer(null);
            }}
          >
            Cancel
          </Button>
          <Button primary type="submit">
            Continue
          </Button>
        </div>
      </form>
    </Dialog>
  );
}

export function ArchiveExplorer({
  explorer,
  compact = false,
}: {
  explorer: Explorer;
  compact?: boolean;
}) {
  const { archive, destination, busy, dropAvailable } = explorer;
  return (
    <>
      {!compact && (
        <Card>
          {archive ? (
            <>
              <FileRow
                icon={Archive}
                name={archive.name}
                path={archive.source}
              />
              <p className="archive-meta">
                {archive.capabilities.format} ·{" "}
                {archive.entries.length.toLocaleString()} entries ·{" "}
                {bytes(archive.archiveBytes)} archive ·{" "}
                {bytes(archive.totalUncompressedBytes)} uncompressed
              </p>
              <p className="archive-meta">{archive.capabilities.limitations}</p>
            </>
          ) : (
            <EmptyState icon={FolderOpen} title="Open an archive">
              Select a file using your system picker
              {dropAvailable ? " or drop one into PACKIT" : ""}. Open ZIP, 7z,
              RAR, TAR, TAR.GZ or supported ISO images.
            </EmptyState>
          )}
          <div className="archive-actions">
            <Button primary disabled={busy} onClick={explorer.chooseArchive}>
              <FolderOpen size={18} aria-hidden="true" /> Open Archive
            </Button>
            {archive && (
              <>
                <Button
                  disabled={busy || explorer.selected.length === 0}
                  onClick={() => explorer.chooseDestination(explorer.selected)}
                >
                  Extract Selected ({explorer.selected.length})
                </Button>
                {explorer.selected.length > 0 && (
                  <Button disabled={busy} onClick={explorer.clearSelection}>
                    Clear selection
                  </Button>
                )}
                <Button disabled={busy} onClick={explorer.closeArchive}>
                  Close Archive
                </Button>
                <Button disabled={busy} onClick={explorer.test}>
                  {archive.capabilities.integrity === "structure"
                    ? "Validate Structure"
                    : "Test Archive"}
                </Button>
                <Button
                  disabled={busy || !archive.capabilities.extraction}
                  onClick={() => explorer.chooseDestination()}
                >
                  Extract All
                </Button>
              </>
            )}
          </div>
          {archive && (
            <ArchiveTable
              key={archive.source}
              archive={archive}
              selected={explorer.selected}
              toggle={explorer.toggle}
              busy={busy}
            />
          )}
        </Card>
      )}
      {explorer.passwordPrompt && (
        <PasswordPrompt
          message={explorer.passwordPrompt}
          answer={explorer.answerPassword}
        />
      )}
      {archive && destination && (
        <Dialog
          title={
            destination.exists
              ? "Extraction destination already exists"
              : destination.selection
                ? "Extract selected files?"
                : "Extract all files?"
          }
          onClose={explorer.cancel}
          actions={
            <>
              <Button onClick={explorer.cancel}>Cancel</Button>
              {destination.exists ? (
                <>
                  <Button primary onClick={() => explorer.extract("keep_both")}>
                    Keep Both
                  </Button>
                  <Button onClick={() => explorer.extract("replace")}>
                    Replace Existing Files
                  </Button>
                </>
              ) : (
                <Button primary onClick={() => explorer.extract("refuse")}>
                  {destination.selection ? "Extract Selected" : "Extract All"}
                </Button>
              )}
            </>
          }
        >
          <p>
            {destination.here ? (
              "Extract directly into:"
            ) : (
              <>
                <strong>{archive.outputFolder}</strong> inside:
              </>
            )}
            {destination.selection && (
              <span>
                {" "}
                Only the selected files and folder contents will be written.
              </span>
            )}
          </p>
          <p className="break-all" dir="auto">
            {destination.path}
          </p>
          {destination.exists ? (
            <>
              <p>
                <strong>Replace Existing Files</strong> overwrites matching
                files after verification and keeps unrelated files. It requires
                an existing folder.
              </p>
              <p>
                <strong>Keep Both</strong> creates a new numbered folder,
                leaving the existing destination untouched.
              </p>
              <p>
                <strong>Cancel</strong> makes no changes.
              </p>
            </>
          ) : (
            <p>
              {destination.here
                ? "PACKIT will extract directly into this existing folder. Unrelated files are preserved."
                : "PACKIT will create this folder and extract the archive into it."}
            </p>
          )}
        </Dialog>
      )}
    </>
  );
}
