import { Children, useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { Icon } from "./Icon";

export function useLayoutPreference<T extends number | boolean | string>(key: string, fallback: T, previousDefault?: T) {
  const storageKey = `svsm-layout:${key}`;
  const [value, setValue] = useState<T>(() => {
    try {
      const saved: unknown = JSON.parse(localStorage.getItem(storageKey) ?? "null");
      if (saved === previousDefault) return fallback;
      if (typeof saved === typeof fallback && (typeof saved !== "number" || Number.isFinite(saved))) {
        return saved as T;
      }
    } catch {
      // Layout still works when webview storage is unavailable.
    }
    return fallback;
  });
  useEffect(() => {
    try {
      localStorage.setItem(storageKey, JSON.stringify(value));
    } catch {
      // Keep the current setting for this session.
    }
  }, [storageKey, value]);
  return [value, setValue] as const;
}

export function SplitWorkspace({ name, side = "right", widePane = false, className, children }: {
  name: string;
  className?: string;
  side?: "left" | "right";
  widePane?: boolean;
  children: [leftPane: ReactNode, rightPane: ReactNode, ...dialogs: ReactNode[]];
}) {
  const [leftPane, rightPane, ...dialogs] = children;
  const isLeft = side === "left";
  const standardWidth = isLeft ? 260 : 320;
  // Older releases saved the standard width even if the pane had never been resized.
  const defaultPreference = widePane ? 0 : standardWidth;
  const [preferredWidth, setWidth] = useLayoutPreference<number>(name, defaultPreference, widePane ? standardWidth : undefined);
  const [available, setAvailable] = useState(0);
  const host = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; width: number } | null>(null);

  useEffect(() => {
    if (!host.current) return;
    const observer = new ResizeObserver(([entry]) => setAvailable(entry.contentRect.width));
    observer.observe(host.current);
    return () => observer.disconnect();
  }, []);

  const minWidth = widePane ? 360 : (isLeft ? 180 : 260);
  const defaultWidth = widePane ? Math.max(440, Math.round(available * 0.42)) : standardWidth;
  const maxWidth = available ? Math.max(minWidth, Math.min(widePane ? 880 : 560, Math.floor(available * 0.5))) : defaultWidth;
  const clamp = (width: number) => Math.max(minWidth, Math.min(width, maxWidth));
  const width = clamp(preferredWidth || defaultWidth);
  const direction = isLeft ? 1 : -1;
  return (
    <div ref={host} className={`installed split-workspace split-${side}${widePane ? " split-wide" : ""}${className ? ` ${className}` : ""}`} style={{ "--pane-width": `${width}px` } as CSSProperties}>
      {leftPane}
      <div
        className="workspace-divider"
        role="separator"
        aria-label={isLeft ? "Resize config file list" : "Resize details pane"}
        aria-orientation="vertical"
        aria-valuemin={minWidth}
        aria-valuemax={maxWidth}
        aria-valuenow={width}
        aria-valuetext={`${width} pixels`}
        tabIndex={0}
        title="Drag to resize. Double-click to reset."
        onPointerDown={(event) => {
          if (event.button !== 0 || !event.isPrimary) return;
          event.preventDefault();
          event.currentTarget.focus();
          event.currentTarget.setPointerCapture(event.pointerId);
          drag.current = { x: event.clientX, width };
        }}
        onPointerMove={(event) => {
          if (!drag.current || !event.currentTarget.hasPointerCapture(event.pointerId)) return;
          setWidth(clamp(drag.current.width + (event.clientX - drag.current.x) * direction));
        }}
        onLostPointerCapture={() => { drag.current = null; }}
        onDoubleClick={() => setWidth(defaultPreference)}
        onKeyDown={(event) => {
          const targets: Record<string, number> = {
            ArrowLeft: width - 16 * direction,
            ArrowRight: width + 16 * direction,
            Home: minWidth,
            End: maxWidth,
          };
          const target = targets[event.key];
          if (typeof target !== "number") return;
          event.preventDefault();
          setWidth(clamp(target));
        }}
      />
      {rightPane}
      {Children.toArray(dialogs)}
    </div>
  );
}

export function CloseDetails({ onClose }: { onClose: () => void }) {
  return (
    <button type="button" className="icon-button close-details" aria-label="Close details" title="Close details" onClick={onClose}>
      <Icon name="close" />
    </button>
  );
}
