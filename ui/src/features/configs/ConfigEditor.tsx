import { useEffect, useId, useMemo, useRef, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { useModConfig, useModConfigs, useOpenKnownFolder, useSaveModConfig } from "../../api";
import { coerce, matchesSearch, setAt, stringify, type Json, type PathPart } from "./jsonTree";
import { SplitWorkspace } from "../../components/SplitWorkspace";
import { copyText, useContextMenu } from "../../components/ContextMenu";

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

  if (!enabled) return <p className="empty-state">The Vintage Story data folder wasn't found. Pick it on the <strong>Settings</strong> tab.</p>;

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
          <button type="button" className="link-button small" onClick={() => openFolder.mutate("modConfig")}>Open the ModConfig folder</button>
        </div>
      </aside>
      <div className="installed-main config-main">
        {selected ? <FileEditor key={selected} path={selected} /> : <p className="empty-state">Pick a config file to edit it.</p>}
      </div>
    </SplitWorkspace>
  );
}

function FileEditor({ path }: { path: string }) {
  const file = useModConfig(path);
  const save = useSaveModConfig();
  const [text, setText] = useState<string | null>(null);
  const [tree, setTree] = useState<Json | null>(null);
  const [mode, setMode] = useState<"tree" | "text">("tree");
  const [search, setSearch] = useState("");

  useEffect(() => {
    if (!file.data) return;
    setText(file.data.document.text);
    setTree(file.data.document.tree ? (JSON.parse(file.data.document.tree) as Json) : null);
    if (!file.data.document.tree) setMode("text");
  }, [file.data]);

  if (file.isPending || text === null) return <p className="muted">Opening…</p>;
  if (file.isError) return <p className="error-text">{file.error.message}</p>;
  const doc = file.data!.document;
  const isJson = doc.format === "json";
  const original = doc.text;
  const current = mode === "tree" && tree !== null ? stringify(tree) : text;
  const dirty = mode === "tree" ? tree !== null && stringify(tree) !== stringify(JSON.parse(doc.tree ?? "null")) : text !== original;

  const switchMode = (m: "tree" | "text") => {
    if (m === "text" && tree !== null && dirty) setText(stringify(tree));
    if (m === "tree") {
      try {
        setTree(JSON.parse(text) as Json);
      } catch {
        return; // stay in text mode until it parses
      }
    }
    setMode(m);
  };

  return (
    <div className="config-editor">
      <div className="mods-toolbar wrap">
        <strong>{doc.relativePath}</strong>
        {isJson && tree !== null && (
          <div className="segmented" role="group" aria-label="Editor">
            <button type="button" aria-pressed={mode === "tree"} onClick={() => switchMode("tree")}>Fields</button>
            <button type="button" aria-pressed={mode === "text"} onClick={() => switchMode("text")}>Text</button>
          </div>
        )}
        {mode === "tree" && <input type="search" placeholder="Find a setting" aria-label="Find a setting" value={search} onChange={(e) => setSearch(e.target.value)} />}
        <div className="header-spacer" />
        {file.data?.previous != null && (
          <button
            type="button"
            title="Put back the file as it was before your last save here"
            onClick={async () =>
              (await ask("Put back the version from before your last save?", { title: "Undo last save" })) &&
              save.mutate({ relativePath: path, text: file.data!.previous! })
            }
          >
            Undo last save
          </button>
        )}
        <button type="button" disabled={!dirty || save.isPending} onClick={() => { setText(original); setTree(doc.tree ? (JSON.parse(doc.tree) as Json) : null); }}>
          Discard changes
        </button>
        <button type="button" className="primary" disabled={!dirty || save.isPending} onClick={() => save.mutate({ relativePath: path, text: current })}>
          Save
        </button>
      </div>
      {doc.parseError && <p className="notice warn-text">{doc.parseError} Edit it as text.</p>}
      {save.isError && <p className="notice error-text">{save.error.message}</p>}
      {save.isSuccess && !dirty && <p className="notice" role="status">Saved. Changes apply the next time the game (or world) loads.</p>}
      {mode === "tree" && tree !== null ? (
        <div className="json-tree">
          <Node name={null} value={tree} path={[]} search={search} onChange={(p, v) => setTree(setAt(tree, p, v))} />
        </div>
      ) : (
        <textarea className="config-text" spellCheck={false} aria-label="File contents" value={text} onChange={(e) => setText(e.target.value)} />
      )}
    </div>
  );
}

function Node({ name, value, path, search, onChange }: { name: string | null; value: Json; path: PathPart[]; search: string; onChange: (p: PathPart[], v: Json) => void }) {
  if (value !== null && typeof value === "object") {
    const entries: [PathPart, Json][] = Array.isArray(value) ? value.map((v, i) => [i, v]) : Object.entries(value);
    const children = entries.filter(([k, v]) => matchesSearch(String(k), v, search) || (name !== null && String(name).toLowerCase().includes(search.toLowerCase())));
    const body = (
      <div className={name === null ? undefined : "json-children"}>
        {children.map(([k, v]) => <Node key={String(k)} name={String(k)} value={v} path={[...path, k]} search={search} onChange={onChange} />)}
        {children.length === 0 && <span className="muted small">{entries.length === 0 ? "(empty)" : "(no matches)"}</span>}
      </div>
    );
    if (name === null) return body;
    return (
      <details className="json-group" open={!!search || path.length < 2}>
        <summary>
          {name} <span className="muted small">{Array.isArray(value) ? `[${value.length}]` : `{${Object.keys(value).length}}`}</span>
        </summary>
        {body}
      </details>
    );
  }
  return <Leaf name={name ?? ""} value={value} onChange={(v) => onChange(path, v)} />;
}

function Leaf({ name, value, onChange }: { name: string; value: Json; onChange: (v: Json) => void }) {
  const [draft, setDraft] = useState(value === null ? "" : String(value));
  const [error, setError] = useState<string | null>(null);
  useEffect(() => setDraft(value === null ? "" : String(value)), [value]);
  const id = useId();
  return (
    <div className="json-leaf">
      <label htmlFor={id}>{name}</label>
      {typeof value === "boolean" ? (
        <input id={id} type="checkbox" checked={value} onChange={(e) => onChange(e.target.checked)} />
      ) : (
        <input
          id={id}
          type="text"
          className={typeof value === "number" ? "num" : undefined}
          value={draft}
          placeholder={value === null ? "null" : undefined}
          onChange={(e) => {
            setDraft(e.target.value);
            const r = coerce(value, e.target.value);
            if (r.ok) {
              setError(null);
              onChange(r.value);
            } else setError(r.error);
          }}
        />
      )}
      {error && <span className="error-text small">{error}</span>}
    </div>
  );
}
