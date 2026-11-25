# Simple VS Manager 3 (Tauri)

Cross-platform rebuild of Simple VS Manager in Rust + Tauri 2 + React. See
`docs/TAURI_MIGRATION_PLAN.md` for the full plan and `docs/PORTING_CUTS.md` for what was
deliberately not ported. The original WPF app lives in the Simple-Mod-Manager repository.

**Status: M1 (installed mods).** Lists installed mods (zip, folder, loose .cs/.dll) with icons,
search, sort and a details pane; enables/disables mods in `clientsettings.json`; flags duplicates
and missing or broken dependencies; moves mods to the Recycle Bin/Trash; refreshes when the mod
folders change; lets you pick the game and data folders; launches the game with `--dataPath`.

**M2 (Mod DB and updates):** looks up installed mods on mods.vintagestory.at (4 requests at a
time, cached on disk for 30 minutes, works offline from the cache); "Latest" column with update
offers for the target game version; Update all / update one with changelogs; version picker for
rollback; skip a version; leave mods out of Update all; a Mod DB tab that searches the whole
catalogue locally (the API has no paging) and installs mods; one size-capped cache of old and
downloaded versions; manager update notice. 2.x skips, exclusions and related settings are
imported once.

**M3 (Modlists):** a Modlists tab with modlists, presets, groups and automatic backups in one
list (all the 2.3.0 JSON format; `Presets/`, `Modlists/` incl. `Rebuilt/`, `Backups/` are shared with
2.x). Save the installed mods as a modlist (versions, details, chosen config files) or preset (on/off
only). One load flow for every source (saved entry, imported file, dropped .json/.pdf): preview of
what changes, then "Only this list's mods" (installs, version switches, other mods to the trash),
"Add to my mods", or "Just switch mods on and off". A backup of the current mods (and any config
files about to be overwritten) is saved first; backups are also made before updates and deletes
(newest 10 kept). Groups (`.svsmgroup`) merge modlists with per-mod conflict choices. PDF export
and import compatible with 2.3.0 (only the PDF metadata is read). "Copy server commands" gives
`/moddb install id@version` lines. Never removes anything from the game's own install folder.

**Mod library and switching:** the Mods folder always holds exactly one version of each mod
(Vintage Story only loads the highest of duplicate versions and duplicate code mods can still
break, so several versions side by side aren't safe even when some are disabled). Every other
version lives in the mod library (`Temp Cache/Cached Mods`, the 2.x cache layout). "Switch to"
a modlist puts mods that aren't in it into the library instead of the trash and takes what it
needs from the library first, so switching back and forth is local and works offline. The
library's size limit never deletes a version that a saved modlist, preset, group or backup uses.
The Modlists tab marks the list that matches the installed mods as Active and shows how many
mods a list would still have to download. Folder mods and loose .cs/.dll files can't go into
the library and still go to the trash.

**M4 (Cloud and community):** an Online tab with the public modlists of every player (2.x's
Firebase project; public reads need no sign-in), search/sort, hide a modlist or all from an
author, Load straight from the cloud, Save a copy, or Follow: a followed copy on the Modlists tab
shows "Update" when the author publishes a new revision and "Get the update" replaces it
(SVSM-Dev 8f81f69 subscriptions, revisions, follower counts and new-follower notices). Publish
any saved modlist to your five cloud slots (new revisions, delete), with 2.x's blocked-word check.
Player reports for each installed mod version in its details, and a "Confirm working mods" prompt
after five 30+ minute sessions found in the game's client logs.
Cloud identity: reads 2.x's `firebase-auth.json` and never changes it (refreshed tokens go to
`svsm3-firebase-session.json`), restores it from `%LOCALAPPDATA%/SVSM Backup` when missing, and
never creates a second account when the existing one can't sign in. Identities still on the old
cloud project can browse and follow, but publishing and reports need the move to the new project.
"Move my cloud identity" (Online tab) does that move like 2.3.0: it copies the player's
old-project slots to the new project, keeps the old identity as `firebase-auth.legacy.json` (and
the SVSM Backup copy likewise), and puts everything back if any step fails.

**M5 (the rest):** Settings has game profiles (own game folder, data folder and target game
version each; 2.x profiles are imported), data folder backups in the 2.x format (`Data Folder
Backups/*.zip` + a shared `.saves` store, optional before every launch, keep the newest N per
data folder and game version, custom location, restore backs up the current state first),
Dark VS / Dark / Light themes plus 2.x custom colors, and a detailed-logging switch. A Configs tab
edits files in `ModConfig` (JSON as fields or text, YAML as text; checked before saving, the
previous version kept for "Undo last save" in `Temp Cache/Config Backups`). One help page.
Not ported (cut list): developer profiles, the palette editor, debug log viewers, the five
logging toggles, "Delete all Simple VS Manager files".

What it writes: `clientsettings.json` (only `disabledMods`, with a `.bkp` of the previous file),
its own `svsm3-settings.json` in the manager folder, modlist/preset/group/backup files in `Modlists/`, `Presets/` and `Backups/`, config files in the data folder's `ModConfig` when a loaded modlist carries them, mods in the data folder's `Mods`, and caches
under `Temp Cache/` (`svsm3 Mod Database/`, `svsm3 Downloads/`, and `Cached Mods/`, which uses the
2.x layout so both apps share it). The 2.x settings file is only read.

## Layout

```
tauri/
├─ crates/svsm-core/   all logic, no Tauri; unit tested (cargo test)
├─ src-tauri/          thin Tauri shell: commands, state, plugins, bundling config
├─ ui/                 React + TypeScript (Vite root); ui/src/bindings.ts is generated
├─ package.json        frontend deps + scripts (run everything from this folder)
└─ vite.config.ts
```

## Prerequisites

If `pnpm` fails with a Corepack error, run `npx pnpm@10 <command>` instead (this repo pins pnpm 10 in
`package.json`).


- Rust stable (`rustup`), Node 20+ and pnpm (`npm i -g pnpm`).
- **Windows:** Visual Studio 2022 "Desktop development with C++" (MSVC) and the WebView2
  runtime (already on Windows 11).
- **macOS:** Xcode command line tools (`xcode-select --install`).
- **Linux (Debian/Ubuntu):**
  `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev build-essential file`

## Commands (from `tauri/`)

| Command | What it does |
|---|---|
| `pnpm install` | Install frontend deps (once). |
| `pnpm app:dev` | Run the app with hot reload. Also regenerates `ui/src/bindings.ts`. |
| `pnpm app:build` | Release build + installers for the current OS (Windows: NSIS; macOS: .app/.dmg; Linux: AppImage/.deb/.rpm). |
| `pnpm bindings` | Regenerate TypeScript bindings without opening a window. |
| `pnpm check` | Rust tests, TypeScript type check and UI tests. |
| `cargo run -p svsm-core --example pe_version -- <file>` | Print the version resource of a game DLL/EXE. |
| `cargo run --release -p svsm-core --example scan -- <VintagestoryData>` | Time a mod scan of a data folder. |
| `cargo run -p svsm-core --example modlist_pdf -- <modlist.json> <out.pdf>` | Write a modlist as a PDF (same as Export PDF). |
| `cargo run -p svsm-core --example cloud_live -- <manager dir>` | Read-only check against the real cloud (needs an existing firebase-auth.json). |
| `cargo test -p simple-vs-manager -- --ignored` | End-to-end modlist load against a temp folder, downloading from the real Mod DB. |
| `cargo run -p svsm-core --example seed_moddb -- <manager dir> <mods list.json> [mod_<id>.json ...]` | Fill the Mod DB cache from saved API responses, to try the UI in offline mode. |

Installers land in `target/release/bundle/`. To build only one type: `pnpm tauri build --bundles nsis`.

## Rules of the road

- Logic goes in `svsm-core`. Commands in `src-tauri/src/commands.rs` stay thin.
- Every OS decision goes in `svsm-core/src/platform.rs`.
- Code that looks at the machine takes a `HostEnv`, so tests can fake a home folder.
- Never case-fold paths (Linux is case-sensitive); mod ids compare case-insensitively.
- The UI calls the backend only through hooks in `ui/src/api`, never `commands` directly.
- The frontend has no generic file system, shell or HTTP permissions (`src-tauri/capabilities`).
- Don't write to `SimpleVSManagerConfiguration.json` or `firebase-auth.json`; 2.x owns them.
