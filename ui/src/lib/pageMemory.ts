// Remembers page state (filters, scroll positions) while the user moves between pages. Pages are
// unmounted when you leave them, so their React state would be lost. Kept in memory and in
// sessionStorage, so a webview reload keeps it too.

import { useEffect, useLayoutEffect, useRef, type RefObject } from "react";

const PREFIX = "svsm-page:";
const memory = new Map<string, unknown>();

export function recall<T>(key: string, fallback: T, valid?: (v: unknown) => v is T): T {
  if (memory.has(key)) return memory.get(key) as T;
  try {
    const raw = sessionStorage.getItem(PREFIX + key);
    if (raw != null) {
      const v: unknown = JSON.parse(raw);
      if (valid ? valid(v) : typeof v === typeof fallback) {
        memory.set(key, v);
        return v as T;
      }
    }
  } catch {
    // Storage unavailable or bad data: start fresh.
  }
  return fallback;
}

const pending = new Map<string, ReturnType<typeof setTimeout>>();

export function remember<T>(key: string, value: T) {
  memory.set(key, value);
  clearTimeout(pending.get(key));
  // Scrolling writes often; storage only needs the last value.
  pending.set(key, setTimeout(() => {
    pending.delete(key);
    try {
      sessionStorage.setItem(PREFIX + key, JSON.stringify(value));
    } catch {
      // Memory still has it for this session.
    }
  }, 150));
}

/** Test helper. */
export function forgetAll() {
  memory.clear();
  for (const t of pending.values()) clearTimeout(t);
  pending.clear();
  try {
    for (const k of Object.keys(sessionStorage)) if (k.startsWith(PREFIX)) sessionStorage.removeItem(k);
  } catch {
    // ignore
  }
}


/**
 * Keeps a scroll box's position under `key` and puts it back when the page comes back. Waits
 * until `ready` (the content is there), then retries for a moment while the layout settles.
 * Scrolling by the user before that wins.
 */
export function useRememberedScroll(key: string, ref: RefObject<HTMLElement | null>, ready: boolean) {
  const target = useRef(recall<number>(key, 0));
  const restored = useRef(target.current <= 0);

  const tryRestore = () => {
    const el = ref.current;
    if (!el || restored.current) return;
    el.scrollTop = target.current;
    if (Math.abs(el.scrollTop - target.current) <= 1) restored.current = true;
  };

  useLayoutEffect(() => {
    if (ready) tryRestore();
  });

  useEffect(() => {
    if (!ready || restored.current) return;
    const started = performance.now();
    let frame = 0;
    const loop = () => {
      tryRestore();
      if (restored.current) return;
      if (performance.now() - started > 2000) {
        restored.current = true;
        return;
      }
      frame = requestAnimationFrame(loop);
    };
    frame = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(frame);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready]);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onScroll = () => {
      if (restored.current) remember(key, el.scrollTop);
    };
    const userScrolled = () => {
      restored.current = true;
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    el.addEventListener("wheel", userScrolled, { passive: true });
    el.addEventListener("pointerdown", userScrolled);
    el.addEventListener("keydown", userScrolled);
    return () => {
      el.removeEventListener("scroll", onScroll);
      el.removeEventListener("wheel", userScrolled);
      el.removeEventListener("pointerdown", userScrolled);
      el.removeEventListener("keydown", userScrolled);
    };
  }, [key, ref]);
}
