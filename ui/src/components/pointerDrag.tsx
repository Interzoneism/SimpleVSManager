import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";

export interface DragPoint {
  x: number;
  y: number;
}

/** True when the point is inside the element's box. */
export function pointIn(el: Element | null, p: DragPoint): boolean {
  if (!el) return false;
  const r = el.getBoundingClientRect();
  return p.x >= r.left && p.x <= r.right && p.y >= r.top && p.y <= r.bottom;
}

const THRESHOLD = 6;

/**
 * Drag and drop with pointer events. (The webview's own HTML drag and drop is taken over by the
 * window's file drop handling, so it can't be used.) A drag starts after the pointer moves a few
 * pixels; a plain click still clicks. Escape cancels. While dragging, a label follows the pointer.
 */
export function usePointerDrag<T>(handlers: {
  /** Called when the pointer moves during a drag, e.g. to light up a drop target. */
  onMove?: (p: DragPoint, payload: T) => void;
  onDrop: (p: DragPoint, payload: T) => void;
  onEnd?: () => void;
}) {
  const latest = useRef(handlers);
  latest.current = handlers;
  const [active, setActive] = useState<{ payload: T; label: ReactNode; at: DragPoint } | null>(null);
  const ghost = useRef<HTMLDivElement>(null);
  const cleanup = useRef<(() => void) | null>(null);
  // A drag ends with a click on the element it started on; swallow that click.
  const swallowClick = useRef(false);

  useEffect(() => () => cleanup.current?.(), []);

  const place = (p: DragPoint) => {
    if (ghost.current) ghost.current.style.transform = `translate(${p.x + 14}px, ${p.y + 12}px)`;
  };

  const begin = (e: ReactPointerEvent, payload: T, label: ReactNode) => {
    if (e.button !== 0 || e.isPrimary === false) return;
    cleanup.current?.();
    const start = { x: e.clientX, y: e.clientY };
    let dragging = false;
    let last = start;
    const move = (ev: PointerEvent) => {
      last = { x: ev.clientX, y: ev.clientY };
      if (!dragging) {
        if (Math.hypot(last.x - start.x, last.y - start.y) < THRESHOLD) return;
        dragging = true;
        document.body.classList.add("pointer-dragging");
        setActive({ payload, label, at: last });
      }
      place(last);
      latest.current.onMove?.(last, payload);
    };
    const finish = (drop: boolean) => {
      stop();
      if (!dragging) return;
      swallowClick.current = true;
      setTimeout(() => (swallowClick.current = false), 0);
      if (drop) latest.current.onDrop(last, payload);
      latest.current.onEnd?.();
    };
    const up = (ev: PointerEvent) => {
      last = { x: ev.clientX, y: ev.clientY };
      finish(true);
    };
    const cancel = () => finish(false);
    const key = (ev: KeyboardEvent) => {
      if (ev.key === "Escape") {
        ev.preventDefault();
        finish(false);
      }
    };
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("blur", cancel);
      document.body.classList.remove("pointer-dragging");
      cleanup.current = null;
      setActive(null);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("keydown", key, true);
    window.addEventListener("blur", cancel);
    cleanup.current = stop;
  };

  /** Put first in a dragged element's onClick: true when that click ended a drag. */
  const endedDrag = () => swallowClick.current;

  const overlay = active
    ? createPortal(
        <div ref={(el) => { ghost.current = el; }} className="drag-ghost" aria-hidden
          style={{ transform: `translate(${active.at.x + 14}px, ${active.at.y + 12}px)` }}>
          {active.label}
        </div>,
        document.body,
      )
    : null;

  return { begin, endedDrag, dragging: active?.payload ?? null, overlay };
}
