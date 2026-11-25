//! End-to-end checks of the modlist commands against a throwaway machine (temporary folders
//! for the manager and the game's data folder). The test marked `#[ignore]` downloads from the
//! real Mod DB: `cargo test -p simple-vs-manager -- --ignored`.

use std::path::{Path, PathBuf};

use svsm_core::modlists::store::ModlistStore;
use svsm_core::modlists::{LoadMode, ModlistKind};
use svsm_core::platform::Os;
use svsm_core::HostEnv;
use tauri::Manager;

use crate::modlist_commands::*;
use crate::state::AppState;

struct Machine {
    _tmp: tempfile::TempDir,
    data: PathBuf,
    manager: PathBuf,
    app: tauri::App<tauri::test::MockRuntime>,
}

fn zip_mod(path: &Path, id: &str, version: &str) {
    use std::io::Write;
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    z.start_file("modinfo.json", zip::write::SimpleFileOptions::default()).unwrap();
    write!(z, r#"{{"type":"code","modid":"{id}","name":"{id}","version":"{version}"}}"#).unwrap();
    z.finish().unwrap();
}

fn machine() -> Machine {
    machine_with(None)
}

fn machine_with(cloud: Option<svsm_core::cloud::CloudConfig>) -> Machine {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let data = root.join("roaming/VintagestoryData");
    std::fs::create_dir_all(data.join("Mods")).unwrap();
    std::fs::create_dir_all(data.join("ModConfig")).unwrap();
    zip_mod(&data.join("Mods/CarryOn-1.14.0.zip"), "carryon", "1.14.0");
    zip_mod(&data.join("Mods/ExtraTestMod.zip"), "extratestmod", "1.0.0");
    zip_mod(&data.join("Mods/HandyTweaks.zip"), "handytweaks", "1.0.0");
    std::fs::write(
        data.join("clientsettings.json"),
        r#"{"stringSettings":{"playername":"Tester","playeruid":"PlayerUid.1"},"stringListSettings":{"modPaths":[],"disabledMods":["handytweaks@1.0.0"]}}"#,
    )
    .unwrap();
    std::fs::write(data.join("ModConfig/CarryOn.json"), r#"{ "mine": true }"#).unwrap();
    let host = HostEnv {
        os: Os::current(),
        home: Some(root.join("home")),
        config_dir: Some(root.join("roaming")),
        data_local_dir: Some(root.join("local")),
        program_files: vec![],
        exe_dir: Some(root.join("app")),
        current_dir: Some(root.join("cwd")),
        vintage_story_env: None,
    };
    let mut state = AppState::with_host(host);
    if let Some(c) = cloud {
        state.cloud_config = c;
    }
    let manager = state.manager_dir();
    assert!(manager.starts_with(&root), "never touch the real manager folder");
    state.rescan().unwrap();
    let app = tauri::test::mock_builder().manage(state).build(tauri::test::mock_context(tauri::test::noop_assets())).unwrap();
    tauri_specta::Builder::<tauri::test::MockRuntime>::new()
        .events(tauri_specta::collect_events![crate::events::ModsChanged, crate::events::UpdateProgress, crate::events::ModlistProgress, crate::events::BackupProgress])
        .mount_events(&app);
    Machine { _tmp: tmp, data, manager, app }
}

fn run<T>(f: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(f)
}

#[test]
fn save_export_import_and_switch_states() {
    let m = machine();
    let state = m.app.state::<AppState>();
    let defaults = run(modlist_save_defaults(state.clone())).unwrap();
    assert_eq!(defaults.uploader.as_deref(), Some("Tester"));
    assert_eq!(defaults.configs[0].mod_id.as_deref(), Some("carryon"));
    assert_eq!(defaults.disabled_mods.iter().map(|d| d.mod_id.as_str()).collect::<Vec<_>>(), ["handytweaks"]);

    let saved = run(save_modlist(
        state.clone(),
        SaveModlistRequest {
            name: "Mine".into(),
            description: Some("desc".into()),
            game_version: None,
            uploader: defaults.uploader.clone(),
            configs: vec![ConfigChoice { relative_path: "CarryOn.json".into(), mod_id: Some("carryon".into()) }],
            overwrite: false,
            icon: Some("md-sword".into()),
            icon_color: Some("sage".into()),
        },
    ))
    .unwrap();
    // The disabled mod (handytweaks) is left out of modlists.
    assert_eq!(saved.saved.mods.len(), 2);
    assert!(saved.saved.mods.iter().all(|x| x.mod_id != "handytweaks"));
    assert_eq!(saved.saved.icon.as_deref(), Some("md-sword"));
    assert_eq!(saved.saved.config_count, 1);

    // Updating it with the enabled mods: still 2 mods, details and the config stay.
    std::fs::write(
        m.manager.join("Modlists/Mine.json"),
        std::fs::read_to_string(m.manager.join("Modlists/Mine.json")).unwrap().replace("\"carryon\"", "\"carryon\", \"x\": 1").replace("\"modId\": \"extratestmod\"", "\"modId\": \"oldmod\""),
    )
    .unwrap();
    let updated = run(crate::modlist_commands::update_modlist_mods(state.clone(), saved.saved.id.clone())).unwrap();
    let mut ids: Vec<_> = updated.mods.iter().map(|m| m.mod_id.as_str()).collect();
    ids.sort();
    assert_eq!(ids, ["carryon", "extratestmod"]);
    assert_eq!((updated.description.as_deref(), updated.icon.as_deref(), updated.config_count), (Some("desc"), Some("md-sword"), 1));
    assert!(updated.active);

    // PDF round trip through export + import.
    let pdf = m.manager.join("out.pdf");
    run(export_modlist(state.clone(), saved.saved.id.clone(), pdf.display().to_string(), ExportFormat::Pdf)).unwrap();
    run(delete_modlists(state.clone(), vec![saved.saved.id.clone()])).unwrap();
    let imported = run(import_modlist(state.clone(), pdf.display().to_string(), false)).unwrap();
    assert_eq!((imported.name.as_str(), imported.mods.len(), imported.config_count), ("Mine", 2, 1));
    assert!(run(export_modlist(state.clone(), imported.id.clone(), m.manager.join("x.exe").display().to_string(), ExportFormat::Json)).is_err());

    // A legacy file loaded as "just switch on and off".
    let legacy = m.manager.join("Presets/Off.json");
    std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    std::fs::write(
        &legacy,
        r#"{"name":"Off","mods":[{"modId":"carryon","isActive":false},{"modId":"handytweaks","isActive":true},{"modId":"notinstalled","isActive":true}]}"#,
    )
    .unwrap();
    let source = ModlistSource { id: None, file: Some(legacy.display().to_string()), online: None };
    let preview = run(preview_modlist(state.clone(), source.clone(), LoadMode::StatesOnly)).unwrap();
    assert_eq!(preview.plan.toggles, 2);
    assert!(preview.plan.install.is_empty());
    let report = run(run_apply(m.app.handle(), &state, source, LoadMode::StatesOnly, true)).unwrap();
    assert_eq!(report.toggled, 2, "{report:?}");
    assert!(report.failures.is_empty());
    let cs = std::fs::read_to_string(m.data.join("clientsettings.json")).unwrap();
    assert!(cs.contains("carryon@1.14.0") && !cs.contains("handytweaks"), "{cs}");
    // The previous state was backed up and can be loaded back.
    let backup = report.backup_id.unwrap();
    let back = run(run_apply(m.app.handle(), &state, ModlistSource { id: Some(backup), file: None, online: None }, LoadMode::StatesOnly, true)).unwrap();
    assert_eq!(back.toggled, 2);
    // Configs untouched by StatesOnly loads unless the list carries some.
    assert_eq!(std::fs::read_to_string(m.data.join("ModConfig/CarryOn.json")).unwrap(), r#"{ "mine": true }"#);
}

#[test]
#[ignore = "downloads from mods.vintagestory.at"]
fn replace_installs_from_the_mod_db() {
    let m = machine();
    let state = m.app.state::<AppState>();
    let list = m.manager.join("list.json");
    std::fs::create_dir_all(&m.manager).unwrap();
    std::fs::write(
        &list,
        r#"{"name":"Test","mods":[{"modId":"carryon","version":"1.14.3","isActive":true},{"modId":"handytweaks","version":"1.0.0","isActive":true}],
            "configurations":[{"modId":"carryon","fileName":"CarryOn.json","content":"{ \"fromList\": true }"}],
            "includeModStatus":true,"includeModVersions":true,"exclusive":true}"#,
    )
    .unwrap();
    let source = ModlistSource { id: None, file: Some(list.display().to_string()), online: None };
    let preview = run(preview_modlist(state.clone(), source.clone(), LoadMode::Replace)).unwrap();
    assert_eq!(preview.plan.change_version.len(), 1);
    assert_eq!(preview.plan.remove.len(), 1);
    assert_eq!(preview.config_files, ["CarryOn.json"]);

    let report = run(run_apply(m.app.handle(), &state, source, LoadMode::Replace, true)).unwrap();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!((report.changed, report.removed, report.configs_written), (1, 1, 1));
    let mods = report.list.unwrap().mods;
    let carry = mods.iter().find(|x| x.mod_id == "carryon").unwrap();
    assert_eq!(carry.version.as_deref(), Some("1.14.3"));
    assert!(mods.iter().all(|x| x.mod_id != "extratestmod"));
    assert!(mods.iter().find(|x| x.mod_id == "handytweaks").unwrap().is_active);
    assert_eq!(std::fs::read_to_string(m.data.join("ModConfig/CarryOn.json")).unwrap(), "{ \"fromList\": true }");
    // The overwritten config is in the backup.
    let backup = ModlistStore::new(&m.manager).load(&report.backup_id.unwrap()).unwrap();
    assert!(backup.modlist.config_files().iter().any(|c| c.content.contains("mine")));
}

#[test]
fn switching_between_lists_uses_the_library_offline() {
    let m = machine();
    let state = m.app.state::<AppState>();
    state.update_settings(|s| s.offline_mode = true).unwrap();
    let store = ModlistStore::new(&m.manager);
    // "Everything" = the current mods; "Small" = only carryon.
    let save = |name: &str| {
        run(save_modlist(
            state.clone(),
            SaveModlistRequest {
                name: name.into(),
                description: None,
                game_version: None,
                uploader: None,
                configs: vec![],
                overwrite: false,
                icon: None,
                icon_color: None,
            },
        ))
        .unwrap()
        .saved
        .id
    };
    let everything = save("Everything");
    std::fs::write(
        store.dir(ModlistKind::Modlist).join("Small.json"),
        r#"{"name":"Small","mods":[{"modId":"carryon","version":"1.14.0","isActive":true}],"includeModStatus":true,"includeModVersions":true}"#,
    )
    .unwrap();

    let lists = run(list_modlists(state.clone())).unwrap();
    assert!(lists.iter().find(|l| l.id == everything).unwrap().active);
    assert!(!lists.iter().find(|l| l.name == "Small").unwrap().active);

    let small = ModlistSource { id: Some("Modlists/Small.json".into()), file: None, online: None };
    let report = run(run_apply(m.app.handle(), &state, small, LoadMode::Replace, true)).unwrap();
    assert_eq!((report.removed, report.to_library), (2, 2), "{report:?}");
    assert_eq!(std::fs::read_dir(m.data.join("Mods")).unwrap().count(), 1);

    // Back to "Everything": comes from the library, no network (offline mode).
    let preview = run(preview_modlist(state.clone(), ModlistSource { id: Some(everything.clone()), file: None, online: None }, LoadMode::Replace)).unwrap();
    assert_eq!((preview.from_library, preview.to_download), (1, 0));
    let lists = run(list_modlists(state.clone())).unwrap();
    assert_eq!(lists.iter().find(|l| l.id == everything).unwrap().downloads_needed, 0);
    let back = run(run_apply(m.app.handle(), &state, ModlistSource { id: Some(everything.clone()), file: None, online: None }, LoadMode::Replace, true)).unwrap();
    assert!(back.failures.is_empty(), "{:?}", back.failures);
    assert_eq!(back.installed, 1);
    let mods = back.list.unwrap().mods;
    // handytweaks was disabled when "Everything" was saved, so it isn't part of it.
    assert_eq!(mods.len(), 2);
    assert!(mods.iter().all(|x| x.mod_id != "handytweaks"));
    assert!(run(list_modlists(state.clone())).unwrap().iter().find(|l| l.id == everything).unwrap().active);
}

#[test]
fn follow_update_and_publish_against_a_fake_cloud() {
    use mockito::Matcher;
    let mut server = mockito::Server::new();
    let m = machine_with(Some(svsm_core::cloud::CloudConfig::for_tests(&server.url())));
    let state = m.app.state::<AppState>();
    // An existing 2.x identity that is still valid.
    std::fs::create_dir_all(&m.manager).unwrap();
    std::fs::write(
        m.manager.join("firebase-auth.json"),
        r#"{"idToken":"tok","refreshToken":"r","expirationUtc":"2999-01-01T00:00:00Z","userId":"fb1"}"#,
    )
    .unwrap();
    let identity_before = std::fs::read_to_string(m.manager.join("firebase-auth.json")).unwrap();

    let rev1 = r#"{"content":{"name":"Cozy","uploader":"Bob","mods":[{"modId":"carryon","version":"1.14.0","isActive":true}],"includeModStatus":true,"includeModVersions":true},"revision":1,"updatedAt":"2026-01-01T00:00:00Z"}"#;
    let reg = server.mock("GET", "/db/registry/r1.json").match_query(Matcher::Missing).with_body(rev1).create();
    server.mock("GET", "/db/modlistSubscriptions/r1/fb1.json").match_query(Matcher::Any).with_body("null").create();
    let subscribe = server.mock("PATCH", "/db/.json").match_query(Matcher::Any).match_body(Matcher::Regex("modlistSubscriptions/r1/fb1".into())).with_body("{}").expect(1).create();

    let copy = run(crate::cloud_commands::save_online_copy(state.clone(), "r1".into(), true)).unwrap();
    subscribe.assert();
    assert_eq!(copy.name, "Cozy");
    assert_eq!(copy.subscription.as_ref().unwrap().synced_revision, 1.0);

    // The author publishes revision 2; the listing cache now knows it.
    reg.remove();
    let rev2 = rev1.replace("\"revision\":1", "\"revision\":2").replace("1.14.0", "1.14.3");
    server.mock("GET", "/db/registry/r1.json").match_query(Matcher::Missing).with_body(&rev2).create();
    server
        .mock("GET", "/db/registrySummaries.json")
        .match_query(Matcher::Missing)
        .with_body(format!("{{\"r1\":{rev2}}}"))
        .create();
    server.mock("GET", "/db/modlistSubscriptions.json").match_query(Matcher::Missing).with_body(r#"{"r1":{"fb1":1}}"#).create();
    server.mock("GET", "/db/owners/PlayerUid_1.json").match_query(Matcher::Any).with_body("\"fb1\"").create();
    server.mock("GET", "/db/users/PlayerUid_1.json").match_query(Matcher::Any).with_body("null").create();
    let listing = run(crate::cloud_commands::online_modlists(state.clone())).unwrap();
    assert!(listing.entries[0].subscribed);
    assert_eq!(listing.entries[0].local_id.as_deref(), Some(copy.id.as_str()));
    let lists = run(list_modlists(state.clone())).unwrap();
    let mine = lists.iter().find(|l| l.id == copy.id).unwrap();
    assert!(mine.subscription.as_ref().unwrap().update_available);

    let synced = run(crate::cloud_commands::sync_subscription(state.clone(), copy.id.clone())).unwrap();
    assert_eq!(synced.mods[0].version.as_deref(), Some("1.14.3"));
    assert!(!synced.subscription.as_ref().unwrap().update_available);

    server.mock("DELETE", "/db/modlistSubscriptions/r1/fb1.json").match_query(Matcher::Any).with_body("null").create();
    let plain = run(crate::cloud_commands::unsubscribe(state.clone(), copy.id.clone())).unwrap();
    assert!(plain.subscription.is_none());

    // Publishing: blocked words are refused before anything is sent.
    let store = ModlistStore::new(&m.manager);
    std::fs::write(store.dir(ModlistKind::Modlist).join("Rude.json"), r#"{"name":"ass mods","mods":[]}"#).unwrap();
    let rude = run(crate::cloud_commands::publish_modlist(
        state.clone(),
        crate::cloud_commands::PublishRequest { id: "Modlists/Rude.json".into(), slot: "slot1".into(), replaces: None },
    ));
    assert!(rude.unwrap_err().message.contains("can't contain"));
    let publish = server
        .mock("PATCH", "/db/.json")
        .match_query(Matcher::Any)
        .match_body(Matcher::AllOf(vec![Matcher::Regex("/users/PlayerUid_1/slot1".into()), Matcher::Regex("\"uploader\":\"Tester\"".into())]))
        .with_body("{}")
        .expect(1)
        .create();
    let _ = run(crate::cloud_commands::publish_modlist(
        state.clone(),
        crate::cloud_commands::PublishRequest { id: plain.id.clone(), slot: "slot1".into(), replaces: None },
    ));
    publish.assert();

    // The 2.x identity file was never touched.
    assert_eq!(std::fs::read_to_string(m.manager.join("firebase-auth.json")).unwrap(), identity_before);
}

#[test]
fn publish_and_update_current_mods_against_a_fake_cloud() {
    use mockito::Matcher;
    let mut server = mockito::Server::new();
    let m = machine_with(Some(svsm_core::cloud::CloudConfig::for_tests(&server.url())));
    let state = m.app.state::<AppState>();
    run(list_modlists(state.clone())).unwrap(); // scans the installed mods
    std::fs::create_dir_all(&m.manager).unwrap();
    std::fs::write(
        m.manager.join("firebase-auth.json"),
        r#"{"idToken":"tok","refreshToken":"r","expirationUtc":"2999-01-01T00:00:00Z","userId":"fb1"}"#,
    )
    .unwrap();
    server.mock("GET", "/db/owners/PlayerUid_1.json").match_query(Matcher::Any).with_body("\"fb1\"").create();
    let slots = r#"{"slot2":{"registryId":"rid","revision":3,"content":{"name":"Mine","description":"keep me","icon":"md-sword","mods":[{"modId":"gone","version":"1.0.0"}],
        "configurations":[{"modId":"carryon","fileName":"CarryOn.json","content":"{}"},{"modId":"gone","fileName":"Gone.json","content":"{}"}]}}}"#;
    server.mock("GET", "/db/users/PlayerUid_1.json").match_query(Matcher::Any).with_body(slots).create();

    // Update: enabled mods replace the old ones (handytweaks is disabled), details and the
    // config of a mod that's still there stay, and it's a new revision.
    let update = server
        .mock("PATCH", "/db/.json")
        .match_query(Matcher::Any)
        .match_body(Matcher::AllOf(vec![
            Matcher::Regex("/users/PlayerUid_1/slot2".into()),
            Matcher::Regex("\"modId\":\"carryon\"".into()),
            Matcher::Regex("\"description\":\"keep me\"".into()),
            Matcher::Regex("\"icon\":\"md-sword\"".into()),
            Matcher::Regex("CarryOn.json".into()),
            Matcher::Regex(r#""revision":4\.0"#.into()),
        ]))
        .with_body("{}")
        .expect(1)
        .create();
    let _ = run(crate::cloud_commands::update_published_mods(state.clone(), "rid".into()));
    update.assert();
    assert!(run(crate::cloud_commands::update_published_mods(state.clone(), "not-mine".into())).is_err());

    // Publishing the current mods into an empty slot; a used slot is refused.
    let publish = server
        .mock("PATCH", "/db/.json")
        .match_query(Matcher::Any)
        .match_body(Matcher::AllOf(vec![Matcher::Regex("/users/PlayerUid_1/slot1".into()), Matcher::Regex("\"name\":\"Fresh\"".into())]))
        .with_body("{}")
        .expect(1)
        .create();
    let request = |slot: &str| crate::cloud_commands::PublishCurrentRequest {
        slot: slot.into(),
        name: "Fresh".into(),
        description: None,
        icon: None,
        icon_color: Some("sage".into()),
    };
    let _ = run(crate::cloud_commands::publish_current_mods(state.clone(), request("slot1")));
    publish.assert();
    assert!(run(crate::cloud_commands::publish_current_mods(state.clone(), request("slot2"))).is_err());
}

#[test]
fn data_backups_configs_and_profiles() {
    let m = machine();
    let state = m.app.state::<AppState>();
    std::fs::create_dir_all(m.data.join("Saves")).unwrap();
    std::fs::write(m.data.join("Saves/world.vcdbs"), "world").unwrap();
    std::fs::write(m.data.join("ModConfig/handytweaks.json"), "{ \"speed\": 1, \"on\": true }").unwrap();

    // Back up, change things, restore with a safety backup.
    let made = run(crate::m5_commands::create_data_backup(m.app.handle().clone(), state.clone())).unwrap();
    assert_eq!(made.save_count, 1);
    std::fs::write(m.data.join("Saves/world.vcdbs"), "broken").unwrap();
    let safety = run(crate::m5_commands::restore_data_backup(m.app.handle().clone(), state.clone(), made.id.clone(), true)).unwrap();
    assert!(safety.is_some());
    assert_eq!(std::fs::read_to_string(m.data.join("Saves/world.vcdbs")).unwrap(), "world");
    let list = run(crate::m5_commands::list_data_backups(state.clone())).unwrap();
    assert_eq!(list.backups.len(), 2);
    // Keep-newest applies per data folder.
    state.update_settings(|s| s.max_data_backups = 1).unwrap();
    run(crate::m5_commands::create_data_backup(m.app.handle().clone(), state.clone())).unwrap();
    assert_eq!(run(crate::m5_commands::list_data_backups(state.clone())).unwrap().backups.len(), 1);

    // Configs: listed, matched to the mod, saved only when valid.
    let configs = run(crate::m5_commands::list_mod_configs(state.clone())).unwrap();
    assert!(configs.iter().any(|c| c.relative_path == "handytweaks.json" && c.mod_id.as_deref() == Some("handytweaks")));
    assert!(run(crate::m5_commands::save_mod_config(state.clone(), "handytweaks.json".into(), "{ nope".into())).is_err());
    let saved = run(crate::m5_commands::save_mod_config(state.clone(), "handytweaks.json".into(), "{\"speed\": 2, \"on\": true}".into())).unwrap();
    assert!(saved.previous.unwrap().contains("\"speed\": 1"));

    // Profiles: a new profile starts on auto-detected folders; switching back restores ours.
    let ours = state.environment().data_dir.unwrap().path;
    state.update_settings(|s| s.data_directory = Some(ours.clone())).unwrap();
    let env = run(crate::m5_commands::change_profile(state.clone(), crate::m5_commands::ProfileChange::Switch { name: "Vanilla".into() })).unwrap();
    assert_eq!(env.settings.active_profile, "Vanilla");
    assert_eq!(env.settings.data_directory, None);
    let env = run(crate::m5_commands::change_profile(state.clone(), crate::m5_commands::ProfileChange::Switch { name: "Default".into() })).unwrap();
    assert_eq!(env.settings.data_directory.as_deref(), Some(ours.as_str()));
    assert!(run(crate::m5_commands::change_profile(state.clone(), crate::m5_commands::ProfileChange::Delete { name: "Default".into() })).is_err());
}

fn zip_mod_with_deps(path: &Path, id: &str, version: &str, deps: &str) {
    use std::io::Write;
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    z.start_file("modinfo.json", zip::write::SimpleFileOptions::default()).unwrap();
    write!(z, r#"{{"type":"code","modid":"{id}","name":"{id}","version":"{version}","dependencies":{{{deps}}}}}"#).unwrap();
    z.finish().unwrap();
}

fn key_of(state: &AppState, id: &str) -> String {
    state.mods.read().unwrap().mods.iter().find(|x| x.mod_id == id).unwrap().key.clone()
}

#[test]
fn fix_dependencies_enables_and_reports_what_it_cannot_fix() {
    let m = machine();
    let state = m.app.state::<AppState>();
    state.update_settings(|s| s.offline_mode = true).unwrap();
    zip_mod_with_deps(
        &m.data.join("Mods/NeedsDeps.zip"),
        "needsdeps",
        "1.0.0",
        r#""game":"1.21.0","handytweaks":"*","carryon":"1.14.3","missinglib":"*""#,
    );
    state.rescan().unwrap();
    let key = key_of(&state, "needsdeps");

    let report = run(crate::deps_commands::fix(m.app.handle(), &state, &key)).unwrap();
    assert_eq!(report.done, ["Enabled handytweaks"]);
    let ids: Vec<_> = report.failures.iter().map(|f| f.split(':').next().unwrap()).collect();
    assert_eq!(ids, ["carryon", "missinglib"], "{:?}", report.failures);
    let mods = report.list.mods;
    assert!(mods.iter().find(|x| x.mod_id == "handytweaks").unwrap().is_active);
    // Nothing was downloaded or removed.
    assert_eq!(std::fs::read_dir(m.data.join("Mods")).unwrap().count(), 4);
}

#[test]
#[ignore = "downloads from mods.vintagestory.at"]
fn fix_dependencies_installs_and_updates_from_the_mod_db() {
    let m = machine();
    let state = m.app.state::<AppState>();
    zip_mod_with_deps(
        &m.data.join("Mods/NeedsDeps.zip"),
        "needsdeps",
        "1.0.0",
        r#""game":"","carryon":"1.14.3","configlib":"*""#,
    );
    state.rescan().unwrap();
    let key = key_of(&state, "needsdeps");

    let report = run(crate::deps_commands::fix(m.app.handle(), &state, &key)).unwrap();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    println!("{:#?}", report.done);
    let mods = report.list.mods;
    let carry = mods.iter().find(|x| x.mod_id == "carryon").unwrap();
    assert!(svsm_core::version::satisfies_minimum_version(Some("1.14.3"), carry.version.as_deref()));
    assert!(mods.iter().any(|x| x.mod_id == "configlib"));
    let root = mods.iter().find(|x| x.mod_id == "needsdeps").unwrap();
    assert!(root.load_error.is_none(), "{:?}", root.load_error);
    // Satisfied now, so a second run does nothing.
    let again = run(crate::deps_commands::fix(m.app.handle(), &state, &key)).unwrap();
    assert!(again.done.is_empty() && again.failures.is_empty());
}

#[test]
fn custom_shortcut_and_moving_the_manager_folder() {
    let m = machine();
    let state = m.app.state::<AppState>();
    let root = m.manager.parent().unwrap().parent().unwrap().to_path_buf();

    // Shortcut: must exist; a missing one is cleared at launch instead of starting anything.
    assert!(run(crate::commands::set_custom_shortcut(state.clone(), Some(root.join("nope.lnk").display().to_string()))).is_err());
    let lnk = root.join("Vintage Story.lnk");
    std::fs::write(&lnk, "x").unwrap();
    let env = run(crate::commands::set_custom_shortcut(state.clone(), Some(lnk.display().to_string()))).unwrap();
    assert_eq!(env.settings.custom_shortcut_path.as_deref(), Some(lnk.display().to_string().as_str()));
    std::fs::remove_file(&lnk).unwrap();
    let err = run(crate::commands::launch_game(m.app.handle().clone(), state.clone())).unwrap_err();
    assert!(err.message.contains("wasn't found"), "{}", err.message);
    assert_eq!(state.environment().settings.custom_shortcut_path, None);

    // Move the manager folder and back; settings travel with it.
    std::fs::create_dir_all(root.join("app")).unwrap();
    state.update_settings(|s| s.close_after_launch = true).unwrap();
    let moved = run(crate::commands::move_manager_folder(state.clone(), Some(root.join("D").display().to_string()))).unwrap();
    assert!(!moved.old_folder_left);
    let new_dir = std::path::PathBuf::from(&moved.folder);
    assert!(new_dir.ends_with(svsm_core::paths::MANAGER_FOLDER_NAME) || new_dir.ends_with(svsm_core::paths::MANAGER_FOLDER_NAME_LINUX));
    assert!(!m.manager.exists());
    let env = state.refresh_environment();
    assert_eq!(std::path::PathBuf::from(&env.manager_dir.path), new_dir);
    assert!(env.settings.close_after_launch);
    run(crate::commands::move_manager_folder(state.clone(), None)).unwrap();
    let env = state.refresh_environment();
    assert_eq!(std::path::PathBuf::from(&env.manager_dir.path), m.manager);
    assert!(env.settings.close_after_launch);
}
