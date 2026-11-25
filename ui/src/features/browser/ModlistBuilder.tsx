import { useEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent, type MouseEvent, type Ref } from "react";
import { useModlists, useSaveBuiltModlist } from "../../api";
import type { ModlistSummary } from "../../bindings";
import { CloseDetails } from "../../components/SplitWorkspace";
import { copyText, useContextMenu } from "../../components/ContextMenu";
import { TableHeader, useColumnLayout } from "../../components/TableHeader";
import { pointIn, usePointerDrag } from "../../components/pointerDrag";
import type { TableColumnDef } from "../../lib/tableColumns";
import { useRememberedScroll } from "../../lib/pageMemory";
import { formatCount } from "../../lib/format";
import { builder, isDirty, resumeLookups, sortedEntries, useBuilder, type BuilderEntry, type BuilderSortKey } from "./builderStore";
import { Logo } from "./ModBrowser";

type BuilderColumn = "name" | "version" | "author" | "side" | "downloads";

export const BUILDER_COLUMNS: TableColumnDef<BuilderColumn, BuilderSortKey>[] = [
  { id: "name", label: "Name", sort: "name", width: 170, min: 110, hideable: false, resizable: true },
  { id: "version", label: "Version", sort: "version", width: 76, min: 50, hideable: true, resizable: true },
  { id: "author", label: "Author", sort: "author", width: 100, min: 50, hideable: true, resizable: true },
  { id: "side", label: "Side", sort: "side", width: 60, min: 44, hideable: true, resizable: true },
  { id: "downloads", label: "Downloads", sort: "downloads", width: 84, min: 50, hideable: true, resizable: true },
];

const HEADER_HEIGHT = 36;
const ROW_HEIGHT = 46;

/** Saved modlists the builder can edit: not backups, groups, or copies that follow an online list. */
export const editableLists = (lists: ModlistSummary[] | undefined) =>
  (lists ?? [])
    .filter((l) => l.kind === "modlist" && !l.subscription && !l.error)
    .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));

/**
 * The modlist builder beside the Mod DB cards: drop cards in, drag rows out to remove them,
 * then save as a new modlist or over a saved one.
 */
export function ModlistBuilder({ paneRef, dropActive, onClose }: {
  /** The pane element, which is the drop target for cards. */
  paneRef: Ref<HTMLElement>;
  dropActive: boolean;
  onClose: () => void;
}) {
  const b = useBuilder();
  const modlists = useModlists();
  const save = useSaveBuiltModlist();
  const openMenu = useContextMenu();
  const lists = useMemo(() => editableLists(modlists.data), [modlists.data]);
  const dirty = isDirty(b);
  const [confirm, setConfirm] = useState<{ label: string; run: () => void } | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [savedName, setSavedName] = useState<string | null>(null);
  const [dragOut, setDragOut] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const ownPane = useRef<HTMLElement | null>(null);
  const anchor = useRef<string | null>(null);
  useRememberedScroll("builder-scroll", scrollRef, true);

  const table = useColumnLayout<BuilderColumn, BuilderSortKey>("builder-columns", BUILDER_COLUMNS, "name");
  const rows = useMemo(() => sortedEntries(b.entries, b.sortKey, b.sortDirection), [b.entries, b.sortKey, b.sortDirection]);
  const source = b.sourceId ? lists.find((l) => l.id === b.sourceId) : null;

  // Finish version and name lookups that a page change or restart interrupted.
  useEffect(() => resumeLookups(), []);

  // The modlist being edited was deleted or renamed elsewhere: keep the mods as a new list.
  useEffect(() => {
    if (b.sourceId && modlists.data && !modlists.isFetching && !source) builder.detach();
  }, [b.sourceId, modlists.data, modlists.isFetching, source]);

  // Forget selected rows that are no longer in the list.
  useEffect(() => {
    setSelected((sel) => {
      const ids = new Set(b.entries.map((e) => e.modId));
      const next = new Set([...sel].filter((id) => ids.has(id)));
      return next.size === sel.size ? sel : next;
    });
  }, [b.entries]);

  useEffect(() => {
    if (!savedName) return;
    const t = setTimeout(() => setSavedName(null), 4000);
    return () => clearTimeout(t);
  }, [savedName]);

  const guarded = (label: string, run: () => void) => {
    if (dirty && b.entries.length > 0) setConfirm({ label, run });
    else run();
  };

  const rowDrag = usePointerDrag<string[]>({
    onMove: (p) => setDragOut(!pointIn(ownPane.current, p)),
    onDrop: (p, ids) => {
      if (!pointIn(ownPane.current, p)) builder.remove(ids);
    },
    onEnd: () => setDragOut(false),
  });

  const setPane = (el: HTMLElement | null) => {
    ownPane.current = el;
    if (typeof paneRef === "function") paneRef(el);
    else if (paneRef) (paneRef as { current: HTMLElement | null }).current = el;
  };

  const doSave = () => {
    setSavedName(null);
    save.mutate(
      { id: b.sourceId, name: b.name, mods: b.entries.map((e) => ({ modId: e.modId, version: e.version })) },
      {
        onSuccess: (list) => {
          builder.saved(list);
          setSavedName(list.name);
        },
      },
    );
  };

  const onRowClick = (e: MouseEvent, entry: BuilderEntry) => {
    if (rowDrag.endedDrag()) return;
    const id = entry.modId;
    const multi = e.ctrlKey || e.metaKey;
    if (e.shiftKey && anchor.current) {
      const from = rows.findIndex((r) => r.modId === anchor.current);
      const to = rows.findIndex((r) => r.modId === id);
      if (from >= 0 && to >= 0) {
        const [lo, hi] = from < to ? [from, to] : [to, from];
        const range = rows.slice(lo, hi + 1).map((r) => r.modId);
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
    if ((e.key === "Delete" || e.key === "Backspace") && selected.size) {
      e.preventDefault();
      builder.remove([...selected]);
    }
    if (e.key === "Escape") setSelected(new Set());
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      e.preventDefault();
      setSelected(new Set(rows.map((r) => r.modId)));
    }
  };

  const onRowMenu = (event: MouseEvent<HTMLDivElement>, entry: BuilderEntry) => {
    const ids = selected.has(entry.modId) ? [...selected] : [entry.modId];
    if (!selected.has(entry.modId)) setSelected(new Set(ids));
    openMenu(event, `Actions for ${entry.name ?? entry.modId}`, [
      [{ label: ids.length > 1 ? `Remove ${ids.length} mods from the list` : "Remove from the list", danger: true, run: () => builder.remove(ids) }],
      [{ label: "Copy mod ID", run: () => copyText(entry.modId) }],
    ]);
  };

  const cell = (id: BuilderColumn, e: BuilderEntry) => {
    switch (id) {
      case "name":
        return (
          <div key={id} className="cell col-name" role="gridcell" title={e.modId}>
            <span className="mod-name-button">
              <Logo url={e.logoUrl} name={e.name ?? e.modId} className="mod-icon" />
              <span className="mod-name">
                <span className="mod-title">{e.name ?? e.modId}</span>
                <span className="mod-id">{e.modId}</span>
              </span>
            </span>
          </div>
        );
      case "version": {
        const looking = e.version == null && b.resolving.includes(e.modId);
        return (
          <div key={id} role="gridcell"
            className={`cell col-version${e.compatible === false ? " warn-text" : e.version ? "" : " muted"}`}
            title={e.compatible === false ? "No release is tagged for your game version; this is the newest release" : e.version ? undefined : "The newest compatible version, looked up when saving"}>
            {looking ? "…" : e.version ?? "Newest"}
          </div>
        );
      }
      case "author":
        return <div key={id} className="cell col-author" role="gridcell" title={e.author ?? undefined}>{e.author ?? ""}</div>;
      case "side":
        return <div key={id} className="cell col-side" role="gridcell">{e.side ?? ""}</div>;
      case "downloads":
        return <div key={id} className="cell col-downloads" role="gridcell">{e.downloads != null ? formatCount(e.downloads) : ""}</div>;
    }
  };

  const canSave = b.entries.length > 0 && (b.sourceId ? dirty : b.name.trim().length > 0) && !save.isPending;
  const sourceValue = b.sourceId ?? "";

  return (
    <aside ref={setPane} aria-label="Modlist builder"
      className={`details builder-pane${dropActive ? " drop-active" : ""}${dragOut ? " drag-out" : ""}`}>
      <div className="builder-head">
        <h2>Modlist builder</h2>
        <CloseDetails onClose={onClose} />
      </div>
      <div className="builder-fields">
        <select aria-label="Modlist to edit" value={sourceValue}
          onChange={(e) => {
            const next = lists.find((l) => l.id === e.target.value) ?? null;
            guarded(next ? `Discard your changes and open “${next.name}”?` : "Discard your changes and start a new list?", () => builder.load(next));
          }}>
          <option value="">New modlist</option>
          {lists.map((l) => (
            <option key={l.id} value={l.id}>{l.name}</option>
          ))}
        </select>
        {!b.sourceId && (
          <input type="text" aria-label="Name for the new modlist" placeholder="Name for the new modlist"
            value={b.name} onChange={(e) => builder.setName(e.target.value)} />
        )}
      </div>
      {confirm && (
        <div className="notice warn-text builder-confirm" role="alertdialog" aria-label="Unsaved changes">
          <span>{confirm.label}</span>
          <span className="builder-confirm-actions">
            <button type="button" className="danger" onClick={() => { confirm.run(); setConfirm(null); setSelected(new Set()); }}>Discard</button>
            <button type="button" onClick={() => setConfirm(null)}>Keep editing</button>
          </span>
        </div>
      )}
      <div className={`mod-table builder-table${table.resizing ? " resizing" : ""}`} role="grid" aria-label="Mods in the modlist"
        aria-rowcount={rows.length + 1} tabIndex={0} onKeyDown={onKeyDown}
        style={{ "--mod-icon-size": "28px", "--table-columns": table.grid.template, "--table-min-width": `${table.grid.minWidth}px` } as CSSProperties}>
        <div className="mod-scroll" ref={scrollRef}>
          <TableHeader table={table} sortKey={b.sortKey} sortDirection={b.sortDirection} onSort={builder.sort} height={HEADER_HEIGHT} />
          {rows.length === 0 ? (
            <div className="builder-empty">
              <strong>Drag mods here</strong>
              <span className="muted">Drag cards from the Mod DB into this list, or right-click a card. Drag a mod out of the list to remove it.</span>
            </div>
          ) : (
            <div className="mod-table-body">
              {rows.map((e, i) => {
                const isSelected = selected.has(e.modId);
                const dragged = rowDrag.dragging?.includes(e.modId);
                return (
                  <div key={e.modId} role="row" aria-rowindex={i + 2} aria-selected={isSelected}
                    className={`mod-row${isSelected ? " selected" : ""}${dragged ? " dragging" : ""}`}
                    style={{ height: ROW_HEIGHT }}
                    onPointerDown={(ev) => {
                      const ids = isSelected ? [...selected] : [e.modId];
                      rowDrag.begin(ev, ids, ids.length > 1 ? `${ids.length} mods` : e.name ?? e.modId);
                    }}
                    onClick={(ev) => onRowClick(ev, e)}
                    onContextMenu={(ev) => onRowMenu(ev, e)}>
                    {table.columns.map((c) => cell(c.id, e))}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>
      {save.error && <p className="notice error-text">{save.error.message}</p>}
      {savedName && !dirty && <p className="notice">Saved “{savedName}”. Load it from the Modlists page.</p>}
      <div className="builder-footer">
        <span className="muted small">
          {b.entries.length === 1 ? "1 mod" : `${b.entries.length} mods`}
          {dirty && b.entries.length > 0 ? " · unsaved changes" : ""}
        </span>
        <div className="header-spacer" />
        <button type="button" disabled={!b.entries.length && !b.sourceId && !b.name}
          onClick={() => guarded("Discard this list?", () => builder.discard())}>
          Cancel
        </button>
        <button type="button" className="primary" disabled={!canSave}
          title={!b.entries.length ? "Add some mods first" : !b.sourceId && !b.name.trim() ? "Give the new modlist a name" : undefined}
          onClick={doSave}>
          {save.isPending ? "Saving…" : b.sourceId ? "Save changes" : "Save modlist"}
        </button>
      </div>
      {rowDrag.overlay}
    </aside>
  );
}
