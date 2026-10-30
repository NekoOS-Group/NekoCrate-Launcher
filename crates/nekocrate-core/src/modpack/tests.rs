use super::*;
use ring::digest::{SHA512, digest};
use serde_json::json;
use sha1::{Digest, Sha1};
use std::{fs, io::Write};
mod content;

#[tokio::test]
async fn sha512_pack_hash_matches_known_vector_and_rejects_corruption() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("mod.jar");
    let cancel = CancellationToken::new();
    let expected = "e718483d0ce769644e2e42c7bc15b4638e1f98b13b2044285632a803afa973ebde0ff244877ea60a4cb0432ce577c31beb009c5c2c49aa2e4eadb217ad8cc09b";
    fs::write(&path, vec![b'a'; 1_000_000]).unwrap();
    verify_sha512(&path, expected, &cancel).await.unwrap();
    fs::write(&path, vec![b'b'; 1_000_000]).unwrap();
    assert!(verify_sha512(&path, expected, &cancel).await.is_err());
    cancel.cancel();
    assert!(
        verify_sha512(&path, expected, &cancel)
            .await
            .unwrap_err()
            .to_string()
            .contains("取消")
    );
}
fn pack_file(path: &str, env: &str) -> Value {
    json!({"path":path,"hashes":{"sha1":hex::encode(Sha1::digest(b"mod")),"sha512":hex::encode(digest(&SHA512, b"mod"))},"downloads":["https://cdn.modrinth.com/data/test/mod.jar"],"fileSize":3,"env":{"client":env}})
}
fn mr(files: Vec<Value>) -> Value {
    json!({"formatVersion":1,"game":"minecraft","name":"测试包","versionId":"1.0","dependencies":{"minecraft":"1.20.1","fabric-loader":"0.16.10"},"files":files})
}
fn archive(path: &Path, manifest: &str, value: &Value, extras: &[(&str, &[u8])]) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file(manifest, options).unwrap();
    zip.write_all(value.to_string().as_bytes()).unwrap();
    for (name, data) in extras {
        zip.start_file(*name, options).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
}
fn make_legacy(launcher: &Launcher, instance: &Instance, id: &str, name: &str) -> PathBuf {
    let mut job = launcher.pack_job(&instance.id, id).unwrap();
    let cache = launcher.root().join("modpacks").join(id);
    fs::create_dir_all(&cache).unwrap();
    let source = cache.join("source.zip");
    fs::rename(&job.archive, &source).unwrap();
    fs::remove_dir_all(
        instance
            .game_dir(launcher.root())
            .join("versions")
            .join(id)
            .join(".nekocrate"),
    )
    .unwrap();
    job.instance = instance.id.clone();
    job.name = name.into();
    job.archive = source.clone();
    storage::write_json(
        &launcher.root().join("modpacks").join(format!("{id}.json")),
        &job,
    )
    .unwrap();
    launcher
        .mutate(|db| {
            db.instances
                .iter_mut()
                .find(|i| i.id == instance.id)
                .unwrap()
                .versions
                .iter_mut()
                .find(|v| v.id == id)
                .unwrap()
                .name = Some(name.into());
            Ok(())
        })
        .unwrap();
    source
}
#[test]
fn modrinth_optional_environment_and_client_override_precedence() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("pack.mrpack");
    archive(
        &path,
        "modrinth.index.json",
        &mr(vec![
            pack_file("mods/required.jar", "required"),
            pack_file("mods/optional.jar", "optional"),
            pack_file("mods/server.jar", "unsupported"),
        ]),
        &[
            ("overrides/config/test.txt", b"common"),
            ("client-overrides/config/test.txt", b"client"),
            ("server-overrides/config/server.txt", b"server"),
        ],
    );
    let pack = archive::parse(&path).unwrap();
    assert_eq!(pack.files.len(), 2);
    assert!(pack.files[1].optional);
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    archive::extract(&path, &pack, &target, &CancellationToken::new()).unwrap();
    assert_eq!(fs::read(target.join("config/test.txt")).unwrap(), b"client");
    assert!(!target.join("config/server.txt").exists());
}
#[test]
fn rejects_traversal_windows_aliases_unsupported_loaders_and_ambiguous_archives() {
    for name in [
        "../outside",
        "/absolute",
        "C:/windows",
        "a\\b",
        "mods/CON.jar",
        "mods/a.",
        "mods/a:stream",
    ] {
        assert!(archive::relative(name).is_err(), "{name}");
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("bad.zip");
    for file in [
        "../outside.jar",
        "hmclversion.cfg",
        "versions/evil/evil.json",
    ] {
        archive(
            &path,
            "modrinth.index.json",
            &mr(vec![]),
            &[(&format!("overrides/{file}"), b"bad")],
        );
        assert!(archive::parse(&path).is_err(), "{file}");
    }
    let mut value = mr(vec![]);
    value["dependencies"]["quilt-loader"] = json!("0.1");
    archive(&path, "modrinth.index.json", &value, &[]);
    assert!(archive::parse(&path).is_err());
    archive(
        &path,
        "modrinth.index.json",
        &mr(vec![]),
        &[("manifest.json", b"{}")],
    );
    assert!(archive::parse(&path).is_err());
}
#[test]
fn curseforge_ids_and_primary_loader_are_preserved_without_guessing_cdn_urls() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("pack.zip");
    archive(
        &path,
        "manifest.json",
        &json!({"manifestType":"minecraftModpack","manifestVersion":1,"name":"RLCraft","version":"2.9","minecraft":{"version":"1.12.2","modLoaders":[{"id":"forge-14.23.5.2860","primary":true}]},"files":[{"projectID":123,"fileID":456,"required":true}],"overrides":"overrides"}),
        &[],
    );
    let pack = archive::parse(&path).unwrap();
    assert_eq!(pack.loader.unwrap().version, "14.23.5.2860");
    assert!(pack.files[0].urls.is_empty());
    assert_eq!(pack.files[0].project, Some(123));
}
#[tokio::test]
async fn prepare_keeps_linked_directory_isolates_repeated_imports_and_persists_pending_state() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("launcher");
    let game = temp.path().join("existing/.minecraft");
    fs::create_dir_all(game.join("mods")).unwrap();
    fs::write(game.join("mods/existing.jar"), b"existing").unwrap();
    let path = temp.path().join("pack.mrpack");
    archive(
        &path,
        "modrinth.index.json",
        &mr(vec![pack_file("mods/optional.jar", "optional")]),
        &[("overrides/config/test.txt", b"pack")],
    );
    let launcher = Launcher::open(Some(root.clone())).unwrap();
    let instance = launcher
        .import_instance(game.clone(), "External".into(), "".into())
        .await
        .unwrap();
    let first = launcher
        .prepare_modpack(&instance.id, &path, "Pack one", &[])
        .await
        .unwrap();
    let first_id = first.selected_version.clone().unwrap();
    assert_eq!(first_id, "Pack one");
    let second = launcher
        .prepare_modpack(&instance.id, &path, "Pack two", &[])
        .await
        .unwrap();
    assert_ne!(second.selected_version.as_ref(), Some(&first_id));
    assert_eq!(second.game_dir(&root), game.canonicalize().unwrap());
    assert_eq!(
        fs::read(game.join("mods/existing.jar")).unwrap(),
        b"existing"
    );
    let job = launcher.pack_job(&instance.id, &first_id).unwrap();
    assert!(job.pack.files.is_empty());
    assert!(game.join("versions").join(&first_id).join("mods").is_dir());
    assert!(!version::metadata_path(&game, &first_id).unwrap().exists());
    assert_eq!(
        storage::read_json::<Value>(&game.join("versions/Pack one/modrinth.index.json")).unwrap(),
        job.pack.manifest
    );
    let metadata = launcher.version_metadata(&instance.id, &first_id).unwrap();
    assert_eq!(metadata.documents.len(), 1);
    assert_eq!(metadata.documents[0].kind, "Modrinth");
    assert!(metadata.documents[0].issue.is_none());
    assert_eq!(
        metadata.working_directory,
        Some(game.canonicalize().unwrap().join("versions/Pack one"))
    );
    let content = launcher
        .version_content(
            &instance.id,
            &first_id,
            crate::version_management::ContentKind::Mods,
        )
        .unwrap();
    assert_eq!(
        content.files.len(),
        1,
        "Only this pack's declarations appear"
    );
    assert_eq!(content.files[0].file_name, "optional.jar");
    assert_eq!(
        content.files[0].declaration.as_ref().unwrap().status,
        "skipped"
    );
    drop(launcher);
    let launcher = Launcher::open(Some(root)).unwrap();
    assert!(
        launcher
            .instance(&instance.id)
            .unwrap()
            .versions
            .iter()
            .all(|v| !v.installed && v.modpack)
    );
}

#[tokio::test]
async fn named_folders_reject_duplicates_and_unsafe_names_without_touching_existing_files() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("root"))).unwrap();
    let instance = launcher.create_instance("Test", "").unwrap();
    let game = instance.game_dir(launcher.root());
    let path = temp.path().join("pack.mrpack");
    archive(&path, "modrinth.index.json", &mr(vec![]), &[]);
    let prepared = launcher
        .prepare_modpack(&instance.id, &path, "逆转未来 1.1.4", &[])
        .await
        .unwrap();
    assert_eq!(prepared.selected_version.as_deref(), Some("逆转未来 1.1.4"));
    let first_job = fs::read(job_path(&game, "逆转未来 1.1.4").unwrap()).unwrap();
    assert!(
        launcher
            .prepare_modpack(&instance.id, &path, "逆转未来 1.1.4", &[])
            .await
            .unwrap_err()
            .to_string()
            .contains("版本名称重复")
    );
    assert_eq!(
        first_job,
        fs::read(job_path(&game, "逆转未来 1.1.4").unwrap()).unwrap()
    );
    fs::create_dir_all(game.join("versions/Existing")).unwrap();
    fs::write(game.join("versions/Existing/keep"), b"keep").unwrap();
    for name in ["Existing", "existing"] {
        assert!(
            launcher
                .prepare_modpack(&instance.id, &path, name, &[])
                .await
                .unwrap_err()
                .to_string()
                .contains("版本名称重复")
        );
    }
    fs::write(game.join("versions/A file"), b"file").unwrap();
    assert!(
        launcher
            .prepare_modpack(&instance.id, &path, "A file", &[])
            .await
            .unwrap_err()
            .to_string()
            .contains("版本名称重复")
    );
    for name in [
        "../escape",
        "a/b",
        "a\\b",
        "CON",
        "Name?",
        "Name.",
        "",
        "manifest",
        "runtime",
        "1.20.1",
    ] {
        assert!(
            launcher
                .prepare_modpack(&instance.id, &path, name, &[])
                .await
                .is_err(),
            "{name}"
        );
    }
    assert_eq!(
        fs::read(game.join("versions/Existing/keep")).unwrap(),
        b"keep"
    );
    assert_eq!(launcher.instance(&instance.id).unwrap().versions.len(), 1);
    assert!(!launcher.root().join("modpacks").exists());
    assert_eq!(fs::read_dir(game.join("versions")).unwrap().count(), 3);
}

#[tokio::test]
async fn identical_pack_names_in_different_game_directories_have_independent_jobs() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("root"))).unwrap();
    let first = launcher.create_instance("First", "").unwrap();
    let second = launcher.create_instance("Second", "").unwrap();
    let path = temp.path().join("pack.mrpack");
    archive(&path, "modrinth.index.json", &mr(vec![]), &[]);
    launcher
        .prepare_modpack(&first.id, &path, "Same name", &[])
        .await
        .unwrap();
    archive(
        &path,
        "modrinth.index.json",
        &mr(vec![pack_file("mods/a.jar", "required")]),
        &[],
    );
    launcher
        .prepare_modpack(&second.id, &path, "Same name", &[])
        .await
        .unwrap();
    let a = launcher.pack_job(&first.id, "Same name").unwrap();
    let b = launcher.pack_job(&second.id, "Same name").unwrap();
    assert_ne!(a.archive, b.archive);
    assert!(a.pack.files.is_empty());
    assert_eq!(b.pack.files.len(), 1);
    launcher
        .delete_version_with_recycle(&first.id, "Same name", crate::recycle::discard_test_fixture)
        .unwrap();
    launcher
        .prepare_modpack(&first.id, &path, "Same name", &[])
        .await
        .unwrap();
    assert_eq!(
        launcher
            .pack_job(&first.id, "Same name")
            .unwrap()
            .pack
            .files
            .len(),
        1
    );
    assert_eq!(
        launcher.pack_job(&second.id, "Same name").unwrap().archive,
        b.archive
    );
}

#[tokio::test]
async fn legacy_pending_pack_metadata_uses_the_cached_manifest_without_a_launch_profile() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("root"))).unwrap();
    let instance = launcher.create_instance("Legacy", "").unwrap();
    let path = temp.path().join("pack.zip");
    let manifest = json!({"manifestType":"minecraftModpack","manifestVersion":1,"name":"All the Mods 9","version":"1.0","minecraft":{"version":"1.20.1","modLoaders":[]},"files":[{"projectID":123,"fileID":456,"required":true}],"overrides":"overrides"});
    archive(&path, "manifest.json", &manifest, &[]);
    let old = format!("pack-{}", uuid::Uuid::new_v4());
    launcher
        .prepare_modpack(&instance.id, &path, &old, &[])
        .await
        .unwrap();
    let old_source = make_legacy(&launcher, &instance, &old, "All the Mods 9");
    fs::remove_file(
        instance
            .game_dir(launcher.root())
            .join("versions")
            .join(&old)
            .join("manifest.json"),
    )
    .unwrap();
    let metadata = launcher.version_metadata(&instance.id, &old).unwrap();
    assert!(metadata.issue.is_none());
    assert_eq!(metadata.documents.len(), 1);
    let document = &metadata.documents[0];
    assert!(document.issue.is_none());
    assert_eq!(document.kind, "CurseForge");
    assert_eq!(
        serde_json::from_str::<Value>(document.json.as_deref().unwrap()).unwrap(),
        manifest
    );
    assert!(
        document
            .path
            .to_string_lossy()
            .ends_with("source.zip::manifest.json")
    );
    let root = launcher.root().to_owned();
    let game = instance.game_dir(&root);
    drop(launcher);
    let launcher = Launcher::open(Some(root)).unwrap();
    let restored = launcher.instance(&instance.id).unwrap();
    assert_eq!(restored.selected_version.as_deref(), Some("All the Mods 9"));
    assert!(!game.join("versions").join(old).exists());
    assert!(!old_source.exists());
    let job = launcher.pack_job(&instance.id, "All the Mods 9").unwrap();
    assert_eq!(job.pack.files.len(), 1);
    assert_eq!(
        job.archive,
        game.join("versions/All the Mods 9/.nekocrate/source.zip")
    );
    assert!(job.archive.is_file());
    assert!(game.join("versions/All the Mods 9/manifest.json").is_file());
}

#[tokio::test]
async fn named_pack_cannot_overwrite_its_launch_profile_or_manifest() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("root"))).unwrap();
    let instance = launcher.create_instance("Test", "").unwrap();
    let path = temp.path().join("pack.mrpack");
    for name in [
        "Pack.json",
        "manifest.json",
        "modrinth.index.json",
        ".nekocrate/modpack.json",
    ] {
        archive(
            &path,
            "modrinth.index.json",
            &mr(vec![]),
            &[(&format!("overrides/{name}"), b"{}")],
        );
        assert!(
            launcher
                .prepare_modpack(&instance.id, &path, "Pack", &[])
                .await
                .is_err()
        );
    }
    assert!(
        !instance
            .game_dir(launcher.root())
            .join("versions/Pack")
            .exists()
    );
    assert!(launcher.instance(&instance.id).unwrap().versions.is_empty());
}

#[tokio::test]
async fn moving_a_game_directory_to_a_fresh_launcher_recovers_pending_pack_state() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("old-launcher"))).unwrap();
    let game = temp.path().join("old-game/.minecraft");
    fs::create_dir_all(&game).unwrap();
    let instance = launcher
        .import_instance(game.clone(), "Game".into(), "".into())
        .await
        .unwrap();
    let source = temp.path().join("pack.mrpack");
    archive(
        &source,
        "modrinth.index.json",
        &mr(vec![pack_file("mods/test.jar", "required")]),
        &[],
    );
    launcher
        .prepare_modpack(&instance.id, &source, "Portable pack", &[])
        .await
        .unwrap();
    launcher
        .save_download_progress(&instance.id, "Portable pack", 42.0)
        .unwrap();
    let spec = FileSpec {
        url: "https://cdn.modrinth.com/data/test/mod.jar".into(),
        path: game.join("versions/Portable pack/mods/test.jar"),
        hash: hex::encode(Sha1::digest(b"mod")),
        sha256: false,
        size: 3,
    };
    let partial = crate::download::object_path(
        &game.join("versions/Portable pack/.nekocrate/downloads"),
        &spec,
    )
    .unwrap()
    .with_extension("part");
    fs::create_dir_all(partial.parent().unwrap()).unwrap();
    fs::write(&partial, b"mo").unwrap();
    let record: Value = storage::read_json(&job_path(&game, "Portable pack").unwrap()).unwrap();
    assert_eq!(record["archive"], "source.zip");
    assert!(record.get("instance").is_none());
    assert!(!launcher.root().join("modpacks").exists());
    drop(launcher);
    fs::remove_dir_all(temp.path().join("old-launcher")).unwrap();
    let moved = temp.path().join("moved/.minecraft");
    fs::create_dir_all(moved.parent().unwrap()).unwrap();
    fs::rename(&game, &moved).unwrap();
    let launcher = Launcher::open(Some(temp.path().join("fresh-launcher"))).unwrap();
    let instance = launcher
        .import_instance(moved.clone(), "Moved".into(), "".into())
        .await
        .unwrap();
    let version = instance
        .versions
        .iter()
        .find(|v| v.id == "Portable pack")
        .unwrap();
    assert!(version.modpack && !version.installed);
    assert_eq!(version.download_progress, 42.0);
    let job = launcher.pack_job(&instance.id, "Portable pack").unwrap();
    assert!(job.archive.starts_with(moved.canonicalize().unwrap()));
    assert!(job.archive.is_file());
    assert_eq!(
        fs::read(moved.join(partial.strip_prefix(&game).unwrap())).unwrap(),
        b"mo"
    );
    assert!(
        !launcher
            .modpack_status(&instance.id, "Portable pack")
            .await
            .unwrap()
            .completed
    );
}

#[tokio::test]
async fn legacy_migration_preserves_progress_and_partial_bytes_and_reports_conflicts() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("launcher");
    let launcher = Launcher::open(Some(root.clone())).unwrap();
    let instance = launcher.create_instance("Legacy", "").unwrap();
    let game = instance.game_dir(&root);
    let source = temp.path().join("pack.mrpack");
    archive(
        &source,
        "modrinth.index.json",
        &mr(vec![pack_file("mods/test.jar", "required")]),
        &[],
    );
    let old = format!("pack-{}", uuid::Uuid::new_v4());
    launcher
        .prepare_modpack(&instance.id, &source, &old, &[])
        .await
        .unwrap();
    launcher
        .save_download_progress(&instance.id, &old, 35.0)
        .unwrap();
    let old_source = make_legacy(&launcher, &instance, &old, "Named pack");
    let spec = FileSpec {
        url: "https://cdn.modrinth.com/data/test/mod.jar".into(),
        path: game.join("versions").join(&old).join("mods/test.jar"),
        hash: hex::encode(Sha1::digest(b"mod")),
        sha256: false,
        size: 3,
    };
    let partial = crate::download::object_path(&root.join("cache/objects"), &spec)
        .unwrap()
        .with_extension("part");
    fs::create_dir_all(partial.parent().unwrap()).unwrap();
    fs::write(&partial, b"mo").unwrap();
    let conflict = game.join("versions/Named pack");
    fs::create_dir_all(&conflict).unwrap();
    fs::write(conflict.join("keep"), b"keep").unwrap();
    drop(launcher);
    let launcher = Launcher::open(Some(root.clone())).unwrap();
    assert!(
        launcher
            .modpack_migration_issue(&instance.id, &old)
            .unwrap()
            .contains("版本名称重复")
    );
    assert_eq!(fs::read(conflict.join("keep")).unwrap(), b"keep");
    assert!(old_source.exists());
    assert!(
        launcher
            .version_metadata(&instance.id, &old)
            .unwrap()
            .issue
            .unwrap()
            .contains("版本名称重复")
    );
    drop(launcher);
    fs::remove_dir_all(conflict).unwrap();
    let launcher = Launcher::open(Some(root)).unwrap();
    let restored = launcher.instance(&instance.id).unwrap();
    assert_eq!(restored.selected_version.as_deref(), Some("Named pack"));
    assert_eq!(restored.selected().unwrap().download_progress, 35.0);
    let local = crate::download::object_path(
        &game.join("versions/Named pack/.nekocrate/downloads"),
        &spec,
    )
    .unwrap()
    .with_extension("part");
    assert_eq!(fs::read(local).unwrap(), b"mo");
    assert!(!old_source.exists());
}

#[tokio::test]
async fn legacy_migration_rolls_back_on_database_failure_and_retries_on_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let launcher = Launcher::open(Some(root.clone())).unwrap();
    let instance = launcher.create_instance("Test", "").unwrap();
    let game = instance.game_dir(&root);
    let source = temp.path().join("pack.mrpack");
    archive(&source, "modrinth.index.json", &mr(vec![]), &[]);
    let old = format!("pack-{}", uuid::Uuid::new_v4());
    launcher
        .prepare_modpack(&instance.id, &source, &old, &[])
        .await
        .unwrap();
    let old_source = make_legacy(&launcher, &instance, &old, "Recovered pack");
    fs::remove_file(root.join("database.json.bak")).unwrap();
    fs::create_dir(root.join("database.json.bak")).unwrap();
    launcher.migrate_modpack_records();
    assert_eq!(
        launcher
            .instance(&instance.id)
            .unwrap()
            .selected_version
            .as_deref(),
        Some(old.as_str())
    );
    assert!(game.join("versions").join(&old).is_dir());
    assert!(!game.join("versions/Recovered pack").exists());
    assert!(old_source.exists());
    drop(launcher);
    fs::remove_dir(root.join("database.json.bak")).unwrap();
    let launcher = Launcher::open(Some(root)).unwrap();
    assert_eq!(
        launcher
            .instance(&instance.id)
            .unwrap()
            .selected_version
            .as_deref(),
        Some("Recovered pack")
    );
    assert!(!old_source.exists());
}

#[tokio::test]
async fn legacy_migration_recovers_a_folder_move_interrupted_before_database_publication() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let launcher = Launcher::open(Some(root.clone())).unwrap();
    let instance = launcher.create_instance("Test", "").unwrap();
    let game = instance.game_dir(&root);
    let source = temp.path().join("pack.mrpack");
    archive(&source, "modrinth.index.json", &mr(vec![]), &[]);
    let old = format!("pack-{}", uuid::Uuid::new_v4());
    launcher
        .prepare_modpack(&instance.id, &source, &old, &[])
        .await
        .unwrap();
    let old_source = make_legacy(&launcher, &instance, &old, "Recovered pack");
    let (_, mut job) = launcher.legacy_pack_job(&instance.id, &old).unwrap();
    let folder = game.join("versions").join(&old);
    let state = folder.join(".nekocrate");
    fs::create_dir(&state).unwrap();
    fs::copy(&old_source, state.join("source.zip")).unwrap();
    storage::write_json(
        &state.join("migration.json"),
        &json!({"instance":instance.id,"version":old,"archive":old_source}),
    )
    .unwrap();
    job.instance.clear();
    job.version = "Recovered pack".into();
    job.archive = "source.zip".into();
    storage::write_json(&state.join("modpack.json"), &job).unwrap();
    fs::rename(folder, game.join("versions/Recovered pack")).unwrap();
    drop(launcher);
    let launcher = Launcher::open(Some(root)).unwrap();
    assert_eq!(
        launcher
            .instance(&instance.id)
            .unwrap()
            .selected_version
            .as_deref(),
        Some("Recovered pack")
    );
    assert!(
        launcher
            .pack_job(&instance.id, "Recovered pack")
            .unwrap()
            .archive
            .is_file()
    );
    assert!(!old_source.exists());
    assert!(
        !game
            .join("versions/Recovered pack/.nekocrate/migration.json")
            .exists()
    );
}
#[tokio::test]
async fn cancellation_and_manual_file_hash_mismatch_cannot_complete_an_install() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("root"))).unwrap();
    let instance = launcher.create_instance("Test", "").unwrap();
    let path = temp.path().join("test.mrpack");
    archive(
        &path,
        "modrinth.index.json",
        &mr(vec![pack_file("mods/test.jar", "required")]),
        &[],
    );
    let instance = launcher
        .prepare_modpack(&instance.id, &path, "Test pack", &[])
        .await
        .unwrap();
    let id = instance.selected_version.clone().unwrap();
    let mut job = launcher.pack_job(&instance.id, &id).unwrap();
    job.pack.files[0].urls.clear();
    record::write_job(&instance.game_dir(launcher.root()), &id, &job).unwrap();
    let wrong = temp.path().join("wrong.jar");
    fs::write(&wrong, b"bad").unwrap();
    assert!(
        launcher
            .supply_modpack_files(&instance.id, &id, vec![wrong])
            .await
            .is_err()
    );
    let correct = temp.path().join("manual.jar");
    fs::write(&correct, b"mod").unwrap();
    let operation = launcher.0.operation.lock().await;
    *launcher.0.active_modpack.lock().unwrap() = Some((instance.id.clone(), id.clone()));
    assert_eq!(
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            launcher.supply_modpack_files(&instance.id, &id, vec![correct])
        )
        .await
        .unwrap()
        .unwrap(),
        1
    );
    *launcher.0.active_modpack.lock().unwrap() = None;
    drop(operation);
    assert!(
        launcher
            .modpack_status(&instance.id, &id)
            .await
            .unwrap()
            .missing
            .is_empty()
    );
    let cancel = CancellationToken::new();
    cancel.cancel();
    let sink: EventSink = Arc::new(|_| {});
    assert!(
        launcher
            .install(&instance.id, &cancel, &sink)
            .await
            .is_err()
    );
    assert!(
        !launcher
            .instance(&instance.id)
            .unwrap()
            .selected()
            .unwrap()
            .installed
    );
}
#[cfg(unix)]
#[test]
fn overrides_cannot_follow_an_existing_symlink() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    let outside = temp.path().join("outside");
    fs::create_dir_all(&target).unwrap();
    fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, target.join("config")).unwrap();
    assert!(archive::destination(&target, "config/secret").is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn offline_install_resumes_after_overrides_and_keeps_user_configuration_on_repair() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let launcher = Launcher::open(Some(root.clone())).unwrap();
    let instance = launcher.create_instance("Offline", "").unwrap();
    let game = instance.game_dir(&root);
    let java = temp.path().join("java");
    fs::write(&java, "#!/bin/sh\necho 'java version \"17.0.1\"' >&2\n").unwrap();
    fs::set_permissions(&java, fs::Permissions::from_mode(0o755)).unwrap();
    launcher
        .update_instance_memory(
            &instance.id,
            &instance.name,
            crate::memory::MemoryConfig {
                mode: MemoryMode::Auto,
                custom_mb: 2048,
            },
            Some(java.to_string_lossy().into()),
            None,
        )
        .unwrap();
    let index = b"{\"objects\":{}}";
    let base = json!({"id":"1.20.1","type":"release","mainClass":"net.minecraft.client.main.Main","libraries":[],"arguments":{"game":[],"jvm":[]},"javaVersion":{"majorVersion":17},"downloads":{"client":{"url":"https://example.invalid/client.jar","sha1":hex::encode(Sha1::digest(b"client")),"size":6}},"assetIndex":{"id":"test","url":"https://example.invalid/index.json","sha1":hex::encode(Sha1::digest(index)),"size":index.len()}});
    storage::write_json(&root.join("metadata/versions/1.20.1.json"), &base).unwrap();
    let bytes = fs::read(root.join("metadata/versions/1.20.1.json")).unwrap();
    storage::write_json(&root.join("metadata/manifest.json"), &json!({"latest":{"release":"1.20.1"},"versions":[{"id":"1.20.1","type":"release","url":"https://example.invalid/version.json","sha1":hex::encode(Sha1::digest(&bytes))}]})).unwrap();
    fs::create_dir_all(game.join("versions/1.20.1")).unwrap();
    fs::write(game.join("versions/1.20.1/1.20.1.jar"), b"client").unwrap();
    fs::create_dir_all(game.join("assets/indexes")).unwrap();
    fs::write(game.join("assets/indexes/test.json"), index).unwrap();
    let source = temp.path().join("offline.mrpack");
    let mut manifest = mr(vec![pack_file("mods/test.jar", "required")]);
    manifest["dependencies"]
        .as_object_mut()
        .unwrap()
        .remove("fabric-loader");
    archive(
        &source,
        "modrinth.index.json",
        &manifest,
        &[
            ("overrides/mods/test.jar", b"overridden"),
            ("overrides/config/options.txt", b"initial"),
        ],
    );
    let instance = launcher
        .prepare_modpack(&instance.id, &source, "Offline pack", &[])
        .await
        .unwrap();
    let id = instance.selected_version.clone().unwrap();
    let directory = game.join("versions").join(&id);
    fs::write(directory.join("mods/test.jar"), b"mod").unwrap();
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    let sink: EventSink = Arc::new(move |event| {
        if event.step.as_deref() == Some("pack_game") {
            stop.cancel();
        }
    });
    assert!(
        launcher
            .install(&instance.id, &cancel, &sink)
            .await
            .is_err()
    );
    assert_eq!(
        fs::read(directory.join("mods/test.jar")).unwrap(),
        b"overridden"
    );
    fs::write(directory.join("config/options.txt"), b"my settings").unwrap();
    assert!(
        directory.join(".nekocrate/source.zip").is_file(),
        "interrupted install keeps its archive"
    );
    let sink: EventSink = Arc::new(|_| {});
    launcher
        .install(&instance.id, &CancellationToken::new(), &sink)
        .await
        .unwrap();
    assert!(
        launcher
            .instance(&instance.id)
            .unwrap()
            .selected()
            .unwrap()
            .installed
    );
    let resolved = version::resolve(&root, &game, &id).unwrap();
    assert!(
        !directory.join(".nekocrate/source.zip").exists(),
        "successful installation releases its archive"
    );
    assert!(source.is_file(), "the user's import is never deleted");
    assert!(
        launcher
            .modpack_status(&instance.id, &id)
            .await
            .unwrap()
            .missing
            .is_empty()
    );
    assert_eq!(
        version::working_directory(&game, &resolved.version).unwrap(),
        directory
    );
    assert_eq!(
        fs::read(directory.join("mods/test.jar")).unwrap(),
        b"overridden"
    );
    launcher
        .repair(&instance.id, &CancellationToken::new(), &sink)
        .await
        .unwrap();
    assert_eq!(
        fs::read(directory.join("config/options.txt")).unwrap(),
        b"my settings"
    );
    // A manual-only dependency must not prevent the other resources and base
    // game from installing, nor publish a runnable incomplete profile.
    let mut job = launcher.pack_job(&instance.id, &id).unwrap();
    let mut manual = job.pack.files[0].clone();
    manual.key = "manual".into();
    manual.path = Some("mods/manual.jar".into());
    manual.urls.clear();
    let mut automatic = manual.clone();
    automatic.key = "automatic".into();
    automatic.path = Some("mods/automatic.jar".into());
    automatic.urls = vec!["https://example.invalid/automatic.jar".into()];
    job.pack.files.extend([manual, automatic]);
    job.files_done = false;
    job.completed = false;
    record::write_job(&game, &id, &job).unwrap();
    let cache = crate::download::object_path(
        &root.join("cache/objects"),
        &FileSpec {
            url: "https://example.invalid/automatic.jar".into(),
            path: directory.join("mods/automatic.jar"),
            hash: hex::encode(Sha1::digest(b"mod")),
            sha256: false,
            size: 3,
        },
    )
    .unwrap();
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    fs::write(&cache, b"mod").unwrap();
    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = events.clone();
    let sink: EventSink = Arc::new(move |event| captured.lock().unwrap().push(event));
    assert!(
        !launcher
            .install(&instance.id, &CancellationToken::new(), &sink)
            .await
            .unwrap()
    );
    assert_eq!(
        fs::read(directory.join("mods/automatic.jar")).unwrap(),
        b"mod"
    );
    assert!(
        !launcher
            .instance(&instance.id)
            .unwrap()
            .selected()
            .unwrap()
            .installed
    );
    assert_eq!(
        launcher
            .modpack_status(&instance.id, &id)
            .await
            .unwrap()
            .missing
            .len(),
        1
    );
    assert!(
        events
            .lock()
            .unwrap()
            .iter()
            .any(|e| e.pack_missing.is_some())
    );
    assert!(
        !events
            .lock()
            .unwrap()
            .iter()
            .any(|e| e.kind == "complete" && e.message == "整合包已安装")
    );
    let supplied = temp.path().join("manual.jar");
    fs::write(&supplied, b"mod").unwrap();
    launcher
        .supply_modpack_files(&instance.id, &id, vec![supplied])
        .await
        .unwrap();
    assert!(
        launcher
            .install(&instance.id, &CancellationToken::new(), &sink)
            .await
            .unwrap()
    );
    assert!(
        launcher
            .instance(&instance.id)
            .unwrap()
            .selected()
            .unwrap()
            .installed
    );
}

#[test]
fn repair_targets_disabled_mods_without_reenabling_them() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("mods")).unwrap();
    let disabled = temp.path().join("mods/example.jar.disabled");
    std::fs::write(&disabled, b"disabled mod").unwrap();
    assert_eq!(
        archive::installed_destination(temp.path(), "mods/example.jar").unwrap(),
        disabled
    );
    let active = temp.path().join("mods/example.jar");
    std::fs::write(&active, b"active mod").unwrap();
    assert_eq!(
        archive::installed_destination(temp.path(), "mods/example.jar").unwrap(),
        active
    );
    assert!(archive::installed_destination(temp.path(), "../outside.jar").is_err());
}
