import { useEffect, useMemo, useState } from "react";
import type { ModlistSummary } from "../../bindings";
import { useModlistSaveDefaults, useMods, useSaveModlist } from "../../api";
import { Icon } from "../../components/Icon";
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
  const installed = useMods(true);
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
  const forMods = candidates.filter((c) => c.modId);
  const other = candidates.filter((c) => !c.modId);
  const enabledCount = installed.data?.mods.filter((m) => m.isActive).length;
  const toggleConfig = (path: string, on: boolean) => {
    const next = new Set(configs);
    if (on) next.add(path);
    else next.delete(path);
    setConfigs(next);
  };
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
      <div className="save-hero">
        <button
          type="button"
          className={`save-hero-icon${iconThemeClass(iconColor)}`}
          title="Choose an icon and colors"
          aria-label="Choose an icon"
          onClick={() => setPicking(true)}
        >
          <ModlistIcon icon={icon} fallback="lists" />
          <span className="save-hero-icon-edit">Change</span>
        </button>
        <div className="save-hero-text">
          <input
            className="save-hero-name"
            type="text"
            autoFocus
            placeholder="Name your modlist"
            aria-label="Name"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <textarea
            className="save-hero-desc"
            rows={2}
            placeholder="What's it for? (optional)"
            aria-label="Description"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
          />
        </div>
      </div>

      <div className="save-chips">
        <span className="save-chip strong"><Icon name="library" />{enabledCount ?? "…"} enabled mods</span>
        <label className="save-chip">
          <span>By</span>
          <input type="text" aria-label="Made by" value={uploader ?? ""} onChange={(e) => setUploader(e.target.value)} />
        </label>
        <label className="save-chip">
          <span>VS</span>
          <input type="text" aria-label="For game version" value={gameVersion ?? ""} onChange={(e) => setGameVersion(e.target.value)} />
        </label>
      </div>

      {disabled.length > 0 && (
        <details className="callout callout-warn save-disabled">
          <summary>
            {disabled.length === 1 ? "1 disabled mod is" : `${disabled.length} disabled mods are`} left out
          </summary>
          <ul>{disabled.map((d) => <li key={d.modId}>{d.name}{d.name !== d.modId && <span className="muted small"> ({d.modId})</span>}</li>)}</ul>
        </details>
      )}

      <section className="save-configs">
        <div className="save-configs-head">
          <h3>Config files</h3>
          {candidates.length > 0 && <span className="muted small">{configs.size} of {candidates.length} included</span>}
          <span className="header-spacer" />
          {candidates.length > 0 && (
            <div className="segmented">
              <button type="button" className="small" onClick={() => setConfigs(new Set(forMods.map((c) => c.relativePath)))}>Your mods' files</button>
              <button type="button" className="small" onClick={() => setConfigs(new Set(candidates.map((c) => c.relativePath)))}>All</button>
              <button type="button" className="small" onClick={() => setConfigs(new Set())}>None</button>
            </div>
          )}
        </div>
        <p className="muted small save-configs-help">Included files are copied to anyone who loads the modlist, so they get your settings too.</p>
        {defaults.isPending && <p className="muted small">Looking for config files…</p>}
        {defaults.isSuccess && candidates.length === 0 && <p className="muted small">No config files in the ModConfig folder.</p>}
        {candidates.length > 0 && (
          <ul className="save-config-list">
            {[...forMods, ...other].map((c, i) => (
              <li key={c.relativePath} className={`${configs.has(c.relativePath) ? "on" : ""}${i === forMods.length && i > 0 ? " first-other" : ""}`}>
                <label>
                  <input type="checkbox" checked={configs.has(c.relativePath)} onChange={(e) => toggleConfig(c.relativePath, e.target.checked)} />
                  <span className="save-config-name">{c.relativePath}</span>
                  {c.modName ? <span className="save-config-mod">{c.modName}</span> : <span className="muted small">no matching mod</span>}
                  <span className="muted small save-config-size">{formatBytes(c.sizeBytes)}</span>
                </label>
              </li>
            ))}
          </ul>
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
              {clash ? "Replace" : "Save modlist"}
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
