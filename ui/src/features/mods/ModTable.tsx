import { useEffect, useRef, type CSSProperties, type MouseEvent, type ReactNode } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { InstalledMod, Os } from "../../bindings";
import { iconUrl } from "../../lib/iconUrl";
import { canToggleMod, latestShown, modStatus, statusLabel, type DbIndex, type SortDirection, type SortKey } from "./modView";

const HEADER_HEIGHT = 42;

export type RowSize = "small" | "medium" | "large";
/** Row height and icon size (px) for each row size. */
export const ROW_SIZES: Record<RowSize, { row: number; icon: number; label: string }> = {
  small: { row: 52, icon: 32, label: "Small" },
  medium: { row: 68, icon: 48, label: "Medium" },
  large: { row: 88, icon: 66, label: "Large" },
};

const COLUMNS: { key: SortKey | null; label: string; className: string }[] = [
  { key: null, label: "", className: "col-active" },
  { key: "name", label: "Name", className: "col-name" },
  { key: "version", label: "Version", className: "col-version" },
  { key: "latest", label: "Latest", className: "col-latest" },
  { key: "authors", label: "Authors", className: "col-authors" },
  { key: "side", label: "Side", className: "col-side" },
  { key: "status", label: "Status", className: "col-status" },
];

interface Props {
  os: Os;
  mods: InstalledMod[];
  db: DbIndex;
  /** Installed mod keys currently being updated, with a short progress label. */
  progress: Map<string, string>;
  onUpdate: (mod: InstalledMod) => void;
  selected: Set<string>;
  onSelectionChange: (next: Set<string>) => void;
  sortKey: SortKey;
  sortDirection: SortDirection;
  onSort: (key: SortKey) => void;
  onToggleActive: (mod: InstalledMod) => void;
  onRowMenu: (event: MouseEvent<HTMLDivElement>, mod: InstalledMod) => void;
  busy: boolean;
  empty: ReactNode;
  rowSize: RowSize;
}

/** Virtualized table: only visible rows are in the DOM, which keeps WebKitGTK fast. */
export function ModTable(props: Props) {
  const { mods, selected, onSelectionChange } = props;
  const scrollRef = useRef<HTMLDivElement>(null);
  const anchor = useRef<string | null>(null);
  const size = ROW_SIZES[props.rowSize] ?? ROW_SIZES.medium;
  const rowHeight = size.row;

  const virtualizer = useVirtualizer({
    count: mods.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    scrollMargin: HEADER_HEIGHT,
    overscan: 12,
  });

  useEffect(() => virtualizer.measure(), [rowHeight, virtualizer]);

  const onRowClick = (e: React.MouseEvent, mod: InstalledMod) => {
    const multi = e.ctrlKey || e.metaKey;
    if (e.shiftKey && anchor.current) {
      const from = mods.findIndex((m) => m.key === anchor.current);
      const to = mods.findIndex((m) => m.key === mod.key);
      if (from >= 0 && to >= 0) {
        const [a, b] = from < to ? [from, to] : [to, from];
        const range = mods.slice(a, b + 1).map((m) => m.key);
        onSelectionChange(new Set(multi ? [...selected, ...range] : range));
        return;
      }
    }
    anchor.current = mod.key;
    if (multi) {
      const next = new Set(selected);
      if (next.has(mod.key)) next.delete(mod.key);
      else next.add(mod.key);
      onSelectionChange(next);
    } else {
      onSelectionChange(new Set([mod.key]));
    }
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") onSelectionChange(new Set());
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      e.preventDefault();
      onSelectionChange(new Set(mods.map((m) => m.key)));
    }
  };

  return (
    <div className={`mod-table rows-${props.rowSize}`} style={{ "--mod-icon-size": `${size.icon}px` } as CSSProperties} role="grid" aria-label="Installed mods" aria-rowcount={mods.length + 1} tabIndex={0} onKeyDown={onKeyDown}>
      <div className="mod-scroll" ref={scrollRef}>
        <div className="mod-row mod-header" role="row" aria-rowindex={1} style={{ height: HEADER_HEIGHT }}>
          {COLUMNS.map((c) => (
            <div key={c.label || "active"} role="columnheader" className={`cell ${c.className}`}
              aria-sort={props.sortKey === c.key ? (props.sortDirection === "asc" ? "ascending" : "descending") : undefined}>
              {c.key ? (
                <button type="button" className="sort-button" onClick={() => props.onSort(c.key!)}>
                  {c.label}
                  {props.sortKey === c.key && <span aria-hidden>{props.sortDirection === "asc" ? " ▲" : " ▼"}</span>}
                </button>
              ) : (
                <span className="sr-only">Active</span>
              )}
            </div>
          ))}
        </div>
        {mods.length === 0 ? (
          props.empty
        ) : (
          <div className="mod-table-body" style={{ height: virtualizer.getTotalSize() }}>
            {virtualizer.getVirtualItems().map((item) => {
              const mod = mods[item.index];
              const status = modStatus(mod);
              const isSelected = selected.has(mod.key);
              return (
                <div
                  key={mod.key}
                  role="row"
                  aria-rowindex={item.index + 2}
                  aria-selected={isSelected}
                  className={`mod-row status-${status}${isSelected ? " selected" : ""}`}
                  style={{ position: "absolute", top: 0, left: 0, right: 0, height: rowHeight, transform: `translateY(${item.start - HEADER_HEIGHT}px)` }}
                  onClick={(e) => onRowClick(e, mod)}
                  onContextMenu={(e) => {
                    if (!isSelected) {
                      anchor.current = mod.key;
                      onSelectionChange(new Set([mod.key]));
                    }
                    props.onRowMenu(e, mod);
                  }}
                >
                  <div className="cell col-active" role="gridcell">
                    <input
                      type="checkbox"
                      aria-label={`${mod.isActive ? "Disable" : "Enable"} ${mod.name}`}
                      checked={mod.isActive}
                      disabled={props.busy || !canToggleMod(mod)}
                      onClick={(e) => e.stopPropagation()}
                      onChange={() => props.onToggleActive(mod)}
                    />
                  </div>
                  <div className="cell col-name" role="gridcell" title={mod.sourcePath}>
                    <button type="button" className="mod-name-button" aria-haspopup="menu">
                      {mod.iconKey ? (
                        <img className="mod-icon" src={iconUrl(props.os, mod.iconKey)} alt="" loading="lazy" />
                      ) : (
                        <span className="mod-icon placeholder" aria-hidden>
                          {mod.name.slice(0, 1).toUpperCase()}
                        </span>
                      )}
                      <span className="mod-name">
                        <span className="mod-title">{mod.name}</span>
                        <span className="mod-id">{mod.modId}</span>
                      </span>
                    </button>
                  </div>
                  <div className="cell col-version" role="gridcell">{mod.version ?? "—"}</div>
                  <LatestCell
                    mod={mod}
                    db={props.db}
                    progress={props.progress.get(mod.key)}
                    busy={props.busy}
                    onUpdate={props.onUpdate}
                  />
                  <div className="cell col-authors" role="gridcell" title={mod.authors.join(", ")}>
                    {mod.authors.join(", ")}
                  </div>
                  <div className="cell col-side" role="gridcell">{mod.side ?? ""}</div>
                  <div className="cell col-status" role="gridcell" title={mod.error ?? mod.loadError ?? undefined}>
                    <span className="mod-status">{statusLabel(mod)}</span>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

function LatestCell(props: {
  mod: InstalledMod;
  db: DbIndex;
  progress: string | undefined;
  busy: boolean;
  onUpdate: (mod: InstalledMod) => void;
}) {
  const s = props.db.get(props.mod.key);
  if (props.progress) {
    return (
      <div className="cell col-latest" role="gridcell">
        <span className="muted">{props.progress}</span>
      </div>
    );
  }
  if (s?.update) {
    return (
      <div className="cell col-latest" role="gridcell">
        <button
          type="button"
          className={`update-pill${s.update.compatible ? "" : " warn"}`}
          title={`${s.update.message}. Click to update.`}
          disabled={props.busy}
          onClick={(e) => {
            e.stopPropagation();
            props.onUpdate(props.mod);
          }}
        >
          ↑ {s.update.version}
        </button>
      </div>
    );
  }
  const shown = latestShown(s);
  return (
    <div
      className={`cell col-latest${s?.installedMayBeIncompatible ? " warn-text" : " muted"}`}
      role="gridcell"
      title={
        s?.installedMayBeIncompatible
          ? `The installed version is tagged for ${s.installedReleaseGameVersions.join(", ")}`
          : s?.skippedVersion
            ? `Skipping ${s.skippedVersion}`
            : undefined
      }
    >
      {shown ?? ""}
      {s?.skippedVersion && <span className="small"> (skipped)</span>}
    </div>
  );
}
