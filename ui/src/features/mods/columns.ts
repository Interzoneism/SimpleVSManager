// Installed mods table columns: which are shown and how wide, saved between sessions.
import type { SortKey } from "./modView";
import {
  clampColumnWidth,
  parseTableLayout,
  setTableColumnWidth,
  tableGrid,
  toggleTableColumn,
  visibleTableColumns,
  type TableColumnDef,
  type TableLayout,
} from "../../lib/tableColumns";

export type ColumnId = "active" | "name" | "version" | "latest" | "authors" | "tags" | "side" | "modified" | "status";
export type ColumnDef = TableColumnDef<ColumnId, SortKey>;

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

export type ColumnLayout = TableLayout<ColumnId>;

export const DEFAULT_LAYOUT: ColumnLayout = { widths: {}, hidden: [] };

export const parseLayout = (text: string): ColumnLayout => parseTableLayout(COLUMN_DEFS, text);
export const clampWidth = clampColumnWidth;
export const visibleColumns = (layout: ColumnLayout) => visibleTableColumns(COLUMN_DEFS, layout);
export const gridFor = (layout: ColumnLayout) => tableGrid(COLUMN_DEFS, layout, "name");
export const toggleHidden = toggleTableColumn<ColumnId>;
export const setWidth = (layout: ColumnLayout, id: ColumnId, width: number | null) => setTableColumnWidth(COLUMN_DEFS, layout, id, width);
