import { useEffect, useRef, useState, type CSSProperties, type MouseEvent, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { InstalledMod, Os } from "../../bindings";
import { iconUrl } from "../../lib/iconUrl";
import { canToggleMod, latestShown, modStatus, modTags, statusLabel, type DbIndex, type SortDirection, type SortKey } from "./modView";
import { formatDateTime } from "../../lib/format";
import { useLayoutPreference } from "../../components/SplitWorkspace";
import { useContextMenu } from "../../components/ContextMenu";
import { COLUMN_DEFS, DEFAULT_LAYOUT, gridFor, parseLayout, setWidth, toggleHidden, visibleColumns, type ColumnId, type ColumnLayout } from "./columns";

const HEADER_HEIGHT = 42;

export type RowSize = "small" | "medium" | "large";
/** Row height and icon size (px) for each row size. */
export const ROW_SIZES: Record<RowSize, { row: number; icon: number; label: string }> = {
  small: { row: 52, icon: 32, label: "Small" },
  medium: { row: 68, icon: 48, label: "Medium" },
  large: { row: 88, icon: 66, label: "Large" },
};


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

  const [savedLayout, setSavedLayout] = useLayoutPreference<string>("installed-columns", JSON.stringify(DEFAULT_LAYOUT));
  const layout = parseLayout(savedLayout);
  const saveLayout = (next: ColumnLayout) => setSavedLayout(JSON.stringify(next));
  // While dragging a column edge: the live width, saved on release.
  const [drag, setDrag] = useState<{ id: ColumnId; width: number } | null>(null);
  const shownLayout = drag ? setWidth(layout, drag.id, drag.width) : layout;
  const columns = visibleColumns(shownLayout);
  const grid = gridFor(shownLayout);
  const openMenu = useContextMenu();

  const onHeaderMenu = (event: MouseEvent<HTMLDivElement>) => {
    let current = layout;
    openMenu(event, "Columns", [
      COLUMN_DEFS.filter((d) => d.hideable).map((d) => ({
        label: d.label,
        checked: !layout.hidden.includes(d.id),
        keepOpen: true,
        run: () => {
          current = toggleHidden(current, d.id);
          saveLayout(current);
        },
      })),
      [{ label: "Reset columns", run: () => saveLayout(DEFAULT_LAYOUT) }],
    ]);
  };

  const startResize = (e: ReactPointerEvent<HTMLSpanElement>, id: ColumnId, width: number) => {
    e.preventDefault();
    e.stopPropagation();
    const handle = e.currentTarget;
    handle.setPointerCapture?.(e.pointerId);
    const startX = e.clientX;
    let latest = width;
    const move = (ev: PointerEvent) => {
      latest = width + ev.clientX - startX;
      setDrag({ id, width: latest });
    };
    const end = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", end);
      handle.removeEventListener("pointercancel", end);
      setDrag(null);
      if (latest !== width) saveLayout(setWidth(layout, id, latest));
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", end);
    handle.addEventListener("pointercancel", end);
  };

  const cell = (id: ColumnId, mod: InstalledMod): ReactNode => {
    switch (id) {
      case "active":
        return (
          <div key={id} className="cell col-active" role="gridcell">
            <input
              type="checkbox"
              aria-label={`${mod.isActive ? "Disable" : "Enable"} ${mod.name}`}
              checked={mod.isActive}
              disabled={props.busy || !canToggleMod(mod)}
              onClick={(e) => e.stopPropagation()}
              onChange={() => props.onToggleActive(mod)}
            />
          </div>
        );
      case "name":
        return (
          <div key={id} className="cell col-name" role="gridcell" title={mod.sourcePath}>
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
        );
      case "version":
        return <div key={id} className="cell col-version" role="gridcell">{mod.version ?? "—"}</div>;
      case "latest":
        return <LatestCell key={id} mod={mod} db={props.db} progress={props.progress.get(mod.key)} busy={props.busy} onUpdate={props.onUpdate} />;
      case "authors":
        return (
          <div key={id} className="cell col-authors" role="gridcell" title={mod.authors.join(", ")}>
            {mod.authors.join(", ")}
          </div>
        );
      case "tags":
        return <TagsCell key={id} tags={modTags(props.db, mod)} />;
      case "side":
        return <div key={id} className="cell col-side" role="gridcell">{mod.side ?? ""}</div>;
      case "modified":
        return (
          <div key={id} className="cell col-modified" role="gridcell" title={mod.modifiedMs != null ? new Date(mod.modifiedMs).toLocaleString() : undefined}>
            {formatDateTime(mod.modifiedMs)}
          </div>
        );
      case "status":
        return (
          <div key={id} className="cell col-status" role="gridcell" title={mod.error ?? mod.loadError ?? undefined}>
            <span className="mod-status">{statusLabel(mod)}</span>
          </div>
        );
    }
  };

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
    <div className={`mod-table rows-${props.rowSize}${drag ? " resizing" : ""}`} style={{ "--mod-icon-size": `${size.icon}px`, "--table-columns": grid.template, "--table-min-width": `${grid.minWidth}px` } as CSSProperties} role="grid" aria-label="Installed mods" aria-rowcount={mods.length + 1} tabIndex={0} onKeyDown={onKeyDown}>
      <div className="mod-scroll" ref={scrollRef}>
        <div className="mod-row mod-header" role="row" aria-rowindex={1} style={{ height: HEADER_HEIGHT }} onContextMenu={onHeaderMenu} title="Right-click to choose columns">
          {columns.map((c) => (
            <div key={c.id} role="columnheader" className={`cell col-${c.id}`}
              aria-sort={c.sort && props.sortKey === c.sort ? (props.sortDirection === "asc" ? "ascending" : "descending") : undefined}>
              {c.sort ? (
                <button type="button" className="sort-button" onClick={() => props.onSort(c.sort!)}>
                  {c.label}
                  {props.sortKey === c.sort && <span aria-hidden>{props.sortDirection === "asc" ? " ▲" : " ▼"}</span>}
                </button>
              ) : (
                <span className="sr-only">{c.label}</span>
              )}
              {c.resizable && (
                <span
                  className={`col-resize${drag?.id === c.id ? " dragging" : ""}`}
                  role="separator"
                  aria-orientation="vertical"
                  aria-label={`Resize ${c.label} column`}
                  title="Drag to resize, double-click to reset"
                  onPointerDown={(e) => startResize(e, c.id, c.current)}
                  onClick={(e) => e.stopPropagation()}
                  onDoubleClick={(e) => {
                    e.stopPropagation();
                    saveLayout(setWidth(layout, c.id, null));
                  }}
                />
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
                  {columns.map((c) => cell(c.id, mod))}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

function TagsCell({ tags }: { tags: string[] }) {
  const text = tags.join(", ");
  return (
    <div className="cell col-tags muted" role="gridcell" title={text || undefined}>
      {text}
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
