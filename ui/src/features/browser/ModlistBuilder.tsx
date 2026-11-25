import { useEffect, useMemo, useRef, useState, type Ref } from "react";
import { useModlists, useOpenUrl, useSaveBuiltModlist } from "../../api";
import type { ModlistSummary } from "../../bindings";
import { CloseDetails } from "../../components/SplitWorkspace";
import { copyText } from "../../components/ContextMenu";
import { pointIn, usePointerDrag } from "../../components/pointerDrag";
import { useRememberedScroll } from "../../lib/pageMemory";
import { builder, isDirty, resumeLookups, useBuilder } from "./builderStore";
import { Icon } from "../../components/Icon";
import { ModlistModsTable, RowSizePicker, useRowSize, type RowMenu } from "../modlists/ModlistModsTable";

/** Opens and closes the builder. Sits at the top right of the page, above the panel. */
export function BuilderToggle() {
  const b = useBuilder();
  return (
    <button
      type="button"
      className={`builder-toggle${b.open ? " active" : ""}`}
      aria-pressed={b.open}
      title={b.open ? "Hide the modlist builder (your list is kept)" : "Build a modlist by dragging mods into it"}
      onClick={() => builder.setOpen(!b.open)}
    >
      <Icon name="lists" />
      Modlist builder{b.entries.length > 0 && <span className="count-badge">{b.entries.length}</span>}
    </button>
  );
}

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
  const openUrl = useOpenUrl();
  const [rowSize, setRowSize] = useRowSize("builder-row-size");
  const lists = useMemo(() => editableLists(modlists.data), [modlists.data]);
  const dirty = isDirty(b);
  const [confirm, setConfirm] = useState<{ label: string; run: () => void } | null>(null);
  const [savedName, setSavedName] = useState<string | null>(null);
  const [dragOut, setDragOut] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const ownPane = useRef<HTMLElement | null>(null);
  useRememberedScroll("builder-scroll", scrollRef, true);

  const source = b.sourceId ? lists.find((l) => l.id === b.sourceId) : null;

  // Finish version and name lookups that a page change or restart interrupted.
  useEffect(() => resumeLookups(), []);

  // The modlist being edited was deleted or renamed elsewhere: keep the mods as a new list.
  useEffect(() => {
    if (b.sourceId && modlists.data && !modlists.isFetching && !source) builder.detach();
  }, [b.sourceId, modlists.data, modlists.isFetching, source]);

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

  const rowMenu: RowMenu = (ids, row, card) => [
    [{ label: ids.length > 1 ? `Remove ${ids.length} mods from the list` : "Remove from the list", danger: true, run: () => builder.remove(ids) }],
    [
      !!card?.pageUrl && { label: "Open Mod DB page", run: () => openUrl.mutate(card.pageUrl!) },
      { label: "Copy mod ID", run: () => copyText(row.modId) },
    ],
  ];

  const canSave = b.entries.length > 0 && (b.sourceId ? dirty : b.name.trim().length > 0) && !save.isPending;
  const sourceValue = b.sourceId ?? "";

  return (
    <aside ref={setPane} aria-label="Modlist builder"
      className={`details builder-pane${dropActive ? " drop-active" : ""}${dragOut ? " drag-out" : ""}`}>
      <div className="builder-head">
        <h2>Modlist builder</h2>
        <span className="builder-head-actions">
          <RowSizePicker value={rowSize} onChange={setRowSize} />
          <CloseDetails onClose={onClose} />
        </span>
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
            <button type="button" className="danger" onClick={() => { confirm.run(); setConfirm(null); }}>Discard</button>
            <button type="button" onClick={() => setConfirm(null)}>Keep editing</button>
          </span>
        </div>
      )}
      <ModlistModsTable rows={b.entries} label="Mods in the modlist" layoutKey="builder-columns" rowSize={rowSize}
        className="builder-table" sortKey={b.sortKey} sortDirection={b.sortDirection} onSort={builder.sort}
        rowMenu={rowMenu} resolving={b.resolving} onDelete={builder.remove} scrollRef={scrollRef}
        onRowPointerDown={rowDrag.begin} endedDrag={rowDrag.endedDrag} draggedIds={rowDrag.dragging}
        empty={
          <div className="builder-empty">
            <strong>Drag mods here</strong>
            <span className="muted">Drag cards from the Mod DB into this list, or right-click a card. Drag a mod out of the list to remove it.</span>
          </div>
        } />
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
