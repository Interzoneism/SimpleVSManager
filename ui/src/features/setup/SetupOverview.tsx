import { ask, message, open } from "@tauri-apps/plugin-dialog";
import type { DetectedEnvironment, DetectedPath, KnownFolder, Os, PathSource } from "../../bindings";
import {
  restartApp,
  useMoveManagerFolder,
  useOpenKnownFolder,
  useSetCustomShortcut,
  useRefreshEnvironment,
  useSetCloseAfterLaunch,
  useSetDataDirectory,
  useSetGameDirectory,
} from "../../api";
import { AppearanceSettings, BackupSettings, ModlistSettings, ProfileSettings } from "./MoreSettings";
import { UpdateSettings } from "./UpdateSettings";
import { copyText, useContextMenu } from "../../components/ContextMenu";
import { Icon } from "../../components/Icon";
import { useErrorToast } from "../../components/Toast";

const SOURCE_LABELS: Record<PathSource, string> = {
  userSetting: "Chosen by you",
  legacyConfig: "From your earlier Simple VS Manager settings",
  customPointer: "Custom folder (CustomConfigFolder.txt)",
  detected: "Detected",
  default: "Default location",
};

const OS_LABELS: Record<Os, string> = { windows: "Windows", macos: "macOS", linux: "Linux" };

/** The Settings tab: detected folders (with overrides) and update preferences. */
export function SetupOverview({ env }: { env: DetectedEnvironment }) {
  const refresh = useRefreshEnvironment();

  return (
    <section className="setup">
      <section className="settings-section settings-folders" aria-labelledby="folders-heading">
        <div className="setup-toolbar">
          <h2 id="folders-heading">Game and data folders</h2>
          <button type="button" onClick={() => refresh.mutate()} disabled={refresh.isPending}>
            <Icon name="search" />{refresh.isPending ? "Searching…" : "Search again"}
          </button>
        </div>
        <EnvironmentDetails env={env} />
      </section>
      <div className="settings-columns">
        <div className="settings-column">
          <ProfileSettings env={env} />
          <BackupSettings env={env} os={env.os} />
          <ModlistSettings />
        </div>
        <div className="settings-column">
          <UpdateSettings env={env} />
          <AppearanceSettings env={env} />
        </div>
      </div>
    </section>
  );
}

export function EnvironmentDetails({ env }: { env: DetectedEnvironment }) {
  const setGame = useSetGameDirectory();
  const setData = useSetDataDirectory();
  const closeAfterLaunch = useSetCloseAfterLaunch();
  const moveManager = useMoveManagerFolder();

  const pick = async (title: string, current: string | undefined, apply: (p: string) => void) => {
    const chosen = await open({ directory: true, multiple: false, title, defaultPath: current });
    if (typeof chosen === "string") apply(chosen);
  };

  const moveManagerTo = async (parent: string | null, target: string) => {
    const ok = await ask(
      `Move the manager's settings, modlists, backups and caches from\n${env.managerDir.path}\nto\n${target}?\n\n` +
        "Files with the same name already there are replaced. Simple VS Manager 2.x keeps looking in the old place unless you move its folder too. The manager restarts afterwards.",
      { title: "Move manager folder", kind: "warning", okLabel: "Move and restart", cancelLabel: "Cancel" },
    );
    if (!ok) return;
    moveManager.mutate(parent, {
      onSuccess: async (moved) => {
        await message(
          moved.oldFolderLeft
            ? `Moved to ${moved.folder}. Some files in the old folder were in use and stayed behind; you can delete it after the restart.`
            : `Moved to ${moved.folder}. The manager restarts now.`,
          { title: "Manager folder moved" },
        );
        await restartApp();
      },
    });
  };

  return (
    <>
      <dl className="summary">
        <dt>Vintage Story</dt>
        <dd>{env.gameVersion ?? "version unknown"}</dd>
        <dt>System</dt>
        <dd>{OS_LABELS[env.os]}</dd>
      </dl>

      {env.warnings.length > 0 && (
        <ul className="warnings" aria-label="Warnings">
          {env.warnings.map((w) => (
            <li key={w}>{w}</li>
          ))}
        </ul>
      )}

      <div className="folder-cards">
        <FolderCard
          title="Game folder"
          folder="game"
          detected={env.gameDir}
          missingText="Vintage Story wasn't found. Choose the folder that contains it."
          executable={env.gameExecutable ?? undefined}
          onChange={() => pick("Choose the Vintage Story game folder", env.gameDir?.path, (p) => setGame.mutate(p))}
          onReset={env.settings.gameDirectory ? () => setGame.mutate(null) : undefined}
          error={setGame.error?.message}
        />
        <FolderCard
          title="Data folder (VintagestoryData)"
          folder="data"
          detected={env.dataDir}
          missingText="No data folder found."
          onChange={() => pick("Choose the VintagestoryData folder", env.dataDir?.path, (p) => setData.mutate(p))}
          onReset={env.settings.dataDirectory ? () => setData.mutate(null) : undefined}
          error={setData.error?.message}
        />
        <FolderCard
          title="Manager folder"
          folder="manager"
          detected={env.managerDir}
          missingText=""
          changeLabel="Change folder…"
          onChange={async () => {
            const parent = await open({ directory: true, multiple: false, title: "Choose where to put the Simple VS Manager folder" });
            if (typeof parent !== "string") return;
            const sep = env.os === "windows" ? "\\" : "/";
            const target = `${parent.replace(/[\\/]+$/, "")}${sep}${env.os === "linux" ? "simple-vs-manager" : "Simple VS Manager"}`;
            void moveManagerTo(parent, target);
          }}
          resetLabel="Move back to default"
          onReset={env.managerDir.source === "customPointer" ? () => void moveManagerTo(null, "the default location") : undefined}
          error={moveManager.error?.message}
        />
      </div>

      <label className="checkbox-row">
        <input
          type="checkbox"
          checked={env.settings.closeAfterLaunch}
          disabled={closeAfterLaunch.isPending}
          onChange={(e) => closeAfterLaunch.mutate(e.target.checked)}
        />
        Close the manager after launching Vintage Story
      </label>
      <CustomShortcut env={env} />
    </>
  );
}

/** 2.x's "Set custom Vintage Story shortcut": Launch opens this file instead of the game. */
function CustomShortcut({ env }: { env: DetectedEnvironment }) {
  const set = useSetCustomShortcut();
  useErrorToast(set.error);
  const current = env.settings.customShortcutPath;
  const pick = async () => {
    const chosen = await open({
      multiple: false,
      title: "Choose the shortcut that starts Vintage Story",
      defaultPath: current ?? undefined,
      filters: env.os === "windows" ? [{ name: "Shortcuts", extensions: ["lnk", "url", "bat", "cmd", "exe"] }, { name: "All files", extensions: ["*"] }] : undefined,
    });
    if (typeof chosen === "string") set.mutate(chosen);
  };
  return (
    <div className="setting-row custom-shortcut">
      <span>Launch through a shortcut</span>
      {current ? <code className="path small">{current}</code> : <span className="muted small">Off: Launch starts the game directly with the data folder above.</span>}
      <button type="button" disabled={set.isPending} onClick={() => void pick()}><Icon name="edit" />{current ? "Change…" : "Choose…"}</button>
      {current && <button type="button" disabled={set.isPending} onClick={() => set.mutate(null)}><Icon name="eraser" />Clear</button>}
    </div>
  );
}

function FolderCard(props: {
  title: string;
  folder: KnownFolder;
  detected: DetectedPath | null;
  missingText: string;
  executable?: string;
  onChange?: () => void;
  onReset?: () => void;
  changeLabel?: string;
  resetLabel?: string;
  error?: string;
}) {
  const open = useOpenKnownFolder();
  useErrorToast(open.error);
  useErrorToast(props.error);
  const openMenu = useContextMenu();
  const { detected } = props;

  return (
    <article className="folder-card" onContextMenu={(event) => {
      if (!detected && !props.onChange && !props.onReset) return;
      openMenu(event, props.title, [[
        !!detected && { label: "Open folder", disabled: !detected?.exists || open.isPending, run: () => open.mutate(props.folder) },
        !!props.onChange && { label: "Change folder…", run: props.onChange },
        !!props.onReset && { label: props.resetLabel ?? "Auto-detect", run: props.onReset },
      ], [
        !!detected && { label: "Copy folder path", run: () => copyText(detected.path) },
        !!props.executable && { label: "Copy executable path", run: () => copyText(props.executable!) },
      ]]);
    }}>
      <h3>{props.title}</h3>
      {detected ? (
        <>
          <code className="path">{detected.path}</code>
          <p className="muted">
            {SOURCE_LABELS[detected.source]}
            {!detected.exists && " · doesn't exist yet"}
          </p>
          {props.executable && (
            <div className="folder-executable">
              <span className="muted small">Executable</span>
              <code className="path">{props.executable}</code>
            </div>
          )}
        </>
      ) : (
        <p className="muted">{props.missingText}</p>
      )}
      <div className="card-actions">
        {detected && (
          <button
            type="button"
            onClick={() => open.mutate(props.folder)}
            disabled={!detected.exists || open.isPending}
          >
            <Icon name="folder" />Open folder
          </button>
        )}
        {props.onChange && (
          <button type="button" onClick={props.onChange}>
            <Icon name="edit" />{props.changeLabel ?? "Change…"}
          </button>
        )}
        {props.onReset && (
          <button type="button" onClick={props.onReset}>
            <Icon name="detect" />{props.resetLabel ?? "Auto-detect"}
          </button>
        )}
      </div>
    </article>
  );
}
