use super::*;
use serde_json::json;
use std::sync::{Arc, Mutex};

fn archive(path: &Path, entries: &[(&str, &str)]) {
    let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
    for (name, text) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}
fn request(path: &Path) -> GameImport {
    GameImport {
        path: path.into(),
        archive_prefix: None,
        target: None,
        name: "Imported".into(),
        directory: None,
        icon: Some(5),
    }
}
fn profile(game: &Path, id: &str) -> PathBuf {
    let path = version::metadata_path(game, id).unwrap();
    storage::write_json(&path, &crate::test_http::version_fixture(id)).unwrap();
    path.parent().unwrap().to_owned()
}
fn sink() -> EventSink {
    Arc::new(|_| {})
}

#[test]
fn distinguishes_versions_from_game_roots_and_preserves_home_drop_selection() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
    let game = temp.path().join("My Pack/.minecraft");
    let version = profile(&game, "Forge pack");
    fs::create_dir_all(version.join("mods")).unwrap();
    profile(&game, "1.20.1");
    let found = launcher
        .inspect_resources(&[version.clone(), game.join("versions"), game.clone()])
        .unwrap();
    assert_eq!(found[0].format, "version_directory");
    assert_eq!(found[0].game_directory.as_ref(), Some(&game));
    assert_eq!(found[0].version.as_deref(), Some("Forge pack"));
    assert_eq!(found[1].format, "versions_directory");
    assert_eq!(found[2].format, "directory");
    let scan = launcher
        .scan_folders(std::slice::from_ref(&version), &CancellationToken::new())
        .unwrap();
    assert_eq!(scan.candidates.len(), 1);
    assert_eq!(
        scan.candidates[0].path,
        game.canonicalize().unwrap().to_string_lossy()
    );
    assert_eq!(
        scan.candidates[0].preferred_version.as_deref(),
        Some("Forge pack")
    );
    assert_eq!(scan.candidates[0].versions.len(), 2);
    assert_eq!(
        crate::folder_import::resolve_game_directory(&version).unwrap(),
        game.canonicalize().unwrap()
    );
    assert!(launcher.snapshot().database.instances.is_empty());

    // A standalone version must never be linked as a new game root with zero versions.
    let standalone = temp.path().join("Forge pack");
    fs::rename(version, &standalone).unwrap();
    let found = launcher
        .inspect_resources(std::slice::from_ref(&standalone))
        .unwrap();
    assert_eq!(found[0].format, "version_directory");
    assert!(found[0].game_directory.is_none());
    assert!(crate::folder_import::resolve_game_directory(&standalone).is_err());
    assert!(
        launcher
            .scan_folders(&[standalone], &CancellationToken::new())
            .unwrap()
            .candidates
            .is_empty()
    );

    // Known managed roots can be located as well, without importing them twice.
    let managed = launcher.create_instance("Managed", "").unwrap();
    let owned = profile(&managed.game_dir(launcher.root()), "1.20.1");
    let scan = launcher
        .scan_folders(&[owned], &CancellationToken::new())
        .unwrap();
    assert_eq!(
        scan.candidates[0].existing_id.as_deref(),
        Some(managed.id.as_str())
    );
}

#[test]
fn wrapped_instance_zip_reuses_managed_and_external_creation_without_importing_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
    let zip = temp.path().join("Collection.zip");
    let profile = serde_json::to_string(&crate::test_http::version_fixture("1.20.1")).unwrap();
    archive(
        &zip,
        &[
            ("Pack A/.minecraft/versions/1.20.1/1.20.1.json", &profile),
            ("Pack A/.minecraft/versions/1.20.1/1.20.1.jar", "client"),
            ("Pack A/.minecraft/saves/My World/level.dat", "world"),
            ("Pack A/.minecraft/launcher_accounts.json", "private"),
            ("Pack A/launcher.exe", "not game data"),
            ("Pack B/.minecraft/options.txt", "other pack"),
        ],
    );
    let original = fs::read(&zip).unwrap();
    let detected = launcher
        .inspect_resources(std::slice::from_ref(&zip))
        .unwrap()
        .remove(0);
    assert_eq!(detected.format, "instance_archive");
    assert_eq!(detected.game_directories.len(), 2);
    assert_eq!(detected.game_directories[0].name, "Pack A");
    for external in [false, true] {
        let parent = temp.path().join("External");
        fs::create_dir_all(&parent).unwrap();
        fs::write(parent.join("keep"), "untouched").unwrap();
        let mut input = request(&zip);
        input.archive_prefix = Some("Pack A/.minecraft/".into());
        input.directory = external.then_some(parent.clone());
        let imported = launcher
            .import_game(input, &CancellationToken::new(), &sink())
            .unwrap();
        let game = imported.game_dir(launcher.root());
        assert_eq!(imported.selected_version.as_deref(), Some("1.20.1"));
        assert!(imported.selected().unwrap().installed);
        assert_eq!(imported.icon, Some(5));
        assert_eq!(imported.game_directory.is_some(), external);
        assert_eq!(
            fs::read(game.join("saves/My World/level.dat")).unwrap(),
            b"world"
        );
        assert!(!game.join("launcher_accounts.json").exists());
        assert!(!game.join("launcher.exe").exists());
        assert!(!game.join("options.txt").exists());
        assert_eq!(fs::read(parent.join("keep")).unwrap(), b"untouched");
    }
    assert_eq!(fs::read(zip).unwrap(), original);
}

#[test]
fn cancellation_rolls_back_only_created_files_and_allows_retry() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
    let zip = temp.path().join("pack.zip");
    archive(
        &zip,
        &[
            (".minecraft/options.txt", "options"),
            (".minecraft/mods/test.jar", "mod"),
        ],
    );
    let parent = temp.path().join("Games");
    fs::create_dir(&parent).unwrap();
    fs::write(parent.join("keep"), "keep").unwrap();
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let stages = Arc::new(Mutex::new(Vec::new()));
    let recorded = stages.clone();
    let cancelling: EventSink = Arc::new(move |event| {
        recorded.lock().unwrap().push(event.step.clone());
        if event.step.as_deref() == Some("import_files") && event.kind == "progress" {
            token.cancel();
        }
    });
    let input = || {
        let mut r = request(&zip);
        r.archive_prefix = Some(".minecraft/".into());
        r.directory = Some(parent.clone());
        r
    };
    assert!(launcher.import_game(input(), &cancel, &cancelling).is_err());
    assert!(launcher.snapshot().database.instances.is_empty());
    assert!(!parent.join(".minecraft").exists());
    assert_eq!(fs::read(parent.join("keep")).unwrap(), b"keep");
    assert!(
        stages
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.as_deref() == Some("import_files"))
    );
    launcher
        .import_game(input(), &CancellationToken::new(), &sink())
        .unwrap();
    assert_eq!(
        fs::read(parent.join(".minecraft/options.txt")).unwrap(),
        b"options"
    );
}

#[test]
fn archives_reject_unsafe_paths_stale_prefixes_and_existing_destinations() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
    let zip = temp.path().join("pack.zip");
    archive(
        &zip,
        &[
            (".minecraft/options.txt", "options"),
            ("../escaped", "unsafe"),
        ],
    );
    let mut input = request(&zip);
    input.archive_prefix = Some(".minecraft/".into());
    assert!(
        launcher
            .import_game(input, &CancellationToken::new(), &sink())
            .is_err()
    );
    assert!(!temp.path().join("escaped").exists());
    assert!(launcher.snapshot().database.instances.is_empty());
    archive(&zip, &[(".minecraft/options.txt", "options")]);
    let mut input = request(&zip);
    input.archive_prefix = Some("Other/.minecraft/".into());
    assert!(
        launcher
            .import_game(input, &CancellationToken::new(), &sink())
            .is_err()
    );
    let parent = temp.path().join("External");
    fs::create_dir_all(parent.join(".minecraft")).unwrap();
    fs::write(parent.join(".minecraft/options.txt"), "original").unwrap();
    let mut input = request(&zip);
    input.archive_prefix = Some(".minecraft/".into());
    input.directory = Some(parent.clone());
    assert!(
        launcher
            .import_game(input, &CancellationToken::new(), &sink())
            .is_err()
    );
    assert_eq!(
        fs::read(parent.join(".minecraft/options.txt")).unwrap(),
        b"original"
    );
    assert!(launcher.snapshot().database.instances.is_empty());
}

#[test]
fn standalone_copy_keeps_source_and_empty_isolation_folders_and_rejects_duplicates() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
    let source = temp.path().join("LocalPack");
    fs::create_dir_all(source.join("mods")).unwrap();
    storage::write_json(
        &source.join("LocalPack.json"),
        &crate::test_http::version_fixture("LocalPack"),
    )
    .unwrap();
    fs::write(source.join("LocalPack.jar"), "client").unwrap();
    let target = launcher.create_instance("Target", "other").unwrap();
    let input = || {
        let mut r = request(&source);
        r.target = Some(target.id.clone());
        r
    };
    let imported = launcher
        .import_game(input(), &CancellationToken::new(), &sink())
        .unwrap();
    assert_eq!(imported.selected_version.as_deref(), Some("LocalPack"));
    assert_eq!(imported.versions.len(), 2);
    assert!(imported.selected().unwrap().installed);
    let version = imported
        .game_dir(launcher.root())
        .join("versions/LocalPack");
    assert!(version.join("mods").is_dir());
    assert_eq!(fs::read(source.join("LocalPack.jar")).unwrap(), b"client");
    let duplicate = launcher
        .import_game(input(), &CancellationToken::new(), &sink())
        .unwrap_err();
    assert!(duplicate.to_string().contains("同名版本"));
    assert_eq!(fs::read(version.join("LocalPack.jar")).unwrap(), b"client");
    assert_eq!(launcher.instance(&target.id).unwrap().versions.len(), 2);
}

#[test]
fn copies_only_referenced_shared_dependencies_and_parent_profiles() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
    let game = temp.path().join("Source/.minecraft");
    let base = profile(&game, "1.20.1");
    fs::write(base.join("1.20.1.jar"), "client").unwrap();
    fs::create_dir_all(base.join("mods")).unwrap();
    fs::write(base.join("mods/unrelated.jar"), "other mod").unwrap();
    profile(&game, "unrelated");
    let custom = game.join("versions/Forge");
    storage::write_json(
        &custom.join("Forge.json"),
        &json!({
            "id":"Forge", "inheritsFrom":"1.20.1", "libraries":[{
                "name":"example:lib:1", "downloads":{"artifact":{"path":"example/lib/1/lib-1.jar"}}
            }]
        }),
    )
    .unwrap();
    fs::create_dir_all(game.join("libraries/example/lib/1")).unwrap();
    fs::write(game.join("libraries/example/lib/1/lib-1.jar"), "library").unwrap();
    fs::write(game.join("libraries/unrelated.jar"), "unrelated").unwrap();
    let hash = "0123456789012345678901234567890123456789";
    storage::write_json(
        &game.join("assets/indexes/1.20.1.json"),
        &json!({"objects":{"sound":{"hash":hash,"size":5}}}),
    )
    .unwrap();
    fs::create_dir_all(game.join("assets/objects/01")).unwrap();
    fs::write(game.join("assets/objects/01").join(hash), "sound").unwrap();
    let imported = launcher
        .import_game(request(&custom), &CancellationToken::new(), &sink())
        .unwrap();
    let target = imported.game_dir(launcher.root());
    assert_eq!(imported.selected_version.as_deref(), Some("Forge"));
    assert!(imported.selected().unwrap().installed);
    assert_eq!(
        fs::read(target.join("libraries/example/lib/1/lib-1.jar")).unwrap(),
        b"library"
    );
    assert_eq!(
        fs::read(target.join("assets/objects/01").join(hash)).unwrap(),
        b"sound"
    );
    assert!(target.join("versions/1.20.1/1.20.1.jar").is_file());
    assert!(!target.join("versions/1.20.1/mods").exists());
    assert!(!target.join("versions/unrelated").exists());
    assert!(!target.join("libraries/unrelated.jar").exists());
}

#[cfg(unix)]
#[test]
fn symlink_failure_rolls_back_and_never_writes_outside_the_target() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
    let source = profile(&temp.path().join("Source/.minecraft"), "1.20.1");
    let outside = temp.path().join("Outside");
    fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, source.join("mods")).unwrap();
    assert!(
        launcher
            .import_game(request(&source), &CancellationToken::new(), &sink())
            .is_err()
    );
    assert!(launcher.snapshot().database.instances.is_empty());
    fs::remove_file(source.join("mods")).unwrap();
    let target = launcher.create_instance("Target", "").unwrap();
    std::os::unix::fs::symlink(&outside, target.game_dir(launcher.root()).join("versions"))
        .unwrap();
    let mut input = request(&source);
    input.target = Some(target.id.clone());
    assert!(
        launcher
            .import_game(input, &CancellationToken::new(), &sink())
            .is_err()
    );
    assert!(fs::read_dir(outside).unwrap().next().is_none());
    assert!(launcher.instance(&target.id).unwrap().versions.is_empty());
    assert!(source.join("1.20.1.json").is_file());
}

#[test]
fn unfinished_loader_folders_remain_versions_and_corrupt_profiles_are_not_associated() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
    let id = "1.20.1-forge-47.4.4";
    let source = temp.path().join(id);
    storage::write_json(
        &source.join(".nekocrate/loader.json"),
        &json!({
            "version":id,"minecraft":"1.20.1","loader":{"kind":"forge","version":"47.4.4"},
            "completed":false,"progress":30.0
        }),
    )
    .unwrap();
    fs::create_dir(source.join("mods")).unwrap();
    let found = launcher
        .inspect_resources(std::slice::from_ref(&source))
        .unwrap();
    assert_eq!(found[0].format, "version_directory");
    let imported = launcher
        .import_game(request(&source), &CancellationToken::new(), &sink())
        .unwrap();
    assert_eq!(imported.selected_version.as_deref(), Some(id));
    assert!(!imported.selected().unwrap().installed);
    assert_eq!(imported.selected().unwrap().download_progress, 30.0);
    // A completed flag from another directory must not hide missing dependencies.
    let record_path = source.join(".nekocrate/loader.json");
    let mut record: Value = storage::read_json(&record_path).unwrap();
    record["completed"] = json!(true);
    storage::write_json(&record_path, &record).unwrap();
    storage::write_json(
        &source.join(format!("{id}.json")),
        &crate::test_http::version_fixture(id),
    )
    .unwrap();
    let incomplete = launcher
        .import_game(request(&source), &CancellationToken::new(), &sink())
        .unwrap();
    assert!(!incomplete.selected().unwrap().installed);
    let copied: Value = storage::read_json(
        &incomplete
            .game_dir(launcher.root())
            .join("versions")
            .join(id)
            .join(".nekocrate/loader.json"),
    )
    .unwrap();
    assert_eq!(copied["completed"], false);
    assert_eq!(
        storage::read_json::<Value>(&record_path).unwrap()["completed"],
        true
    );
    let corrupt = temp.path().join("Corrupt");
    storage::write_json(&corrupt.join("Corrupt.json"), &json!({"id":"Other"})).unwrap();
    fs::create_dir(corrupt.join("mods")).unwrap();
    assert_eq!(
        launcher
            .inspect_resources(std::slice::from_ref(&corrupt))
            .unwrap()[0]
            .format,
        "unknown"
    );
    assert!(
        launcher
            .scan_folders(&[corrupt], &CancellationToken::new())
            .unwrap()
            .candidates
            .is_empty()
    );
}

#[test]
fn corrupt_zip_and_cancelled_existing_instance_import_remove_partial_files() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
    let zip = temp.path().join("Corrupt.zip");
    archive(&zip, &[(".minecraft/options.txt", "unique payload")]);
    let mut bytes = fs::read(&zip).unwrap();
    let offset = {
        let mut archive = archive_support::open(&zip).unwrap();
        let entry = archive.by_index(0).unwrap();
        (entry.data_start() + entry.compressed_size() / 2) as usize
    };
    bytes[offset] ^= 1;
    fs::write(&zip, bytes).unwrap();
    let mut input = request(&zip);
    input.archive_prefix = Some(".minecraft/".into());
    assert!(
        launcher
            .import_game(input, &CancellationToken::new(), &sink())
            .is_err()
    );
    assert!(launcher.snapshot().database.instances.is_empty());
    let source = profile(&temp.path().join("Source/.minecraft"), "1.20.1");
    let target = launcher.create_instance("Target", "pending").unwrap();
    let game = target.game_dir(launcher.root());
    fs::write(game.join("options.txt"), "original").unwrap();
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let cancelling: EventSink = Arc::new(move |event| {
        if event.step.as_deref() == Some("import_register") {
            token.cancel();
        }
    });
    let mut input = request(&source);
    input.target = Some(target.id.clone());
    assert!(launcher.import_game(input, &cancel, &cancelling).is_err());
    assert_eq!(launcher.instance(&target.id).unwrap().versions.len(), 1);
    assert_eq!(fs::read(game.join("options.txt")).unwrap(), b"original");
    assert!(!game.join("versions/1.20.1").exists());
    // Copying a profile must not launch against an absolute source gameDir later.
    storage::write_json(
        &source.join("hmclversion.cfg"),
        &json!({"usesGlobal":false,"gameDirType":2,"gameDir":"/external/worlds"}),
    )
    .unwrap();
    let mut input = request(&source);
    input.target = Some(target.id.clone());
    assert!(
        launcher
            .import_game(input, &CancellationToken::new(), &sink())
            .unwrap_err()
            .to_string()
            .contains("外部游戏目录")
    );
}
