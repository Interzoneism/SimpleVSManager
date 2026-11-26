import { useEffect, useState } from "react";
import type { InstalledMod, UpdateReport } from "../../bindings";
import { cancelUpdates, useModDbDetails } from "../../api";
import { releasesBetween } from "./changelog";
import type { DbIndex } from "./modView";
import { Icon } from "../../components/Icon";

interface Props {
  mods: InstalledMod[];
  db: DbIndex;
  progress: Map<string, string>;
  running: boolean;
  report: UpdateReport | null;
  onStart: (keys: string[]) => void;
  onClose: () => void;
}

/** Confirms which mods to update, shows changelogs and progress, then the outcome. */
export function UpdateDialog({ mods, db, progress, running, report, onStart, onClose }: Props) {
  const [chosen, setChosen] = useState<Set<string>>(() => new Set(mods.map((m) => m.key)));
  const [open, setOpen] = useState<string | null>(mods.length === 1 ? mods[0].key : null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !running) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [running, onClose]);

  const outcome = (key: string) => report?.outcomes.find((o) => o.key === key);
  const failed = report?.outcomes.filter((o) => !o.ok).length ?? 0;
  const done = !!report;

  return (
    <div className="modal-backdrop" role="presentation">
      <div className="modal" role="dialog" aria-modal="true" aria-labelledby="update-title">
        <h2 id="update-title">
          {done
            ? failed > 0
              ? `Updated ${report.outcomes.length - failed} of ${report.outcomes.length} mods`
              : `Updated ${report.outcomes.length} mod${report.outcomes.length === 1 ? "" : "s"}`
            : mods.length === 1
              ? `Update ${mods[0].name}`
              : `Update ${mods.length} mods`}
        </h2>
        <ul className="update-list">
          {mods.map((m) => {
            const s = db.get(m.key);
            const o = outcome(m.key);
            const p = progress.get(m.key);
            return (
              <li key={m.key} className="update-item">
                <div className="update-row">
                  <input
                    type="checkbox"
                    aria-label={`Update ${m.name}`}
                    checked={chosen.has(m.key)}
                    disabled={running || done}
                    onChange={(e) => {
                      const next = new Set(chosen);
                      if (e.target.checked) next.add(m.key);
                      else next.delete(m.key);
                      setChosen(next);
                    }}
                  />
                  <span className="update-name">{m.name}</span>
                  <span className="muted">
                    {m.version ?? "?"} → {o?.toVersion ?? s?.update?.version ?? "?"}
                  </span>
                  {s?.update && !s.update.compatible && (
                    <span className="warn-text small" title={s.update.message}>
                      may not support your game version
                    </span>
                  )}
                  <span className="header-spacer" />
                  {p && <span className="muted small">{p}</span>}
                  {o && (o.ok ? <span className="ok-text small">Done</span> : <span className="error-text small">{o.message}</span>)}
                  <button
                    type="button"
                    className="link-button small"
                    aria-expanded={open === m.key}
                    onClick={() => setOpen(open === m.key ? null : m.key)}
                  >
                    <Icon name={open === m.key ? "chevronUp" : "chevronDown"} size={13} />{open === m.key ? "Hide changes" : "Changes"}
                  </button>
                </div>
                {open === m.key && s?.update && (
                  <Changelog modId={m.modId} installed={m.version} target={s.update.version} />
                )}
              </li>
            );
          })}
        </ul>
        <div className="modal-actions">
          {report?.cancelled && <span className="muted small">Cancelled; the rest were left as they were.</span>}
          <span className="header-spacer" />
          {running ? (
            <button type="button" onClick={() => void cancelUpdates()}>
              <Icon name="stop" />Stop after this one
            </button>
          ) : done ? (
            <button type="button" className="primary" onClick={onClose}>
              <Icon name="close" />Close
            </button>
          ) : (
            <>
              <button type="button" onClick={onClose}>
                <Icon name="close" />Cancel
              </button>
              <button
                type="button"
                className="primary"
                disabled={chosen.size === 0}
                onClick={() => onStart(mods.filter((m) => chosen.has(m.key)).map((m) => m.key))}
              >
                <Icon name="update" />Update {chosen.size === 1 ? "1 mod" : `${chosen.size} mods`}
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

export function Changelog({ modId, installed, target, collapsed }: {
  modId: string;
  installed: string | null;
  target: string;
  /** Show it folded away under a "Changelog" toggle. */
  collapsed?: boolean;
}) {
  const details = useModDbDetails(modId);
  if (details.isPending) return collapsed ? null : <p className="muted small changelog">Loading changes…</p>;
  if (details.isError) return <p className="error-text small changelog">{details.error.message}</p>;
  const releases = releasesBetween(details.data.info?.releases ?? [], installed, target);
  if (releases.length === 0) return collapsed ? null : <p className="muted small changelog">No changelog.</p>;
  const body = (
    <div className="changelog">
      {releases.map((r) => (
        <section key={r.version}>
          <h4>
            {r.version}
            {r.gameVersions.length > 0 && <span className="muted small"> · for {summarizeVersions(r.gameVersions)}</span>}
          </h4>
          <pre>{r.changelog ?? "No notes."}</pre>
        </section>
      ))}
    </div>
  );
  if (!collapsed) return body;
  return (
    <details className="changelog-toggle">
      <summary>
        Changelog <span className="muted">({releases.length === 1 ? "1 version" : `${releases.length} versions`})</span>
      </summary>
      {body}
    </details>
  );
}

/** "1.21.0 – 1.21.6" for a run of tags, or a short list. */
export function summarizeVersions(tags: string[]): string {
  if (tags.length <= 3) return tags.join(", ");
  return `${tags[0]} – ${tags[tags.length - 1]}`;
}
