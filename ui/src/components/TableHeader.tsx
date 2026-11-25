import { useState, type MouseEvent, type PointerEvent as ReactPointerEvent } from "react";
import { useLayoutPreference } from "./SplitWorkspace";
import { useContextMenu } from "./ContextMenu";
import {
  parseTableLayout,
  setTableColumnWidth,
  tableGrid,
  toggleTableColumn,
  visibleTableColumns,
  type TableColumnDef,
  type TableLayout,
} from "../lib/tableColumns";

/**
 * Column layout of a table (widths, hidden columns), saved under `prefKey`, with the header's
 * right-click column menu and drag-to-resize.
 */
export function useColumnLayout<Id extends string, Sort extends string>(prefKey: string, defs: TableColumnDef<Id, Sort>[], flex: Id) {
  const empty: TableLayout<Id> = { widths: {}, hidden: [] };
  const [saved, setSaved] = useLayoutPreference<string>(prefKey, JSON.stringify(empty));
  const layout = parseTableLayout(defs, saved);
  const save = (next: TableLayout<Id>) => setSaved(JSON.stringify(next));
  // While dragging a column edge: the live width, saved on release.
  const [drag, setDrag] = useState<{ id: Id; width: number } | null>(null);
  const shown = drag ? setTableColumnWidth(defs, layout, drag.id, drag.width) : layout;
  const openMenu = useContextMenu();

  const onHeaderMenu = (event: MouseEvent<HTMLElement>) => {
    let current = layout;
    openMenu(event, "Columns", [
      defs.filter((d) => d.hideable).map((d) => ({
        label: d.label,
        checked: !layout.hidden.includes(d.id),
        keepOpen: true,
        run: () => {
          current = toggleTableColumn(current, d.id);
          save(current);
        },
      })),
      [{ label: "Reset columns", run: () => save(empty) }],
    ]);
  };

  const startResize = (e: ReactPointerEvent<HTMLSpanElement>, id: Id, width: number) => {
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
      if (latest !== width) save(setTableColumnWidth(defs, layout, id, latest));
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", end);
    handle.addEventListener("pointercancel", end);
  };

  return {
    columns: visibleTableColumns(defs, shown),
    grid: tableGrid(defs, shown, flex),
    resizing: drag?.id ?? null,
    onHeaderMenu,
    startResize,
    resetWidth: (id: Id) => save(setTableColumnWidth(defs, layout, id, null)),
  };
}

/** The sticky header row: sort buttons, resize handles, right-click to pick columns. */
export function TableHeader<Id extends string, Sort extends string>({ table, sortKey, sortDirection, onSort, height }: {
  table: ReturnType<typeof useColumnLayout<Id, Sort>>;
  sortKey: Sort | null;
  sortDirection: "asc" | "desc";
  onSort: (key: Sort) => void;
  height: number;
}) {
  return (
    <div className="mod-row mod-header" role="row" aria-rowindex={1} style={{ height }} onContextMenu={table.onHeaderMenu} title="Right-click to choose columns">
      {table.columns.map((c) => (
        <div key={c.id} role="columnheader" className={`cell col-${c.id}`}
          aria-sort={c.sort && sortKey === c.sort ? (sortDirection === "asc" ? "ascending" : "descending") : undefined}>
          {c.sort ? (
            <button type="button" className="sort-button" onClick={() => onSort(c.sort!)}>
              {c.label}
              {sortKey === c.sort && <span aria-hidden>{sortDirection === "asc" ? " ▲" : " ▼"}</span>}
            </button>
          ) : (
            <span className="sr-only">{c.label}</span>
          )}
          {c.resizable && (
            <span
              className={`col-resize${table.resizing === c.id ? " dragging" : ""}`}
              role="separator"
              aria-orientation="vertical"
              aria-label={`Resize ${c.label} column`}
              title="Drag to resize, double-click to reset"
              onPointerDown={(e) => table.startResize(e, c.id, c.current)}
              onClick={(e) => e.stopPropagation()}
              onDoubleClick={(e) => {
                e.stopPropagation();
                table.resetWidth(c.id);
              }}
            />
          )}
        </div>
      ))}
    </div>
  );
}
