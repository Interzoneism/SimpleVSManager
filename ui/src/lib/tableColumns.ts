// Table columns that can be sorted, resized and hidden, with the layout saved between sessions.
// Shared by the Installed mods table and the modlist builder.

export interface TableColumnDef<Id extends string, Sort extends string> {
  id: Id;
  label: string;
  sort: Sort | null;
  width: number;
  min: number;
  /** Can be hidden from the header menu. */
  hideable: boolean;
  /** Can be dragged wider or narrower. */
  resizable: boolean;
}

export interface TableLayout<Id extends string> {
  widths: Partial<Record<Id, number>>;
  hidden: Id[];
}

export const clampColumnWidth = (def: { min: number }, w: number) => Math.round(Math.min(800, Math.max(def.min, w)));

/** Reads a saved layout, dropping anything unknown or out of range. */
export function parseTableLayout<Id extends string>(defs: TableColumnDef<Id, string>[], text: string): TableLayout<Id> {
  try {
    const raw = JSON.parse(text) as Partial<TableLayout<Id>>;
    const widths: TableLayout<Id>["widths"] = {};
    for (const def of defs) {
      const w = raw.widths?.[def.id];
      if (def.resizable && typeof w === "number" && Number.isFinite(w)) widths[def.id] = clampColumnWidth(def, w);
    }
    const hidden = Array.isArray(raw.hidden) ? defs.filter((d) => d.hideable && raw.hidden!.includes(d.id)).map((d) => d.id) : [];
    return { widths, hidden };
  } catch {
    return { widths: {}, hidden: [] };
  }
}

export function visibleTableColumns<Id extends string, Sort extends string>(defs: TableColumnDef<Id, Sort>[], layout: TableLayout<Id>) {
  return defs.filter((d) => !layout.hidden.includes(d.id)).map((d) => ({ ...d, current: layout.widths[d.id] ?? d.width }));
}

/** CSS grid columns and the table's minimum width. The `flex` column takes any spare room. */
export function tableGrid<Id extends string>(defs: TableColumnDef<Id, string>[], layout: TableLayout<Id>, flex: Id) {
  const cols = visibleTableColumns(defs, layout);
  return {
    template: cols.map((c) => (c.id === flex ? `minmax(${c.current}px, 1fr)` : `${c.current}px`)).join(" "),
    minWidth: cols.reduce((sum, c) => sum + c.current, 0),
  };
}

export function toggleTableColumn<Id extends string>(layout: TableLayout<Id>, id: Id): TableLayout<Id> {
  const hidden = layout.hidden.includes(id) ? layout.hidden.filter((h) => h !== id) : [...layout.hidden, id];
  return { ...layout, hidden };
}

export function setTableColumnWidth<Id extends string>(defs: TableColumnDef<Id, string>[], layout: TableLayout<Id>, id: Id, width: number | null): TableLayout<Id> {
  const def = defs.find((d) => d.id === id)!;
  const widths = { ...layout.widths };
  if (width === null) delete widths[id];
  else widths[id] = clampColumnWidth(def, width);
  return { ...layout, widths };
}
