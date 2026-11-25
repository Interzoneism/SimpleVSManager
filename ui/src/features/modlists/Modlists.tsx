import { useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
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
  useUpdateModlistMods,
  useSyncSubscription,
  useUnsubscribe,
} from "../../api";
import { formatAgo } from "../../lib/format";
import { canQuickSwitch, filterModlists, kindLabel, serverCommands, type QuickSwitchPreference } from "./modlistView";
import { SaveModlistDialog } from "./SaveModlistDialog";
import { LoadModlistDialog } from "./LoadModlistDialog";
import { GroupDialog } from "./GroupDialog";
import { ModCards } from "./ModCards";
import { ModlistCard } from "./ModlistCard";
import { ModlistSection } from "./ModlistSection";
import { useOnlineModlistSections, type OnlineModlistDialog } from "../online/useOnlineModlistSections";
import { IconPicker, iconThemeClass, ModlistIcon } from "./ModlistIcon";
import { CloseDetails, SplitWorkspace, useLayoutPreference } from "../../components/SplitWorkspace";
import type { IconName } from "../../components/Icon";
import { useContextMenu } from "../../components/ContextMenu";

type Dialog =
  | OnlineModlistDialog
  | { kind: "save" }
  | { kind: "load"; source: ModlistSource; listKind: ModlistKind | null; title: string; mode?: LoadMode; quick?: boolean }
  | { kind: "group"; group: ModlistSummary | null; members: string[] }
  | { kind: "icon"; entry: ModlistSummary };

interface Props {
  os: Os;
  offline: boolean;
  /** A .json/.pdf file dropped on the window, to load. */
  droppedFile: string | null;
  onDroppedFileHandled: () => void;
}

const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;
const kindIcons: Record<ModlistKind, IconName> = { group: "layers", modlist: "lists", backup: "history" };

type Selection = { kind: "local"; ids: string[] } | { kind: "online"; id: string };

export function Modlists({ os, offline, droppedFile, onDroppedFileHandled }: Props) {
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
  const updateMods = useUpdateModlistMods();
  const openMenu = useContextMenu();
  const [search, setSearch] = useState("");
  const [selection, setSelection] = useState<Selection>({ kind: "local", ids: [] });
  const selected = selection.kind === "local" ? selection.ids : [];
  const selectLocal = (ids: string[]) => setSelection({ kind: "local", ids });
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [flash, setFlash] = useState<string | null>(null);
  const [backupsCollapsed, setBackupsCollapsed] = useLayoutPreference<boolean>("modlists-backups-collapsed", true);
  const [savedCollapsed, setSavedCollapsed] = useLayoutPreference<boolean>("modlists-saved-collapsed", false);
  const [publishedCollapsed, setPublishedCollapsed] = useLayoutPreference<boolean>("modlists-published-collapsed", false);
  const [communityCollapsed, setCommunityCollapsed] = useLayoutPreference<boolean>("modlists-community-collapsed", false);
  const [revealId, setRevealId] = useState<string | null>(null);
  const revealCard = useRef<HTMLButtonElement>(null);
  const [rememberedSwitch, setRememberedSwitch] = useLayoutPreference<QuickSwitchPreference>("modlist-quick-switch", "review");
  const quickSwitchEnabled = rememberedSwitch === "keep-configs" || rememberedSwitch === "use-list-configs";
  // The config choice used when Quick switch is turned on again from the toolbar.
  const [lastSwitchConfigs, setLastSwitchConfigs] = useLayoutPreference<QuickSwitchPreference>("modlist-quick-switch-configs", "keep-configs");
  const rememberSwitch = (p: QuickSwitchPreference) => {
    if (p !== "review") setLastSwitchConfigs(p);
    setRememberedSwitch(p);
  };

  useEffect(() => {
    if (droppedFile) {
      setDialog({ kind: "load", source: { id: null, file: droppedFile, online: null }, listKind: null, title: fileName(droppedFile) });
      onDroppedFileHandled();
    }
  }, [droppedFile, onDroppedFileHandled]);

  useEffect(() => {
    if (!flash) return;
    const t = setTimeout(() => setFlash(null), 4000);
    return () => clearTimeout(t);
  }, [flash]);

  const all = useMemo(() => lists.data ?? [], [lists.data]);

  useEffect(() => {
    if (revealId && revealCard.current) {
      revealCard.current.scrollIntoView?.({ block: "nearest" });
      revealCard.current.focus({ preventScroll: true });
      setRevealId(null);
    }
  }, [all, revealId, search, backupsCollapsed]);

  const chosen = all.filter((m) => selected.includes(m.id));
  const error = del.error ?? importFile.error ?? exportList.error ?? resolve.error ?? openFolder.error ?? sync.error ?? unsubscribe.error ?? updateMods.error;
  const busy = del.isPending || resolve.isPending || exportList.isPending || sync.isPending || unsubscribe.isPending || updateMods.isPending;
  const canUpdateMods = (entry: ModlistSummary) => entry.kind === "modlist" && !entry.subscription && !entry.error;

  const doUpdateMods = async (entry: ModlistSummary) => {
    const ok = await ask(`Replace the mods in "${entry.name}" with the mods you have enabled now? The name, description and icon stay.`, {
      title: "Update with enabled mods",
      okLabel: "Update",
      cancelLabel: "Cancel",
    });
    if (ok) updateMods.mutate(entry.id, { onSuccess: (s) => setFlash(`Updated "${s.name}": ${s.mods.length} mods.`) });
  };

  const select = (id: string, additive: boolean) =>
    selectLocal(additive ? (selected.includes(id) ? selected.filter((p) => p !== id) : [...selected, id]) : [id]);

  const startLoad = (entry: ModlistSummary, mode?: LoadMode, quick = false) => {
    setDialog({ kind: "load", source: { id: entry.id, file: null, online: null }, listKind: entry.kind, title: entry.name, mode, quick });
  };

  const showEntry = (id: string) => {
    if (id.startsWith("Backups/")) setBackupsCollapsed(false);
    setSearch("");
    selectLocal([id]);
    setRevealId(id);
  };

  const online = useOnlineModlistSections({
    offline,
    search,
    selected: selection.kind === "online" ? selection.id : null,
    onSelect: (id) => setSelection({ kind: "online", id }),
    onDeselect: (id) => setSelection((previous) =>
      previous.kind === "online" && previous.id === id ? { kind: "local", ids: [] } : previous,
    ),
    onShowLocal: showEntry,
    onLoad: (id, title) => setDialog({ kind: "load", source: { id: null, file: null, online: id }, listKind: null, title }),
    dialog: dialog?.kind === "publish" || dialog?.kind === "editPublished" ? dialog : null,
    onDialogChange: setDialog,
    publishedCollapsed,
    communityCollapsed,
    onTogglePublished: () => setPublishedCollapsed(!publishedCollapsed),
    onToggleCommunity: () => setCommunityCollapsed(!communityCollapsed),
  });

  const remove = async (targets: ModlistSummary[]) => {
    const what = targets.length === 1 ? `"${targets[0].name}"` : `${targets.length} items`;
    const ok = await ask(`Move ${what} to the ${os === "windows" ? "Recycle Bin" : "Trash"}?`, {
      title: "Delete",
      kind: "warning",
      okLabel: "Delete",
      cancelLabel: "Cancel",
    });
    if (ok) del.mutate(targets.map((t) => t.id), { onSuccess: () => selectLocal([]) });
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
    if (!alreadySelected) selectLocal([entry.id]);
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
      !entry.active && { label: quick ? "Quick switch" : "Switch to this…", disabled: cannotLoad, run: () => startLoad(entry, "replace", quick) },
      { label: "Load…", disabled: cannotLoad, run: () => startLoad(entry) },
      canUpdateMods(entry) && { label: "Update with enabled mods…", disabled: busy, run: () => void doUpdateMods(entry) },
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

  const saved = filterModlists(all, ["modlist", "group"], search).sort((a, b) => a.name.localeCompare(b.name));
  const backups = filterModlists(all, ["backup"], search);
  const hasDetails = chosen.length > 0 || !!online.details;
  const card = (m: ModlistSummary) => {
    const lines = [
      m.group ? <>Group of <strong>{m.group.members.length}</strong> lists</>
        : m.uploader ? <>By <strong>{m.uploader}</strong></>
        : kindLabel(m.kind),
      m.error ? "Can't be read" : <><strong>{m.mods.length}</strong> mod{m.mods.length === 1 ? "" : "s"}</>,
      m.gameVersion ? `VS ${m.gameVersion}` : "Any game version",
    ];
    const facts = [
      m.subscription && `Revision ${m.subscription.syncedRevision}`,
      m.configCount > 0 && `${m.configCount} config${m.configCount === 1 ? "" : "s"}`,
      m.group && m.group.conflicts.length > 0 && `${m.group.conflicts.length} to check`,
      !m.active && m.downloadsNeeded > 0 && `${m.downloadsNeeded} to download`,
    ].filter((fact): fact is string => !!fact);
    return (
      <ModlistCard
        key={m.id}
        buttonRef={m.id === revealId ? revealCard : undefined}
        title={m.name}
        icon={m.icon}
        iconColor={m.iconColor}
        kindIcon={kindIcons[m.kind]}
        rows={lines}
        facts={facts}
        date={m.modifiedMs ? formatAgo(m.modifiedMs) : undefined}
        following={!!m.subscription}
        badges={(m.active || m.subscription?.updateAvailable) && <>
          {m.active && <span className="badge active-badge">Active</span>}
          {m.subscription?.updateAvailable && <span className="badge update-badge">Update available</span>}
        </>}
        selected={selected.includes(m.id)}
        onSelect={(event) => select(m.id, event.ctrlKey || event.metaKey)}
        onContextMenu={(event) => onCardMenu(event, m)}
        onLoad={m.error ? undefined : () => startLoad(m)}
      />
    );
  };

  return (
    <SplitWorkspace name="modlists" widePane>
      <div className="installed-main">
        <div className="mods-toolbar wrap">
          <input
            type="search"
            className="search"
            placeholder="Search all modlists"
            aria-label="Search modlists"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <div className="header-spacer" />
          <button type="button" className="primary" onClick={() => setDialog({ kind: "save" })}>
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
          {!lists.isPending && (
            <ModlistSection
              title="Saved modlists"
              count={saved.length}
              collapsed={savedCollapsed}
              onToggle={() => setSavedCollapsed(!savedCollapsed)}
              empty={saved.length ? undefined : search.trim() ? "No matches." : "Save your current mods, import a file, or create a group."}
            >
              {saved.map(card)}
            </ModlistSection>
          )}
          {!lists.isPending && (
            <ModlistSection
              title="Backups"
              count={backups.length}
              collapsed={backupsCollapsed && !search.trim()}
              onToggle={search.trim() ? undefined : () => setBackupsCollapsed(!backupsCollapsed)}
              empty={backups.length ? undefined : search.trim() ? "No matches." : "Backups are saved automatically before loading a modlist or updating or deleting mods."}
            >
              {backups.map(card)}
            </ModlistSection>
          )}
          {online.sections}
        </div>
      </div>
      <aside className={`details${hasDetails ? "" : " empty"}`} aria-label="Modlist details">
        {hasDetails && <CloseDetails onClose={() => selectLocal([])} />}
        {!hasDetails && (
          <p className="muted">
            Select a modlist to see what's in it. Double-click a card or drop a modlist JSON or PDF here to load it.
          </p>
        )}
        {online.details}
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
            onUpdateMods={canUpdateMods(chosen[0]) ? () => void doUpdateMods(chosen[0]) : undefined}
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
      {online.modal}
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
  onUpdateMods?: () => void;
  onChangeIcon: () => void;
  onDelete: () => void;
  onSelectMember: (id: string) => void;
  onResolve: (modId: string, reference: string) => void;
  onSync: () => void;
  onUnsubscribe: () => void;
}

function ModlistDetails({ entry, busy, onLoad, onQuickSwitch, onExport, onCopy, onEditGroup, onUpdateMods, onChangeIcon, onDelete, onSelectMember, onResolve, onSync, onUnsubscribe }: DetailsProps) {
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
        {!entry.active && (
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
          disabled={!!entry.error || busy}
          onClick={() => onLoad()}
        >
          Load…
        </button>
        {onUpdateMods && !entry.active && (
          <button type="button" disabled={busy} title="Replace its mods with the mods you have enabled now" onClick={onUpdateMods}>Update</button>
        )}
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
