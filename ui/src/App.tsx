import { Activity, memo, useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useAppInfo, useEnvironment, useLaunchGame, useManagerUpdate,
  useInstallManagerUpdate, useOpenUrl } from "./api";
import { BuilderToggle } from "./features/browser/ModlistBuilder";
import { ModBrowser } from "./features/browser/ModBrowser";
import { SetupOverview } from "./features/setup/SetupOverview";
import { InstalledMods } from "./features/mods/InstalledMods";
import { Modlists } from "./features/modlists/Modlists";
import { ConfirmMods } from "./features/usage/ConfirmMods";
import { ConfigEditor } from "./features/configs/ConfigEditor";
import { HelpDialog } from "./features/help/HelpDialog";
import { Icon, type IconName } from "./components/Icon";
import { toast, Toaster, useErrorToast } from "./components/Toast";
import { useLayoutPreference } from "./components/SplitWorkspace";

// Memoized so a hidden page doesn't re-render whenever the app shell does (tab switches etc.).
const InstalledModsPage = memo(InstalledMods);
const ModlistsPage = memo(Modlists);
const ModBrowserPage = memo(ModBrowser);
const ConfigEditorPage = memo(ConfigEditor);
const SetupOverviewPage = memo(SetupOverview);

type Tab = "mods" | "modlists" | "browse" | "configs" | "settings";
const pages: Record<Tab, { title: string; icon: IconName }> = {
  mods: { title: "Installed mods", icon: "library" },
  modlists: { title: "Modlists", icon: "lists" },
  browse: { title: "Browse the Mod DB", icon: "compass" },
  configs: { title: "Mod configs", icon: "config" },
  settings: { title: "Settings", icon: "settings" },
};

export default function App() {
  const appInfo = useAppInfo();
  const env = useEnvironment();
  const launch = useLaunchGame();
  const [tab, setTab] = useState<Tab>("mods");
  const [sidebarCollapsed, setSidebarCollapsed] = useLayoutPreference<boolean>("sidebar", false);
  const managerUpdate = useManagerUpdate();
  const openUrl = useOpenUrl();
  const [droppedFile, setDroppedFile] = useState<string | null>(null);
  const clearDropped = useCallback(() => setDroppedFile(null), []);
  // A new object each time, so asking for the same mod again still selects its file.
  const [configMod, setConfigMod] = useState<{ modId: string } | null>(null);
  const [help, setHelp] = useState(false);
  const editConfig = useCallback((modId: string) => {
    setConfigMod({ modId });
    setTab("configs");
  }, []);
  // Pages stay mounted (hidden) once visited, so switching back is instant and keeps their
  // scroll, search and selection. Each page starts at the top of the shared scroll area.
  const [visited, setVisited] = useState<ReadonlySet<Tab>>(() => new Set([tab]));
  if (!visited.has(tab)) setVisited(new Set([...visited, tab]));
  const browse = useCallback(() => setTab("browse"), []);
  const main = useRef<HTMLElement>(null);
  useLayoutEffect(() => {
    main.current?.scrollTo?.(0, 0);
  }, [tab]);

  // Dropping a modlist (.json or .pdf) anywhere opens it on the Modlists tab.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void getCurrentWebview()
      .onDragDropEvent((e) => {
        if (e.payload.type !== "drop") return;
        const file = e.payload.paths.find((p) => /\.(json|pdf)$/i.test(p));
        if (file) {
          setDroppedFile(file);
          setTab("modlists");
        }
      })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const dataReady = !!env.data?.dataDir?.exists;

  // Theme: palettes are CSS variables; 2.x custom colors are applied inline.
  const theme = env.data?.settings.theme ?? "darkVs";
  const palette = env.data?.settings.customPalette;
  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = theme;
    const applied: string[] = [];
    if (theme === "custom" && palette) {
      for (const [k, v] of Object.entries(palette)) {
        if (k.startsWith("--palette-") && v) {
          root.style.setProperty(k, v);
          applied.push(k);
        }
      }
    }
    return () => applied.forEach((k) => root.style.removeProperty(k));
  }, [theme, palette]);
  useErrorToast(launch.error, { title: "Couldn't launch Vintage Story" });
  // A new manager version: shown once per run, until closed. "Update and restart" downloads
  // it, checks its signature and restarts; otherwise the release page has the downloads.
  const update = managerUpdate.data;
  const installUpdate = useInstallManagerUpdate();
  useErrorToast(installUpdate.error, { title: "Couldn't update Simple VS Manager" });
  useEffect(() => {
    if (!update) return;
    const action = update.canInstall
      ? {
          label: "Update and restart",
          icon: "download" as const,
          onClick: () => {
            toast.info(`Downloading Simple VS Manager ${update.version}. It restarts when done.`, { id: "manager-update", title: "Updating", duration: null });
            installUpdate.mutate();
          },
        }
      : update.pageUrl
        ? { label: "Open the download page", icon: "external" as const, onClick: () => openUrl.mutate(update.pageUrl!) }
        : undefined;
    toast.info(`Simple VS Manager ${update.version} is available.`, { id: "manager-update", title: "Update available", duration: null, action });
    // The mutations are fresh objects each render; the toast only needs to appear when the update does.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [update?.version, update?.pageUrl, update?.canInstall]);
  const shortcut = env.data?.settings.customShortcutPath;
  const canLaunch = !!shortcut || (!!env.data?.gameExecutable && dataReady);

  return (
    <div className={`app${sidebarCollapsed ? " sidebar-collapsed" : ""}`}>
      <aside className="sidebar">
        <div className="brand">
          <img src="/manager-icon.png" alt="" className="app-icon" />
          <span className="brand-name">Simple VS Manager</span>
        </div>
        <nav id="app-navigation" className="sidebar-nav" aria-label="Main navigation">
          <div className="nav-group">
            <NavButton current={tab} value="mods" onSelect={setTab} />
            <NavButton current={tab} value="modlists" onSelect={setTab} />
            <NavButton current={tab} value="browse" onSelect={setTab} />
            <NavButton current={tab} value="configs" onSelect={setTab} />
          </div>
          <div className="nav-group nav-bottom">
            <NavButton current={tab} value="settings" onSelect={setTab} />
            <button type="button" className="nav-button" title="Help" aria-label="Help" onClick={() => setHelp(true)}>
              <Icon name="help" /><span>Help</span>
            </button>
          </div>
        </nav>
      </aside>
      <div className="app-content">
        <header className="app-header">
          <button
            type="button"
            className="icon-button sidebar-toggle"
            aria-label={sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
            title={sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
            aria-controls="app-navigation"
            aria-expanded={!sidebarCollapsed}
            onClick={() => setSidebarCollapsed(!sidebarCollapsed)}
          >
            <Icon name="sidebar" />
          </button>
          <div className="header-spacer" />
          {env.data?.settings.offlineMode && (
            <button type="button" className="badge-button" title="Offline mode" onClick={() => setTab("settings")}>
              <Icon name="offline" size={13} />Offline
            </button>
          )}
          {(env.data?.gameVersion || appInfo.data) && (
            <div className="header-versions">
              {env.data?.gameVersion && <span className="game-version">Vintage Story {env.data.gameVersion}</span>}
              {appInfo.data && <span className="manager-version">Simple VS Manager {appInfo.data.version}</span>}
            </div>
          )}
          {!!env.data?.settings.profiles.length && (
            <button type="button" className="profile-button" title="Game profile" onClick={() => setTab("settings")}>
              <span className="profile-dot" aria-hidden="true" />
              <span>{env.data.settings.activeProfile}</span>
            </button>
          )}
          <button
            type="button"
            className="primary launch-button"
            disabled={!canLaunch || launch.isPending}
            title={shortcut ? `Launch through ${shortcut}` : canLaunch ? undefined : "Set the game and data folders first"}
            onClick={() => launch.mutate()}
          >
            <Icon name="play" />
            {launch.isPending ? "Launching…" : "Launch Vintage Story"}
          </button>
        </header>
        <ConfirmMods enabled={dataReady} onOpenMod={() => setTab("mods")} />
        <main className="app-main" ref={main}>
          <div className="page-heading">
            <div className="page-title">
              <h1>{pages[tab].title}</h1>
              {/* The Mod DB page fills this in with its list size and age. */}
              <span id="browse-heading-info" className="page-title-info muted small" hidden={tab !== "browse"} />
            </div>
            {tab === "browse" && dataReady && <BuilderToggle />}
          </div>
          {env.isPending && <p className="empty-state">Looking for Vintage Story…</p>}
          {env.isError && <p className="empty-state error-text">Detection failed: {env.error.message}</p>}
          {env.data && !dataReady && tab !== "settings" && (
            <div className="setup-empty">
              <span className="empty-icon"><Icon name="folder" /></span>
              <h2>Data folder not found</h2>
              <p className="muted">Set your Vintage Story data folder in Settings.</p>
              <button type="button" className="primary" onClick={() => setTab("settings")}><Icon name="settings" />Open Settings</button>
            </div>
          )}
          {env.data && dataReady && (
            <>
              <Page tab="mods" current={tab} visited={visited}>
                <InstalledModsPage os={env.data.os} enabled={dataReady} onEditConfig={editConfig} onBrowse={browse} />
              </Page>
              <Page tab="modlists" current={tab} visited={visited}>
                <ModlistsPage
                  os={env.data.os}
                  offline={env.data.settings.offlineMode}
                  droppedFile={droppedFile}
                  onDroppedFileHandled={clearDropped}
                />
              </Page>
              <Page tab="browse" current={tab} visited={visited}>
                <ModBrowserPage enabled={dataReady} />
              </Page>
              <Page tab="configs" current={tab} visited={visited}>
                <ConfigEditorPage enabled={dataReady} focusMod={configMod} />
              </Page>
            </>
          )}
          {env.data && (
            <Page tab="settings" current={tab} visited={visited}>
              <SetupOverviewPage env={env.data} />
            </Page>
          )}
        </main>
      </div>
      {help && <HelpDialog onClose={() => setHelp(false)} />}
      <Toaster />
    </div>
  );
}

/** A page that has been opened at least once; hidden pages keep their state but run no effects. */
function Page({ tab, current, visited, children }: { tab: Tab; current: Tab; visited: ReadonlySet<Tab>; children: ReactNode }) {
  if (!visited.has(tab)) return null;
  return <Activity mode={tab === current ? "visible" : "hidden"}>{children}</Activity>;
}

function NavButton({ current, value, onSelect }: { current: Tab; value: Tab; onSelect: (t: Tab) => void }) {
  const { title, icon } = pages[value];
  return (
    <button
      type="button"
      aria-current={current === value ? "page" : undefined}
      className="nav-button"
      aria-label={title}
      title={title}
      onClick={() => onSelect(value)}
    >
      <Icon name={icon} />
      <span>{title}</span>
    </button>
  );
}
