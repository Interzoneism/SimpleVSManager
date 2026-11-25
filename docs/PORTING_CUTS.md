# Simple VS Manager 2.3.0: what not to port

Agreed with Martin on 2026-11-24. Based on a read-only review of the WPF app (`VintageStoryModManager/`, master `4f629e4`). Milestones refer to `TAURI_MIGRATION_PLAN.md` section 8.

## A. Dead code (unreachable in 2.3.0, skip entirely)

| What | Evidence | Saves |
|---|---|---|
| `ModCompatibilityCommentsService` (695 lines, scrapes ModDB comments and scores them) | Only caller is `ExperimentalCompReviewMenuItem_OnClick`, which no XAML wires up. | M2 |
| Old mod DB search inside `ModDatabaseService` (`SearchModsAsync`, most downloaded, 10/30-day downloads, new, recently added/updated, trending, random, local search scoring, roughly lines 436 to 900) | Zero callers. The browser uses `ModApiService.QueryModsAsync`. | M2 |
| Old "Database tab" in `MainViewModel` (`SearchResultsView`, `UpdateSearchResultsAsync`, `CreateSearchResultEntry`, `ViewSection.DatabaseTab`) | `UpdateSearchResultsAsync` is never called; the Mod Database tab hosts `ModBrowserView`. | M2 |
| `UseModDbDesignView` setting | Saved and loaded, never bound in any XAML. | M2, settings import |
| Server install macro (`ServerMacroGenerator`) | Only reachable from an unwired handler. Keep only the `/moddb install id@version` copy command (`ServerCommandBuilder`). | M3 |
| `BulkCompatibilityPromptWindow` + `CompatibilityDecisionKind` | Never instantiated. | M2 |
| `Views/Controls/InstalledModsView`, `NavigationHeader`, `Design/*` | Never referenced / designer only. | none |
| Unwired handlers: save/load/delete cloud modlist menu items, delete cloud auth, install mod button, save installed mods PDF menu item, ModDB card preview click | No XAML references. These flows exist elsewhere (cloud dialogs, Save Modlist dialog's PDF option); port those. | M3, M4 |
| Developer Profiles (`DeveloperProfileManager`, fake identities, hidden menu) | `DevDebug = false`, menu item collapsed. | M5 |
| `StatusLogService` | Already a no-op. | none |

## B. Debug and diagnostics (use `tracing`, don't port as features)

| What | Why | Saves |
|---|---|---|
| `ModLoadingTimingService` (619 lines) | Only output is one log summary at exit; `tracing` spans replace it. | M1/M2 |
| "Debug: show game log lines for selected / any mod" + `ExperimentalModDebugDialog` | Experimental, rarely useful; "Open Logs folder" covers it. | M5 |
| Five logging toggles + trace listener | One "verbose logging" switch. | M5 |

## C. One-time migrations (the importer only reads the 2.3.0 shape)

| What | Saves |
|---|---|
| Legacy Firebase project migration (`FirebaseModlistMigrationService`, legacy API key and DB URL). Release notes: "update to 2.3 first". | M4 |
| Documents to LocalAppData move (`ConfigurationMigrationService`) | M1/M5 |
| `Rebuilt_*` modlist folder move | M3 |
| Legacy config keys (`useDarkVsMode`, `useCorrectThumbnails` inversion, root-level profile data, legacy usage-count shapes) | M1/M5 |
| PDF modlist import via text extraction. Read only the `Subject`/`Keywords` metadata; no text-extracting PDF crate. | M3 |

## D. Merge duplicates instead of porting both

| What | Decision | Saves |
|---|---|---|
| Settings in both menus and `SettingsWindow` | One settings page; menus for actions only. | M5 + UI |
| Presets vs modlists vs automatic "mods backups" (same `SerializablePreset` JSON) | One list: presets = modlists without metadata, backups = auto-saved modlists. Still read `Presets/` and `Backups/`. | M3, M5 |
| "Back up current mods?" prompt plus automatic `ModlistLoaded` backup | Keep only the automatic snapshot. | M3 |
| Four near-identical load-modlist code paths (group, local, cloud, file) | One install-modlist flow with sources. | M3 |
| "Cache all versions locally", "Delete cached mods", "Backup Local Mods" | One mod cache with a size cap. | M2 |
| "Check mods compatibility" vs profile target version vs "Require exact VS version" | Fold the check into the target-version picker. | M2 |
| Help, Menu Guide, Guide dialogs | One Markdown help page. | M5 |

## E. WPF workarounds (drop)

| What | Saves |
|---|---|
| "Faster thumbnails", "Disable hover and transition effects" (use `prefers-reduced-motion`) | M2, M5 |
| "Cache Mod Database results (experimental)" (normal query caching covers it) | M2 |
| "Disable auto-refresh" (keep only if the Rust scan proves slow) | M1/M2 |
| `HtmlToFlowDocument` (599 lines): render HTML in the webview with a sanitizer | M2 |
| `TagCacheService`/`TagFilterService`, `MultiSelectDropdown`, `OverlappingTagPanel`, converters: plain React view logic | M1/M2 |

## F. Post-3.0 (not in the parity target)

| What | Saves |
|---|---|
| Custom theme palette editor + saved themes. Ship Dark VS / Dark / Light; import old custom palettes as CSS variables without an editor. | M5 |
| ModDB HTML page scraping fallback when `/api/mod/{id}` fails; show "unavailable" instead. | M2 |
| "Restore firebase-auth.json using backup" menu; restore automatically at startup when the main file is missing. | M4 |
| "Delete all Simple VS Manager files (Uninstall)" | M5 |
| "Enable server options" switch; always show "Copy server install command". | M3 |

## Effect on the plan

- **M2**: no comments scraper, no second search pipeline, no bulk compatibility window, no HTML-to-FlowDocument, fewer perf toggles.
- **M3**: from M–L to about M. One load flow, no server macros, metadata-only PDF import, presets merged into modlists.
- **M4**: no legacy Firebase migration.
- **M5**: no developer profiles, debug log viewers, palette editor, duplicate settings page, logging toggles or three help dialogs.
