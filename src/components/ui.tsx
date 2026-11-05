// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
import { useEffect, useRef, type ComponentProps, type ReactNode } from "react";
import type { LucideIcon } from "lucide-react";

export function Button({
  primary,
  className = "",
  children,
  ...props
}: ComponentProps<"button"> & { primary?: boolean }) {
  return (
    <button
      type="button"
      className={`button fill-hover ${primary ? "primary" : ""} ${className}`}
      {...props}
    >
      <span className="button-content">{children}</span>
    </button>
  );
}

export function ActionSurface({
  icon: Icon,
  label,
  description,
  primary,
  ...props
}: ComponentProps<"button"> & {
  icon: LucideIcon;
  label: string;
  description: string;
  primary?: boolean;
}) {
  return (
    <button
      type="button"
      className={`action-surface fill-hover ${primary ? "primary" : ""}`}
      {...props}
    >
      <span className="action-icon">
        <Icon size={22} strokeWidth={1.7} aria-hidden="true" />
      </span>
      <span className="action-copy">
        <strong>{label}</strong>
        <small>{description}</small>
      </span>
      <span className="action-arrow" aria-hidden="true">
        ↗
      </span>
    </button>
  );
}

export function Navigation<T extends string>({
  items,
  current,
  onSelect,
}: {
  items: ReadonlyArray<{ label: T; icon: LucideIcon }>;
  current: T;
  onSelect: (label: T) => void;
}) {
  return (
    <nav aria-label="Main navigation">
      {items.map(({ label, icon: Icon }) => (
        <button
          key={label}
          type="button"
          className="nav-item fill-hover"
          aria-label={label}
          aria-current={current === label ? "page" : undefined}
          onClick={() => onSelect(label)}
        >
          <Icon size={18} aria-hidden="true" />
          <span>{label}</span>
        </button>
      ))}
    </nav>
  );
}

export function Card({ children }: { children: ReactNode }) {
  return <section className="card">{children}</section>;
}

export function EmptyState({
  icon: Icon,
  title,
  children,
}: {
  icon: LucideIcon;
  title: string;
  children: ReactNode;
}) {
  return (
    <div className="empty">
      <span className="glyph">
        <Icon size={24} aria-hidden="true" />
      </span>
      <h2>{title}</h2>
      <div className="muted">{children}</div>
    </div>
  );
}

export function FileRow({
  name,
  path,
  icon: Icon,
}: {
  name: string;
  path: string;
  icon: LucideIcon;
}) {
  return (
    <div className="file-row">
      <Icon size={20} aria-hidden="true" />
      <div>
        <strong>{name}</strong>
        <p className="muted">{path}</p>
      </div>
    </div>
  );
}

export function Status({
  error,
  children,
}: {
  error?: boolean;
  children: ReactNode;
}) {
  return (
    <div
      className={error ? "status error" : "status"}
      role={error ? "alert" : "status"}
    >
      {children}
    </div>
  );
}

export function Dialog({
  title,
  children,
  onClose,
  actions,
  className = "",
}: {
  className?: string;
  title: string;
  children: ReactNode;
  onClose: () => void;
  actions?: ReactNode;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const element = ref.current;
    const returnFocus =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    element?.showModal();
    return () => {
      element?.close();
      if (returnFocus?.isConnected) returnFocus.focus({ preventScroll: true });
    };
  }, []);
  return (
    <dialog
      ref={ref}
      className={className}
      aria-label={title}
      onCancel={onClose}
    >
      <h2>{title}</h2>
      {children}
      <div className="mt-5 flex flex-wrap gap-2">
        {actions ?? <Button onClick={onClose}>Close</Button>}
      </div>
    </dialog>
  );
}
