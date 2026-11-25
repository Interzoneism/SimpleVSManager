import { useCallback, useEffect, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useAppInfo, useEnvironment, useLaunchGame, useManagerUpdate, useOpenUrl } from "./api";
import { ModBrowser } from "./features/browser/ModBrowser";
import { SetupOverview } from "./features/setup/SetupOverview";
import { InstalledMods } from "./features/mods/InstalledMods";
import { Modlists } from "./features/modlists/Modlists";
import { ConfirmMods } from "./features/usage/ConfirmMods";
import { ConfigEditor } from "./features/configs/ConfigEditor";
import { HelpDialog } from "./features/help/HelpDialog";
import { Icon, type IconName } from "./components/Icon";
import { useLayoutPreference } from "./components/SplitWorkspace";

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
  const [configMod, setConfigMod] = useState<string | null>(null);
  const [help, setHelp] = useState(false);
  const editConfig = useCallback((modId: string) => {
    setConfigMod(modId);
    setTab("configs");
  }, []);

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
        {appInfo.data && <span className="app-version">v{appInfo.data.version}</span>}
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
            <button type="button" className="badge-button" title="Change offline mode in Settings" onClick={() => setTab("settings")}>
              Offline
            </button>
          )}
          {env.data?.gameVersion && <span className="game-version">Vintage Story {env.data.gameVersion}</span>}
          {!!env.data?.settings.profiles.length && (
            <button type="button" className="profile-button" title="Manage game profiles in Settings" onClick={() => setTab("settings")}>
              <span className="profile-dot" aria-hidden="true" />
              <span>{env.data.settings.activeProfile}</span>
            </button>
          )}
          <button
            type="button"
            className="primary launch-button"
            disabled={!canLaunch || launch.isPending}
            title={shortcut ? `Start Vintage Story through ${shortcut}` : canLaunch ? "Start Vintage Story with this data folder" : "Set the game and data folders first"}
            onClick={() => launch.mutate()}
          >
            <Icon name="play" />
            {launch.isPending ? "Launching…" : "Launch Vintage Story"}
          </button>
        </header>
        {launch.isError && <p className="notice error-text banner">{launch.error.message}</p>}
        <ConfirmMods enabled={dataReady} onOpenMod={() => setTab("mods")} />
        {managerUpdate.data && (
          <p className="notice banner">
            Simple VS Manager {managerUpdate.data.version} is available.{" "}
            {managerUpdate.data.pageUrl && (
              <button type="button" className="link-button" onClick={() => openUrl.mutate(managerUpdate.data!.pageUrl!)}>
                Open the Mod DB page
              </button>
            )}
          </p>
        )}
        <main className="app-main" key={tab}>
          <div className="page-heading">
            <h1>{pages[tab].title}</h1>
          </div>
          {env.isPending && <p className="empty-state">Looking for Vintage Story…</p>}
          {env.isError && <p className="empty-state error-text">Detection failed: {env.error.message}</p>}
          {env.data && !dataReady && tab !== "settings" ? (
            <div className="setup-empty">
              <span className="empty-icon"><Icon name="folder" /></span>
              <h2>Connect your game data folder</h2>
              <p className="muted">Choose your Vintage Story data folder to manage mods, modlists and configs.</p>
              <button type="button" className="primary" onClick={() => setTab("settings")}>Open Settings</button>
            </div>
          ) : env.data && tab === "mods" && <InstalledMods os={env.data.os} enabled={dataReady} onEditConfig={editConfig} onBrowse={() => setTab("browse")} />}
          {env.data && dataReady && tab === "modlists" && (
            <Modlists
              os={env.data.os}
              offline={env.data.settings.offlineMode}
              droppedFile={droppedFile}
              onDroppedFileHandled={clearDropped}
            />
          )}
          {env.data && dataReady && tab === "browse" && <ModBrowser enabled={dataReady} />}
          {env.data && dataReady && tab === "configs" && <ConfigEditor enabled={dataReady} focusMod={configMod} />}
          {env.data && tab === "settings" && <SetupOverview env={env.data} />}
        </main>
      </div>
      {help && <HelpDialog onClose={() => setHelp(false)} />}
    </div>
  );
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
