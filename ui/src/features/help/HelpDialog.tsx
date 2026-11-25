import { useOpenUrl } from "../../api";
import { Modal } from "../modlists/Modal";

/** Same links as 2.3's Help menu. */
export const DISCORD_URL = "https://discord.gg/Zhm3QnD2s9";
export const MANAGER_MODDB_URL = "https://mods.vintagestory.at/simplevsmanager";

const SECTIONS: { title: string; body: string[] }[] = [
  {
    title: "Installed mods",
    body: [
      "Everything in your Mods folder. Click the checkbox to turn a mod on or off; select several with Ctrl or Shift.",
      "The Latest column shows updates from the Mod DB for your game version. Manage updates skips mods you left out.",
      "Delete moves a mod to the Recycle Bin / Trash. Older versions stay in your mod library for rollback.",
      "Drag the divider to resize a pane, or focus it and use the arrow keys. Double-click the divider to reset its width.",
      "Right-click the column titles to choose which columns show. Drag the edge of a column title to resize it; double-click the edge to reset it.",
    ],
  },
  {
    title: "Modlists",
    body: [
      "Save your enabled mods with their exact versions. Saved modlists also includes groups and copies of online modlists.",
      "Switch to a modlist and your Mods folder ends up with exactly that list. Other mods go to your mod library, so switching back needs no download.",
      "Enable Quick switch in the review to reuse your config-file choice for saved modlists and groups. Use Load to open the review, or turn off Quick switch from the Modlists toolbar.",
      "Groups combine several modlists. Backups are saved automatically before a modlist loads and before updates or deletes.",
      "Export a modlist as PDF or JSON; Simple VS Manager 2.3 can load both. Drop a .json or .pdf on the window to load it.",
      "Load modlists other players published, save a copy, or follow one to get its updates.",
      "Publish your own to five cloud slots under your player name.",
    ],
  },
  {
    title: "Browse the Mod DB",
    body: ["Search every mod on mods.vintagestory.at and install with one click. The whole catalogue is cached, so search is instant."],
  },
  {
    title: "Mod configs",
    body: [
      "Edit the files in VintagestoryData/ModConfig. JSON files show as fields; YAML and broken files as text.",
      "Saving checks the file first. Undo last save puts back the version from before your last save.",
    ],
  },
  {
    title: "Settings",
    body: [
      "Folders: the game and data folders are found automatically; pick them if not.",
      "Launch through a shortcut: Launch opens your own shortcut (for example one with extra options) instead of starting the game directly.",
      "Manager folder: Change folder… moves the manager's settings, modlists and caches to another place, for example another drive.",
      "Game profiles: separate folders and target versions, for example vanilla and modded.",
      "Data folder backups: worlds, settings and mods in one backup; optional before every launch.",
      "Offline mode stops all network use. Cached Mod DB data and your mod library keep working.",
    ],
  },
];

/** One help page instead of 2.x's Help, Guide and Menu Guide dialogs. */
export function HelpDialog({ onClose }: { onClose: () => void }) {
  const openUrl = useOpenUrl();
  return (
    <Modal title="Help" onClose={onClose}>
      <div className="help">
        {SECTIONS.map((s) => (
          <section key={s.title}>
            <h3>{s.title}</h3>
            <ul>{s.body.map((b) => <li key={b}>{b}</li>)}</ul>
          </section>
        ))}
      </div>
      <div className="modal-actions">
        <button type="button" onClick={() => openUrl.mutate(DISCORD_URL)} title={DISCORD_URL}>Discord</button>
        <button type="button" onClick={() => openUrl.mutate(MANAGER_MODDB_URL)} title={MANAGER_MODDB_URL}>Mod DB page</button>
        {openUrl.isError && <span className="error-text small">{openUrl.error.message}</span>}
        <span className="header-spacer" />
        <button type="button" className="primary" onClick={onClose}>Close</button>
      </div>
    </Modal>
  );
}
