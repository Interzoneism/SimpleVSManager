import { useCallback, useState } from "react";
import { ask, open } from "@tauri-apps/plugin-dialog";
import type { BackupProgress, DetectedEnvironment } from "../../bindings";
import {
  useBackupProgress,
  useChangeProfile,
  useCreateDataBackup,
  useDataBackups,
  useDeleteDataBackup,
  useMorePreferences,
  useRestoreDataBackup,
} from "../../api";
import { formatBytes } from "../../lib/format";

/** Game profiles: separate game/data folders, switched in one click. */
export function ProfileSettings({ env }: { env: DetectedEnvironment }) {
  const change = useChangeProfile();
  const s = env.settings;
  const names = [s.activeProfile, ...s.profiles.map((p) => p.name).filter((n) => n !== s.activeProfile)];
  const [newName, setNewName] = useState("");
  const [renaming, setRenaming] = useState<string | null>(null);

  return (
    <section className="settings-section" aria-labelledby="profiles-heading">
      <h2 id="profiles-heading">Game profiles</h2>
      <p className="muted small">
        Each profile has its own game folder, data folder (with its own mods) and target game version. Handy for a
        modded and a vanilla setup, or several game versions.
      </p>
      <div className="setting-row">
        <label htmlFor="profile">Profile</label>
        <select id="profile" value={s.activeProfile} disabled={change.isPending} onChange={(e) => change.mutate({ action: "switch", name: e.target.value })}>
          {names.map((n) => <option key={n} value={n}>{n}</option>)}
        </select>
        {renaming === null ? (
          <button type="button" onClick={() => setRenaming(s.activeProfile)}>Rename</button>
        ) : (
          <>
            <input type="text" aria-label="New name" value={renaming} onChange={(e) => setRenaming(e.target.value)} />
            <button type="button" className="primary" onClick={() => change.mutate({ action: "rename", from: s.activeProfile, to: renaming }, { onSuccess: () => setRenaming(null) })}>
              Save
            </button>
            <button type="button" onClick={() => setRenaming(null)}>Cancel</button>
          </>
        )}
      </div>
      <div className="setting-row">
        <input type="text" placeholder="New profile name" aria-label="New profile name" value={newName} onChange={(e) => setNewName(e.target.value)} />
        <button
          type="button"
          disabled={!newName.trim() || change.isPending}
          title="Creates the profile with auto-detected folders and switches to it"
          onClick={() => change.mutate({ action: "switch", name: newName }, { onSuccess: () => setNewName("") })}
        >
          Add profile
        </button>
        {names.length > 1 && (
          <select
            aria-label="Delete a profile"
            value=""
            onChange={async (e) => {
              const name = e.target.value;
              if (name && (await ask(`Delete the profile "${name}"? Its folders aren't touched.`, { title: "Delete profile", kind: "warning" })))
                change.mutate({ action: "delete", name });
            }}
          >
            <option value="">Delete a profile…</option>
            {names.filter((n) => n !== s.activeProfile).map((n) => <option key={n} value={n}>{n}</option>)}
          </select>
        )}
      </div>
      {change.isError && <p className="error-text small">{change.error.message}</p>}
    </section>
  );
}

/** Whole data folder backups (saves, settings, mods), compatible with 2.x. */
export function BackupSettings({ env, os }: { env: DetectedEnvironment; os: string }) {
  const s = env.settings;
  const prefs = useMorePreferences();
  const list = useDataBackups();
  const create = useCreateDataBackup();
  const restore = useRestoreDataBackup();
  const del = useDeleteDataBackup();
  const [progress, setProgress] = useState<BackupProgress | null>(null);
  useBackupProgress(useCallback((p: BackupProgress) => setProgress(p), []));
  const busy = create.isPending || restore.isPending || del.isPending;
  const error = prefs.error ?? create.error ?? restore.error ?? del.error ?? list.error;

  const pickLocation = async () => {
    const chosen = await open({ directory: true, multiple: false, title: "Where to keep data folder backups" });
    if (typeof chosen === "string") prefs.mutate({ dataBackupLocation: chosen });
  };

  const doRestore = async (id: string, when: string, sameFolder: boolean) => {
    const ok = await ask(
      `Replace everything in the data folder with the backup from ${when}?${sameFolder ? "" : "\n\nThis backup was made from a different data folder."}\n\nYour current data folder is backed up first, so you can go back.`,
      { title: "Restore backup", kind: "warning", okLabel: "Restore", cancelLabel: "Cancel" },
    );
    if (ok) restore.mutate({ id, safety: true });
  };

  return (
    <section className="settings-section" aria-labelledby="backups-heading">
      <h2 id="backups-heading">Data folder backups</h2>
      <p className="muted small">
        A backup holds the whole VintagestoryData folder: worlds, settings, mods and configs. Worlds that didn't change are
        stored only once. Simple VS Manager 2.x can restore these too.
      </p>
      <label className="checkbox-row">
        <input type="checkbox" checked={s.autoDataBackups} disabled={prefs.isPending} onChange={(e) => prefs.mutate({ autoDataBackups: e.target.checked })} />
        Back up before every launch (big worlds make starting slower)
      </label>
      <div className="setting-row">
        <label htmlFor="max-backups">Keep</label>
        <select id="max-backups" value={s.maxDataBackups} onChange={(e) => prefs.mutate({ maxDataBackups: Number(e.target.value) })}>
          {[0, 3, 5, 10, 20].map((n) => <option key={n} value={n}>{n === 0 ? "all backups" : `the newest ${n}`}</option>)}
        </select>
        <span className="muted small">per data folder and game version</span>
      </div>
      <div className="setting-row">
        <span>Location</span>
        <code className="path small">{list.data?.location ?? "…"}</code>
        <button type="button" onClick={() => void pickLocation()}>Change…</button>
        {s.dataBackupLocation && <button type="button" onClick={() => prefs.mutate({ dataBackupLocation: "" })}>Use default</button>}
      </div>
      <div className="setting-row">
        <button type="button" className="primary" disabled={busy} onClick={() => create.mutate()}>Back up now</button>
        {busy && progress && (
          <span className="muted small">
            {Math.round(progress.percent)}% · {progress.label}
          </span>
        )}
        {restore.isSuccess && <span className="ok-text small">Restored. The previous state was saved as a backup.</span>}
      </div>
      {error && <p className="error-text small">{error.message}</p>}
      {list.data && list.data.backups.length > 0 && (
        <table className="backup-table">
          <thead>
            <tr><th>Made</th><th>Game</th><th>Size</th><th>From</th><th /></tr>
          </thead>
          <tbody>
            {list.data.backups.map((b) => {
              const when = b.createdMs ? new Date(b.createdMs).toLocaleString() : b.id;
              const sameFolder = !!b.sourceDataDirectory && !!list.data.dataDir && samePath(b.sourceDataDirectory, list.data.dataDir, os);
              return (
                <tr key={b.id}>
                  <td>{when}</td>
                  <td>{b.gameVersion ?? "?"}</td>
                  <td>{formatBytes(b.sizeBytes)}{b.saveCount > 0 && ` + ${b.saveCount} worlds`}</td>
                  <td className="small">{sameFolder ? "this data folder" : b.sourceDataDirectory ?? "?"}</td>
                  <td className="backup-actions">
                    <button type="button" disabled={busy} onClick={() => void doRestore(b.id, when, sameFolder)}>Restore…</button>{" "}
                    <button
                      type="button"
                      className="danger"
                      disabled={busy}
                      onClick={async () => (await ask(`Delete the backup from ${when}? This can't be undone.`, { title: "Delete backup", kind: "warning" })) && del.mutate(b.id)}
                    >
                      Delete
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
      {list.data && list.data.backups.length === 0 && <p className="muted small">No backups yet.</p>}
    </section>
  );
}

function samePath(a: string, b: string, os: string) {
  const norm = (p: string) => p.replace(/[\\/]+$/, "").replace(/\\/g, "/");
  return os === "windows" ? norm(a).toLowerCase() === norm(b).toLowerCase() : norm(a) === norm(b);
}

/** Theme and logging. */
export function AppearanceSettings({ env }: { env: DetectedEnvironment }) {
  const prefs = useMorePreferences();
  const s = env.settings;
  const hasCustom = Object.keys(s.customPalette).length > 0;
  return (
    <section className="settings-section" aria-labelledby="appearance-heading">
      <h2 id="appearance-heading">Appearance and logging</h2>
      <div className="setting-row">
        <label htmlFor="theme">Theme</label>
        <select id="theme" value={s.theme} onChange={(e) => prefs.mutate({ theme: e.target.value })}>
          <option value="darkVs">Dark VS</option>
          <option value="dark">Dark</option>
          <option value="light">Light</option>
          {hasCustom && <option value="custom">Your 2.x colors</option>}
        </select>
      </div>
      <label className="checkbox-row">
        <input type="checkbox" checked={s.verboseLogging} onChange={(e) => prefs.mutate({ verboseLogging: e.target.checked })} />
        Detailed logging (for troubleshooting; takes effect at the next start)
      </label>
      {prefs.isError && <p className="error-text small">{prefs.error.message}</p>}
    </section>
  );
}
