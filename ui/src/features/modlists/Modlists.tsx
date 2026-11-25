import { useEffect, useMemo, useState, type MouseEvent } from "react";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import type { LoadMode, ModlistKind, ModlistSource, ModlistSummary, Os } from "../../bindings";
import {
  useDeleteModlists,
  useExportModlist,
  useImportModlist,
  useModlists,
  useModsChangedListener,
  useOpenKnownFolder,
  useResolveGroupConflict,
  useSetModlistIcon,
  useSyncSubscription,
  useUnsubscribe,
} from "../../api";
import { formatAgo } from "../../lib/format";
import { canQuickSwitch, filterModlists, kindLabel, SECTIONS, serverCommands, type QuickSwitchPreference } from "./modlistView";
import { SaveModlistDialog } from "./SaveModlistDialog";
import { LoadModlistDialog } from "./LoadModlistDialog";
import { GroupDialog } from "./GroupDialog";
import { ModCards } from "./ModCards";
import { ModlistCard } from "./ModlistCard";
import { IconPicker, iconThemeClass, ModlistIcon } from "./ModlistIcon";
import { CloseDetails, SplitWorkspace, useLayoutPreference } from "../../components/SplitWorkspace";
import type { IconName } from "../../components/Icon";
import { useContextMenu } from "../../components/ContextMenu";

type Dialog =
  | { kind: "save"; as: "modlist" | "preset" }
  | { kind: "load"; source: ModlistSource; listKind: ModlistKind | null; title: string; mode?: LoadMode; quick?: boolean }
  | { kind: "group"; group: ModlistSummary | null; members: string[] }
  | { kind: "icon"; entry: ModlistSummary };

interface Props {
  os: Os;
  enabled: boolean;
  /** A .json/.pdf file dropped on the window, to load. */
  droppedFile: string | null;
  onDroppedFileHandled: () => void;
  /** Select this entry (e.g. after following an online modlist). */
  focusId?: string | null;
  onFocusHandled?: () => void;
}

const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;
const kindIcons: Record<ModlistKind, IconName> = { group: "layers", modlist: "lists", preset: "settings", backup: "history" };

/** The Modlists tab: modlists, presets, groups and backups in one place. */
export function Modlists({ os, enabled, droppedFile, onDroppedFileHandled, focusId, onFocusHandled }: Props) {
  useModsChangedListener();
  const lists = useModlists();
  const del = useDeleteModlists();
  const importFile = useImportModlist();
  const exportList = useExportModlist();
  const openFolder = useOpenKnownFolder();
  const resolve = useResolveGroupConflict();
  const sync = useSyncSubscription();
  const unsubscribe = useUnsubscribe();
  const setIcon = useSetModlistIcon();
  const openMenu = useContextMenu();
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<string[]>([]);
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [flash, setFlash] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<Set<ModlistKind>>(new Set(["backup"]));
  const [rememberedSwitch, setRememberedSwitch] = useLayoutPreference<QuickSwitchPreference>("modlist-quick-switch", "review");
  const quickSwitchEnabled = rememberedSwitch === "keep-configs" || rememberedSwitch === "use-list-configs";
  // The config choice used when Quick switch is turned on again from the toolbar.
  const [lastSwitchConfigs, setLastSwitchConfigs] = useLayoutPreference<QuickSwitchPreference>("modlist-quick-switch-configs", "keep-configs");
  const rememberSwitch = (p: QuickSwitchPreference) => {
    if (p !== "review") setLastSwitchConfigs(p);
    setRememberedSwitch(p);
  };

  useEffect(() => {
    if (droppedFile && enabled) {
      setDialog({ kind: "load", source: { id: null, file: droppedFile, online: null }, listKind: null, title: fileName(droppedFile) });
      onDroppedFileHandled();
    }
  }, [droppedFile, enabled, onDroppedFileHandled]);

  useEffect(() => {
    if (!flash) return;
    const t = setTimeout(() => setFlash(null), 4000);
    return () => clearTimeout(t);
  }, [flash]);

  const all = useMemo(() => lists.data ?? [], [lists.data]);

  useEffect(() => {
    if (!focusId || !lists.data) return;
    const entry = lists.data.find((m) => m.id === focusId);
    if (entry) {
      setCollapsed((c) => new Set([...c].filter((k) => k !== entry.kind)));
      setSelected([focusId]);
    }
    onFocusHandled?.();
  }, [focusId, lists.data, onFocusHandled]);
  const chosen = all.filter((m) => selected.includes(m.id));
  const error = del.error ?? importFile.error ?? exportList.error ?? resolve.error ?? openFolder.error ?? sync.error ?? unsubscribe.error;
  const busy = del.isPending || resolve.isPending || exportList.isPending || sync.isPending || unsubscribe.isPending;

  const select = (id: string, additive: boolean) =>
    setSelected((prev) => (additive ? (prev.includes(id) ? prev.filter((p) => p !== id) : [...prev, id]) : [id]));

  const startLoad = (entry: ModlistSummary, mode?: LoadMode, quick = false) => {
    setDialog({ kind: "load", source: { id: entry.id, file: null, online: null }, listKind: entry.kind, title: entry.name, mode, quick });
  };

  const showEntry = (id: string) => {
    const entry = all.find((m) => m.id === id);
    if (entry) setCollapsed((c) => new Set([...c].filter((k) => k !== entry.kind)));
    setSelected([id]);
  };

  const remove = async (targets: ModlistSummary[]) => {
    const what = targets.length === 1 ? `"${targets[0].name}"` : `${targets.length} items`;
    const ok = await ask(`Move ${what} to the ${os === "windows" ? "Recycle Bin" : "Trash"}?`, {
      title: "Delete",
      kind: "warning",
      okLabel: "Delete",
      cancelLabel: "Cancel",
    });
    if (ok) del.mutate(targets.map((t) => t.id), { onSuccess: () => setSelected([]) });
  };

  const doImport = async () => {
    const path = await open({ multiple: false, filters: [{ name: "Modlists", extensions: ["json", "pdf"] }] });
    if (typeof path !== "string") return;
    importFile.mutate(
      { path, overwrite: false },
      {
        onSuccess: (s) => {
          showEntry(s.id);
          setFlash(`Imported "${s.name}".`);
        },
      },
    );
  };

  const doExport = async (entry: ModlistSummary, format: "json" | "pdf") => {
    const path = await save({
      defaultPath: `${entry.name.replace(/[<>:"/\\|?*]/g, "_")}.${format}`,
      filters: [{ name: format.toUpperCase(), extensions: [format] }],
    });
    if (!path) return;
    exportList.mutate({ id: entry.id, path, format }, { onSuccess: () => setFlash(`Exported to ${path}`) });
  };

  const copy = async (text: string, what: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setFlash(`Copied ${what}.`);
    } catch {
      setFlash("Couldn't use the clipboard.");
    }
  };

  const onCardMenu = (event: MouseEvent<HTMLButtonElement>, entry: ModlistSummary) => {
    const alreadySelected = selected.includes(entry.id);
    const targets = alreadySelected ? chosen : [entry];
    if (!alreadySelected) setSelected([entry.id]);
    if (targets.length > 1) {
      const members = targets.filter((item) => item.kind !== "group").map((item) => item.id);
      openMenu(event, "Selected modlist actions", [[
        { label: "New group from these…", disabled: members.length === 0, run: () => setDialog({ kind: "group", group: null, members }) },
      ], [
        { label: "Delete selected…", disabled: busy, danger: true, run: () => remove(targets) },
      ]]);
      return;
    }
    const quick = quickSwitchEnabled && canQuickSwitch(entry.kind);
    const cannotLoad = busy || !!entry.error;
    openMenu(event, `Actions for ${entry.name}`, [[
      !entry.active && entry.kind !== "preset" && { label: quick ? "Quick switch" : "Switch to this…", disabled: cannotLoad, run: () => startLoad(entry, "replace", quick) },
      { label: "Load…", disabled: cannotLoad, run: () => startLoad(entry) },
      entry.kind === "group" && { label: "Edit group…", disabled: busy, run: () => setDialog({ kind: "group", group: entry, members: [] }) },
      { label: "Change icon…", disabled: busy || !!entry.error, run: () => setDialog({ kind: "icon", entry }) },
    ], [
      { label: "Export JSON…", disabled: cannotLoad, run: () => doExport(entry, "json") },
      { label: "Export PDF…", disabled: cannotLoad, run: () => doExport(entry, "pdf") },
      entry.mods.some((mod) => mod.version) && { label: "Copy server commands", run: () => copy(serverCommands(entry), "server install commands") },
    ], [
      { label: "Delete…", disabled: busy, danger: true, run: () => remove([entry]) },
    ]]);
  };

  if (!enabled) {
    return (
      <p className="empty-state">
        The Vintage Story data folder wasn't found. Pick it on the <strong>Settings</strong> tab.
      </p>
    );
  }

  return (
    <SplitWorkspace name="modlists" widePane>
      <div className="installed-main">
        <div className="mods-toolbar wrap">
          <input
            type="search"
            className="search"
            placeholder="Search names, descriptions and mod ids"
            aria-label="Search modlists"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <div className="header-spacer" />
          <button type="button" className="primary" onClick={() => setDialog({ kind: "save", as: "modlist" })}>
            Save current mods…
          </button>
          <button type="button" onClick={() => void doImport()} disabled={importFile.isPending}>Import file…</button>
          <button
            type="button"
            title="Combine the selected modlists (or pick them in the next step)"
            onClick={() => setDialog({ kind: "group", group: null, members: chosen.filter((c) => c.kind !== "group").map((c) => c.id) })}
          >
            New group…
          </button>
          <button type="button" onClick={() => openFolder.mutate("modlists")}>Open folder</button>
          <label
            className="checkbox toolbar-toggle"
            title={
              lastSwitchConfigs === "use-list-configs"
                ? "Switch to a modlist in one step, without the review dialog. Config files from the modlist are used."
                : "Switch to a modlist in one step, without the review dialog. Your own config files are kept."
            }
          >
            <input
              type="checkbox"
              checked={quickSwitchEnabled}
              onChange={(e) => setRememberedSwitch(e.target.checked ? lastSwitchConfigs : "review")}
            />
            Quick switch
          </label>
        </div>
        {flash && <p className="notice" role="status">{flash}</p>}
        {error && <p className="notice error-text">{error.message}</p>}
        {lists.isPending && <p className="empty-state">Loading modlists…</p>}
        {lists.isError && <p className="empty-state error-text">{lists.error.message}</p>}
        <div className="modlist-scroll">
          {SECTIONS.map((section) => {
            const rows = filterModlists(all, section.kind, search);
            const isCollapsed = collapsed.has(section.kind) && !search;
            return (
              <section key={section.kind} className="modlist-section">
                <button
                  type="button"
                  className="section-toggle"
                  aria-expanded={!isCollapsed}
                  onClick={() =>
                    setCollapsed((c) => {
                      const next = new Set(c);
                      if (next.has(section.kind)) next.delete(section.kind);
                      else next.add(section.kind);
                      return next;
                    })
                  }
                >
                  {isCollapsed ? "▸" : "▾"} {section.title} <span className="muted small">({rows.length})</span>
                </button>
                {!isCollapsed &&
                  (rows.length === 0 ? (
                    <p className="muted small section-empty">{search ? "No matches." : section.empty}</p>
                  ) : (
                    <ul className="card-grid modlist-card-grid" aria-label={section.title}>
                      {rows.map((m) => {
                        const facts = [
                          m.error ? "Can't be read" : `${m.mods.length} mod${m.mods.length === 1 ? "" : "s"}`,
                          m.gameVersion && `VS ${m.gameVersion}`,
                          m.subscription && `Revision ${m.subscription.syncedRevision}`,
                          m.configCount > 0 && `${m.configCount} config${m.configCount === 1 ? "" : "s"}`,
                          m.group && m.group.conflicts.length > 0 && `${m.group.conflicts.length} to check`,
                          !m.active && m.downloadsNeeded > 0 && `${m.downloadsNeeded} to download`,
                        ].filter((fact): fact is string => !!fact);
                        return (
                          <ModlistCard
                            key={m.id}
                            title={m.name}
                            icon={m.icon}
                            iconColor={m.iconColor}
                            kindIcon={kindIcons[m.kind]}
                            facts={facts}
                            date={m.modifiedMs ? formatAgo(m.modifiedMs) : undefined}
                            badges={(m.active || m.subscription) && <>
                              {m.active && <span className="badge active-badge">Active</span>}
                              {m.subscription && <span className={`badge${m.subscription.updateAvailable ? " update-badge" : ""}`}>
                                {m.subscription.updateAvailable ? "Update available" : "Following"}
                              </span>}
                            </>}
                            selected={selected.includes(m.id)}
                            onSelect={(event) => select(m.id, event.ctrlKey || event.metaKey)}
                            onContextMenu={(event) => onCardMenu(event, m)}
                            onLoad={m.error ? undefined : () => startLoad(m)}
                          />
                        );
                      })}
                    </ul>
                  ))}
              </section>
            );
          })}
        </div>
      </div>
      <aside className={`details${chosen.length === 0 ? " empty" : ""}`} aria-label="Modlist details">
        {chosen.length > 0 && <CloseDetails onClose={() => setSelected([])} />}
        {chosen.length === 0 && (
          <p className="muted">
            Select a modlist to see what's in it. Double-click to load. Drop a modlist .json or .pdf on the window to
            load it.
          </p>
        )}
        {chosen.length > 1 && (
          <>
            <div className="details-header">
              <h2>{chosen.length} selected</h2>
            </div>
            <div className="details-actions">
              <button
                type="button"
                disabled={chosen.every((c) => c.kind === "group")}
                onClick={() => setDialog({ kind: "group", group: null, members: chosen.filter((c) => c.kind !== "group").map((c) => c.id) })}
              >
                New group from these…
              </button>
              <button type="button" className="danger" disabled={del.isPending} onClick={() => void remove(chosen)}>Delete…</button>
            </div>
            <ul className="details-list">{chosen.map((c) => <li key={c.id}>{c.name}</li>)}</ul>
          </>
        )}
        {chosen.length === 1 && (
          <ModlistDetails
            entry={chosen[0]}
            onLoad={(mode) => startLoad(chosen[0], mode)}
            onQuickSwitch={quickSwitchEnabled && canQuickSwitch(chosen[0].kind) ? () => startLoad(chosen[0], "replace", true) : undefined}
            onExport={(f) => void doExport(chosen[0], f)}
            onCopy={() => void copy(serverCommands(chosen[0]), "server install commands")}
            onEditGroup={() => setDialog({ kind: "group", group: chosen[0], members: [] })}
            onChangeIcon={() => setDialog({ kind: "icon", entry: chosen[0] })}
            onDelete={() => void remove([chosen[0]])}
            onSelectMember={showEntry}
            onResolve={(modId, reference) => resolve.mutate({ id: chosen[0].id, modId, reference })}
            onSync={() => sync.mutate(chosen[0].id, { onSuccess: (s) => setFlash(`Updated "${s.name}" to the latest online revision.`) })}
            onUnsubscribe={() => unsubscribe.mutate(chosen[0].id, { onSuccess: (s) => setFlash(`"${s.name}" is now a normal modlist.`) })}
            busy={busy}
          />
        )}
      </aside>
      {dialog?.kind === "save" && (
        <SaveModlistDialog
          initialKind={dialog.as}
          existing={all}
          onSaved={(s) => {
            setDialog(null);
            showEntry(s.id);
            setFlash(`Saved "${s.name}".`);
          }}
          onClose={() => setDialog(null)}
        />
      )}
      {dialog?.kind === "load" && (
        <LoadModlistDialog
          os={os}
          source={dialog.source}
          kind={dialog.listKind}
          title={dialog.title}
          initialMode={dialog.mode}
          quickSwitch={dialog.source.id && canQuickSwitch(dialog.listKind) ? {
            start: !!dialog.quick && quickSwitchEnabled,
            preference: quickSwitchEnabled ? rememberedSwitch : "review",
            onRemember: rememberSwitch,
            onDone: () => {
              setDialog(null);
              setFlash(`Switched to "${dialog.title}".`);
            },
          } : undefined}
          onShowBackup={(id) => {
            setDialog(null);
            showEntry(id);
          }}
          onClose={() => setDialog(null)}
        />
      )}
      {dialog?.kind === "icon" && (
        <IconPicker
          current={dialog.entry.icon}
          currentColor={dialog.entry.iconColor}
          fallback={kindIcons[dialog.entry.kind]}
          busy={setIcon.isPending}
          error={setIcon.error?.message}
          onPick={(icon, color) => setIcon.mutate({ id: dialog.entry.id, icon, color }, { onSuccess: () => setDialog(null) })}
          onClose={() => {
            setIcon.reset();
            setDialog(null);
          }}
        />
      )}
      {dialog?.kind === "group" && (
        <GroupDialog
          group={dialog.group}
          all={all}
          initialMembers={dialog.members}
          onSaved={(s) => {
            setDialog(null);
            showEntry(s.id);
          }}
          onClose={() => setDialog(null)}
        />
      )}
    </SplitWorkspace>
  );
}

interface DetailsProps {
  entry: ModlistSummary;
  busy: boolean;
  onLoad: (mode?: LoadMode) => void;
  onQuickSwitch?: () => void;
  onExport: (format: "json" | "pdf") => void;
  onCopy: () => void;
  onEditGroup: () => void;
  onChangeIcon: () => void;
  onDelete: () => void;
  onSelectMember: (id: string) => void;
  onResolve: (modId: string, reference: string) => void;
  onSync: () => void;
  onUnsubscribe: () => void;
}

function ModlistDetails({ entry, busy, onLoad, onQuickSwitch, onExport, onCopy, onEditGroup, onChangeIcon, onDelete, onSelectMember, onResolve, onSync, onUnsubscribe }: DetailsProps) {
  const hasCommands = entry.mods.some((m) => m.version);
  return (
    <>
      <div className="details-header">
        <h2>
          <button
            type="button"
            className={`details-icon-button${iconThemeClass(entry.iconColor)}`}
            title="Change icon"
            aria-label="Change icon"
            disabled={busy || !!entry.error}
            onClick={onChangeIcon}
          >
            <ModlistIcon icon={entry.icon} fallback={kindIcons[entry.kind]} />
          </button>
          {entry.name}
          <span className="badge">{kindLabel(entry.kind)}</span>
          {entry.active && <span className="badge active-badge" title="Your installed mods match this list">Active</span>}
        </h2>
      </div>
      {entry.error && <p className="notice error-text">{entry.error}</p>}
      {entry.subscription && (
        <div className={`notice${entry.subscription.updateAvailable ? "" : " quiet"}`}>
          {entry.subscription.sourceGone
            ? "The author deleted the online modlist this one followed. Your copy still works."
            : entry.subscription.updateAvailable
              ? `The author published revision ${entry.subscription.remoteRevision} (you have ${entry.subscription.syncedRevision}).`
              : `Follows an online modlist (revision ${entry.subscription.syncedRevision}).`}
          <div className="link-row">
            {entry.subscription.updateAvailable && !entry.subscription.sourceGone && (
              <button type="button" className="primary" disabled={busy} onClick={onSync}>Get the update</button>
            )}
            <button type="button" className="link-button small" disabled={busy} onClick={onUnsubscribe}>Stop following</button>
          </div>
        </div>
      )}
      {entry.description && <p className="description">{entry.description}</p>}
      <dl className="details-facts">
        {entry.uploader && (<><dt>Made by</dt><dd>{entry.uploader}</dd></>)}
        {entry.subscription && (<><dt>Revision</dt><dd>{entry.subscription.syncedRevision}</dd></>)}
        {entry.gameVersion && (<><dt>Game version</dt><dd>{entry.gameVersion}</dd></>)}
        {entry.modifiedMs && (<><dt>Saved</dt><dd>{new Date(entry.modifiedMs).toLocaleString()}</dd></>)}
        <dt>File</dt>
        <dd className="small">{entry.id}</dd>
      </dl>
      <div className="details-actions">
        {!entry.active && entry.kind !== "preset" && (
            <button
              type="button"
              className="primary"
              disabled={!!entry.error || busy}
              title={entry.downloadsNeeded ? `${entry.downloadsNeeded} mods need downloading` : "Everything needed is installed or in your mod library"}
              onClick={onQuickSwitch ?? (() => onLoad("replace"))}
            >
              {onQuickSwitch ? "Quick switch" : "Switch to this…"}
            </button>
        )}
        <button
          type="button"
          className={entry.kind === "preset" && !entry.active ? "primary" : undefined}
          disabled={!!entry.error || busy}
          onClick={() => onLoad()}
        >
          Load…
        </button>
        {entry.kind === "group" && <button type="button" disabled={busy} onClick={onEditGroup}>Edit group…</button>}
        <button type="button" disabled={!!entry.error || busy} onClick={() => onExport("pdf")}>Export PDF…</button>
        <button type="button" disabled={!!entry.error || busy} onClick={() => onExport("json")}>Export JSON…</button>
        {hasCommands && (
          <button type="button" title="/moddb install commands for a server console" onClick={onCopy}>
            Copy server commands
          </button>
        )}
        <button type="button" className="danger" disabled={busy} onClick={onDelete}>Delete…</button>
      </div>

      {entry.group && (
        <div className="details-section">
          <h3>Modlists in this group</h3>
          <ul className="details-list">
            {entry.group.members.map((m) => (
              <li key={m.reference}>
                {m.id ? (
                  <button type="button" className="link-button" onClick={() => onSelectMember(m.id!)}>{m.name}</button>
                ) : (
                  m.name
                )}
                {m.missing && <span className="error-text small"> (missing)</span>}
              </li>
            ))}
          </ul>
          {entry.group.conflicts.length > 0 && (
            <>
              <h3>Listed differently</h3>
              <p className="muted small">Pick which modlist's entry to use. Newest version is the default.</p>
              <ul className="conflict-list">
                {entry.group.conflicts.map((c) => (
                  <li key={c.modId}>
                    <label>
                      <span className="conflict-mod">{c.modId}</span>
                      <select
                        value={c.selected}
                        disabled={busy}
                        onChange={(e) => onResolve(c.modId, e.target.value)}
                      >
                        {c.choices.map((ch) => (
                          <option key={ch.reference} value={ch.reference}>
                            {ch.modlistName}: {ch.version ?? "any version"}
                            {ch.hasConfig ? " + config" : ""}
                          </option>
                        ))}
                      </select>
                    </label>
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}

      <ModCards mods={entry.mods} />
    </>
  );
}
