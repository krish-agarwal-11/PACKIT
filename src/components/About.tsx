// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
import { useState } from "react";
import { desktop } from "../desktop";
import logo from "../../branding/packit-logo.png";
import { appVersion } from "../version";
import { Button, Dialog } from "./ui";

export function About({
  onClose,
  onCheckUpdates,
}: {
  onClose: () => void;
  onCheckUpdates?: () => void;
}) {
  const [legal, setLegal] = useState<{ title: string; text: string } | null>(
    null,
  );
  const [error, setError] = useState("");
  async function showLegal(document: "license" | "notices") {
    try {
      setLegal({
        title:
          document === "license" ? "GNU GPLv3 license" : "Third-party notices",
        text: await desktop.legalDocument(document),
      });
      setError("");
    } catch {
      setError(
        "Could not open the document. The installed LICENSE and THIRD_PARTY_NOTICES.md files are also available in PACKIT’s installation folder.",
      );
    }
  }
  if (legal)
    return (
      <Dialog
        title={legal.title}
        className="legal-dialog"
        onClose={() => setLegal(null)}
      >
        <pre className="legal-text" tabIndex={0}>
          {legal.text}
        </pre>
      </Dialog>
    );
  return (
    <Dialog title="About PACKIT" className="about-dialog" onClose={onClose}>
      <div className="about-identity">
        <img src={logo} alt="" />
        <div>
          <h3>PACKIT</h3>
          <p>A modern archive manager</p>
        </div>
      </div>
      <p className="about-version">
        Version <strong>{appVersion}</strong>
      </p>
      <p>Pack, unpack, browse and manage your archives with ease.</p>
      <p>
        PACKIT is a free and open-source desktop archive manager built to make
        everyday file compression and extraction simple, intuitive and reliable.
      </p>
      <div className="about-privacy">
        <strong>Local processing. No account or cloud required.</strong>
        <p>PACKIT does not collect telemetry.</p>
      </div>
      <p className="about-license">
        PACKIT's original source code is licensed under GNU GPLv3. Third-party
        components retain their respective licenses.
      </p>
      <div className="flex flex-wrap gap-2">
        {onCheckUpdates && (
          <Button onClick={onCheckUpdates}>Check for Updates</Button>
        )}
        <Button onClick={() => void showLegal("license")}>GPLv3 license</Button>
        <Button onClick={() => void showLegal("notices")}>
          Third-party notices
        </Button>
      </div>
      {error && <p role="alert">{error}</p>}
    </Dialog>
  );
}
