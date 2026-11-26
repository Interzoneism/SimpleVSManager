import { useEffect, useState, useSyncExternalStore, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { Icon, type IconName } from "./Icon";

/**
 * App-wide notifications: cards stacked in the lower-right corner. They time out on their own
 * (paused while hovered or focused), can be closed, and are colored by kind. Errors that need
 * action can pass `duration: null` to stay until closed. Blocking questions stay dialogs.
 */
export type ToastKind = "info" | "success" | "warning" | "error";

export interface ToastAction {
  label: string;
  icon?: IconName;
  onClick: () => void;
}

export interface ToastOptions {
  /** Showing a toast with an id that is already on screen replaces it. */
  id?: string;
  title?: string;
  /** Milliseconds on screen; `null` keeps it until closed. Defaults depend on the kind. */
  duration?: number | null;
  action?: ToastAction;
  /** Called when the toast goes away, by timing out or being closed. */
  onDismiss?: () => void;
}

interface ToastEntry extends ToastOptions {
  id: string;
  kind: ToastKind;
  message: ReactNode;
  /** Bumped when the same toast is shown again, to restart its timer. */
  version: number;
  leaving: boolean;
}

const DEFAULT_DURATION: Record<ToastKind, number> = { info: 5000, success: 4500, warning: 8000, error: 10000 };
const KIND_ICON: Record<ToastKind, IconName> = { info: "info", success: "check", warning: "warning", error: "alert" };
const MAX_TOASTS = 5;
const LEAVE_MS = 180;

let toasts: ToastEntry[] = [];
let nextId = 1;
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => void listeners.delete(l);
};

function show(kind: ToastKind, message: ReactNode, options: ToastOptions = {}): string {
  // The same text again (or the same id) refreshes the toast that is already up.
  const existing = toasts.find((t) => !t.leaving && (options.id ? t.id === options.id : t.kind === kind && typeof message === "string" && t.message === message));
  if (existing) {
    toasts = toasts.map((t) => (t === existing ? { ...t, ...options, id: t.id, kind, message, version: t.version + 1 } : t));
  } else {
    const id = options.id ?? `toast-${nextId++}`;
    toasts = [...toasts, { ...options, id, kind, message, version: 0, leaving: false }];
    const live = toasts.filter((t) => !t.leaving);
    if (live.length > MAX_TOASTS) {
      // Drop the oldest one that times out on its own; sticky ones stay.
      const oldest = live.find((t) => t.duration !== null) ?? live[0];
      dismiss(oldest.id);
    }
  }
  emit();
  return existing?.id ?? toasts[toasts.length - 1].id;
}

function dismiss(id: string) {
  const entry = toasts.find((t) => t.id === id);
  if (!entry || entry.leaving) return;
  toasts = toasts.map((t) => (t.id === id ? { ...t, leaving: true } : t));
  emit();
  entry.onDismiss?.();
  setTimeout(() => {
    toasts = toasts.filter((t) => t.id !== id || !t.leaving);
    emit();
  }, LEAVE_MS);
}

export const toast = {
  info: (message: ReactNode, options?: ToastOptions) => show("info", message, options),
  success: (message: ReactNode, options?: ToastOptions) => show("success", message, options),
  warning: (message: ReactNode, options?: ToastOptions) => show("warning", message, options),
  error: (message: ReactNode, options?: ToastOptions) => show("error", message, options),
  dismiss,
  /** For tests: removes every toast at once. */
  clear: () => {
    toasts = [];
    emit();
  },
};

/** Shows an error toast each time `error` changes to a new error. */
export function useErrorToast(error: { message: string } | string | null | undefined, options?: ToastOptions) {
  useEffect(() => {
    if (!error) return;
    toast.error(typeof error === "string" ? error : error.message, options);
    // Options are read when the error appears; a new options object alone shouldn't re-show it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [error]);
}

/** Renders the toast stack. Mount once, near the app root. */
export function Toaster() {
  const list = useSyncExternalStore(subscribe, () => toasts, () => toasts);
  return createPortal(
    <section className="toaster" aria-label="Notifications">
      {list.map((t) => <ToastCard key={t.id} entry={t} />)}
    </section>,
    document.body,
  );
}

function ToastCard({ entry }: { entry: ToastEntry }) {
  const duration = entry.duration === undefined ? DEFAULT_DURATION[entry.kind] : entry.duration;
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const [windowHidden, setWindowHidden] = useState(() => typeof document !== "undefined" && document.hidden);
  useEffect(() => {
    const onVisibility = () => setWindowHidden(document.hidden);
    document.addEventListener("visibilitychange", onVisibility);
    return () => document.removeEventListener("visibilitychange", onVisibility);
  }, []);
  const paused = hovered || focused || windowHidden;
  return (
    <div
      className={`toast toast-${entry.kind}${entry.leaving ? " leaving" : ""}`}
      role={entry.kind === "error" ? "alert" : "status"}
      aria-live={entry.kind === "error" ? "assertive" : "polite"}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onFocus={() => setFocused(true)}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setFocused(false);
      }}
    >
      <span className="toast-icon" aria-hidden="true"><Icon name={KIND_ICON[entry.kind]} /></span>
      <div className="toast-body">
        {entry.title && <strong className="toast-title">{entry.title}</strong>}
        <div className="toast-message">{entry.message}</div>
        {entry.action && (
          <button
            type="button"
            className="toast-action small"
            onClick={() => {
              entry.action!.onClick();
              dismiss(entry.id);
            }}
          >
            {entry.action.icon && <Icon name={entry.action.icon} />}
            {entry.action.label}
          </button>
        )}
      </div>
      <button type="button" className="icon-button toast-close" aria-label="Close notification" title="Close" onClick={() => dismiss(entry.id)}>
        <Icon name="close" />
      </button>
      {duration !== null && !entry.leaving && (
        <span
          key={entry.version}
          className="toast-timer"
          aria-hidden="true"
          style={{ animationDuration: `${duration}ms`, animationPlayState: paused ? "paused" : "running" }}
          onAnimationEnd={() => dismiss(entry.id)}
        />
      )}
    </div>
  );
}
