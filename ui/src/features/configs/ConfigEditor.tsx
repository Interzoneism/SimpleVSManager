import { createContext, useContext, useEffect, useId, useMemo, useRef, useState, type ReactNode } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { useModConfig, useModConfigs, useOpenKnownFolder, useSaveModConfig } from "../../api";
import {
  coerce, countChanges, humanize, isContainer, JsonNumber, JsonObject, lookup, matchesSearch, newItemLike, parse, sameJson, setAt,
  splitMatches, stepNumber, stringify, type Json, type PathPart,
} from "./jsonTree";
import { SplitWorkspace } from "../../components/SplitWorkspace";
import { copyText, useContextMenu } from "../../components/ContextMenu";
import { Icon } from "../../components/Icon";
import { toast, useErrorToast } from "../../components/Toast";

const SAVED = "Saved. Applies next time the world loads.";

/** The Configs tab: mod config files in ModConfig, edited as a tree (JSON) or as text. */
/** `focusMod` is a request from "Edit config": a new object each time, handled once. */
export function ConfigEditor({ enabled, focusMod }: { enabled: boolean; focusMod: { modId: string } | null }) {
  const list = useModConfigs(enabled);
  const openFolder = useOpenKnownFolder();
  const openMenu = useContextMenu();
  const [selected, setSelected] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  // The page stays mounted between visits (and its effects re-run when shown), so each request
  // is applied once: filter to the mod, then open its first file once the list is there.
  const handled = useRef<{ modId: string } | null>(null);
  const [pendingFocus, setPendingFocus] = useState(false);

  useEffect(() => {
    if (!focusMod || handled.current === focusMod) return;
    handled.current = focusMod;
    setFilter(focusMod.modId);
    setSelected(null);
    setPendingFocus(true);
  }, [focusMod]);

  const files = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return (list.data ?? []).filter(
      (c) => !q || c.relativePath.toLowerCase().includes(q) || (c.modId ?? "").toLowerCase().includes(q) || (c.modName ?? "").toLowerCase().includes(q),
    );
  }, [list.data, filter]);

  useEffect(() => {
    if (!pendingFocus || !list.isSuccess) return;
    if (files.length > 0) setSelected(files[0].relativePath);
    setPendingFocus(false);
  }, [pendingFocus, list.isSuccess, files]);

  if (!enabled) return <p className="empty-state">Data folder not found. Set it in <strong>Settings</strong>.</p>;

  return (
    <SplitWorkspace name="config-files" side="left">
      <aside className="config-list" aria-label="Config files">
        <div className="mods-toolbar">
          <input type="search" className="search" placeholder="Filter by file or mod" aria-label="Filter config files" value={filter} onChange={(e) => setFilter(e.target.value)} />
        </div>
        {list.isPending && <p className="muted">Looking for config files…</p>}
        {list.isError && <p className="error-text">{list.error.message}</p>}
        {list.isSuccess && files.length === 0 && <p className="muted small">No config files{filter ? " match" : " in the ModConfig folder yet. Mods create them the first time the game runs with them"}.</p>}
        <ul className="modlist-rows" role="listbox" aria-label="Config files">
          {files.map((c) => (
            <li
              key={c.relativePath}
              role="option"
              aria-selected={selected === c.relativePath}
              aria-haspopup="menu"
              tabIndex={0}
              className={`modlist-row single${selected === c.relativePath ? " selected" : ""}`}
              onClick={() => setSelected(c.relativePath)}
              onContextMenu={(event) => openMenu(event, `Actions for ${c.relativePath}`, [[
                { label: "Open config", run: () => setSelected(c.relativePath) },
                { label: "Open the ModConfig folder", disabled: openFolder.isPending, run: () => openFolder.mutate("modConfig") },
              ], [
                { label: "Copy relative path", run: () => copyText(c.relativePath) },
              ]])}
              onKeyDown={(e) => e.key === "Enter" && setSelected(c.relativePath)}
            >
              <span className="modlist-name">{c.relativePath}</span>
              <span className="muted small">{c.modName ?? "no matching mod"}</span>
            </li>
          ))}
        </ul>
        <div className="link-row">
          <button type="button" className="link-button small" onClick={() => openFolder.mutate("modConfig")}><Icon name="folder" size={13} />Open the ModConfig folder</button>
        </div>
      </aside>
      <div className="installed-main config-main">
        {selected ? <FileEditor key={selected} path={selected} /> : <p className="empty-state">Select a config file.</p>}
      </div>
    </SplitWorkspace>
  );
}

function FileEditor({ path }: { path: string }) {
  const file = useModConfig(path);
  const save = useSaveModConfig();
  useErrorToast(save.error);
  const [text, setText] = useState<string | null>(null);
  const [tree, setTree] = useState<Json | null>(null);
  const [mode, setMode] = useState<"tree" | "text">("tree");
  const [search, setSearch] = useState("");

  // The file as it is on disk, in the order-keeping tree form.
  const originalTree = useMemo(() => (file.data?.document.tree ? parse(file.data.document.tree) : null), [file.data]);

  // Load each read once. The page stays mounted between visits and effects re-run when it is
  // shown again; an unchanged file keeps the same data object, so unsaved edits survive.
  const loaded = useRef<typeof file.data>(undefined);
  useEffect(() => {
    if (!file.data || loaded.current === file.data) return;
    loaded.current = file.data;
    setText(file.data.document.text);
    setTree(originalTree);
    if (!file.data.document.tree) setMode("text");
  }, [file.data, originalTree]);

  if (file.isPending || text === null) return <p className="muted">Opening…</p>;
  if (file.isError) return <p className="error-text">{file.error.message}</p>;
  const doc = file.data!.document;
  const isJson = doc.format === "json";
  const original = doc.text;
  const inTree = mode === "tree" && tree !== null;
  const current = inTree ? stringify(tree) : text;
  const changes = inTree ? countChanges(tree, originalTree ?? undefined) : text !== original ? 1 : 0;
  const dirty = inTree ? !sameJson(tree, originalTree ?? undefined) : text !== original;

  const switchMode = (m: "tree" | "text") => {
    if (m === mode) return;
    if (m === "text" && tree !== null && dirty) setText(stringify(tree));
    if (m === "tree") {
      try {
        setTree(parse(text));
      } catch (e) {
        toast.error(`The text isn't valid JSON yet, so it can't be shown as fields. ${(e as Error).message}.`);
        return; // stay in text mode until it parses
      }
    }
    setMode(m);
  };

  return (
    <div className="config-editor">
      <div className="mods-toolbar wrap cfg-toolbar">
        <div className="cfg-file">
          <Icon name="config" size={16} />
          <strong>{doc.relativePath}</strong>
        </div>
        {isJson && tree !== null && (
          <div className="segmented" role="group" aria-label="Editor">
            <button type="button" aria-pressed={mode === "tree"} onClick={() => switchMode("tree")}><Icon name="lists" />Fields</button>
            <button type="button" aria-pressed={mode === "text"} onClick={() => switchMode("text")}><Icon name="code" />Text</button>
          </div>
        )}
        {inTree && (
          <div className="search-field">
            <Icon name="search" size={16} />
            <input type="search" className="search" placeholder="Find a setting" aria-label="Find a setting" value={search} onChange={(e) => setSearch(e.target.value)} />
          </div>
        )}
        <div className="header-spacer" />
        {dirty && (
          <span className="cfg-changes" role="status">
            <span className="cfg-dot" />
            {inTree ? `${changes} unsaved ${changes === 1 ? "change" : "changes"}` : "Unsaved changes"}
          </span>
        )}
        {file.data?.previous != null && (
          <button
            type="button"
            onClick={async () =>
              (await ask("Put back the version from before your last save?", { title: "Undo last save" })) &&
              save.mutate({ relativePath: path, text: file.data!.previous! }, { onSuccess: () => toast.success(SAVED) })
            }
          >
            <Icon name="undo" />Undo last save
          </button>
        )}
        <button type="button" disabled={!dirty || save.isPending} onClick={() => { setText(original); setTree(originalTree); }}>
          <Icon name="reset" />Discard changes
        </button>
        <button type="button" className="primary" disabled={!dirty || save.isPending} onClick={() => save.mutate({ relativePath: path, text: current }, { onSuccess: () => toast.success(SAVED) })}>
          <Icon name="save" />Save
        </button>
      </div>
      {doc.parseError && <p className="notice warn-text">{doc.parseError} Edit it as text.</p>}
      {inTree ? (
        <div className="cfg-scroll">
          <div className="cfg-content">
            <FieldsContext.Provider value={{ search: search.trim().toLowerCase(), original: originalTree ?? undefined, onChange: (p, v) => setTree((t) => setAt(t, p, v)) }}>
              <RootView value={tree} />
            </FieldsContext.Provider>
          </div>
        </div>
      ) : (
        <textarea className="config-text" spellCheck={false} aria-label="File contents" value={text} onChange={(e) => setText(e.target.value)} />
      )}
    </div>
  );
}

// -----------------------------------------------------------------------------------------------
// Fields view

interface FieldsCtx {
  /** Lower-cased search text ("" when not searching). */
  search: string;
  /** The tree as saved on disk, to mark changed values. */
  original: Json | undefined;
  onChange: (path: PathPart[], value: Json) => void;
}
const FieldsContext = createContext<FieldsCtx>({ search: "", original: undefined, onChange: () => {} });
const useFields = () => useContext(FieldsContext);

/** Text with the search matches highlighted. */
function Hl({ text }: { text: string }) {
  const { search } = useFields();
  return <>{splitMatches(text, search).map((p, i) => (p.hit ? <mark key={i} className="cfg-hit">{p.text}</mark> : p.text))}</>;
}

const keyMatches = (key: string, q: string) => !!q && (key.toLowerCase().includes(q) || humanize(key).toLowerCase().includes(q));
const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;
const describe = (v: Json | undefined) => (v === undefined ? "nothing" : v instanceof JsonNumber ? v.raw : typeof v === "string" ? JSON.stringify(v) : String(v));

function RootView({ value }: { value: Json }) {
  if (value instanceof JsonObject) return <Fields value={value} path={[]} depth={0} showAll={false} />;
  if (Array.isArray(value)) return <Section k="" title="Items" value={value} path={[]} depth={0} showAll={false} />;
  return (
    <div className="cfg-card">
      <div className="cfg-rows"><Row k="Value" value={value} path={[]} /></div>
    </div>
  );
}

/** An object's settings in key order: runs of plain values as rows, nested objects and lists as sections. */
function Fields({ value, path, depth, showAll }: { value: JsonObject; path: PathPart[]; depth: number; showAll: boolean }) {
  const { search } = useFields();
  const visible = value.entries.filter(([k, v]) => showAll || matchesSearch(k, v, search));
  if (visible.length === 0) {
    return <p className="cfg-empty">{value.entries.length === 0 ? "No settings in here." : depth === 0 ? "No matches." : "No matches in here."}</p>;
  }
  const blocks: ReactNode[] = [];
  let rows: ReactNode[] = [];
  const flush = () => {
    if (rows.length === 0) return;
    const block = <div className="cfg-rows" key={`rows-${blocks.length}`}>{rows}</div>;
    blocks.push(depth === 0 ? <div className="cfg-card" key={`card-${blocks.length}`}>{block}</div> : block);
    rows = [];
  };
  for (const [k, v] of visible) {
    if (isContainer(v)) {
      flush();
      blocks.push(<Section key={k} k={k} value={v} path={[...path, k]} depth={depth} showAll={showAll || keyMatches(k, search)} />);
    } else rows.push(<Row key={k} k={k} value={v} path={[...path, k]} />);
  }
  flush();
  return <>{blocks}</>;
}

/** A collapsible group for a nested object or a list. */
function Section({ k, title, value, path, depth, showAll, onRemove }: {
  k: string; title?: string; value: Json[] | JsonObject; path: PathPart[]; depth: number; showAll: boolean; onRemove?: () => void;
}) {
  const { search, original } = useFields();
  const [open, setOpen] = useState(depth < 2);
  const isOpen = open || !!search;
  const changed = !sameJson(value, lookup(original, path));
  const label = title ?? humanize(k);
  const count = Array.isArray(value) ? plural(value.length, "item") : plural(value.entries.length, "setting");
  const bodyId = useId();
  return (
    <section className={`${depth === 0 ? "cfg-card " : "cfg-sub "}cfg-section${isOpen ? " open" : ""}`}>
      <div className="cfg-section-head">
        <button type="button" className="cfg-section-toggle" aria-expanded={isOpen} aria-controls={bodyId} onClick={() => setOpen(!isOpen)}>
          <Icon name={isOpen ? "chevronDown" : "chevronRight"} size={16} />
          <span className="cfg-section-title"><Hl text={label} /></span>
          {!title && label !== k && <code className="cfg-key"><Hl text={k} /></code>}
          <span className="cfg-count">{count}</span>
          {changed && <span className="cfg-dot" title="Changed" />}
        </button>
        {onRemove && (
          <button type="button" className="icon-button cfg-remove" title={`Remove ${label.toLowerCase()}`} aria-label={`Remove ${label.toLowerCase()}`} onClick={onRemove}>
            <Icon name="trash" size={15} />
          </button>
        )}
      </div>
      {isOpen && (
        <div className="cfg-section-body" id={bodyId}>
          {Array.isArray(value)
            ? <ListItems items={value} path={path} depth={depth + 1} showAll={showAll} />
            : <Fields value={value} path={path} depth={depth + 1} showAll={showAll} />}
        </div>
      )}
    </section>
  );
}

/** A list: each item with a remove button, and a button to add one. */
function ListItems({ items, path, depth, showAll }: { items: Json[]; path: PathPart[]; depth: number; showAll: boolean }) {
  const { search, onChange } = useFields();
  const remove = (i: number) => onChange(path, items.filter((_, j) => j !== i));
  const shown = items.map((v, i) => [v, i] as const).filter(([v]) => showAll || !search || matchesSearch("", v, search));
  return (
    <div className="cfg-list">
      {items.length === 0 && <p className="cfg-empty">No items yet.</p>}
      {items.length > 0 && shown.length === 0 && <p className="cfg-empty">No matches in here.</p>}
      {shown.map(([v, i]) =>
        isContainer(v)
          ? <Section key={i} k={String(i)} title={`Item ${i + 1}`} value={v} path={[...path, i]} depth={depth} showAll={showAll} onRemove={() => remove(i)} />
          : <ListItem key={i} index={i} value={v} path={[...path, i]} onRemove={() => remove(i)} />,
      )}
      <button type="button" className="cfg-add" onClick={() => onChange(path, [...items, newItemLike(items)])}>
        <Icon name="plus" size={15} />Add item
      </button>
    </div>
  );
}

function ListItem({ index, value, path, onRemove }: { index: number; value: Json; path: PathPart[]; onRemove: () => void }) {
  const { original, onChange } = useFields();
  const before = lookup(original, path);
  const changed = !sameJson(value, before);
  const id = useId();
  return (
    <div className={`cfg-item${changed ? " changed" : ""}`}>
      <span className="cfg-index" aria-hidden="true">{index + 1}</span>
      <Control id={id} label={`Item ${index + 1}`} value={value} wide onChange={(v) => onChange(path, v)} />
      <button type="button" className="icon-button cfg-remove" title="Remove item" aria-label={`Remove item ${index + 1}`} onClick={onRemove}>
        <Icon name="trash" size={15} />
      </button>
    </div>
  );
}

/** One setting: a readable label with the key under it, the control next to it. */
function Row({ k, value, path }: { k: string; value: Json; path: PathPart[] }) {
  const { original, onChange } = useFields();
  const before = lookup(original, path);
  const changed = !sameJson(value, before);
  const label = humanize(k);
  const id = useId();
  return (
    <div className={`cfg-row${changed ? " changed" : ""}`}>
      <div className="cfg-label">
        <label htmlFor={id}><Hl text={label} /></label>
        {label !== k && <code className="cfg-key"><Hl text={k} /></code>}
      </div>
      <div className="cfg-control">
        {changed && before !== undefined && (
          <button type="button" className="icon-button cfg-reset" title={`Changed. Put back ${describe(before)}`} aria-label={`Put back ${label} to ${describe(before)}`} onClick={() => onChange(path, before)}>
            <Icon name="reset" size={14} />
          </button>
        )}
        <Control id={id} value={value} onChange={(v) => onChange(path, v)} />
      </div>
    </div>
  );
}

/** `label` names the control when no <label> points at it (list items). */
function Control({ id, label, value, wide, onChange }: { id: string; label?: string; value: Json; wide?: boolean; onChange: (v: Json) => void }) {
  if (typeof value === "boolean") return <Switch id={id} label={label} checked={value} onChange={onChange} />;
  return <TextValue id={id} label={label} value={value} wide={wide} onChange={onChange} />;
}

function Switch({ id, label, checked, onChange }: { id: string; label?: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <span className="cfg-switch-wrap">
      <span className="cfg-switch-text" aria-hidden="true">{checked ? "On" : "Off"}</span>
      <button id={id} type="button" role="switch" aria-label={label} aria-checked={checked} className="cfg-switch" onClick={() => onChange(!checked)}>
        <span className="cfg-switch-thumb" />
      </button>
    </span>
  );
}

const asText = (v: Json) => (v === null ? "" : v instanceof JsonNumber ? v.raw : String(v));

/** A text, number or empty (null) value. Numbers get − and + steppers and arrow-key steps. */
function TextValue({ id, label, value, wide, onChange }: { id: string; label?: string; value: Json; wide?: boolean; onChange: (v: Json) => void }) {
  const [draft, setDraft] = useState(asText(value));
  const [error, setError] = useState<string | null>(null);
  // Follow outside changes (reset, discard, steppers) but leave a draft alone that already means
  // this value, so typing "1." on the way to "1.5" isn't undone.
  const shown = asText(value);
  useEffect(() => {
    setDraft((d) => {
      const r = coerce(value, d);
      return r.ok && sameJson(r.value, value) ? d : shown;
    });
    setError(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shown]);

  const type = (typed: string) => {
    setDraft(typed);
    const r = coerce(value, typed);
    if (r.ok) {
      setError(null);
      if (!sameJson(r.value, value)) onChange(r.value);
    } else setError(r.error);
  };
  const isNumber = value instanceof JsonNumber;
  const step = (n: number) => isNumber && onChange(stepNumber(value, n));

  const input = (
    <input
      id={id}
      aria-label={label}
      type="text"
      inputMode={isNumber ? "decimal" : undefined}
      className={isNumber ? "cfg-input num" : `cfg-input${wide ? " wide" : ""}`}
      value={draft}
      placeholder={value === null ? "empty (null)" : undefined}
      aria-invalid={!!error}
      spellCheck={false}
      onChange={(e) => type(e.target.value)}
      onBlur={() => !error && setDraft(asText(value))}
      onKeyDown={(e) => {
        if (!isNumber || error || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
        e.preventDefault();
        step((e.key === "ArrowUp" ? 1 : -1) * (e.shiftKey ? 10 : 1));
      }}
    />
  );
  return (
    <span className={`cfg-value${wide ? " wide" : ""}`}>
      {isNumber ? (
        <span className={`cfg-number${error ? " invalid" : ""}`}>
          <button type="button" tabIndex={-1} aria-label="Decrease" disabled={!!error} onClick={() => step(-1)}><Icon name="minus" size={14} /></button>
          {input}
          <button type="button" tabIndex={-1} aria-label="Increase" disabled={!!error} onClick={() => step(1)}><Icon name="plus" size={14} /></button>
        </span>
      ) : input}
      {error && <span className="cfg-error">{error}</span>}
    </span>
  );
}
