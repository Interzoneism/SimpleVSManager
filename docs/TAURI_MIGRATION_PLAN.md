# Simple VS Manager: Rust + Tauri + React rebuild plan

Status: draft plan, no app code written yet.
Based on: `master` at `4f629e4` (v2.3.0), WPF / .NET 8, about 69k lines of C# and XAML.
Goal: one codebase that ships native builds for Windows, macOS and Linux, with feature parity with 2.3.0 and painless migration for existing users.

---

## 1. Where we are today

### 1.1 Shape of the current app

| Area | Files | Notes |
|---|---|---|
| UI shell | `Views/MainWindow.xaml` (3.2k) + `MainWindow.xaml.cs` (**16k lines**) | A lot of business logic lives in code-behind: launching, backups, PDF export/import, cloud modlists, migrations. This is the biggest porting risk. |
| View models | `MainViewModel` (4.8k), `ModListItemViewModel` (2.5k), `ModBrowserViewModel` (1.7k), config editor VMs | CommunityToolkit.Mvvm. |
| Services | ~45 files in `Services/` | Mostly UI-free and portable in spirit. These map almost 1:1 to Rust modules. |
| Dialogs | ~25 windows in `Views/Dialogs/` | Become React modals/routes. |
| Theming | `DarkVsTheme.xaml`, `MainWindowStyles.xaml`, palette editor | Becomes CSS variables. |
| Config | `DevConfig.cs` (constants), `UserConfigurationService` (3.9k, the settings file) | |

Windows-only dependencies: WPF, WinForms (clipboard), ModernWpfUI, `user32.dll` P/Invoke (bring existing window to front), named `Mutex` (single instance), `explorer.exe /select`, `FileVersionInfo` (game version), `net8.0-windows` / `win-x64` target. QuestPDF, PdfPig, HtmlAgilityPack and YamlDotNet are used for PDF, HTML descriptions and YAML configs.

### 1.2 Feature inventory (what parity means)

**Installed mods**
- Discover mods the way the game does: `.zip` archives, folders with `modinfo.json`, and loose `.cs` / `.dll` (flagged as "not manageable" when there's no `modinfo.json`). Lenient JSON (trailing commas, comments, case-insensitive keys).
- Read mod search paths and disabled mods from `VintagestoryData/clientsettings.json` (`modPaths`, `disabledMods` as `modid` or `modid@version`). Writes are atomic (`.tmp` then `.bkp`), unknown keys preserved.
- Enable/disable, delete, duplicate detection and "Fix duplicate mods", dependency checks and "Resolve dependency issues", error states from game logs.
- Watch the mod folders and `clientsettings.json` for changes (`ModDirectoryWatcher`, `ClientSettingsWatcher`) and rescan incrementally.
- Manifest/icon cache so zips aren't reopened (`Temp Cache/Mod Metadata`, `Mod Icons`).
- Columns (icon, name, version, latest version, authors, tags, side, status, user reports), sort, search, compact view, details pane left/right, multi-select batch actions, tag filter dropdown.

**Mod Database (mods.vintagestory.at)**
- Endpoints in `DevConfig`: `/api/mod/{id}`, `/api/mods?search=…`, most downloaded, recently created/updated, `/api/gameversions`.
- Metadata enrichment for installed mods with a soft-expiry disk cache (`Temp Cache/Mod Database Cache`, schema v2) and image cache.
- Browser view: search, sort, tag/version/side filters, favourites, "only compatible", "exclude installed", load-more paging, ModDB-style design view, faster thumbnails.
- Update single mod, "Update to latest" bulk updates with changelog window, skip versions, exclusions, version picker, compatibility prompts, "target game version" override.
- Cached downloads of every version (`Temp Cache/Cached Mods/<modid>/<version>/…`) for quick rollback.
- Manager self-update notice (checks mod 5545 on ModDB).

**Modlists and presets**
- Presets (save/load mod set + active state + optional versions + optional configs, "exclusive" mode). JSON (`SerializablePreset`).
- Local modlists folder, "Rebuilt" modlists, modlist groups (`ModlistGroupDefinition`) with conflict dialog.
- PDF export/import: a human-readable PDF whose `Subject` holds `SVSM:MODLIST:<base64 json>` and `Keywords` holds `SVSM:CONFIG:<base64>|NONE`. This format must stay readable.
- Install a modlist (download missing, set versions, enable/disable), "always clear" / "always add" behaviours.
- Server helpers: `/moddb install id@version` command copy, server macro files.

**Cloud (Firebase)**
- Anonymous auth via Identity Toolkit REST (`accounts:signUp`, `securetoken` refresh, delete), token stored in `firebase-auth.json` with a backup in `%LOCALAPPDATA%/SVSM Backup`.
- Realtime Database (REST): `/users/{uid}/{slot}`, `/registry`, `/registryOwners`, `/registrySummaries`; slot management, rename, details, public browsing; legacy project migration.
- Compatibility votes / user reports under `compatVotes/{mod}/{version}/users/{user}`, local cache file `compat-votes-cache.json` plus watcher.
- Profanity filter for public text (embedded `swearWords.xml`).

**Game integration**
- Locate game folder and data folder; manual override; "custom shortcut" launch.
- Launch `Vintagestory[.exe] --dataPath <data>`; optional close-after-launch; optional automatic data backup before launch.
- Detect installed game version from `Vintagestory.exe` / `VintagestoryAPI.dll` file version, `VINTAGE_STORY` env var.
- Game session monitor: tail `Logs/client-main*.log|txt`, `server-main*`, detect sessions longer than 30 min, prompt "Confirm working mods!" after 5 sessions (feeds votes).
- Debug views: game log lines mentioning a mod / any installed mod.

**Configs, backups, profiles, misc**
- Mod config scanner + editor (JSON and YAML, tree editor), per-mod config path memory.
- VintagestoryData backups (zip + manifest, dedup'd `.saves` store), restore, max count, custom location. Mods backup restore.
- Game profiles (each: data dir, game dir, target version, shortcut, skipped versions, usage counters), developer profiles.
- Themes: light, dark, Dark VS, custom palettes with editor, saved themes.
- Settings: internet access off switch, auto-refresh off, hover effects off, logging toggles (installs/updates/deletes/errors/app launch), custom manager folder (`CustomConfigFolder.txt`), "clear temp cache", "uninstall (delete all SVSM files)".
- Help, Guide, Menu Guide, Discord link, About.
- Migration of old Documents-based config to `%LOCALAPPDATA%/Simple VS Manager`.

### 1.3 On-disk formats we must keep reading

| File / folder | Where | Notes |
|---|---|---|
| `clientsettings.json` | VintagestoryData | Owned by the game. Preserve key order and unknown keys. |
| `SimpleVSManagerConfiguration.json` | manager dir | ~95 keys incl. `gameProfiles`, `configurationVersion`. Import on first run. |
| `SimpleVSManagerModConfigPaths.json` | manager dir | |
| `Presets/`, `Modlists/`, `Modlists/Rebuilt/` | manager dir | Preset/modlist JSON. Shared with the new app as is. |
| `Temp Cache/*` | manager dir | Disposable. New app can reuse or rebuild. |
| `firebase-auth.json` (+ `SVSM Backup/`) | manager dir / LocalAppData | **Critical**: losing it loses ownership of cloud modlists. |
| `Data Folder Backups/` (+ `data-backup.json`, `.saves`) | manager dir or custom | Must stay restorable. |
| Modlist PDFs | anywhere | Metadata in `Subject` / `Keywords`. |
| `CustomConfigFolder.txt` | next to the exe | Portable/relocated manager folder pointer. |

---

## 2. Target architecture

```
┌──────────────────────────── Tauri window (system WebView) ───────────────────────────┐
│ React + TypeScript (Vite)                                                             │
│  routes: Installed · Mod DB · Modlists · Settings · Dialogs                           │
│  TanStack Query (cache of backend data)  ·  Zustand (UI state)  ·  TanStack Virtual   │
│  generated TS bindings  ◀── tauri-specta ──▶  invoke(commands) / listen(events)       │
└───────────────────────────────────────────────┬──────────────────────────────────────┘
                                                │ IPC (JSON) + custom protocol for images
┌───────────────────────────────────────────────▼──────────────────────────────────────┐
│ src-tauri  (thin app crate)                                                          │
│  commands/*.rs  → validate args, call core, map errors                               │
│  events: mods-changed, download-progress, backup-progress, session-detected, log     │
│  AppState: Arc<RwLock<…>>, tokio runtime, background tasks (watchers, monitors)       │
│  plugins: single-instance, window-state, dialog, opener, clipboard, log, updater(opt)│
└───────────────────────────────────────────────┬──────────────────────────────────────┘
                                                │ plain Rust API, no Tauri types
┌───────────────────────────────────────────────▼──────────────────────────────────────┐
│ svsm-core (library, unit-tested, no UI)                                               │
│  paths · discovery · clientsettings · versions · moddb · cache · update · modlists    │
│  pdf · cloud (firebase) · votes · sessions · backups · profiles · configs · settings  │
│  platform/{windows,macos,linux}.rs  for anything OS-specific                          │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

Principles:
1. **All logic in Rust, UI is a view.** The React side never touches the file system or network directly. This fixes the biggest problem in the current code (logic inside `MainWindow.xaml.cs`) and makes the core testable.
2. **`svsm-core` knows nothing about Tauri.** That keeps tests fast, lets us add a CLI later (handy for servers and for testing on Linux/macOS without a GUI), and keeps OS code in one `platform` module.
3. **Typed IPC.** Commands and event payloads are Rust structs with `serde` + `specta`; TypeScript types are generated, so the two sides can't drift.
4. **Long work is async with progress events.** Scans, downloads, backups and modlist installs run on tokio tasks, report progress through events, and are cancellable via a token.
5. **Images don't go through JSON.** Mod icons and DB thumbnails are served via a custom URI scheme (`svsm://icon/<hash>`) backed by the disk cache.

### 2.1 Technology choices

| Concern | Choice | Replaces |
|---|---|---|
| Shell | Tauri 2.x | WPF |
| Frontend | React + TypeScript + Vite | XAML |
| Data fetching / cache | TanStack Query | ObservableCollections + manual refresh |
| UI state | Zustand | View models |
| Long lists | TanStack Virtual | WPF virtualization (needed: WebKitGTK is slow with big DOMs) |
| Components | Radix UI primitives (menus, dialogs, dropdowns) + own CSS | ModernWpfUI |
| Styling / themes | CSS custom properties per palette key | `DarkVsTheme.xaml`, palette editor |
| HTML mod descriptions | Sanitize with DOMPurify, render directly | HtmlAgilityPack + HtmlToFlowDocument (600 lines gone) |
| IPC typing | `tauri-specta` | n/a |
| Async | `tokio` | Task/async |
| HTTP | `reqwest` (rustls) | HttpClient |
| JSON | `serde_json` with `preserve_order` | System.Text.Json |
| Lenient modinfo | `json5` (or a small tolerant pre-pass) | `AllowTrailingCommas`, custom converters |
| YAML | a maintained serde YAML crate (e.g. `serde_norway`/`serde_yml`, pick during M0) | YamlDotNet |
| Zip | `zip` | System.IO.Compression |
| FS watching | `notify` + `notify-debouncer-full` | FileSystemWatcher |
| Paths | `directories` / `dirs` | `Environment.SpecialFolder` |
| PE version info | `pelite` (works on any OS, game DLLs are PE everywhere) | FileVersionInfo |
| PDF write | `printpdf` (or `genpdf`) | QuestPDF |
| PDF read metadata | `lopdf` | PdfPig |
| Logging | `tracing` + `tracing-appender` (+ `tauri-plugin-log` to surface in UI) | trace listener + activity log |
| Errors | `thiserror` in core, `anyhow` at edges, a serialisable `AppError` for IPC | exceptions |
| Single instance | `tauri-plugin-single-instance` (focuses existing window) | Mutex + user32 |
| Window state | `tauri-plugin-window-state` | `windowLeft/Top/Width/Height` keys |
| Open URL / reveal file | `tauri-plugin-opener` | `Process.Start`, `explorer /select` |
| Clipboard | `tauri-plugin-clipboard-manager` | WinForms clipboard |
| File pickers / message boxes | `tauri-plugin-dialog` + in-app modals | WPF dialogs |

---

## 3. Project layout

Build the new app **inside this repo, next to the WPF app**, so both can share docs and the WPF app keeps shipping until parity:

```
Simple-Mod-Manager/
├─ VintageStoryModManager/          # existing WPF app, untouched until retirement
├─ tauri/                           # new app root
│  ├─ Cargo.toml                    # workspace
│  ├─ crates/
│  │  ├─ svsm-core/
│  │  │  ├─ src/
│  │  │  │  ├─ lib.rs
│  │  │  │  ├─ paths.rs            # manager dir, data dir, game dir resolution
│  │  │  │  ├─ platform/{mod,windows,macos,linux}.rs
│  │  │  │  ├─ discovery/          # zip/folder/cs/dll scanning, modinfo parsing, deps
│  │  │  │  ├─ clientsettings.rs   # read/write VS clientsettings.json
│  │  │  │  ├─ version.rs          # port of VersionStringUtility + game version detection
│  │  │  │  ├─ moddb/              # API client, models, disk cache, images, tags
│  │  │  │  ├─ updates.rs          # download, cache, install, rollback
│  │  │  │  ├─ modlists/           # presets, local modlists, groups, PDF, server cmds
│  │  │  │  ├─ cloud/              # firebase auth, RTDB modlists, votes, validator
│  │  │  │  ├─ sessions.rs         # log tailing, session detection
│  │  │  │  ├─ backups.rs          # data folder backups and restore
│  │  │  │  ├─ configs.rs          # mod config scan, JSON/YAML load/save
│  │  │  │  ├─ profiles.rs         # game profiles, developer profiles
│  │  │  │  ├─ settings/           # new settings model + importer for WPF config
│  │  │  │  └─ activity_log.rs
│  │  │  ├─ tests/fixtures/        # sample mods, clientsettings, presets, PDFs, configs
│  │  │  └─ Cargo.toml
│  │  └─ svsm-cli/                  # optional, later: headless list/install/export
│  ├─ src-tauri/
│  │  ├─ src/{main.rs, state.rs, commands/*.rs, events.rs, protocol.rs}
│  │  ├─ capabilities/default.json  # Tauri 2 permission scopes
│  │  ├─ icons/
│  │  └─ tauri.conf.json
│  ├─ ui/
│  │  ├─ src/
│  │  │  ├─ bindings.ts            # generated by tauri-specta, don't edit
│  │  │  ├─ api/                   # query hooks wrapping commands
│  │  │  ├─ features/{installed,moddb,modlists,cloud,settings,configs,backups,profiles}/
│  │  │  ├─ components/            # table, details pane, tag dropdown, modal, toasts
│  │  │  ├─ theme/                 # palettes, ThemeProvider, palette editor
│  │  │  └─ main.tsx
│  │  ├─ index.html
│  │  ├─ package.json
│  │  └─ vite.config.ts
│  └─ README.md
└─ TAURI_MIGRATION_PLAN.md
```

---

## 4. Feature mapping

Size: S = days, M = 1–2 weeks, L = several weeks (one person, part-time-ish, with AI help; rough).

| Current (C#) | New home | How it changes | Size |
|---|---|---|---|
| `DataDirectoryLocator`, `GameDirectoryLocator`, `ModCacheLocator`, `CustomConfigFolderManager`, `DevConfig.GetManagerDirectory` | `core::paths` + `core::platform` | One resolver with per-OS candidate lists (section 5). Keeps `CustomConfigFolder.txt` and portable `data/` support. | M |
| `ModDiscoveryService`, `ModManifestCacheService`, `ModDirectoryWatcher` | `core::discovery` | Parallel scan with `rayon`; cache keyed by path + size + mtime; watcher via `notify`, debounced, emits `mods-changed`. | L |
| `ClientSettingsStore`, `ClientSettingsWatcher` | `core::clientsettings` | Edit a `serde_json::Value` with `preserve_order`; same tmp/bkp atomic write. | S |
| `VintageStoryVersionLocator`, `VintageStoryGameVersionService`, `VersionStringUtility`, `TargetGameVersion*` | `core::version` | Port comparison logic line by line with tests. Read PE version from `VintagestoryAPI.dll` with `pelite` on every OS; macOS fallback to `Info.plist`. | S |
| `ModApiService`, `ModDatabaseService`, `ModDatabaseCacheService`, `TagCacheService`, `TagFilterService`, `ModImageCacheService` | `core::moddb` | Same endpoints, `reqwest` with a 4-request semaphore (from `DevConfig`), same cache schema v2 if cheap, otherwise new cache (it's disposable). Tag filtering moves to the UI (it's view logic). | L |
| `ModUpdateService`, `ModCacheService` | `core::updates` | Stream download to temp, verify, move into mods folder, keep cached copy; progress events. | M |
| `ModCompatibilityCommentsService` | `core::moddb::comments` | Scrapes ModDB comment HTML today; use `scraper` crate. Keep behind a flag, it's fragile. | S |
| Presets, modlists, groups, `ModlistGroupService`, Rebuilt modlists | `core::modlists` | Same JSON models via serde (`#[serde(rename_all = "camelCase")]` to match current output; verify against fixtures). | M |
| `PdfModlistSerializer` + PDF code in `MainWindow.xaml.cs` | `core::modlists::pdf` | Write with `printpdf`, put payload in Info `Subject`/`Keywords` with the same `SVSM:` prefixes; read with `lopdf`. Round-trip tests with PDFs exported by 2.3.0. | M |
| `ServerCommandBuilder`, `ServerMacroGenerator` | `core::modlists::server` | Straight port. | S |
| `FirebaseAnonymousAuthenticator`, `FirebaseModlistStore`, `FirebaseModlistMigrationService` | `core::cloud` | Same REST calls via `reqwest`. Reuse the existing `firebase-auth.json` so users keep their identity. Keys/URLs in a `cloud::config` module. | L |
| `ModVersionVoteService`, `VotesCacheWatcher`, `OnlineModlistContentValidator` | `core::cloud::votes`, `core::cloud::validator` | Port; `swearWords.xml` via `include_str!`. | M |
| `GameSessionMonitor`, `ModUsageTracking` | `core::sessions` | Tail logs with `notify` + seek offsets; same timestamp regexes; same 30 min / 5 session thresholds. | M |
| Game launch (code-behind) | `core::platform::launch_game` | Per-OS launch (section 5.3). Custom shortcut becomes "custom launch target" (any file or command). | S |
| `DataBackupService`, restore dialogs | `core::backups` | Same zip + manifest + `.saves` store layout so old backups restore. Progress + cancel. | M |
| `ModConfiguration*` VMs, scan for configs | `core::configs` + React tree editor | Load JSON/YAML into a generic tree (`serde_json::Value`), edit in UI, save back preserving format. | M |
| `UserConfigurationService`, `SettingsService` | `core::settings` | New, smaller, versioned `settings.json` (serde with defaults). One-time importer from `SimpleVSManagerConfiguration.json`. | M |
| Game profiles, `DeveloperProfileManager` | `core::profiles` | Profiles become first-class: each holds paths, overrides and per-profile state. | S |
| `ModActivityLoggingService`, trace listener, status log | `tracing` + `core::activity_log` | Status bar subscribes to a `log` event stream. | S |
| `ConfigurationMigrationService` | `core::settings::import` | Also covers Documents to LocalAppData legacy move. | S |
| `InternetAccessManager` | `core::net` | Single `reqwest::Client` wrapper that refuses calls when offline mode is on. | S |
| Themes, palette editor | `ui/theme` | Palette = map of CSS variables; import existing `customThemes` / `themePalette` from old config. | M |
| Main window, installed list, details pane, menus | `ui/features/installed` | Virtualized table, column chooser, keyboard shortcuts, context menu. The UI is the largest chunk of work. | L |
| Mod browser (`ModBrowserView`) | `ui/features/moddb` | Card grid + list mode, infinite scroll, filters. | L |
| ~25 dialogs | modal components | Many collapse into a few generic ones (confirm, choose, text input, progress). | M |
| Help / Guide / Menu Guide | Markdown files rendered in-app | Easier to edit than XAML. | S |
| "Uninstall: delete all SVSM files" | `core::paths::purge` | Must also handle per-OS dirs. | S |

Things to drop or rethink:
- `StatusLogService` is already a placeholder: drop.
- `BatchedObservableCollection`, converters, `BindingProxy`, `HoverEffectHelper`, `OverlappingTagPanel`, design-time data: WPF-only, no port needed.
- "Disable hover effects" and "faster thumbnails" were WPF perf workarounds; keep as settings only if the new UI needs them.

---

## 5. Cross-platform concerns

### 5.1 Paths

| | Windows | macOS | Linux |
|---|---|---|---|
| VintagestoryData (default) | `%APPDATA%\VintagestoryData` | `~/Library/Application Support/VintagestoryData` | `~/.config/VintagestoryData` |
| Game install (common) | `%APPDATA%\Vintagestory`, `Program Files\Vintagestory`, `C:\Games`, `D:\Games` | `/Applications/Vintage Story.app`, `~/Applications/…` | `~/.local/share/vintagestory`, `/opt/vintagestory`, Flatpak and AUR locations |
| Executable | `Vintagestory.exe` | `Vintagestory.app/Contents/MacOS/Vintagestory` | `Vintagestory` (or `dotnet Vintagestory.dll`) |
| Manager dir | `%LOCALAPPDATA%\Simple VS Manager` (same as today) | `~/Library/Application Support/Simple VS Manager` | `$XDG_DATA_HOME/simple-vs-manager` (default `~/.local/share/…`) |
| Cache (`Temp Cache`) | inside manager dir (as today) | same | could move to `$XDG_CACHE_HOME`; keep inside manager dir for parity at first |

The macOS and Linux rows are educated defaults and **must be verified on real installs in M0** (installer tarball, Flatpak, AUR, Mac app bundle). Every candidate list also keeps: env var `VINTAGE_STORY`, the folder next to the manager exe, `VSFOLDER`, the current dir, and the user's manual choice, which always wins.

Data dir rule: the game puts the data folder in the OS config dir unless `--dataPath` is passed, so the manager should always pass `--dataPath` on launch, as it does today.

### 5.2 File system behaviour
- **Case sensitivity.** Linux (and some macOS volumes) are case-sensitive. Compare **mod ids** case-insensitively (the game does), but never case-fold **paths**. The C# code uses `OrdinalIgnoreCase` for path dictionaries in many places; don't copy that.
- **Path separators and normalisation.** Use `PathBuf` everywhere; store relative paths in modlists with `/`; normalise `modPaths` entries from `clientsettings.json` that may be relative to the data dir.
- **Watchers.** `notify` uses ReadDirectoryChangesW / FSEvents / inotify. FSEvents is coarse and inotify has a watch limit, so debounce (≈500 ms) and always do a cheap "rescan changed dirs" rather than trusting individual events.
- **Atomic writes.** Write to temp in the same dir, `fsync`, rename. On Windows, retry rename on sharing violations (antivirus, game holding the file).
- **Files in use.** Windows locks mod zips while the game runs; detect "game is running" (log tail or process check) and warn before updates/deletes.
- **Permissions.** Linux AppImage and macOS app bundle are read-only; never write next to the executable except in explicit portable mode.

### 5.3 Launching the game
- Windows: spawn `Vintagestory.exe --dataPath <dir>` with working dir = game dir.
- Linux: spawn `Vintagestory` if executable; otherwise `dotnet Vintagestory.dll`. Clear AppImage-specific env vars (`LD_LIBRARY_PATH`, `APPDIR`…) before spawning so the game doesn't inherit our bundled libs. For Flatpak installs, use `flatpak run <app id> --dataPath …`.
- macOS: run the binary inside the bundle with args (or `open -a <app> --args --dataPath …`).
- Custom target: any executable, `.lnk`/`.url` (Windows), `.desktop` (Linux), `.app` (macOS) opened with the OS opener.
- Detach the child process so closing the manager doesn't kill the game.

### 5.4 OS integration
- Reveal in folder: `tauri-plugin-opener` `revealItemInDir` (Explorer `/select`, Finder `-R`, Linux via FileManager1 D-Bus with `xdg-open` fallback).
- Single instance: plugin forwards argv to the running instance and focuses it. Later this enables `svsm://` deep links (e.g. "Install modlist" from a web page).
- Menus: today's menu bar becomes an in-app React menu bar (consistent on all three OS). On macOS also set a minimal native app menu (About, Quit, Edit for copy/paste shortcuts).
- Keyboard shortcuts: map Ctrl to Cmd on macOS.

### 5.5 WebView differences
- Windows WebView2 (Chromium), macOS WKWebView (Safari), Linux WebKitGTK. WebKitGTK is the slowest and has the most quirks.
- Test every milestone on all three. Keep the DOM small (virtualize), avoid heavy CSS filters/backdrop blur, use system font stack with a bundled fallback.
- Windows needs the WebView2 runtime: present on Windows 11 and updated Windows 10; the NSIS installer can bootstrap it.

### 5.6 Security (Tauri capabilities)
- Frontend gets no generic `fs` or `http` access; only our commands. Commands validate that paths are inside known roots (mods dirs, data dir, manager dir, user-picked file).
- CSP: allow `svsm:` protocol and `https://moddbcdn.vintagestory.at` images only.
- Mod descriptions are untrusted HTML: sanitize with DOMPurify, open links externally.
- Firebase web API keys are public client keys by design (they're in the current source too). Move them to `cloud::config` and rely on RTDB security rules, which should be reviewed during M4.

---

## 6. Build, test and packaging

### 6.1 Toolchain
- Rust stable (pin in `rust-toolchain.toml`), Node LTS + pnpm, Tauri CLI 2.x.
- Linux build deps: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `patchelf`.
- macOS: Xcode command line tools.
- Windows: MSVC build tools + WebView2 (already on Martin's machine with VS).

### 6.2 Tests
- `svsm-core` unit tests with fixtures copied from real installs: modinfo variants, clientsettings, presets/modlists from 2.3.0, PDFs exported by 2.3.0, data backups made by 2.3.0, YAML/JSON configs, game log snippets.
- Golden tests for anything users share: PDF modlists, JSON modlists, cloud payloads.
- HTTP tests with recorded responses (`wiremock` or `mockito`) so CI never hits ModDB/Firebase.
- UI: Vitest + Testing Library for components; a few Playwright smoke tests against `vite dev` with mocked commands.
- Manual test matrix per milestone: Windows 11, macOS (Apple silicon), Ubuntu LTS (X11 + Wayland), one Flatpak'd game install.

### 6.3 Packaging

| OS | Artifacts | Notes |
|---|---|---|
| Windows | NSIS installer `.exe`, plus a **portable zip** | Portable mode matters: the current app is "portable, user-friendly". Portable = `CustomConfigFolder.txt`/`data/` next to exe. Code signing optional (SmartScreen warning without it). |
| macOS | `.dmg` with universal binary (arm64 + x86_64) | Unsigned apps get Gatekeeper warnings; signing + notarization needs an Apple Developer account. Decide before first public mac release. |
| Linux | AppImage, `.deb`, `.rpm` | Flatpak / Flathub later (separate manifest). AUR package can be community-maintained. |

### 6.4 Where builds happen
Tauri apps can't practically be cross-compiled for macOS, so each OS builds on that OS. Options:
1. **GitHub Actions matrix with `tauri-action`** (simplest, free for public repos). Needs pushing to GitHub, which conflicts with the current local-only workflow.
2. **Local builds**: Windows on Martin's PC, Linux in WSL2 or a VM, macOS needs a Mac.

This is an open decision (section 9).

### 6.5 Versioning and updates
- Single version source: `tauri/Cargo.toml` workspace version, copied into `tauri.conf.json` and `package.json` by a small script (replaces `Directory.Build.props`).
- Keep the current behaviour: check ModDB mod 5545 and show "Manager update available!". Optionally add `tauri-plugin-updater` later (needs signed update manifests hosted somewhere).

---

## 7. Migrating existing users

1. On first start, look for the WPF manager dir (and `CustomConfigFolder.txt` next to the old exe if the user points to it).
2. Import `SimpleVSManagerConfiguration.json` into the new `settings.json` (paths, profiles, theme, column visibility, filters, skipped versions, exclusions, usage counters). Keep the old file untouched so the WPF app still works.
3. Share in place: `Presets/`, `Modlists/`, `Data Folder Backups/`, `Temp Cache/Cached Mods/`. Formats are kept, so both apps can read them during the transition.
4. **Copy and verify `firebase-auth.json`** (and check `SVSM Backup/`). Never delete or rewrite the original during import.
5. Rebuild other caches (metadata, images, tags) rather than migrating them.
6. Show a one-time "Imported from Simple VS Manager 2.x" summary.

---

## 8. Milestones

Each milestone ends with a build that runs on all three OS.

**M0: Foundations and spike** (S–M)
- Scaffold workspace, Vite + React + TS, tauri-specta bindings, error type, tracing, CI-less local build scripts.
- Path resolver + platform module; **verify real game/data paths on macOS and Linux installs**.
- Perf spike: render 500 fake mods with icons in a virtualized table on WebKitGTK.
- Pick YAML and PDF crates; confirm PDF metadata round-trip with a 2.3.0-exported PDF.
- Exit: empty app window on 3 OS that shows detected game dir, data dir and game version.

**M1: Installed mods, enable/disable, launch** (L)
- Discovery (zip/folder/cs/dll), manifest + icon cache, `clientsettings.json` read/write, watcher, dependency and duplicate checks, delete mod.
- Installed mods table: columns, sort, search, details pane, multi-select, context menu, reveal in folder, copy mod id.
- Game launch with `--dataPath`, close-after-launch, custom launch target. Settings import from WPF config (paths and profiles only).
- Exit: a usable minimal manager on all OS. Good point for a private test build.

**M2: Mod Database and updates** (L)
- ModDB client, metadata enrichment + disk cache, latest version column, update one / update all with changelog, skip versions, exclusions, version picker, target game version (the compatibility check and "require exact version" fold into this picker), cached versions in one size-capped mod cache, offline mode.
- Mod browser view: search, filters, tags, favourites, sort, paging. The API returns the whole catalogue in one response (no paging), so it is cached on disk and filtered locally.
- Manager update notice.
- Not ported (see `NOT_WORTH_PORTING.md`): comments scraper, the old second search pipeline, bulk compatibility window, ModDB HTML page-scrape fallback (show "unavailable" instead), HtmlToFlowDocument (sanitized HTML in the webview), "cache Mod DB results" and thumbnail perf toggles.

**M3: Modlists and presets** (M–L)
- Presets save/load, local modlists, Rebuilt modlists, groups + conflicts, install modlist flow with progress, PDF export/import (compatible with 2.3.0), server command and macro generation, include configs.

**M4: Cloud and community** (L)
- Firebase anonymous auth reusing `firebase-auth.json`, cloud slots (save/rename/delete/browse/details), legacy migration, content validator.
- Compatibility votes / user reports, votes cache, game session monitor and "Confirm working mods!" prompt, user reports column.

**M5: The rest of parity** (M–L)
- Mod config scanner + JSON/YAML tree editor, data folder backups (auto before launch, max count, custom location, restore), mods backup restore, game profiles + developer profiles, themes + palette editor, activity logging toggles, debug log viewers, clear cache, uninstall, help/guide/menu guide.
- Full settings import (themes, filters, usage counters, etc.).

**M6: Hardening and release** (M)
- Parity checklist pass against section 1.2, test matrix on all OS, accessibility basics (keyboard nav, focus), packaging for all targets, release notes, ModDB page update.
- Ship as 3.0.0 beta alongside 2.x; retire the WPF app after a beta period.

Suggested order rationale: M1 gives a working cross-platform tool early (Linux and macOS users have nothing today), M2 adds the most-used online features, and cloud work (M4) is held back until the core is stable because it touches users' shared data.

---

## 9. Decisions for Martin

1. **Where builds happen.** Local-only means macOS builds need a Mac; GitHub Actions would build all three automatically but needs pushes. Recommendation: local Windows + WSL2 Linux now, decide on mac builds before M6.
2. **macOS signing.** Pay for an Apple Developer account for notarized builds, or ship unsigned with instructions. Recommendation: unsigned for beta.
3. **Repo layout.** New app in `tauri/` in this repo (recommended) vs a new repo.
4. **Settings sharing.** New `settings.json` imported once (recommended) vs reading/writing the WPF file directly.
5. **UI fidelity.** Recreate the current look closely vs take the chance to redesign. Recommendation: keep layout and terminology (users know it), refresh visuals.
6. **License.** `CONTRIBUTIONS.md` mentions a `LICENSE` file that isn't in the repo. Worth adding before a 3.0 release.

---

## 10. Risks

| Risk | Mitigation |
|---|---|
| Logic hidden in the 16k-line `MainWindow.xaml.cs` gets missed | Before each milestone, grep the code-behind for that area and list behaviours in the parity checklist. |
| WebKitGTK performance with large mod lists | Virtualize from day one; spike in M0. |
| Users losing cloud identity | Never modify the original `firebase-auth.json`; copy + verify; restore-from-backup action kept. |
| Old PDFs / backups not loading | Golden tests using files produced by 2.3.0. |
| Unknown macOS/Linux game layouts (Flatpak, AUR, Steam-less tarballs) | Verify in M0; always allow manual folder selection; add `VINTAGE_STORY` env var support. |
| ModDB HTML scraping (comments) breaks | Isolate, feature-flag, fail soft. |
| Scope: ~69k lines is a lot to port | Strict milestones, parity checklist, drop WPF-only workarounds. |
