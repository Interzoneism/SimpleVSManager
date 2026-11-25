import { useEffect, useMemo, useState } from "react";
import type { ModlistKind, ModlistSummary } from "../../bindings";
import { useModlistSaveDefaults, useSaveModlist } from "../../api";
import { formatBytes } from "../../lib/format";
import { Modal } from "./Modal";
import { IconPicker, ModlistIcon } from "./ModlistIcon";

interface Props {
  initialKind: "modlist" | "preset";
  existing: ModlistSummary[];
  onSaved: (s: ModlistSummary) => void;
  onClose: () => void;
}

/** Saves the installed mods as a modlist (with versions and details) or a preset (on/off only). */
export function SaveModlistDialog({ initialKind, existing, onSaved, onClose }: Props) {
  const defaults = useModlistSaveDefaults(true);
  const save = useSaveModlist();
  const [kind, setKind] = useState<"modlist" | "preset">(initialKind);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [version, setVersion] = useState("");
  const [uploader, setUploader] = useState<string | null>(null);
  const [gameVersion, setGameVersion] = useState<string | null>(null);
  const [configs, setConfigs] = useState<Set<string>>(new Set());
  const [warnings, setWarnings] = useState<string[]>([]);
  const [icon, setIcon] = useState<string | null>(null);
  const [picking, setPicking] = useState(false);

  useEffect(() => {
    if (!defaults.data) return;
    setUploader((u) => u ?? defaults.data.uploader ?? "");
    setGameVersion((g) => g ?? defaults.data.gameVersion ?? "");
  }, [defaults.data]);

  const disabled = defaults.data?.disabledMods ?? [];
  const disabledIds = useMemo(() => new Set(disabled.map((d) => d.modId.toLowerCase())), [disabled]);
  const isLeftOut = (modId: string | null) => kind === "modlist" && !!modId && disabledIds.has(modId.toLowerCase());
  const candidates = (defaults.data?.configs ?? []).filter((c) => !isLeftOut(c.modId));
  const targetKind: ModlistKind = kind;
  const clash = useMemo(
    () => existing.some((e) => e.kind === targetKind && e.name.trim().toLowerCase() === name.trim().toLowerCase()),
    [existing, targetKind, name],
  );

  const submit = (overwrite: boolean) =>
    save.mutate(
      {
        kind,
        name,
        description: description || null,
        version: version || null,
        gameVersion: gameVersion || null,
        uploader: uploader || null,
        configs: candidates
          .filter((c) => configs.has(c.relativePath))
          .map((c) => ({ relativePath: c.relativePath, modId: c.modId })),
        overwrite,
        icon,
      },
      {
        onSuccess: (r) => {
          if (r.warnings.length) setWarnings(r.warnings);
          else onSaved(r.saved);
        },
      },
    );

  const isPreset = kind === "preset";

  return (
    <Modal title={isPreset ? "Save preset" : "Save modlist"} onClose={onClose} busy={save.isPending}>
      <div className="form-grid">
        <span className="form-label">Save as</span>
        <div className="radio-row" role="radiogroup" aria-label="Save as">
          <label className="checkbox">
            <input type="radio" name="kind" checked={!isPreset} onChange={() => setKind("modlist")} />
            Modlist <span className="muted small">exact versions; can install the mods on another PC</span>
          </label>
          <label className="checkbox">
            <input type="radio" name="kind" checked={isPreset} onChange={() => setKind("preset")} />
            Preset <span className="muted small">just which mods are on and off</span>
          </label>
        </div>
        <label className="form-label" htmlFor="ml-name">Name</label>
        <input id="ml-name" type="text" autoFocus value={name} onChange={(e) => setName(e.target.value)} />
        {!isPreset && (
          <>
            <span className="form-label">Icon</span>
            <div className="icon-field">
              <button type="button" className="details-icon-button" aria-label="Choose an icon" title="Choose an icon" onClick={() => setPicking(true)}>
                <ModlistIcon icon={icon} fallback="lists" />
              </button>
              <button type="button" className="link-button small" onClick={() => setPicking(true)}>Choose…</button>
            </div>
            <label className="form-label" htmlFor="ml-desc">Description</label>
            <textarea id="ml-desc" rows={3} value={description} onChange={(e) => setDescription(e.target.value)} />
            <label className="form-label" htmlFor="ml-version">Modlist version</label>
            <input id="ml-version" type="text" placeholder="e.g. 1.0" value={version} onChange={(e) => setVersion(e.target.value)} />
            <label className="form-label" htmlFor="ml-by">Made by</label>
            <input id="ml-by" type="text" value={uploader ?? ""} onChange={(e) => setUploader(e.target.value)} />
            <label className="form-label" htmlFor="ml-gv">For game version</label>
            <input id="ml-gv" type="text" value={gameVersion ?? ""} onChange={(e) => setGameVersion(e.target.value)} />
          </>
        )}
      </div>
      {!isPreset && disabled.length > 0 && (
        <div className="notice warn-text disabled-mods-note" role="status">
          {disabled.length === 1 ? "1 mod is" : `${disabled.length} mods are`} disabled and won't be in the modlist
          (a preset keeps them, switched off):
          <ul>{disabled.map((d) => <li key={d.modId}>{d.name}{d.name !== d.modId && <span className="muted small"> ({d.modId})</span>}</li>)}</ul>
        </div>
      )}
      {!isPreset && (
        <section className="config-picker">
          <h3>Include config files</h3>
          {defaults.isPending && <p className="muted small">Looking for config files…</p>}
          {defaults.isSuccess && candidates.length === 0 && <p className="muted small">No config files in the ModConfig folder.</p>}
          {candidates.length > 0 && (
            <>
              <div className="link-row">
                <button type="button" className="link-button small" onClick={() => setConfigs(new Set(candidates.filter((c) => c.modId).map((c) => c.relativePath)))}>
                  Select files that belong to installed mods
                </button>
                <button type="button" className="link-button small" onClick={() => setConfigs(new Set())}>
                  Select none
                </button>
              </div>
              <ul className="check-list">
                {candidates.map((c) => (
                  <li key={c.relativePath}>
                    <label className="checkbox">
                      <input
                        type="checkbox"
                        checked={configs.has(c.relativePath)}
                        onChange={(e) => {
                          const next = new Set(configs);
                          if (e.target.checked) next.add(c.relativePath);
                          else next.delete(c.relativePath);
                          setConfigs(next);
                        }}
                      />
                      <span>{c.relativePath}</span>
                      <span className="muted small">
                        {c.modName ?? "no matching mod"} · {formatBytes(c.sizeBytes)}
                      </span>
                    </label>
                  </li>
                ))}
              </ul>
            </>
          )}
        </section>
      )}
      {defaults.isError && <p className="notice error-text">{defaults.error.message}</p>}
      {save.isError && <p className="notice error-text">{save.error.message}</p>}
      {warnings.length > 0 && (
        <div className="notice warn-text">
          Saved, but some config files couldn't be read:
          <ul>{warnings.map((w) => <li key={w}>{w}</li>)}</ul>
        </div>
      )}
      <div className="modal-actions">
        {clash && !warnings.length && <span className="warn-text small">A {kind} with this name exists and will be replaced.</span>}
        <span className="header-spacer" />
        {warnings.length ? (
          <button type="button" className="primary" onClick={onClose}>Close</button>
        ) : (
          <>
            <button type="button" onClick={onClose} disabled={save.isPending}>Cancel</button>
            <button type="button" className="primary" disabled={!name.trim() || save.isPending} onClick={() => submit(clash)}>
              {clash ? "Replace" : "Save"}
            </button>
          </>
        )}
      </div>
      {picking && (
        <IconPicker
          current={icon}
          fallback="lists"
          onPick={(picked) => {
            setIcon(picked);
            setPicking(false);
          }}
          onClose={() => setPicking(false)}
        />
      )}
    </Modal>
  );
}
