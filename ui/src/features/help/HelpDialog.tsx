import { Modal } from "../modlists/Modal";

const SECTIONS: { title: string; body: string[] }[] = [
  {
    title: "Installed mods",
    body: [
      "Everything in your Mods folder. Click the checkbox to turn a mod on or off; select several with Ctrl or Shift.",
      "The Latest column shows updates from the Mod DB for your game version. Update all skips mods you left out.",
      "Delete moves a mod to the Recycle Bin / Trash. Older versions stay in your mod library for rollback.",
      "Drag the divider to resize a pane, or focus it and use the arrow keys. Double-click the divider to reset its width.",
    ],
  },
  {
    title: "Modlists",
    body: [
      "Save your current mods as a modlist (exact versions, can be shared) or a preset (only which mods are on).",
      "Switch to a modlist and your Mods folder ends up with exactly that list. Other mods go to your mod library, so switching back needs no download.",
      "Enable Quick switch in the review to reuse your config-file choice for saved modlists and groups. Use Load to open the review, or turn off Quick switch from the Modlists toolbar.",
      "Groups combine several modlists. Backups are saved automatically before a modlist loads and before updates or deletes.",
      "Export a modlist as PDF or JSON; Simple VS Manager 2.3 can load both. Drop a .json or .pdf on the window to load it.",
    ],
  },
  {
    title: "Community modlists",
    body: [
      "Modlists other players published. Load one, save a copy, or follow it to get its updates.",
      "Publish your own to five cloud slots under your player name.",
    ],
  },
  {
    title: "Browse mods",
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
      "Game profiles: separate folders and target versions, for example vanilla and modded.",
      "Data folder backups: worlds, settings and mods in one backup; optional before every launch.",
      "Offline mode stops all network use. Cached Mod DB data and your mod library keep working.",
    ],
  },
];

/** One help page instead of 2.x's Help, Guide and Menu Guide dialogs. */
export function HelpDialog({ onClose }: { onClose: () => void }) {
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
        <span className="header-spacer" />
        <button type="button" className="primary" onClick={onClose}>Close</button>
      </div>
    </Modal>
  );
}
