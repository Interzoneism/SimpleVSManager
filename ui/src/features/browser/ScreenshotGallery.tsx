import { useEffect } from "react";
import { createPortal } from "react-dom";
import type { ModDbScreenshot } from "../../bindings";

/** A screenshot shown full size over everything; arrows flip through the rest. */
export function Lightbox({ screenshots, index, onIndex, onClose, modName }: {
  screenshots: ModDbScreenshot[];
  index: number;
  onIndex: (i: number) => void;
  onClose: () => void;
  modName: string;
}) {
  const count = screenshots.length;
  const go = (step: number) => onIndex((index + step + count) % count);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      else if (e.key === "ArrowRight") go(1);
      else if (e.key === "ArrowLeft") go(-1);
      else return;
      e.preventDefault();
      e.stopPropagation();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });
  const shot = screenshots[index];

  return createPortal(
    <div className="lightbox" role="dialog" aria-modal="true" aria-label={`${modName} screenshot ${index + 1} of ${count}`} onClick={onClose}>
      <img src={shot.url} alt={shot.fileName ?? `Screenshot ${index + 1}`} onClick={(e) => e.stopPropagation()} />
      <div className="lightbox-bar" onClick={(e) => e.stopPropagation()}>
        {count > 1 && <button type="button" onClick={() => go(-1)} aria-label="Previous screenshot">‹</button>}
        <span>{index + 1} / {count}</span>
        {count > 1 && <button type="button" onClick={() => go(1)} aria-label="Next screenshot">›</button>}
        <button type="button" onClick={onClose} aria-label="Close">✕</button>
      </div>
    </div>,
    document.body,
  );
}
