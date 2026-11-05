// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
import {
  Activity,
  Archive,
  ArrowRight,
  Check,
  File,
  Folder,
  FolderOpen,
  Plus,
} from "lucide-react";
import { ActionSurface, Button } from "../components/ui";
import { OperationPanel } from "./ArchiveExplorer";
import { bytes, type useArchive } from "./useArchive";

type Explorer = ReturnType<typeof useArchive>;
export function HomeWorkspace({
  explorer,
  onCreate,
  onBrowse,
  onTasks,
  disabled,
}: {
  explorer: Explorer;
  onCreate: () => void;
  onBrowse: () => void;
  onTasks: () => void;
  disabled: boolean;
}) {
  const archive = explorer.archive;
  const preview = archive?.entries.slice(0, 7) ?? [];
  return (
    <div className="hybrid-workspace">
      <section className="launch-zone" aria-labelledby="desk-heading">
        <div className="desk-intro">
          <span className="desk-kicker">PACKIT / Workspace</span>
          <h1 id="desk-heading">Your archive desk.</h1>
          <p>
            Unpack what you need.
            <br />
            Bring your files together.
          </p>
        </div>
        <div className="launch-actions">
          <ActionSurface
            primary
            icon={FolderOpen}
            label="Open Archive"
            description="Browse, test and extract"
            onClick={explorer.chooseArchive}
            disabled={disabled}
          />
          <ActionSurface
            icon={Plus}
            label="Create Archive"
            description="Package files and folders"
            onClick={onCreate}
          />
        </div>
        <div className="desk-note">
          <Check size={15} aria-hidden="true" />
          <p>
            Local files. Verified output.
            <br />
            <span>No uploads, no cloud.</span>
          </p>
        </div>
      </section>
      <section
        className="archive-zone"
        aria-labelledby="current-archive-heading"
      >
        <header className="zone-heading">
          <span className="zone-icon">
            <Archive size={18} aria-hidden="true" />
          </span>
          <h2 id="current-archive-heading">Current archive</h2>
          <span className="state-chip">
            {archive ? archive.capabilities.format : "Not opened"}
          </span>
        </header>
        {archive ? (
          <>
            <div className="archive-identity">
              <h3 dir="auto">{archive.name}</h3>
              <p title={archive.source} dir="auto">
                {archive.source}
              </p>
              <div className="archive-facts">
                <span>{archive.entries.length.toLocaleString()} entries</span>
                <span>{bytes(archive.archiveBytes)}</span>
              </div>
            </div>
            <div className="preview-caption">
              Inside this archive <span>Preview</span>
            </div>
            <ul
              className="desk-file-preview"
              aria-label="Archive content preview"
            >
              {preview.map((entry, index) => (
                <li key={index}>
                  {entry.kind === "directory" ? (
                    <Folder size={16} aria-hidden="true" />
                  ) : (
                    <File size={16} aria-hidden="true" />
                  )}
                  <span title={entry.path} dir="auto">
                    {entry.path}
                  </span>
                  <small>
                    {entry.kind === "directory"
                      ? "Folder"
                      : bytes(entry.uncompressedBytes)}
                  </small>
                </li>
              ))}
            </ul>
            {archive.entries.length === 0 && (
              <p className="desk-no-entries">This archive is empty.</p>
            )}
            <footer className="archive-zone-footer">
              <span>
                {archive.entries.length > preview.length
                  ? `${archive.entries.length - preview.length} more entries in Explorer`
                  : "All entries shown"}
              </span>
              <Button primary onClick={onBrowse}>
                Continue Browsing
                <ArrowRight size={15} aria-hidden="true" />
              </Button>
            </footer>
          </>
        ) : (
          <>
            <div className="archive-desk-empty">
              <div className="empty-archive-mark">
                <FolderOpen size={38} strokeWidth={1.3} aria-hidden="true" />
              </div>
              <h3>A clear desk. A fresh start.</h3>
              <p>
                Your open archive lives here.
                <br />
                Browse its contents, then extract all or just a few files.
              </p>
              <span className="desk-empty-hint">
                Choose Open Archive to begin
              </span>
            </div>
            <footer className="archive-zone-footer">
              <span>Supported archive families</span>
              <div className="desk-formats">
                {["ZIP", "7z", "RAR", "TAR", "ISO"].map((format) => (
                  <span key={format}>{format}</span>
                ))}
              </div>
            </footer>
          </>
        )}
      </section>
      <section
        className="operations-zone"
        aria-labelledby="desk-operation-heading"
      >
        <header className="zone-heading">
          <Activity size={18} aria-hidden="true" />
          <h2 id="desk-operation-heading">Operations</h2>
        </header>
        {explorer.operation.state === "idle" ? (
          <div className="desk-idle">
            <span className="idle-indicator" />
            <div>
              <h3>No operation running</h3>
              <p>Progress and controls appear here when you start.</p>
            </div>
          </div>
        ) : (
          <OperationPanel explorer={explorer} inline />
        )}
        <button type="button" className="desk-task-link" onClick={onTasks}>
          View Tasks
          <ArrowRight size={14} aria-hidden="true" />
        </button>
      </section>
    </div>
  );
}
