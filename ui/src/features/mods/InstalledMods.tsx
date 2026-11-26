import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import type { InstalledMod, Os, UpdateProgress } from "../../bindings";
import {
  useDeleteMods,
  useModDb,
  useModlists,
  useMods,
  useModsChangedListener,
  useOpenModWebsite,
  useRefreshModDb,
  useRevealMod,
  useSetModsActive,
  useUpdateMods,
  useUpdateProgress,
} from "../../api";
import { formatAgo } from "../../lib/format";
import {
  bulkUpdateKeys,
  canToggleMod,
  filterMods,
  sortMods,
  summarize,
  type DbIndex,
  type SortDirection,
  type SortKey,
  type StatusFilter,
} from "./modView";
import { ModTable, ROW_SIZES, type RowSize } from "./ModTable";
import { ModDetails } from "./ModDetails";
import { UpdateDialog } from "./UpdateDialog";
import { SaveModlistDialog } from "../modlists/SaveModlistDialog";
import { LoadModlistDialog } from "../modlists/LoadModlistDialog";
import { ModlistPicker } from "../modlists/ModlistPicker";
import { canQuickSwitch } from "../modlists/modlistView";
import { quickSwitchNotice, showQuickSwitchToast, useQuickSwitch } from "../modlists/quickSwitch";
import type { ModlistSummary } from "../../bindings";
import { Icon, type IconName } from "../../components/Icon";
import { SplitWorkspace, useLayoutPreference } from "../../components/SplitWorkspace";
import { copyText, useContextMenu } from "../../components/ContextMenu";
import { toast, useErrorToast } from "../../components/Toast";

/** Short progress label for the table, from an UpdateProgress event. */
export function progressLabel(p: UpdateProgress): string | null {
  switch (p.stage) {
    case "queued":
      return "Waiting…";
    case "downloading":
      return p.percent != null ? `Downloading ${Math.round(p.percent)}%` : (p.message ?? "Downloading…");
    case "installing":
      return "Installing…";
    default:
      return null;
  }
}

/** The "Installed mods" screen: toolbar, virtualized table and details pane. */
export function InstalledMods({ os, enabled, onEditConfig, onBrowse }: {
  os: Os;
  enabled: boolean;
  onEditConfig?: (modId: string) => void;
  onBrowse: () => void;
}) {
  useModsChangedListener();
  const mods = useMods(enabled);
  const setActive = useSetModsActive();
  const deleteMods = useDeleteMods();
  const moddb = useModDb(mods.dataUpdatedAt);
  const refreshDb = useRefreshModDb();
  const updateMods = useUpdateMods();
  const reveal = useRevealMod();
  const website = useOpenModWebsite();
  const openMenu = useContextMenu();

  const [search, setSearch] = useState("");
  const [rowSize, setRowSize] = useLayoutPreference<RowSize>("installed-row-size", "medium");
  const [status, setStatus] = useState<StatusFilter>("all");
  const [sortKey, setSortKey] = useState<SortKey>("name");
  const [sortDirection, setSortDirection] = useState<SortDirection>("asc");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  // Snapshot of the mods in the update dialog (keys change when an update renames a file).
  const [dialogMods, setDialogMods] = useState<InstalledMod[] | null>(null);
  const [progress, setProgress] = useState<Map<string, string>>(new Map());
  const modlists = useModlists();
  const [savingModlist, setSavingModlist] = useState(false);
  const [loadingModlist, setLoadingModlist] = useState<ModlistSummary | null>(null);
  const [picking, setPicking] = useState(false);
  const quickSwitch = useQuickSwitch();
  const loadable = (modlists.data ?? []).filter((m) => m.kind !== "backup" && !m.error);

  // Check the Mod DB once per session, after the first scan.
  const checked = useRef(false);
  useEffect(() => {
    if (mods.isSuccess && !checked.current) {
      checked.current = true;
      refreshDb.mutate("refresh");
    }
  }, [mods.isSuccess, refreshDb]);

  const onProgress = useCallback((p: UpdateProgress) => {
    setProgress((prev) => {
      const next = new Map(prev);
      const label = progressLabel(p);
      if (label) next.set(p.key, label);
      else next.delete(p.key);
      return next;
    });
  }, []);
  useUpdateProgress(onProgress);

  const all = useMemo(() => mods.data?.mods ?? [], [mods.data]);
  const db: DbIndex = useMemo(
    () => new Map((moddb.data?.entries ?? []).map((e) => [e.key, e.summary])),
    [moddb.data],
  );
  const visible = useMemo(
    () => sortMods(filterMods(all, search, status, db), sortKey, sortDirection, db),
    [all, search, status, sortKey, sortDirection, db],
  );
  const summary = useMemo(() => summarize(all), [all]);
  const updatable = useMemo(() => bulkUpdateKeys(all, db), [all, db]);
  const updateCount = all.filter((m) => db.get(m.key)?.update).length;
  const enabledCount = all.filter((m) => m.isActive).length;
  const selectedMods = all.filter((m) => selected.has(m.key));
  // Toggling doesn't lock the table: toggles are queued and shown right away.
  const busy = deleteMods.isPending || updateMods.isPending;
  const actionError = setActive.error ?? deleteMods.error ?? updateMods.error ?? reveal.error ?? website.error;
  useErrorToast(actionError);

  const onSort = (key: SortKey) => {
    if (key === sortKey) setSortDirection((d) => (d === "asc" ? "desc" : "asc"));
    else {
      setSortKey(key);
      setSortDirection("asc");
    }
  };

  const toggle = (targets: InstalledMod[], active: boolean) =>
    setActive.mutate({ keys: targets.map((m) => m.key), active });

  const remove = async (targets: InstalledMod[]) => {
    const what = targets.length === 1 ? `"${targets[0].name}"` : `${targets.length} mods`;
    const ok = await ask(`Move ${what} to the ${os === "windows" ? "Recycle Bin" : "Trash"}?`, {
      title: "Delete mods",
      kind: "warning",
      okLabel: "Delete",
      cancelLabel: "Cancel",
    });
    if (!ok) return;
    deleteMods.mutate(
      targets.map((m) => m.key),
      { onSuccess: () => setSelected(new Set()) },
    );
  };

  const onRowMenu = (event: MouseEvent<HTMLDivElement>, mod: InstalledMod) => {
    const targets = selected.has(mod.key) ? selectedMods : [mod];
    const multiple = targets.length > 1;
    const enable = targets.filter((item) => canToggleMod(item) && !item.isActive);
    const disable = targets.filter((item) => canToggleMod(item) && item.isActive);
    const updates = targets.filter((item) => db.get(item.key)?.update);
    openMenu(event, multiple ? "Selected mod actions" : `Actions for ${mod.name}`, [[
      enable.length > 0 && { label: multiple ? "Enable selected" : "Enable", disabled: busy, run: () => toggle(enable, true) },
      disable.length > 0 && { label: multiple ? "Disable selected" : "Disable", disabled: busy, run: () => toggle(disable, false) },
      updates.length > 0 && { label: multiple ? "Update selected…" : "Update…", disabled: busy, run: () => setDialogMods(updates) },
    ], [
      !multiple && { label: "Show in folder", run: () => reveal.mutate(mod.key) },
      !multiple && !!mod.website && { label: "Open website", run: () => website.mutate(mod.key) },
      !multiple && !!onEditConfig && !!mod.modId && { label: "Edit config…", run: () => onEditConfig(mod.modId) },
      { label: multiple ? "Copy file paths" : "Copy file path", run: () => copyText(targets.map((item) => item.sourcePath).join("\n")) },
    ], [
      { label: multiple ? "Delete selected…" : "Delete…", danger: true, disabled: busy, run: () => remove(targets) },
    ]]);
  };

  /** Installs specific versions right away (version picker, single update pill). */
  const installVersions = (requests: { key: string; version: string | null }[]) => updateMods.mutate(requests);

  const dbProblem = refreshDb.error?.message ?? moddb.data?.error ?? null;
  useEffect(() => {
    if (dbProblem) toast.warning(`Some mods couldn't be checked: ${dbProblem}`, { id: "moddb-check", title: "Mod DB check incomplete" });
  }, [dbProblem]);

  if (!enabled) {
    return (
      <p className="empty-state">
        Data folder not found. Set it in <strong>Settings</strong>.
      </p>
    );
  }
  if (mods.isPending) return <p className="empty-state">Loading mods…</p>;
  if (mods.isError) return <p className="empty-state error-text">Could not load mods: {mods.error.message}</p>;

  const dbState = moddb.data;
  const checking = refreshDb.isPending;
  const lastChecked = dbState?.entries.reduce((max, e) => Math.max(max, e.summary.fetchedMs), 0) ?? 0;
  const filters: { value: StatusFilter; label: string; count: number; icon: IconName }[] = [
    { value: "all", label: "Installed", count: summary.total, icon: "library" },
    { value: "active", label: "Enabled", count: enabledCount, icon: "check" },
    { value: "disabled", label: "Disabled", count: summary.total - enabledCount, icon: "panel" },
    { value: "updates", label: "Updates", count: updateCount, icon: "update" },
    { value: "problems", label: "Problems", count: summary.problems, icon: "warning" },
  ];

  return (
    <div className="workspace-page">
      <div className="library-overview" role="group" aria-label="Filter installed mods by status">
        {filters.map((filter) => (
          <button
            key={filter.value}
            type="button"
            className={`library-stat stat-${filter.value}`}
            aria-pressed={status === filter.value}
            onClick={() => setStatus(filter.value)}
          >
            <span className="stat-icon"><Icon name={filter.icon} /></span>
            <span className="stat-body"><strong>{filter.count}</strong><span>{filter.label}</span></span>
          </button>
        ))}
      </div>
      <SplitWorkspace name="installed-mods">
        <div className="installed-main">
          <div className="mods-toolbar wrap library-toolbar">
            <label className="search-field">
              <Icon name="search" />
              <input
                type="search"
                className="search"
                placeholder="Search mods"
                aria-label="Search installed mods"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
            </label>
            <span className="muted small results-count" aria-live="polite">
              {visible.length === all.length ? "" : `${visible.length} of ${all.length} mods`}
            </span>
            <div className="segmented row-size" role="group" aria-label="Row size">
              {(Object.keys(ROW_SIZES) as RowSize[]).map((key) => (
                <button
                  key={key}
                  type="button"
                  className="small"
                  title={`${ROW_SIZES[key].label} rows`}
                  aria-pressed={rowSize === key}
                  onClick={() => setRowSize(key)}
                >
                  {ROW_SIZES[key].label[0]}
                </button>
              ))}
            </div>
            <div className="modlist-quick-actions" role="group" aria-label="Modlists">
              <button type="button" disabled={loadable.length === 0} title={loadable.length ? undefined : "No saved modlists"} onClick={() => setPicking(true)}>
                <Icon name="lists" />Load a modlist…
              </button>
              <button type="button" onClick={() => setSavingModlist(true)}>
                <Icon name="save" />Save current mods…
              </button>
            </div>
            <div className="header-spacer" />
            <span className={`muted small update-check-status${checking ? " checking" : ""}`} aria-live="polite">
              {checking
                ? "Checking the Mod DB…"
                : dbState?.offline
                  ? "Offline mode"
                  : lastChecked > 0
                    ? `Checked ${formatAgo(lastChecked)}`
                    : ""}
            </span>
            <button
              type="button"
              disabled={checking || busy || dbState?.offline}
              title={dbState?.offline ? "Offline mode is on" : undefined}
              onClick={() => refreshDb.mutate("force")}
            >
              <Icon name="refresh" />Check for updates
            </button>
            <button
              type="button"
              className="primary"
              disabled={updatable.length === 0 || busy}
              onClick={() => setDialogMods(all.filter((m) => updatable.includes(m.key)))}
            >
              <Icon name="update" />Manage updates{updatable.length > 0 ? ` (${updatable.length})` : ""}
            </button>
          </div>
          {!mods.data.clientSettingsFound && (
            <p className="notice">
              clientsettings.json wasn't found in the data folder. Start the game once so it creates it; until then
              enabling and disabling mods writes a new one.
            </p>
          )}
          <ModTable
            os={os}
            mods={visible}
            db={db}
            progress={progress}
            onUpdate={(m) => setDialogMods([m])}
            selected={selected}
            onSelectionChange={setSelected}
            sortKey={sortKey}
            sortDirection={sortDirection}
            onSort={onSort}
            onToggleActive={(m) => toggle([m], !m.isActive)}
            onRowMenu={onRowMenu}
            busy={busy}
            rowSize={rowSize}
            empty={all.length === 0 ? (
              <div className="empty-state">
                <p>No mods installed yet.</p>
                <button type="button" onClick={onBrowse}><Icon name="compass" />Browse the Mod DB</button>
              </div>
            ) : (
              <div className="empty-state">
                <p>No mods match.</p>
                <button type="button" onClick={() => { setSearch(""); setStatus("all"); }}><Icon name="eraser" />Clear filters</button>
              </div>
            )}
          />
        </div>
        <ModDetails
          os={os}
          mods={selectedMods}
          db={db}
          busy={busy}
          progress={progress}
          onSetActive={toggle}
          onDelete={remove}
          onUpdate={(targets) => setDialogMods(targets)}
          onInstallVersion={(m, version) => installVersions([{ key: m.key, version }])}
          onEditConfig={onEditConfig}
          onClose={() => setSelected(new Set())}
        />
        {savingModlist && (
          <SaveModlistDialog
            existing={modlists.data ?? []}
            onSaved={(s) => {
              setSavingModlist(false);
              toast.success(`Saved "${s.name}" (${s.mods.length} mods).`);
            }}
            onClose={() => setSavingModlist(false)}
          />
        )}
        {picking && (
          <ModlistPicker
            lists={loadable}
            onPick={(entry) => {
              setPicking(false);
              setLoadingModlist(entry);
            }}
            onClose={() => setPicking(false)}
          />
        )}
        {loadingModlist && (
          <LoadModlistDialog
            os={os}
            source={{ id: loadingModlist.id, file: null, online: null }}
            kind={loadingModlist.kind}
            title={loadingModlist.name}
            quickSwitch={canQuickSwitch(loadingModlist.kind) ? {
              start: quickSwitch.enabled,
              preference: quickSwitch.enabled ? quickSwitch.preference : "review",
              onRemember: quickSwitch.remember,
              onDone: (report) => {
                showQuickSwitchToast(quickSwitchNotice(loadingModlist.name, report, os));
                setLoadingModlist(null);
              },
            } : undefined}
            onShowBackup={() => setLoadingModlist(null)}
            onClose={() => setLoadingModlist(null)}
          />
        )}
        {dialogMods && (
          <UpdateDialog
            mods={dialogMods}
            db={db}
            progress={progress}
            running={updateMods.isPending}
            report={updateMods.data ?? null}
            onStart={(keys) => updateMods.mutate(keys.map((key) => ({ key, version: null })))}
            onClose={() => {
              setDialogMods(null);
              updateMods.reset();
            }}
          />
        )}
      </SplitWorkspace>
    </div>
  );
}
