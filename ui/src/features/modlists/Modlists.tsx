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
import { canQuickSwitch, filterModlists, kindLabel, serverCommands, sortSaved } from "./modlistView";
import { LoadModlistDialog } from "./LoadModlistDialog";
import { quickSwitchNotice, showQuickSwitchToast, useQuickSwitch, useShowServerCommands } from "./quickSwitch";
import { GroupDialog } from "./GroupDialog";
import { ModCards } from "./ModCards";
import { ModlistCard } from "./ModlistCard";
import { ModlistSection } from "./ModlistSection";
import { useOnlineModlistSections } from "../online/useOnlineModlistSections";
import { canBePublished, usePublishing, type Publishing } from "../online/usePublishing";
import { IconPicker, iconThemeClass, ModlistIcon } from "./ModlistIcon";
import { CloseDetails, SplitWorkspace, useLayoutPreference } from "../../components/SplitWorkspace";
import type { IconName } from "../../components/Icon";
import { useContextMenu } from "../../components/ContextMenu";
import { toast, useErrorToast } from "../../components/Toast";
import { Icon } from "../../components/Icon";

type Dialog =
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
  const [backupsCollapsed, setBackupsCollapsed] = useLayoutPreference<boolean>("modlists-backups-collapsed", true);
  const [savedCollapsed, setSavedCollapsed] = useLayoutPreference<boolean>("modlists-saved-collapsed", false);
  const [communityCollapsed, setCommunityCollapsed] = useLayoutPreference<boolean>("modlists-community-collapsed", false);
  const [revealId, setRevealId] = useState<string | null>(null);
  const revealCard = useRef<HTMLButtonElement>(null);
  const { enabled: quickSwitchEnabled, preference: rememberedSwitch, remember: rememberSwitch } = useQuickSwitch();

  useEffect(() => {
    if (droppedFile) {
      setDialog({ kind: "load", source: { id: null, file: droppedFile, online: null }, listKind: null, title: fileName(droppedFile) });
      onDroppedFileHandled();
    }
  }, [droppedFile, onDroppedFileHandled]);

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
  useErrorToast(error);
  const publishing = usePublishing({ offline, onFlash: toast.success });
  const busy = del.isPending || resolve.isPending || exportList.isPending || sync.isPending || unsubscribe.isPending || updateMods.isPending || publishing.busy;
  const canUpdateMods = (entry: ModlistSummary) => entry.kind === "modlist" && !entry.subscription && !entry.error;

  const doUpdateMods = async (entry: ModlistSummary) => {
    const followers = publishing.slotFor(entry.published?.registryId)?.subscribers ?? 0;
    const ok = await ask(
      `Replace the mods in "${entry.name}" with the mods you have enabled now? The name, description and icon stay.` +
        (entry.published
          ? ` It's published, so the changes are published too${followers ? ` and your ${followers} follower${followers === 1 ? "" : "s"} get an update` : ""}.`
          : ""),
      { title: "Update with enabled mods", okLabel: "Update", cancelLabel: "Cancel" },
    );
    if (!ok) return;
    updateMods.mutate(entry.id, {
      onSuccess: (s) => {
        toast.success(`Updated "${s.name}": ${s.mods.length} mods.`);
        if (s.published?.changed && !publishing.blocked) publishing.publish(s);
      },
    });
  };

  const changeIcon = (entry: ModlistSummary, icon: string | null, color: string | null) =>
    setIcon.mutate({ id: entry.id, icon, color }, {
      onSuccess: (s) => {
        setDialog(null);
        // Keep a published list in step, unless it has other changes that aren't published yet.
        if (entry.published && !entry.published.changed && s.published?.changed && !publishing.blocked) publishing.publish(s);
      },
    });

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
    communityCollapsed,
    onToggleCommunity: () => setCommunityCollapsed(!communityCollapsed),
  });

  const remove = async (targets: ModlistSummary[]) => {
    const what = targets.length === 1 ? `"${targets[0].name}"` : `${targets.length} items`;
    const published = targets.filter((t) => t.published);
    const bin = os === "windows" ? "Recycle Bin" : "Trash";
    if (published.length && publishing.blocked) {
      toast.warning(`Published modlists can only be deleted while the cloud is reachable, since they are unpublished first. ${publishing.blocked}`);
      return;
    }
    const names = published.map((t) => `"${t.name}"`).join(", ");
    const ok = await ask(
      published.length
        ? `Move ${what} to the ${bin}? ${names} ${published.length === 1 ? "is" : "are"} published and will be unpublished first: removed from the cloud, while followers keep their copies.`
        : `Move ${what} to the ${bin}?`,
      { title: "Delete", kind: "warning", okLabel: published.length ? "Unpublish and delete" : "Delete", cancelLabel: "Cancel" },
    );
    if (!ok) return;
    for (const t of published) if (!(await publishing.unpublish(t, true))) return;
    del.mutate(targets.map((t) => t.id), { onSuccess: () => selectLocal([]) });
  };

  const doImport = async () => {
    const path = await open({ multiple: false, filters: [{ name: "Modlists", extensions: ["json", "pdf"] }] });
    if (typeof path !== "string") return;
    importFile.mutate(
      { path, overwrite: false },
      {
        onSuccess: (s) => {
          showEntry(s.id);
          toast.success(`Imported "${s.name}".`);
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
    exportList.mutate({ id: entry.id, path, format }, { onSuccess: () => toast.success(`Exported to ${path}`) });
  };

  const copy = async (text: string, what: string) => {
    try {
      await navigator.clipboard.writeText(text);
      toast.success(`Copied ${what}.`);
    } catch {
      toast.error("Couldn't use the clipboard.");
    }
  };

  const publishMenu = (entry: ModlistSummary) => {
    const off = busy || !!publishing.blocked;
    if (entry.published) {
      return [
        entry.published.changed && { label: "Publish changes", disabled: off, run: () => publishing.publish(entry) },
        { label: "Edit online details…", disabled: off, run: () => publishing.editDetails(entry) },
        { label: "Unpublish…", disabled: off, run: () => void publishing.unpublish(entry) },
      ];
    }
    return [canBePublished(entry) && { label: "Publish", disabled: off, run: () => publishing.publish(entry) }];
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
      !entry.subscription && { label: "Change icon…", disabled: busy || !!entry.error, run: () => setDialog({ kind: "icon", entry }) },
    ], publishMenu(entry), [
      { label: "Export JSON…", disabled: cannotLoad, run: () => doExport(entry, "json") },
      { label: "Export PDF…", disabled: cannotLoad, run: () => doExport(entry, "pdf") },
      entry.mods.some((mod) => mod.version) && { label: "Copy server commands", run: () => copy(serverCommands(entry), "server install commands") },
    ], [
      { label: "Delete…", disabled: busy, danger: true, run: () => remove([entry]) },
    ]]);
  };

  const saved = sortSaved(filterModlists(all, ["modlist", "group"], search));
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
    const slot = publishing.slotFor(m.published?.registryId);
    const facts = [
      m.subscription && `Revision ${m.subscription.syncedRevision}`,
      m.published && `Revision ${slot?.info.revision ?? m.published.revision}`,
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
        followers={slot?.subscribers}
        following={!!m.subscription}
        published={!!m.published}
        badges={(m.active || m.subscription?.updateAvailable || m.published?.changed) && <>
          {m.active && <span className="badge active-badge">Active</span>}
          {m.subscription?.updateAvailable && <span className="badge update-badge">Update available</span>}
          {m.published?.changed && <span className="badge published-badge" title="This saved modlist changed since it was published">Changes to publish</span>}
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
        <div className="mods-toolbar wrap modlist-actions" role="group" aria-label="Modlist actions">
          <button
            type="button"
            className="primary"
            title="Combine the selected modlists (or pick them in the next step)"
            onClick={() => setDialog({ kind: "group", group: null, members: chosen.filter((c) => c.kind !== "group").map((c) => c.id) })}
          >
            <Icon name="plus" />New group…
          </button>
          <button type="button" onClick={() => void doImport()} disabled={importFile.isPending}><Icon name="import" />Import file…</button>
          <button type="button" onClick={() => openFolder.mutate("modlists")}><Icon name="folder" />Open folder</button>
        </div>
        <div className="mods-toolbar wrap">
          <input
            type="search"
            className="search"
            placeholder="Search all modlists"
            aria-label="Search modlists"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
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
          <p className="muted details-empty-text">
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
                <Icon name="plus" />New group from these…
              </button>
              <button type="button" className="danger" disabled={del.isPending} onClick={() => void remove(chosen)}><Icon name="trash" />Delete…</button>
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
            onSync={() => sync.mutate(chosen[0].id, { onSuccess: (s) => toast.success(`Updated "${s.name}" to the latest online revision.`) })}
            onUnsubscribe={() => unsubscribe.mutate(chosen[0].id, { onSuccess: (s) => toast.success(`"${s.name}" is now a normal modlist.`) })}
            publishing={publishing}
            busy={busy}
          />
        )}
      </aside>
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
            onDone: (report) => {
              setDialog(null);
              showQuickSwitchToast(quickSwitchNotice(dialog.title, report, os));
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
          onPick={(icon, color) => changeIcon(dialog.entry, icon, color)}
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
      {publishing.modal}
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
  publishing: Publishing;
}

function ModlistDetails({ entry, busy, onLoad, onQuickSwitch, onExport, onCopy, onEditGroup, onChangeIcon, onDelete, onSelectMember, onResolve, onSync, onUnsubscribe, publishing }: DetailsProps) {
  const hasCommands = entry.mods.some((m) => m.version);
  const [showServerCommands] = useShowServerCommands();
  const openMenu = useContextMenu();
  const slot = publishing.slotFor(entry.published?.registryId);
  const blockedTitle = publishing.blocked ?? undefined;
  return (
    <>
      <div className="details-header">
        <h2>
          <button
            type="button"
            className={`details-icon-button${iconThemeClass(entry.iconColor)}`}
            title={entry.subscription ? "Followed modlists keep the author's icon" : "Change icon"}
            aria-label="Change icon"
            disabled={busy || !!entry.error || !!entry.subscription}
            onClick={onChangeIcon}
          >
            <ModlistIcon icon={entry.icon} fallback={kindIcons[entry.kind]} />
          </button>
          {entry.name}
          <span className="badge">{kindLabel(entry.kind)}</span>
          {entry.active && <span className="badge active-badge" title="Your installed mods match this list">Active</span>}
          {entry.published && <span className="badge published-badge" title="Other players can find, load and follow it">Published</span>}
        </h2>
      </div>
      {entry.error && <p className="notice error-text">{entry.error}</p>}
      {entry.published && (
        <div className="notice publish-notice">
          {entry.published.changed
            ? "Published, but this saved modlist changed since then. Publish the changes to share them."
            : "Published: other players can find, load and follow it."}{" "}
          Revision {slot?.info.revision ?? entry.published.revision}
          {slot ? `, ${slot.subscribers} following` : ""}.
          <div className="link-row">
            {entry.published.changed && (
              <button type="button" className="primary" disabled={busy || !!publishing.blocked} title={blockedTitle} onClick={() => publishing.publish(entry)}>
                <Icon name="published" />Publish changes
              </button>
            )}
            <button type="button" disabled={busy || !!publishing.blocked} title={blockedTitle ?? "Description, game version and icon, online and here"} onClick={() => publishing.editDetails(entry)}>
              <Icon name="edit" />Edit online details…
            </button>
            <button type="button" className="link-button small" disabled={busy || !!publishing.blocked} title={blockedTitle} onClick={() => void publishing.unpublish(entry)}>
              <Icon name="unpublish" size={13} />Unpublish…
            </button>
          </div>
        </div>
      )}
      {entry.subscription && (
        <div className={`notice${entry.subscription.updateAvailable ? "" : " quiet"}`}>
          {entry.subscription.sourceGone
            ? "The author deleted the online modlist this one followed. Your copy still works."
            : entry.subscription.updateAvailable
              ? `The author published revision ${entry.subscription.remoteRevision} (you have ${entry.subscription.syncedRevision}).`
              : `Follows an online modlist (revision ${entry.subscription.syncedRevision}).`}{" "}
          {!entry.subscription.sourceGone && "It always matches what the author published, so it can't be edited here. Stop following to make it your own copy."}
          <div className="link-row">
            {entry.subscription.updateAvailable && !entry.subscription.sourceGone && (
              <button type="button" className="primary" disabled={busy} onClick={onSync}><Icon name="update" />Get the update</button>
            )}
            <button type="button" className="link-button small" disabled={busy} onClick={onUnsubscribe}><Icon name="unfollow" size={13} />Stop following</button>
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
              <Icon name="swap" />{onQuickSwitch ? "Quick switch" : "Switch to this…"}
            </button>
        )}
        <button
          type="button"
          disabled={!!entry.error || busy}
          onClick={() => onLoad()}
        >
          <Icon name="load" />Load…
        </button>
        {entry.kind === "group" && <button type="button" disabled={busy} onClick={onEditGroup}><Icon name="edit" />Edit group…</button>}
        {canBePublished(entry) && !entry.published && (
          <button
            type="button"
            disabled={busy || !!publishing.blocked}
            title={blockedTitle ?? `Share it with other players (${publishing.usedSlots} of 5 published)`}
            onClick={() => publishing.publish(entry)}
          >
            <Icon name="published" />Publish
          </button>
        )}
        <button
          type="button"
          aria-haspopup="menu"
          disabled={!!entry.error || busy}
          onClick={(event) => openMenu(event, "Export as", [[
            { label: "PDF file…", run: () => onExport("pdf") },
            { label: "JSON file…", run: () => onExport("json") },
          ]], { below: true })}
        >
          <Icon name="export" />Export…
        </button>
        {hasCommands && showServerCommands && (
          <button type="button" title="/moddb install commands for a server console" onClick={onCopy}>
            <Icon name="copy" />Copy server commands
          </button>
        )}
        <button type="button" className="danger" disabled={busy} onClick={onDelete}><Icon name="trash" />Delete…</button>
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
