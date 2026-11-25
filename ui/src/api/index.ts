// Query hooks around the generated bindings. Components use these, never `commands` directly,
// so caching and error handling stay in one place.
import { useEffect } from "react";
import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  commands,
  events,
  type AppError,
  type BrowseQuery,
  type DetectedEnvironment,
  type ExportFormat,
  type LoadMode,
  type ModlistProgress,
  type ModlistSource,
  type ModlistSummary,
  type SaveGroupRequest,
  type SaveModlistRequest,
  type KnownFolder,
  type LookupMode,
  type ModDbStatus,
  type ModList,
  type PreferencesPatch,
  type BackupProgress,
  type MorePreferences,
  type ProfileChange,
  type DetailsEdit,
  type PublishCurrentRequest,
  type ReportRequest,
  type Result,
  type UpdateProgress,
  type UpdateRequest,
} from "../bindings";

/** Unwraps a tauri-specta Result, throwing the backend error so React Query can catch it. */
export async function unwrap<T>(promise: Promise<Result<T, AppError>>): Promise<T> {
  const result = await promise;
  if (result.status === "error") throw new Error(result.error.message);
  return result.data;
}

export const queryKeys = {
  appInfo: ["appInfo"] as const,
  environment: ["environment"] as const,
  mods: ["mods"] as const,
  moddb: ["moddb"] as const,
  moddbDetails: (modId: string) => ["moddbDetails", modId.toLowerCase()] as const,
  browse: ["browse"] as const,
  facets: ["browseFacets"] as const,
  cacheStats: ["modCacheStats"] as const,
  managerUpdate: ["managerUpdate"] as const,
  modlists: ["modlists"] as const,
  saveDefaults: ["modlistSaveDefaults"] as const,
};

export function useAppInfo() {
  return useQuery({ queryKey: queryKeys.appInfo, queryFn: () => commands.appInfo() });
}

// --- Environment and settings ------------------------------------------------------------------

export function useEnvironment() {
  return useQuery({
    queryKey: queryKeys.environment,
    queryFn: () => unwrap(commands.getEnvironment()),
  });
}

/** Mutation that returns a fresh environment; also refetches mods since folders may change. */
function useEnvironmentMutation<A>(fn: (arg: A) => Promise<Result<DetectedEnvironment, AppError>>) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (arg: A) => unwrap(fn(arg)),
    onSuccess: (env) => {
      client.setQueryData(queryKeys.environment, env);
      void client.invalidateQueries({ queryKey: queryKeys.mods });
    },
  });
}

export const useRefreshEnvironment = () => useEnvironmentMutation<void>(() => commands.refreshEnvironment());
export const useSetGameDirectory = () =>
  useEnvironmentMutation<string | null>((p) => commands.setGameDirectory(p));
export const useSetDataDirectory = () =>
  useEnvironmentMutation<string | null>((p) => commands.setDataDirectory(p));
export const useSetCloseAfterLaunch = () =>
  useEnvironmentMutation<boolean>((v) => commands.setCloseAfterLaunch(v));
export const useSetCustomShortcut = () =>
  useEnvironmentMutation<string | null>((p) => commands.setCustomShortcut(p));

/** Moves the manager folder (`null` = back to the default place). Restart afterwards. */
export function useMoveManagerFolder() {
  return useMutation({ mutationFn: (parent: string | null) => unwrap(commands.moveManagerFolder(parent)) });
}
export const restartApp = () => commands.restartApp();

export function useOpenKnownFolder() {
  return useMutation({
    mutationFn: (folder: KnownFolder) => unwrap(commands.openKnownFolder(folder)),
  });
}

// --- Installed mods ----------------------------------------------------------------------------

export function useMods(enabled: boolean) {
  return useQuery({
    queryKey: queryKeys.mods,
    queryFn: () => unwrap(commands.listMods()),
    enabled,
  });
}

function useModListMutation<A>(fn: (arg: A) => Promise<Result<ModList, AppError>>) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (arg: A) => unwrap(fn(arg)),
    onSuccess: (list) => {
      client.setQueryData(queryKeys.mods, list);
      void client.invalidateQueries({ queryKey: queryKeys.moddb });
      // Deleting saves an automatic backup.
      void client.invalidateQueries({ queryKey: queryKeys.modlists });
    },
  });
}

export const useSetModsActive = () =>
  useModListMutation<{ keys: string[]; active: boolean }>(({ keys, active }) => commands.setModsActive(keys, active));
export const useDeleteMods = () => useModListMutation<string[]>((keys) => commands.deleteMods(keys));

export function useRevealMod() {
  return useMutation({ mutationFn: (key: string) => unwrap(commands.revealMod(key)) });
}

export function useOpenModWebsite() {
  return useMutation({ mutationFn: (key: string) => unwrap(commands.openModWebsite(key)) });
}

/** Refetches the mod list whenever the backend sees mod folders or clientsettings.json change. */
export function useModsChangedListener() {
  const client = useQueryClient();
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void events.modsChanged
      .listen(() => {
        void client.invalidateQueries({ queryKey: queryKeys.mods });
        void client.invalidateQueries({ queryKey: queryKeys.moddb });
      })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [client]);
}

// --- Game --------------------------------------------------------------------------------------

export function useLaunchGame() {
  const client = useQueryClient();
  // A missing custom shortcut is cleared by the backend, so refetch the settings on failure.
  return useMutation({
    mutationFn: () => unwrap(commands.launchGame()),
    onError: () => void client.invalidateQueries({ queryKey: queryKeys.environment }),
  });
}

// --- Mod DB: installed mods ---------------------------------------------------------------------

/**
 * Mod DB data for installed mods. The query itself only reads the disk cache (instant); the
 * network refresh runs through `useRefreshModDb` and writes into the same cache entry.
 * `modsStamp` ties it to the current mod list, since the backend summarises the last scan.
 */
export function useModDb(modsStamp: number) {
  return useQuery({
    queryKey: [...queryKeys.moddb, modsStamp],
    queryFn: () => unwrap(commands.moddbStatus("cacheOnly")),
    enabled: modsStamp > 0,
  });
}

export function useRefreshModDb() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (mode: LookupMode) => unwrap(commands.moddbStatus(mode)),
    onSuccess: (status: ModDbStatus) => {
      // Every cached status entry (any mods stamp) is now stale; seed the latest.
      client.setQueriesData({ queryKey: queryKeys.moddb }, status);
      void client.invalidateQueries({ queryKey: queryKeys.managerUpdate });
    },
  });
}

export function useModDbDetails(modId: string | null) {
  return useQuery({
    queryKey: queryKeys.moddbDetails(modId ?? ""),
    queryFn: () => unwrap(commands.moddbDetails(modId!, false)),
    enabled: !!modId,
    staleTime: 5 * 60_000,
  });
}

const EMPTY_PATCH: PreferencesPatch = {
  offlineMode: null,
  targetGameVersion: null,
  requireExactVersionMatch: null,
  modCacheLimitMb: null,
  skip: null,
  updateExcluded: null,
  favourite: null,
  modUsageTracking: null,
};

/** Changes update / Mod DB preferences. Pass only the fields to change. */
export function useUpdatePreferences() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (patch: Partial<PreferencesPatch>) => unwrap(commands.updatePreferences({ ...EMPTY_PATCH, ...patch })),
    onSuccess: (env) => {
      client.setQueryData(queryKeys.environment, env);
      void client.invalidateQueries({ queryKey: queryKeys.moddb });
      void client.invalidateQueries({ queryKey: ["moddbDetails"] });
      void client.invalidateQueries({ queryKey: queryKeys.browse });
      void client.invalidateQueries({ queryKey: queryKeys.cacheStats });
    },
  });
}

export function useUpdateMods() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (requests: UpdateRequest[]) => unwrap(commands.updateMods(requests)),
    onSettled: (report) => {
      if (report) client.setQueryData(queryKeys.mods, report.list);
      void client.invalidateQueries({ queryKey: queryKeys.modlists });
      void client.invalidateQueries({ queryKey: queryKeys.moddb });
      void client.invalidateQueries({ queryKey: ["moddbDetails"] });
      void client.invalidateQueries({ queryKey: queryKeys.cacheStats });
      void client.invalidateQueries({ queryKey: queryKeys.browse });
    },
  });
}

export const cancelUpdates = () => commands.cancelUpdates();

export function useInstallMod() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ modId, version }: { modId: string; version: string | null }) =>
      unwrap(commands.installMod(modId, version)),
    onSuccess: (list) => {
      client.setQueryData(queryKeys.mods, list);
      void client.invalidateQueries({ queryKey: queryKeys.moddb });
      void client.invalidateQueries({ queryKey: queryKeys.browse });
      void client.invalidateQueries({ queryKey: queryKeys.cacheStats });
    },
  });
}

/** Installs, updates and enables whatever a mod's dependencies need. */
export function useFixDependencies() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (key: string) => unwrap(commands.fixDependencies(key)),
    onSuccess: (report) => {
      client.setQueryData(queryKeys.mods, report.list);
      void client.invalidateQueries({ queryKey: queryKeys.moddb });
      void client.invalidateQueries({ queryKey: queryKeys.modlists });
      void client.invalidateQueries({ queryKey: queryKeys.cacheStats });
    },
  });
}

/** Subscribes to per-mod download/install progress. */
export function useUpdateProgress(onProgress: (p: UpdateProgress) => void) {
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void events.updateProgress
      .listen((e) => onProgress(e.payload))
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [onProgress]);
}

// --- Mod DB: browser, cache, misc ---------------------------------------------------------------

export function useBrowseMods(query: BrowseQuery, gameVersion: string | null, enabled = true) {
  return useQuery({
    queryKey: [...queryKeys.browse, query, gameVersion],
    queryFn: () => unwrap(commands.browseMods(query, gameVersion, false)),
    enabled,
    placeholderData: (prev) => prev,
  });
}

const THUMB_CHUNK = 12;

/**
 * The Mod DB's own card pictures (`logofiledb`) for browser cards, by lower-case mod id. Looked
 * up in small batches so cards fill in as they arrive; a mod missing from the map keeps the
 * catalogue's logo, `null` means it has no picture.
 */
export function useBrowseThumbnails(modIds: string[], enabled = true): Record<string, string | null> {
  const chunks: string[][] = [];
  for (let i = 0; i < modIds.length; i += THUMB_CHUNK) chunks.push(modIds.slice(i, i + THUMB_CHUNK));
  return useQueries({
    queries: chunks.map((ids) => ({
      queryKey: ["browseThumbnails", ids] as const,
      queryFn: () => unwrap(commands.browseThumbnails(ids)),
      enabled,
      staleTime: Infinity,
      retry: false,
    })),
    combine: (results) => {
      const out: Record<string, string | null> = {};
      for (const r of results) {
        for (const [k, v] of Object.entries(r.data ?? {})) if (v !== undefined) out[k] = v;
      }
      return out;
    },
  });
}

export function useRefreshCatalogue() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.browseMods(defaultBrowseQuery(), null, true)),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: queryKeys.browse });
      void client.invalidateQueries({ queryKey: queryKeys.facets });
    },
  });
}

export function defaultBrowseQuery(): BrowseQuery {
  return {
    text: "",
    tags: [],
    side: null,
    sort: "relevance",
    reverse: false,
    onlyFavourites: false,
    hideInstalled: false,
    offset: 0,
    limit: 60,
  };
}

export function useBrowseFacets(enabled = true) {
  return useQuery({ queryKey: queryKeys.facets, queryFn: () => unwrap(commands.browseFacets()), enabled });
}

export function useModCacheStats() {
  return useQuery({ queryKey: queryKeys.cacheStats, queryFn: () => unwrap(commands.modCacheStats()) });
}

export function useClearModCache() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.clearModCache()),
    onSuccess: (stats) => {
      client.setQueryData(queryKeys.cacheStats, stats);
      void client.invalidateQueries({ queryKey: ["moddbDetails"] });
    },
  });
}

export function useManagerUpdate() {
  return useQuery({ queryKey: queryKeys.managerUpdate, queryFn: () => unwrap(commands.managerUpdate()) });
}

export function useOpenUrl() {
  return useMutation({ mutationFn: (url: string) => unwrap(commands.openUrl(url)) });
}

// --- Modlists, presets, groups, backups -------------------------------------------------------

export function useModlists() {
  return useQuery({ queryKey: queryKeys.modlists, queryFn: () => unwrap(commands.listModlists()) });
}

/** Mutation whose result replaces (or is merged into) the modlist list. */
function useModlistMutation<A, R>(fn: (arg: A) => Promise<R>, toList?: (r: R, prev: ModlistSummary[]) => ModlistSummary[]) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSuccess: (r) => {
      if (toList) client.setQueryData<ModlistSummary[]>(queryKeys.modlists, (prev) => toList(r, prev ?? []));
      void client.invalidateQueries({ queryKey: queryKeys.modlists });
    },
  });
}

const upsert = (s: ModlistSummary, prev: ModlistSummary[]) => [...prev.filter((p) => p.id !== s.id), s];

export function useModlistSaveDefaults(enabled: boolean) {
  return useQuery({
    queryKey: queryKeys.saveDefaults,
    queryFn: () => unwrap(commands.modlistSaveDefaults()),
    enabled,
    staleTime: 0,
  });
}

export const useSaveModlist = () =>
  useModlistMutation((r: SaveModlistRequest) => unwrap(commands.saveModlist(r)), (r, prev) => upsert(r.saved, prev));
export const useSaveModlistGroup = () =>
  useModlistMutation((r: SaveGroupRequest) => unwrap(commands.saveModlistGroup(r)), upsert);
export const useResolveGroupConflict = () =>
  useModlistMutation(
    (a: { id: string; modId: string; reference: string }) => unwrap(commands.resolveGroupConflict(a.id, a.modId, a.reference)),
    upsert,
  );
export const useSetModlistIcon = () =>
  useModlistMutation(
    (a: { id: string; icon: string | null; color: string | null }) => unwrap(commands.setModlistIcon(a.id, a.icon, a.color)),
    upsert,
  );
export const useDeleteModlists = () =>
  useModlistMutation((ids: string[]) => unwrap(commands.deleteModlists(ids)), (r) => r);
export const useImportModlist = () =>
  useModlistMutation((a: { path: string; overwrite: boolean }) => unwrap(commands.importModlist(a.path, a.overwrite)), upsert);

export function useExportModlist() {
  return useMutation({
    mutationFn: (a: { id: string; path: string; format: ExportFormat }) => unwrap(commands.exportModlist(a.id, a.path, a.format)),
  });
}

export function useModlistPreview(source: ModlistSource | null, mode: LoadMode) {
  return useQuery({
    queryKey: ["modlistPreview", source, mode],
    queryFn: () => unwrap(commands.previewModlist(source!, mode)),
    enabled: !!source,
    staleTime: 0,
    gcTime: 0,
  });
}

export function useModlistModCards(modIds: string[], enabled: boolean) {
  return useQuery({
    queryKey: ["modlistModCards", modIds],
    queryFn: () => unwrap(commands.modlistModCards(modIds, true)),
    enabled: enabled && modIds.length > 0,
    staleTime: 5 * 60_000,
  });
}

export function useApplyModlist() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (a: { source: ModlistSource; mode: LoadMode; importConfigs: boolean }) =>
      unwrap(commands.applyModlist(a.source, a.mode, a.importConfigs)),
    onSettled: (report) => {
      if (report?.list) client.setQueryData(queryKeys.mods, report.list);
      void client.invalidateQueries({ queryKey: queryKeys.mods });
      void client.invalidateQueries({ queryKey: queryKeys.moddb });
      void client.invalidateQueries({ queryKey: queryKeys.modlists });
      void client.invalidateQueries({ queryKey: queryKeys.cacheStats });
      void client.invalidateQueries({ queryKey: queryKeys.browse });
    },
  });
}

export function useModlistProgress(onProgress: (p: ModlistProgress) => void) {
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void events.modlistProgress
      .listen((e) => onProgress(e.payload))
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [onProgress]);
}

// --- Cloud: online modlists, publishing, reports ----------------------------------------------

export function useCloudStatus(enabled = true) {
  return useQuery({ queryKey: ["cloudStatus"], queryFn: () => unwrap(commands.cloudStatus()), enabled, staleTime: 60_000 });
}

export function useOnlineModlists(enabled: boolean) {
  return useQuery({ queryKey: ["onlineModlists"], queryFn: () => unwrap(commands.onlineModlists()), enabled, staleTime: 5 * 60_000 });
}

function useCloudMutation<A, R>(fn: (a: A) => Promise<R>) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSettled: () => {
      void client.invalidateQueries({ queryKey: ["onlineModlists"] });
      void client.invalidateQueries({ queryKey: queryKeys.modlists });
      void client.invalidateQueries({ queryKey: ["cloudSlots"] });
    },
  });
}

export const useHideOnline = () =>
  useCloudMutation((r: { registryId?: string; author?: string; clear?: boolean }) =>
    unwrap(commands.hideOnline({ registryId: r.registryId ?? null, author: r.author ?? null, clear: !!r.clear })),
  );
export const useSaveOnlineCopy = () =>
  useCloudMutation((a: { registryId: string; follow: boolean }) => unwrap(commands.saveOnlineCopy(a.registryId, a.follow)));
export const useSyncSubscription = () => useCloudMutation((id: string) => unwrap(commands.syncSubscription(id)));
export const useUnsubscribe = () => useCloudMutation((id: string) => unwrap(commands.unsubscribe(id)));
export const usePublishModlist = () =>
  useCloudMutation((r: { id: string; slot: string; replaces: string | null }) => unwrap(commands.publishModlist(r)));
export const useEditPublished = () =>
  useCloudMutation((r: { registryId: string; edit: DetailsEdit }) => unwrap(commands.editPublishedModlist(r)));
export const usePublishCurrent = () =>
  useCloudMutation((r: PublishCurrentRequest) => unwrap(commands.publishCurrentMods(r)));
export const useUpdatePublished = () => useCloudMutation((registryId: string) => unwrap(commands.updatePublishedMods(registryId)));
export const useDeleteCloudSlot = () => useCloudMutation((slot: string) => unwrap(commands.deleteCloudSlot(slot)));
export const useAcknowledgeNotices = () =>
  useCloudMutation((ids: string[]) => unwrap(commands.acknowledgeSubscriberNotices(ids)));

export function useCloudSlots(enabled: boolean) {
  return useQuery({ queryKey: ["cloudSlots"], queryFn: () => unwrap(commands.myCloudSlots()), enabled, staleTime: 60_000, retry: false });
}

export function useModReports(modId: string | null, version: string | null, enabled: boolean) {
  return useQuery({
    queryKey: ["modReports", modId?.toLowerCase(), version],
    queryFn: () => unwrap(commands.modReports(modId!, version!)),
    enabled: enabled && !!modId && !!version,
    staleTime: 10 * 60_000,
  });
}

export function useSubmitModReport() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (r: ReportRequest) => unwrap(commands.submitModReport(r)),
    onSuccess: (summary, r) => client.setQueryData(["modReports", r.modId.toLowerCase(), r.version], summary),
  });
}

export function usePlaySessions(enabled: boolean) {
  const query = useQuery({ queryKey: ["playSessions"], queryFn: () => unwrap(commands.checkPlaySessions()), enabled, staleTime: 0 });
  const { refetch } = query;
  // Check again when the window comes back (the player may have just finished a session).
  useEffect(() => {
    if (!enabled) return;
    const onFocus = () => void refetch();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [enabled, refetch]);
  return query;
}

export function useAnswerPlaySessions() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (a: { keys: string[]; stop: boolean }) => unwrap(commands.answerPlaySessions(a.keys, a.stop)),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ["playSessions"] });
      void client.invalidateQueries({ queryKey: queryKeys.environment });
    },
  });
}

export function useMoveCloudIdentity() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.moveCloudIdentity()),
    onSettled: () => {
      void client.invalidateQueries({ queryKey: ["cloudStatus"] });
      void client.invalidateQueries({ queryKey: ["onlineModlists"] });
      void client.invalidateQueries({ queryKey: ["cloudSlots"] });
    },
  });
}

// --- Data folder backups, configs, profiles, appearance ---------------------------------------

export function useDataBackups(enabled = true) {
  return useQuery({ queryKey: ["dataBackups"], queryFn: () => unwrap(commands.listDataBackups()), enabled });
}

function useBackupMutation<A, R>(fn: (a: A) => Promise<R>) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSettled: () => {
      void client.invalidateQueries({ queryKey: ["dataBackups"] });
      void client.invalidateQueries({ queryKey: queryKeys.mods });
    },
  });
}

export const useCreateDataBackup = () => useBackupMutation((_: void) => unwrap(commands.createDataBackup()));
export const useRestoreDataBackup = () =>
  useBackupMutation((a: { id: string; safety: boolean }) => unwrap(commands.restoreDataBackup(a.id, a.safety)));
export const useDeleteDataBackup = () => useBackupMutation((id: string) => unwrap(commands.deleteDataBackup(id)));

export function useBackupProgress(onProgress: (p: BackupProgress) => void) {
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void events.backupProgress
      .listen((e) => onProgress(e.payload))
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [onProgress]);
}

export function useModConfigs(enabled = true) {
  return useQuery({ queryKey: ["modConfigs"], queryFn: () => unwrap(commands.listModConfigs()), enabled });
}

export function useModConfig(relativePath: string | null) {
  return useQuery({
    queryKey: ["modConfig", relativePath],
    queryFn: () => unwrap(commands.readModConfig(relativePath!)),
    enabled: !!relativePath,
    staleTime: 0,
    gcTime: 0,
  });
}

export function useSaveModConfig() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (a: { relativePath: string; text: string }) => unwrap(commands.saveModConfig(a.relativePath, a.text)),
    onSuccess: (file, a) => client.setQueryData(["modConfig", a.relativePath], file),
  });
}

export const useChangeProfile = () => useEnvironmentMutation<ProfileChange>((c) => commands.changeProfile(c));
export const useMorePreferences = () =>
  useEnvironmentMutation<Partial<MorePreferences>>((p) =>
    commands.updateMorePreferences({ theme: null, verboseLogging: null, autoDataBackups: null, maxDataBackups: null, dataBackupLocation: null, ...p }),
  );
