import { useMemo, useRef, useState, type CSSProperties, type KeyboardEvent, type MouseEvent, type PointerEvent as ReactPointerEvent, type ReactNode, type RefObject } from "react";
import type { ModlistModCard } from "../../bindings";
import { useModlistModCards } from "../../api";
import { useContextMenu } from "../../components/ContextMenu";
import { useLayoutPreference } from "../../components/SplitWorkspace";
import { TableHeader, useColumnLayout } from "../../components/TableHeader";
import type { TableColumnDef } from "../../lib/tableColumns";
import { ROW_SIZES, type RowSize } from "../mods/ModTable";
import { Logo } from "../browser/ModBrowser";
import { libraryStatus, sortRows, type ModlistSortKey, type ModlistTableRow } from "./modlistTable";

export type { ModlistSortKey, ModlistTableRow } from "./modlistTable";
type ColumnId = ModlistSortKey;

export const MODLIST_COLUMNS: TableColumnDef<ColumnId, ModlistSortKey>[] = [
  { id: "name", label: "Name", sort: "name", width: 170, min: 110, hideable: false, resizable: true },
  { id: "version", label: "Version", sort: "version", width: 76, min: 50, hideable: true, resizable: true },
  { id: "library", label: "Library", sort: "library", width: 112, min: 60, hideable: true, resizable: true },
  { id: "author", label: "Author", sort: "author", width: 100, min: 50, hideable: true, resizable: true },
  { id: "side", label: "Side", sort: "side", width: 60, min: 44, hideable: true, resizable: true },
];

const HEADER_HEIGHT = 36;

/** Small / Medium / Large buttons, like on Installed mods. */
export function RowSizePicker({ value, onChange }: { value: RowSize; onChange: (s: RowSize) => void }) {
  return (
    <div className="segmented row-size" role="group" aria-label="Row size">
      {(Object.keys(ROW_SIZES) as RowSize[]).map((key) => (
        <button key={key} type="button" className="small" title={`${ROW_SIZES[key].label} rows`} aria-pressed={value === key} onClick={() => onChange(key)}>
          {ROW_SIZES[key].label[0]}
        </button>
      ))}
    </div>
  );
}

/** Row size of a modlist table, saved under `key`. */
export const useRowSize = (key: string) => useLayoutPreference<RowSize>(key, "small");

export type RowMenu = (ids: string[], row: ModlistTableRow, card: ModlistModCard | undefined) => Parameters<ReturnType<typeof useContextMenu>>[2];

/**
 * A modlist's mods as a table like Installed mods: sortable, resizable columns you can pick by
 * right-clicking the header, row sizes, selection and a right-click menu on rows.
 */
export function ModlistModsTable<R extends ModlistTableRow>(props: {
  rows: R[];
  label: string;
  /** Saves the column layout under this name. */
  layoutKey: string;
  rowSize: RowSize;
  sortKey: ModlistSortKey | null;
  sortDirection: "asc" | "desc";
  onSort: (key: ModlistSortKey) => void;
  rowMenu: RowMenu;
  /** Mod ids whose version is being looked up. */
  resolving?: string[];
  /** Delete / Backspace on selected rows. */
  onDelete?: (ids: string[]) => void;
  /** Start of a possible row drag (builder only). */
  onRowPointerDown?: (event: ReactPointerEvent, ids: string[], label: string) => void;
  /** True when this click ended a drag. */
  endedDrag?: () => boolean;
  draggedIds?: string[] | null;
  scrollRef?: RefObject<HTMLDivElement | null>;
  empty?: ReactNode;
  className?: string;
}) {
  const { rows } = props;
  const ids = useMemo(() => rows.map((r) => r.modId), [rows]);
  const cards = useModlistModCards(ids, true);
  const byId = useMemo(() => new Map((cards.data ?? []).map((c) => [c.modId.toLowerCase(), c])), [cards.data]);
  const table = useColumnLayout<ColumnId, ModlistSortKey>(props.layoutKey, MODLIST_COLUMNS, "name");
  const sorted = useMemo(() => sortRows(rows, props.sortKey, props.sortDirection, byId), [rows, props.sortKey, props.sortDirection, byId]);
  const [selection, setSelected] = useState<Set<string>>(new Set());
  // Rows that left the list are no longer selected.
  const selected = useMemo(() => new Set(ids.filter((id) => selection.has(id))), [ids, selection]);
  const anchor = useRef<string | null>(null);
  const openMenu = useContextMenu();
  const size = ROW_SIZES[props.rowSize] ?? ROW_SIZES.small;

  const onRowClick = (e: MouseEvent, id: string) => {
    if (props.endedDrag?.()) return;
    const multi = e.ctrlKey || e.metaKey;
    if (e.shiftKey && anchor.current) {
      const from = sorted.findIndex((r) => r.modId === anchor.current);
      const to = sorted.findIndex((r) => r.modId === id);
      if (from >= 0 && to >= 0) {
        const [lo, hi] = from < to ? [from, to] : [to, from];
        const range = sorted.slice(lo, hi + 1).map((r) => r.modId);
        setSelected(new Set(multi ? [...selected, ...range] : range));
        return;
      }
    }
    anchor.current = id;
    if (multi) {
      const next = new Set(selected);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      setSelected(next);
    } else {
      setSelected(new Set([id]));
    }
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (props.onDelete && (e.key === "Delete" || e.key === "Backspace") && selected.size) {
      e.preventDefault();
      props.onDelete([...selected]);
    }
    if (e.key === "Escape") setSelected(new Set());
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      e.preventDefault();
      setSelected(new Set(ids));
    }
  };

  const onRowMenu = (event: MouseEvent<HTMLDivElement>, row: R) => {
    const menuIds = selected.has(row.modId) ? [...selected] : [row.modId];
    if (!selected.has(row.modId)) {
      anchor.current = row.modId;
      setSelected(new Set(menuIds));
    }
    const card = byId.get(row.modId.toLowerCase());
    openMenu(event, `Actions for ${row.name ?? card?.name ?? row.modId}`, props.rowMenu(menuIds, row, card));
  };

  const cell = (id: ColumnId, r: R, card: ModlistModCard | undefined) => {
    const name = r.name ?? card?.name ?? r.modId;
    switch (id) {
      case "name":
        return (
          <div key={id} className="cell col-name" role="gridcell" title={r.modId}>
            <span className="mod-name-button">
              <Logo url={r.logoUrl ?? card?.logoUrl ?? null} name={name} className="mod-icon" />
              <span className="mod-name">
                <span className="mod-title">{name}</span>
                <span className="mod-id">{r.modId}</span>
              </span>
            </span>
          </div>
        );
      case "version": {
        const looking = r.version == null && props.resolving?.includes(r.modId);
        return (
          <div key={id} role="gridcell"
            className={`cell col-version${r.compatible === false ? " warn-text" : r.version ? "" : " muted"}`}
            title={r.compatible === false ? "No release is tagged for your game version; this is the newest release" : r.version ? undefined : "The newest compatible version"}>
            {looking ? "…" : r.version ?? "Newest"}
          </div>
        );
      }
      case "library": {
        const s = libraryStatus(r.version, card);
        return (
          <div key={id} role="gridcell" className={`cell col-library${s && !s.have ? " muted" : ""}`} title={s?.detail}>
            {s?.label ?? (cards.isFetching ? "…" : "")}
          </div>
        );
      }
      case "author":
      case "side": {
        const v = r[id] ?? card?.[id] ?? "";
        return <div key={id} className={`cell col-${id}`} role="gridcell" title={v || undefined}>{v}</div>;
      }
    }
  };

  return (
    <div className={`mod-table modlist-table rows-${props.rowSize}${table.resizing ? " resizing" : ""}${props.className ? ` ${props.className}` : ""}`}
      role="grid" aria-label={props.label} aria-rowcount={rows.length + 1} tabIndex={0} onKeyDown={onKeyDown}
      style={{ "--mod-icon-size": `${size.icon}px`, "--table-columns": table.grid.template, "--table-min-width": `${table.grid.minWidth}px` } as CSSProperties}>
      <div className="mod-scroll" ref={props.scrollRef}>
        <TableHeader table={table} sortKey={props.sortKey} sortDirection={props.sortDirection} onSort={props.onSort} height={HEADER_HEIGHT} />
        {rows.length === 0 ? (
          props.empty
        ) : (
          <div className="mod-table-body">
            {sorted.map((r, i) => {
              const isSelected = selected.has(r.modId);
              const card = byId.get(r.modId.toLowerCase());
              return (
                <div key={r.modId} role="row" aria-rowindex={i + 2} aria-selected={isSelected}
                  className={`mod-row${isSelected ? " selected" : ""}${r.isActive === false ? " status-disabled" : ""}${props.draggedIds?.includes(r.modId) ? " dragging" : ""}`}
                  style={{ height: size.row }}
                  onPointerDown={props.onRowPointerDown && ((ev) => {
                    const dragIds = isSelected ? [...selected] : [r.modId];
                    props.onRowPointerDown!(ev, dragIds, dragIds.length > 1 ? `${dragIds.length} mods` : r.name ?? card?.name ?? r.modId);
                  })}
                  onClick={(ev) => onRowClick(ev, r.modId)}
                  onContextMenu={(ev) => onRowMenu(ev, r)}>
                  {table.columns.map((c) => cell(c.id, r, card))}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
