// Installed mods table columns: which are shown and how wide, saved between sessions.
import type { SortKey } from "./modView";

export type ColumnId = "active" | "name" | "version" | "latest" | "authors" | "tags" | "side" | "modified" | "status";

export interface ColumnDef {
  id: ColumnId;
  label: string;
  sort: SortKey | null;
  width: number;
  min: number;
  /** Can be hidden from the header menu. */
  hideable: boolean;
  /** Can be dragged wider or narrower. */
  resizable: boolean;
}

export const COLUMN_DEFS: ColumnDef[] = [
  { id: "active", label: "Active", sort: null, width: 38, min: 38, hideable: false, resizable: false },
  { id: "name", label: "Name", sort: "name", width: 220, min: 120, hideable: false, resizable: true },
  { id: "version", label: "Version", sort: "version", width: 80, min: 50, hideable: true, resizable: true },
  { id: "latest", label: "Latest", sort: "latest", width: 92, min: 50, hideable: true, resizable: true },
  { id: "authors", label: "Authors", sort: "authors", width: 130, min: 50, hideable: true, resizable: true },
  { id: "tags", label: "Tags", sort: "tags", width: 120, min: 50, hideable: true, resizable: true },
  { id: "side", label: "Side", sort: "side", width: 70, min: 44, hideable: true, resizable: true },
  { id: "modified", label: "Modified", sort: "modified", width: 150, min: 60, hideable: true, resizable: true },
  { id: "status", label: "Status", sort: "status", width: 120, min: 60, hideable: true, resizable: true },
];

export interface ColumnLayout {
  widths: Partial<Record<ColumnId, number>>;
  hidden: ColumnId[];
}

export const DEFAULT_LAYOUT: ColumnLayout = { widths: {}, hidden: [] };

/** Reads a saved layout, dropping anything unknown or out of range. */
export function parseLayout(text: string): ColumnLayout {
  try {
    const raw = JSON.parse(text) as Partial<ColumnLayout>;
    const widths: ColumnLayout["widths"] = {};
    for (const def of COLUMN_DEFS) {
      const w = raw.widths?.[def.id];
      if (def.resizable && typeof w === "number" && Number.isFinite(w)) widths[def.id] = clampWidth(def, w);
    }
    const hidden = Array.isArray(raw.hidden)
      ? COLUMN_DEFS.filter((d) => d.hideable && raw.hidden!.includes(d.id)).map((d) => d.id)
      : [];
    return { widths, hidden };
  } catch {
    return DEFAULT_LAYOUT;
  }
}

export const clampWidth = (def: ColumnDef, w: number) => Math.round(Math.min(800, Math.max(def.min, w)));

export function visibleColumns(layout: ColumnLayout): (ColumnDef & { current: number })[] {
  return COLUMN_DEFS.filter((d) => !layout.hidden.includes(d.id)).map((d) => ({ ...d, current: layout.widths[d.id] ?? d.width }));
}

/** CSS grid columns and the table's minimum width. The name column takes any spare room. */
export function gridFor(layout: ColumnLayout): { template: string; minWidth: number } {
  const cols = visibleColumns(layout);
  return {
    template: cols.map((c) => (c.id === "name" ? `minmax(${c.current}px, 1fr)` : `${c.current}px`)).join(" "),
    minWidth: cols.reduce((sum, c) => sum + c.current, 0),
  };
}

export function toggleHidden(layout: ColumnLayout, id: ColumnId): ColumnLayout {
  const hidden = layout.hidden.includes(id) ? layout.hidden.filter((h) => h !== id) : [...layout.hidden, id];
  return { ...layout, hidden };
}

export function setWidth(layout: ColumnLayout, id: ColumnId, width: number | null): ColumnLayout {
  const def = COLUMN_DEFS.find((d) => d.id === id)!;
  const widths = { ...layout.widths };
  if (width === null) delete widths[id];
  else widths[id] = clampWidth(def, width);
  return { ...layout, widths };
}
