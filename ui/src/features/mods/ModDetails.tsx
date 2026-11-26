import { useState } from "react";
import type { InstalledMod, ModDbSummary, Os } from "../../bindings";
import { useModDbDetails, useOpenModWebsite, useOpenUrl, useRevealMod, useUpdatePreferences } from "../../api";
import { formatCount, formatDate } from "../../lib/format";
import { iconUrl } from "../../lib/iconUrl";
import { canToggleMod, modStatus, statusLabel, type DbIndex } from "./modView";
import { Changelog, summarizeVersions } from "./UpdateDialog";
import { Reports } from "./Reports";
import { FixDependencies } from "./FixDependencies";
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
  /** Shown at the top of the panel, whatever is selected (modlist save/load). */
}

/** Right-hand pane: one mod's details, or batch actions for several. */
export function ModDetails({ os, mods, db, busy, progress, onSetActive, onDelete, onUpdate, onInstallVersion, onEditConfig, onClose }: Props) {
  const prefs = useUpdatePreferences();
  const reveal = useRevealMod();
  const website = useOpenModWebsite();

  if (mods.length === 0) {
    return (
      <aside className="details empty" aria-label="Mod details">
        <div className="details-empty-text">
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
            Leave out of updates
          </button>
          <button type="button" disabled={prefs.isPending} onClick={() => prefs.mutate({ updateExcluded: { modIds: ids, on: false } })}>
            Include in updates
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
  const status = modStatus(mod);
  const summary = db.get(mod.key);
  const missing = new Set(mod.missingDependencies.map((d) => d.modId.toLowerCase()));

  return (
    <aside className="details mod-details">
      <CloseDetails onClose={onClose} />
      <header className="mod-details-head">
        {mod.iconKey ? (
          <img className="details-icon" src={iconUrl(os, mod.iconKey)} alt="" />
        ) : (
          <span className="details-icon placeholder" aria-hidden>{mod.name.slice(0, 1).toUpperCase()}</span>
        )}
        <div className="mod-details-title">
          <h2>{mod.name}</h2>
          <p className="muted small">
            {mod.version ? `v${mod.version}` : "Version unknown"}
            {mod.authors.length > 0 && <> · by {mod.authors.join(", ")}</>}
          </p>
          <span className={`status-pill status-${status}`}>{statusLabel(mod)}</span>
        </div>
      </header>

      {problem && (
        <div className="callout callout-error" role="alert">
          <strong>{status === "error" ? "This mod can't be used" : "This mod won't load"}</strong>
          {mod.missingDependencies.length > 0 ? (
            <ul>
              {mod.missingDependencies.map((d) => (
                <li key={d.modId}>
                  It needs <strong>{d.modId}</strong>
                  {d.version && d.version !== "*" ? ` ${d.version} or newer` : ""}, which isn't installed.
                </li>
              ))}
            </ul>
          ) : (
            <p>{problem}</p>
          )}
        </div>
      )}

      <FixDependencies mod={mod} busy={busy} />

      {mod.description && <p className="mod-details-description">{mod.description}</p>}

      <div className="mod-actions">
        {manageable.length > 0 && (
          <button type="button" className={mod.isActive ? undefined : "primary"} disabled={busy} onClick={() => onSetActive([mod], !mod.isActive)}>
            {mod.isActive ? "Disable" : "Enable"}
          </button>
        )}
        {summary?.update && (
          <button type="button" className="primary" disabled={busy} title={summary.update.message} onClick={() => onUpdate([mod])}>
            <Icon name="update" />Update to {summary.update.version}
          </button>
        )}
        {onEditConfig && mod.modId && (
          <button type="button" title="Edit this mod's config files" onClick={() => onEditConfig(mod.modId)}>
            <Icon name="config" />Config
          </button>
        )}
        <button type="button" title="Show the mod file in your file manager" onClick={() => reveal.mutate(mod.key)}>
          <Icon name="folder" />Folder
        </button>
        {mod.website && (
          <button type="button" title={mod.website} onClick={() => website.mutate(mod.key)}>Website</button>
        )}
        <span className="header-spacer" />
        <button type="button" className="danger" disabled={busy} onClick={() => onDelete([mod])}>Delete…</button>
      </div>
      {progress.get(mod.key) && <p className="muted small">{progress.get(mod.key)}</p>}
      {actionError && <p className="error-text small">{actionError.message}</p>}

      {mod.modId && (
        <ModDbPanel
          mod={mod}
          summary={summary}
          busy={busy}
          onInstallVersion={(v) => onInstallVersion(mod, v)}
        />
      )}

      {mod.modId && mod.version && <Reports modId={mod.modId} version={mod.version} />}

      <Section title="About this file">
        <dl className="details-facts">
          <dt>Mod ID</dt>
          <dd>{mod.modId || "—"}</dd>
          {mod.side && (<><dt>Runs on</dt><dd>{sideLabel(mod.side)}</dd></>)}
          {mod.dependencies.length > 0 && (
            <>
              <dt>Needs</dt>
              <dd>
                <ul className="dependency-list">
                  {mod.dependencies.map((d) => {
                    const isMissing = missing.has(d.modId.toLowerCase());
                    return (
                      <li key={d.modId} className={isMissing ? "missing" : "ok"}>
                        <span aria-hidden="true">{isMissing ? "✕" : "✓"}</span>
                        {d.modId === "game" ? "Vintage Story" : d.modId}
                        {d.version && d.version !== "*" ? ` ${d.version}+` : ""}
                        {isMissing && <span className="sr-only"> (missing)</span>}
                      </li>
                    );
                  })}
                </ul>
              </dd>
            </>
          )}
          <dt>File</dt>
          <dd><code className="path small">{mod.sourcePath}</code></dd>
          {mod.duplicates.length > 0 && (
            <>
              <dt>Other copies</dt>
              <dd>{mod.duplicates.map((d) => <code key={d} className="path small">{d}</code>)}</dd>
            </>
          )}
        </dl>
      </Section>
    </aside>
  );
}

const sideLabel = (side: string) =>
  ({ both: "Client and server", client: "Client only", server: "Server only", universal: "Client and server" } as Record<string, string>)[side.toLowerCase()] ?? side;

function DetailsHeader({ title, onClose }: { title: string; onClose: () => void }) {
  return (
    <div className="details-header">
      <h2>{title}</h2>
      <CloseDetails onClose={onClose} />
    </div>
  );
}

function Section({ title, aside, children }: { title: string; aside?: React.ReactNode; children: React.ReactNode }) {
  return (
    <section className="details-section">
      <div className="details-section-head">
        <h3>{title}</h3>
        {aside}
      </div>
      {children}
    </section>
  );
}

/** Mod DB facts and version actions for one installed mod. */
function ModDbPanel(props: {
  mod: InstalledMod;
  summary: ModDbSummary | undefined;
  busy: boolean;
  onInstallVersion: (version: string) => void;
}) {
  const { mod, summary } = props;
  const details = useModDbDetails(summary ? mod.modId : null);
  const prefs = useUpdatePreferences();
  const openUrl = useOpenUrl();
  const [picked, setPicked] = useState<string>("");
  // Reset the picked version for another mod (while rendering: effects re-run whenever the
  // page is shown again, which would clear the choice).
  const [pickedFor, setPickedFor] = useState(mod.key);
  if (pickedFor !== mod.key) {
    setPickedFor(mod.key);
    setPicked("");
  }

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
  const upToDate = !summary.update && summary.latestVersion && summary.latestVersion === mod.version;

  return (
    <Section
      title="Mod DB"
      aside={summary.pageUrl && (
        <button type="button" className="link-button small" onClick={() => openUrl.mutate(summary.pageUrl!)}>Open page</button>
      )}
    >
      <div className="stat-tiles">
        <div>
          <span>Latest</span>
          <strong>{summary.latestVersion ?? "—"}</strong>
          {upToDate && <em className="ok-text">You have it</em>}
          {summary.update && <em className="accent-text">Update available</em>}
        </div>
        {summary.downloads != null && (
          <div><span>Downloads</span><strong>{formatCount(summary.downloads)}</strong></div>
        )}
        {info?.lastReleasedMs != null && (
          <div><span>Last release</span><strong>{formatDate(info.lastReleasedMs)}</strong></div>
        )}
      </div>
      {summary.latestCompatibleVersion && summary.latestCompatibleVersion !== summary.latestVersion && (
        <p className="muted small">Newest for your game version: {summary.latestCompatibleVersion}</p>
      )}
      {summary.installedMayBeIncompatible && (
        <p className="callout callout-warn small">
          The installed version is made for {summarizeVersions(summary.installedReleaseGameVersions)}, not your game version.
        </p>
      )}

      <label className="setting-line">
        <input
          type="checkbox"
          checked={!excluded}
          disabled={prefs.isPending}
          onChange={(e) => prefs.mutate({ updateExcluded: { modIds: [mod.modId], on: !e.target.checked } })}
        />
        <span>
          Include in updates
          <span className="muted small">Turn off to keep this version when you update everything.</span>
        </span>
      </label>
      {(summary.update || summary.skippedVersion) && (
        <div className="link-row">
          {summary.update && (
            <button
              type="button"
              className="link-button small"
              disabled={prefs.isPending}
              title="Don't offer this version again; newer ones will still be offered"
              onClick={() => prefs.mutate({ skip: { modId: mod.modId, version: summary.update!.version } })}
            >
              Skip {summary.update.version}
            </button>
          )}
          {summary.skippedVersion && (
            <button type="button" className="link-button small" disabled={prefs.isPending} onClick={() => prefs.mutate({ skip: { modId: mod.modId, version: null } })}>
              Stop skipping {summary.skippedVersion}
            </button>
          )}
        </div>
      )}

      {info && info.releases.length > 0 && (
        <div className="version-picker">
          <label className="small muted" htmlFor="other-version">Other versions</label>
          <div className="version-picker-row">
            <select id="other-version" value={picked} onChange={(e) => setPicked(e.target.value)}>
              <option value="">Choose a version…</option>
              {info.releases.map((r) => (
                <option key={r.version} value={r.version}>
                  {r.version}
                  {r.version === mod.version ? " (installed)" : ""}
                  {!r.compatible ? " (other game version)" : ""}
                  {cached.has(r.version) ? " (cached)" : ""}
                </option>
              ))}
            </select>
            <button type="button" disabled={!picked || picked === mod.version || props.busy} onClick={() => props.onInstallVersion(picked)}>
              Install
            </button>
          </div>
          {pickedRelease && (
            <p className="muted small">
              {pickedRelease.gameVersions.length > 0 ? `For ${summarizeVersions(pickedRelease.gameVersions)}. ` : ""}
              Released {formatDate(pickedRelease.createdMs)}.
            </p>
          )}
        </div>
      )}
      {(picked || summary.update) && (
        <Changelog modId={mod.modId} installed={mod.version} target={picked || summary.update!.version} collapsed />
      )}
      {details.data?.error && <p className="warn-text small">{details.data.error}</p>}
    </Section>
  );
}
