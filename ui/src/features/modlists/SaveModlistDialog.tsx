import { useEffect, useMemo, useState } from "react";
import type { ModlistSummary } from "../../bindings";
import { useModlistSaveDefaults, useSaveModlist } from "../../api";
import { formatBytes } from "../../lib/format";
import { Modal } from "./Modal";
import { IconPicker, iconThemeClass, ModlistIcon } from "./ModlistIcon";

interface Props {
  existing: ModlistSummary[];
  onSaved: (s: ModlistSummary) => void;
  onClose: () => void;
}

export function SaveModlistDialog({ existing, onSaved, onClose }: Props) {
  const defaults = useModlistSaveDefaults(true);
  const save = useSaveModlist();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [uploader, setUploader] = useState<string | null>(null);
  const [gameVersion, setGameVersion] = useState<string | null>(null);
  const [configs, setConfigs] = useState<Set<string>>(new Set());
  const [warnings, setWarnings] = useState<string[]>([]);
  const [icon, setIcon] = useState<string | null>(null);
  const [iconColor, setIconColor] = useState<string | null>(null);
  const [picking, setPicking] = useState(false);

  useEffect(() => {
    if (!defaults.data) return;
    setUploader((u) => u ?? defaults.data.uploader ?? "");
    setGameVersion((g) => g ?? defaults.data.gameVersion ?? "");
  }, [defaults.data]);

  const disabled = defaults.data?.disabledMods ?? [];
  const disabledIds = useMemo(() => new Set(disabled.map((d) => d.modId.toLowerCase())), [disabled]);
  const isLeftOut = (modId: string | null) => !!modId && disabledIds.has(modId.toLowerCase());
  const candidates = (defaults.data?.configs ?? []).filter((c) => !isLeftOut(c.modId));
  const clash = useMemo(
    () => existing.some((e) => e.kind === "modlist" && e.name.trim().toLowerCase() === name.trim().toLowerCase()),
    [existing, name],
  );

  const submit = (overwrite: boolean) =>
    save.mutate(
      {
        name,
        description: description || null,
        gameVersion: gameVersion || null,
        uploader: uploader || null,
        configs: candidates
          .filter((c) => configs.has(c.relativePath))
          .map((c) => ({ relativePath: c.relativePath, modId: c.modId })),
        overwrite,
        icon,
        iconColor,
      },
      {
        onSuccess: (r) => {
          if (r.warnings.length) setWarnings(r.warnings);
          else onSaved(r.saved);
        },
      },
    );

  return (
    <Modal title="Save modlist" onClose={onClose} busy={save.isPending}>
      <div className="form-grid">
        <label className="form-label" htmlFor="ml-name">Name</label>
        <input id="ml-name" type="text" autoFocus value={name} onChange={(e) => setName(e.target.value)} />
        <span className="form-label">Icon</span>
        <div className="icon-field">
          <button type="button" className={`details-icon-button${iconThemeClass(iconColor)}`} aria-label="Choose an icon" title="Choose an icon" onClick={() => setPicking(true)}>
            <ModlistIcon icon={icon} fallback="lists" />
          </button>
          <button type="button" className="link-button small" onClick={() => setPicking(true)}>Choose…</button>
        </div>
        <label className="form-label" htmlFor="ml-desc">Description</label>
        <textarea id="ml-desc" rows={3} value={description} onChange={(e) => setDescription(e.target.value)} />
        <label className="form-label" htmlFor="ml-by">Made by</label>
        <input id="ml-by" type="text" value={uploader ?? ""} onChange={(e) => setUploader(e.target.value)} />
        <label className="form-label" htmlFor="ml-gv">For game version</label>
        <input id="ml-gv" type="text" value={gameVersion ?? ""} onChange={(e) => setGameVersion(e.target.value)} />
      </div>
      {disabled.length > 0 && (
        <div className="notice warn-text disabled-mods-note" role="status">
          Disabled mods excluded from this modlist ({disabled.length})
          <ul>{disabled.map((d) => <li key={d.modId}>{d.name}{d.name !== d.modId && <span className="muted small"> ({d.modId})</span>}</li>)}</ul>
        </div>
      )}
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
      {defaults.isError && <p className="notice error-text">{defaults.error.message}</p>}
      {save.isError && <p className="notice error-text">{save.error.message}</p>}
      {warnings.length > 0 && (
        <div className="notice warn-text">
          Saved, but some config files couldn't be read:
          <ul>{warnings.map((w) => <li key={w}>{w}</li>)}</ul>
        </div>
      )}
      <div className="modal-actions">
        {clash && !warnings.length && <span className="warn-text small">A modlist with this name exists and will be replaced.</span>}
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
          currentColor={iconColor}
          fallback="lists"
          onPick={(picked, color) => {
            setIcon(picked);
            setIconColor(color);
            setPicking(false);
          }}
          onClose={() => setPicking(false)}
        />
      )}
    </Modal>
  );
}
