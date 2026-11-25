import { useEffect, useState } from "react";
import type { InstalledMod, ModDbSummary, Os } from "../../bindings";
import { useModDbDetails, useOpenModWebsite, useOpenUrl, useRevealMod, useUpdatePreferences } from "../../api";
import { formatCount, formatDate } from "../../lib/format";
import { iconUrl } from "../../lib/iconUrl";
import { canToggleMod, statusLabel, type DbIndex } from "./modView";
import { Changelog, summarizeVersions } from "./UpdateDialog";
import { Reports } from "./Reports";
import { Icon } from "../../components/Icon";
import { CloseDetails } from "../../components/SplitWorkspace";

interface Props {
  os: Os;
  mods: InstalledMod[];
  db: DbIndex;
  busy: boolean;
  progress: Map<string, string>;
  onSetActive: (mods: InstalledMod[], active: boolean) => void;
  onDelete: (mods: InstalledMod[]) => void;
  onUpdate: (mods: InstalledMod[]) => void;
  onInstallVersion: (mod: InstalledMod, version: string) => void;
  onEditConfig?: (modId: string) => void;
  onClose: () => void;
}

/** Right-hand pane: one mod's details, or batch actions for several. */
export function ModDetails({ os, mods, db, busy, progress, onSetActive, onDelete, onUpdate, onInstallVersion, onEditConfig, onClose }: Props) {
  const prefs = useUpdatePreferences();
  const reveal = useRevealMod();
  const website = useOpenModWebsite();

  if (mods.length === 0) {
    return (
      <aside className="details empty" aria-label="Mod details">
        <div>
          <span className="empty-icon"><Icon name="panel" /></span>
          <h2>Mod details</h2>
          <p className="muted">Select a mod to see its versions, dependencies, and settings.</p>
        </div>
      </aside>
    );
  }

  const manageable = mods.filter(canToggleMod);

  if (mods.length > 1) {
    const withUpdates = mods.filter((m) => db.get(m.key)?.update);
    const ids = mods.map((m) => m.modId).filter(Boolean);
    return (
      <aside className="details">
        <DetailsHeader title={`${mods.length} mods selected`} onClose={onClose} />
        <div className="details-actions">
          <button type="button" disabled={busy} onClick={() => onSetActive(manageable, true)}>Enable all</button>
          <button type="button" disabled={busy} onClick={() => onSetActive(manageable, false)}>Disable all</button>
          {withUpdates.length > 0 && (
            <button type="button" className="primary" disabled={busy} onClick={() => onUpdate(withUpdates)}>
              Update {withUpdates.length}
            </button>
          )}
          <button type="button" disabled={prefs.isPending} onClick={() => prefs.mutate({ updateExcluded: { modIds: ids, on: true } })}>
            Leave out of Update all
          </button>
          <button type="button" disabled={prefs.isPending} onClick={() => prefs.mutate({ updateExcluded: { modIds: ids, on: false } })}>
            Include in Update all
          </button>
          <button type="button" className="danger" disabled={busy} onClick={() => onDelete(mods)}>Delete…</button>
        </div>
        <ul className="details-list">
          {mods.map((m) => (
            <li key={m.key}>{m.name}</li>
          ))}
        </ul>
      </aside>
    );
  }

  const mod = mods[0];
  const problem = mod.error ?? mod.loadError;
  const actionError = reveal.error ?? website.error;

  return (
    <aside className="details">
      <DetailsHeader title={mod.name} onClose={onClose} />
      <div className="details-hero">
        {mod.iconKey ? (
          <img className="details-icon" src={iconUrl(os, mod.iconKey)} alt="" />
        ) : (
          <span className="details-icon placeholder" aria-hidden>{mod.name.slice(0, 1).toUpperCase()}</span>
        )}
        <dl className="details-facts">
          <dt>Version</dt>
          <dd>{mod.version ?? "unknown"}</dd>
          <dt>Mod ID</dt>
          <dd>{mod.modId || "—"}</dd>
          <dt>Status</dt>
          <dd>{statusLabel(mod)}</dd>
          {mod.side && (
            <>
              <dt>Side</dt>
              <dd>{mod.side}</dd>
            </>
          )}
        </dl>
      </div>

      {problem && <p className="notice error-text">{problem}</p>}

      <div className="details-actions">
        {manageable.length > 0 && (
          <button type="button" disabled={busy} onClick={() => onSetActive([mod], !mod.isActive)}>
            {mod.isActive ? "Disable" : "Enable"}
          </button>
        )}
        <button type="button" onClick={() => reveal.mutate(mod.key)}>
          Show in folder
        </button>
        {mod.website && (
          <button type="button" onClick={() => website.mutate(mod.key)}>
            Website
          </button>
        )}
        {onEditConfig && mod.modId && (
          <button type="button" title="Edit this mod's config files" onClick={() => onEditConfig(mod.modId)}>
            Config…
          </button>
        )}
        <button type="button" className="danger" disabled={busy} onClick={() => onDelete([mod])}>
          Delete…
        </button>
      </div>
      {actionError && <p className="error-text small">{actionError.message}</p>}

      {mod.modId && (
        <ModDbPanel
          mod={mod}
          summary={db.get(mod.key)}
          busy={busy}
          progress={progress.get(mod.key)}
          onUpdate={() => onUpdate([mod])}
          onInstallVersion={(v) => onInstallVersion(mod, v)}
        />
      )}

      {mod.modId && mod.version && <Reports modId={mod.modId} version={mod.version} />}

      {mod.authors.length > 0 && (
        <Section title="Authors">
          <p>{mod.authors.join(", ")}</p>
        </Section>
      )}
      {mod.description && (
        <Section title="Description">
          <p className="description">{mod.description}</p>
        </Section>
      )}
      {mod.dependencies.length > 0 && (
        <Section title="Dependencies">
          <ul className="details-list">
            {mod.dependencies.map((d) => {
              const missing = mod.missingDependencies.some((m) => m.modId === d.modId);
              return (
                <li key={d.modId} className={missing ? "error-text" : undefined}>
                  {d.modId}
                  {d.version && d.version !== "*" ? ` ≥ ${d.version}` : ""}
                  {missing && " (missing)"}
                </li>
              );
            })}
          </ul>
        </Section>
      )}
      {mod.duplicates.length > 0 && (
        <Section title="Other copies installed">
          <ul className="details-list">
            {mod.duplicates.map((d) => (
              <li key={d}>
                <code className="path small">{d}</code>
              </li>
            ))}
          </ul>
        </Section>
      )}
      <Section title="File">
        <code className="path small">{mod.sourcePath}</code>
      </Section>
    </aside>
  );
}

function DetailsHeader({ title, onClose }: { title: string; onClose: () => void }) {
  return (
    <div className="details-header">
      <h2>{title}</h2>
      <CloseDetails onClose={onClose} />
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="details-section">
      <h3>{title}</h3>
      {children}
    </section>
  );
}

/** Mod DB facts and version actions for one installed mod. */
function ModDbPanel(props: {
  mod: InstalledMod;
  summary: ModDbSummary | undefined;
  busy: boolean;
  progress: string | undefined;
  onUpdate: () => void;
  onInstallVersion: (version: string) => void;
}) {
  const { mod, summary } = props;
  const details = useModDbDetails(summary ? mod.modId : null);
  const prefs = useUpdatePreferences();
  const openUrl = useOpenUrl();
  const [picked, setPicked] = useState<string>("");
  useEffect(() => setPicked(""), [mod.key]);

  if (!summary) {
    return (
      <Section title="Mod DB">
        <p className="muted small">Not found on the Mod DB (or not checked yet).</p>
      </Section>
    );
  }

  const info = details.data?.info;
  const cached = new Set((details.data?.cachedVersions ?? []).map((c) => c.version));
  const pickedRelease = info?.releases.find((r) => r.version === picked);
  const excluded = summary.excludedFromBulkUpdate;

  return (
    <Section title="Mod DB">
      <dl className="details-facts">
        <dt>Latest</dt>
        <dd>
          {summary.latestVersion ?? "—"}
          {summary.latestCompatibleVersion && summary.latestCompatibleVersion !== summary.latestVersion && (
            <span className="muted"> (for your game: {summary.latestCompatibleVersion})</span>
          )}
        </dd>
        {summary.downloads != null && (
          <>
            <dt>Downloads</dt>
            <dd>{formatCount(summary.downloads)}</dd>
          </>
        )}
        {info?.lastReleasedMs != null && (
          <>
            <dt>Last release</dt>
            <dd>{formatDate(info.lastReleasedMs)}</dd>
          </>
        )}
      </dl>
      {summary.installedMayBeIncompatible && (
        <p className="notice warn-text small">
          The installed version is tagged for {summarizeVersions(summary.installedReleaseGameVersions)}, not your target
          game version.
        </p>
      )}
      {props.progress && <p className="muted small">{props.progress}</p>}
      <div className="details-actions">
        {summary.update && (
          <button type="button" className="primary" disabled={props.busy} title={summary.update.message} onClick={props.onUpdate}>
            Update to {summary.update.version}
          </button>
        )}
        {summary.update && (
          <button
            type="button"
            disabled={prefs.isPending}
            title="Don't offer this version again; newer ones will still be offered"
            onClick={() => prefs.mutate({ skip: { modId: mod.modId, version: summary.update!.version } })}
          >
            Skip this version
          </button>
        )}
        {summary.skippedVersion && (
          <button type="button" disabled={prefs.isPending} onClick={() => prefs.mutate({ skip: { modId: mod.modId, version: null } })}>
            Stop skipping {summary.skippedVersion}
          </button>
        )}
        {summary.pageUrl && (
          <button type="button" onClick={() => openUrl.mutate(summary.pageUrl!)}>
            Mod DB page
          </button>
        )}
      </div>
      <label className="checkbox small">
        <input
          type="checkbox"
          checked={!excluded}
          disabled={prefs.isPending}
          onChange={(e) => prefs.mutate({ updateExcluded: { modIds: [mod.modId], on: !e.target.checked } })}
        />
        Include in "Update all"
      </label>

      {info && info.releases.length > 0 && (
        <div className="version-picker">
          <label className="small">
            Install a specific version{" "}
            <select value={picked} onChange={(e) => setPicked(e.target.value)} aria-label="Version">
              <option value="">Choose…</option>
              {info.releases.map((r) => (
                <option key={r.version} value={r.version}>
                  {r.version}
                  {r.version === mod.version ? " (installed)" : ""}
                  {!r.compatible ? " (other game version)" : ""}
                  {cached.has(r.version) ? " (cached)" : ""}
                </option>
              ))}
            </select>
          </label>
          <button
            type="button"
            disabled={!picked || picked === mod.version || props.busy}
            onClick={() => props.onInstallVersion(picked)}
          >
            Install
          </button>
          {pickedRelease && (
            <p className="muted small">
              {pickedRelease.gameVersions.length > 0 ? `For ${summarizeVersions(pickedRelease.gameVersions)}. ` : ""}
              {formatDate(pickedRelease.createdMs)}
            </p>
          )}
        </div>
      )}
      {(picked || summary.update) && (
        <Changelog
          modId={mod.modId}
          installed={mod.version}
          target={picked || summary.update!.version}
        />
      )}
      {details.data?.error && <p className="warn-text small">{details.data.error}</p>}
    </Section>
  );
}
