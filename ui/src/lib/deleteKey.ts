import { useEffect, useRef } from "react";

/** True when a Delete key press should be left alone: typing, a dialog or menu is open, or someone else handled it. */
export function ignoreDeleteKey(e: KeyboardEvent): boolean {
  if (e.key !== "Delete" || e.repeat || e.defaultPrevented) return true;
  if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return true;
  const target = e.target instanceof Element ? e.target : null;
  if (target?.closest("input, textarea, select, [contenteditable]:not([contenteditable='false']), .config-editor")) return true;
  return !!document.querySelector("[aria-modal='true'], [role='menu']");
}

/**
 * Calls `onDelete` when the Delete key is pressed on the visible page. Pages stay mounted but
 * hidden pages run no effects, so only the page on screen listens.
 */
export function useDeleteKey(onDelete: () => void, enabled = true) {
  const handler = useRef(onDelete);
  handler.current = onDelete;
  useEffect(() => {
    if (!enabled) return;
    const onKey = (e: KeyboardEvent) => {
      if (ignoreDeleteKey(e)) return;
      e.preventDefault();
      handler.current();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [enabled]);
}
