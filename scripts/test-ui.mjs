// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
import assert from "node:assert/strict";
import { createServer } from "vite";
import React from "react";
import { renderToStaticMarkup as render } from "react-dom/server";
import { readFile } from "node:fs/promises";

// Render actual components. Only the portal transport is replaced: its children
// are rendered inline so progress and popup content can be checked without a DOM.
const server = await createServer({
  server: { middlewareMode: true },
  ssr: { noExternal: ["react-dom"] },
  plugins: [
    {
      name: "test-portal",
      enforce: "pre",
      resolveId(id) {
        if (id === "react-dom") return "\0test-portal";
      },
      load(id) {
        if (id === "\0test-portal")
          return "export const createPortal = children => children;";
      },
    },
  ],
});
globalThis.window = { location: { search: "" } };
globalThis.document = { body: {} };
let passed = 0;
function check(name, action) {
  action();
  passed++;
  console.log(`PASS ${name}`);
}
try {
  const { default: App } = await server.ssrLoadModule("/src/App.tsx");
  const { ArchiveExplorer, OperationPanel } = await server.ssrLoadModule(
    "/src/archive/ArchiveExplorer.tsx",
  );
  const { HomeWorkspace } = await server.ssrLoadModule(
    "/src/archive/HomeWorkspace.tsx",
  );
  const { CreateArchive } = await server.ssrLoadModule(
    "/src/archive/CreateArchive.tsx",
  );
  const idle = {
    operation: { state: "idle" },
    progress: null,
    busy: false,
    selected: [],
    dropAvailable: true,
  };
  const { About } = await server.ssrLoadModule("/src/components/About.tsx");
  const requireReadMetadata = await readFile(
    "src-tauri/tauri.conf.json",
    "utf8",
  );
  const home = render(React.createElement(App));
  check(
    "Brand is one accessible native Home button, without milestone UI",
    () => {
      assert.match(home, /<button[^>]+aria-label="PACKIT — Home"/);
      assert.doesNotMatch(home, /Phase [45]|Welcome to PACKIT|Workspace tools/);
      assert.match(home, /No operation running/);
    },
  );
  check("About and footer use the same authoritative Tauri version", () => {
    const metadata = JSON.parse(requireReadMetadata);
    assert.match(
      home,
      new RegExp(`v${metadata.version.replaceAll(".", "\\.")}<`),
    );
    const about = render(React.createElement(About, { onClose: () => {}, onCheckUpdates: () => {} }));
    assert.match(about, /class="about-dialog"/);
    assert.ok(about.includes(`<strong>${metadata.version}</strong>`));
    assert.match(about, /original source code is licensed under GNU GPLv3/);
    assert.doesNotMatch(about, /github.com|audit completed/);
    assert.match(about, />Close</);
    assert.match(about, />GPLv3 license</);
    assert.match(about, />Check for Updates</);
    assert.match(about, />Third-party notices</);
    assert.match(about, /Local processing/);
  });
  check("Updater stays private-feed gated and popup-safe", () => {
    const metadata=JSON.parse(requireReadMetadata);
    assert.equal(metadata.plugins.updater.requireSignedVersion,true);
    assert.match(metadata.plugins.updater.endpoints[0], /^https:\/\/github\.com\/krish-agarwal-11\/PACKIT\/releases\//);
  });
  const archive = {
    name: "日本.zip",
    source: "D:/日本.zip",
    entries: [
      {
        path: "café.txt",
        kind: "file",
        compressedBytes: "2",
        uncompressedBytes: "3",
        method: "Stored",
        modified: null,
        issue: null,
      },
    ],
    archiveBytes: "20",
    totalUncompressedBytes: "3",
    capabilities: {
      format: "ZIP",
      extraction: true,
      integrity: "checksums",
      limitations: "",
    },
  };
  check(
    "Hybrid Home renders real archive preview and one pair of launch surfaces",
    () => {
      const html = render(
        React.createElement(HomeWorkspace, {
          explorer: { ...idle, archive },
          onCreate: () => {},
          onBrowse: () => {},
          onTasks: () => {},
          disabled: false,
        }),
      );
      assert.match(html, /Current archive/);
      assert.match(html, /日本.zip/);
      assert.match(html, /café.txt/);
      assert.match(html, /1 entries/);
      assert.equal((html.match(/class="action-surface/g) || []).length, 2);
      assert.match(html, /Continue Browsing/);
      assert.doesNotMatch(html, /A clear desk/);
    },
  );
  check(
    "Home shares live progress and cancellation without a floating duplicate",
    () => {
      const html = render(
        React.createElement(HomeWorkspace, {
          explorer: {
            ...idle,
            operation: { state: "extracting" },
            progress: {
              state: "running",
              processedBytes: "25",
              totalBytes: "100",
              completedFiles: 1,
              totalFiles: 3,
            },
          },
          onCreate: () => {},
          onBrowse: () => {},
          onTasks: () => {},
          disabled: true,
        }),
      );
      assert.match(html, /operation-inline/);
      assert.match(html, /max="100" value="25"/);
      assert.equal((html.match(/Cancel operation/g) || []).length, 1);
      assert.match(html, /class="action-surface fill-hover primary" disabled/);
    },
  );
  check("Explorer selection and long-name disclosure use actual paths", () => {
    const html = render(
      React.createElement(ArchiveExplorer, {
        explorer: { ...idle, archive, selected: ["café.txt"] },
      }),
    );
    assert.match(html, /data-selected="true"/);
    assert.match(html, /title="café.txt"/);
    assert.match(html, /Select café.txt/);
    assert.match(html, /checked=""/);
  });
  check("Selected extraction confirmation describes selected output", () => {
    const html = render(
      React.createElement(ArchiveExplorer, {
        compact: true,
        explorer: {
          ...idle,
          archive,
          destination: {
            path: "D:/out",
            exists: false,
            selection: ["café.txt"],
          },
        },
      }),
    );
    assert.match(html, /Extract selected files\?/);
    assert.match(html, /Only the selected files/);
  });
  const seed = {
    sources: ["D:/Photos/café.txt"],
    filename: "café.tar",
    destination: "D:/out",
    format: "tar",
    quick: false,
    id: "test",
  };
  check(
    "Compact creation keeps editable naming, real sources and disabled TAR compression",
    () => {
      const html = render(
        React.createElement(CreateArchive, {
          compact: true,
          seed,
          explorer: idle,
        }),
      );
      assert.match(html, /value="café.tar"/);
      assert.match(html, /id="creation-preset" disabled/);
      assert.match(html, /Remove café.txt/);
      assert.match(html, /button fill-hover primary/);
      assert.match(html, /button-content/);
      assert.doesNotMatch(html, /<option[^>]*value="rar"/);
    },
  );
  check("Progress uses reported bytes, and cancellation stays visible", () => {
    const html = render(
      React.createElement(OperationPanel, {
        compact: true,
        explorer: {
          ...idle,
          operation: { state: "extracting" },
          progress: {
            state: "running",
            currentFile: "café.txt",
            processedBytes: "25",
            totalBytes: "100",
            completedFiles: 1,
            totalFiles: 3,
          },
        },
      }),
    );
    assert.match(html, /Cancel operation/);
    assert.match(html, /max="100" value="25"/);
    assert.match(html, /1 \/ 3 files/);
  });
  check(
    "Unknown totals are indeterminate; completed popup offers Close",
    () => {
      const html = render(
        React.createElement(OperationPanel, {
          compact: true,
          explorer: { ...idle, operation: { state: "extracting" } },
        }),
      );
      assert.match(html, /<progress/);
      assert.doesNotMatch(html, /value="/);
      const done = render(
        React.createElement(OperationPanel, {
          compact: true,
          explorer: {
            ...idle,
            operation: { state: "completed", message: "Verified and saved" },
          },
        }),
      );
      assert.match(done, /Verified and saved/);
      assert.match(done, />Close</);
      assert.doesNotMatch(done, /Cancel operation|<progress/);
    },
  );
  check("Conflict and password popup content uses shared dialogs", () => {
    const html = render(
      React.createElement(ArchiveExplorer, {
        compact: true,
        explorer: {
          ...idle,
          archive,
          passwordPrompt: "Password required",
          destination: { exists: true, path: "D:/out" },
        },
      }),
    );
    assert.match(html, /type="password"/);
    assert.match(html, /Keep Both/);
    assert.match(html, /Replace Existing Files/);
    assert.doesNotMatch(html, /archive-table/);
  });
  const css = await readFile("src/index.css", "utf8");
  check("Shared theme tokens and reduced-motion override cover popups", () => {
    assert.match(css, /prefers-reduced-motion: reduce/);
    assert.match(css, /animation: none !important/);
    assert.match(css, /transition: none !important/);
    assert.match(css, /data-theme="system"/);
    assert.match(css, /data-theme="light"/);
    assert.match(css, /\.operation-workspace/);
  });
  console.log(
    `${passed} UI render checks passed. Pixel appearance and real keyboard interaction require desktop acceptance.`,
  );
} finally {
  await server.close();
  delete globalThis.window;
  delete globalThis.document;
}
