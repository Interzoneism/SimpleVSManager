import { useMemo } from "react";
import type { ModlistModView } from "../../bindings";
import { useOpenUrl } from "../../api";
import { copyText } from "../../components/ContextMenu";
import { useLayoutPreference } from "../../components/SplitWorkspace";
import { ModlistModsTable, RowSizePicker, useRowSize, type ModlistSortKey, type RowMenu } from "./ModlistModsTable";
import { SORT_KEYS } from "./modlistTable";

/** The list's mods as a table like Installed mods: names and pictures from the Mod DB, and where each one is. */
export function ModCards({ mods }: { mods: ModlistModView[] }) {
  const openUrl = useOpenUrl();
  const [rowSize, setRowSize] = useRowSize("modlist-mods-row-size");
  const [sort, setSort] = useLayoutPreference<string>("modlist-mods-sort", "");
  const [key, dir] = sort.split(":");
  const sortKey = SORT_KEYS.includes(key as ModlistSortKey) ? (key as ModlistSortKey) : null;
  const sortDirection = dir === "desc" ? "desc" : "asc";
  const onSort = (k: ModlistSortKey) => setSort(`${k}:${sortKey === k && sortDirection === "asc" ? "desc" : "asc"}`);
  const rows = useMemo(() => mods.map((m) => ({ modId: m.modId, version: m.version, isActive: m.isActive })), [mods]);

  const rowMenu: RowMenu = (ids, row, card) => [
    [
      !!card?.pageUrl && { label: "Open Mod DB page", run: () => openUrl.mutate(card.pageUrl!) },
      { label: ids.length > 1 ? `Copy ${ids.length} mod IDs` : "Copy mod ID", run: () => copyText(ids.length > 1 ? ids.join("\n") : row.modId) },
    ],
  ];

  return (
    <div className="details-section modlist-mods">
      <div className="modlist-mods-head">
        <h3>Mods ({mods.length})</h3>
        <RowSizePicker value={rowSize} onChange={setRowSize} />
      </div>
      {openUrl.error && <p className="notice error-text">{openUrl.error.message}</p>}
      <ModlistModsTable rows={rows} label="Mods in this modlist" layoutKey="modlist-mods-columns" rowSize={rowSize}
        sortKey={sortKey} sortDirection={sortDirection} onSort={onSort} rowMenu={rowMenu}
        empty={<p className="muted small modlist-mods-empty">This list has no mods.</p>} />
    </div>
  );
}
