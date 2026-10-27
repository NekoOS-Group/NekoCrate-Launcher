use super::*;
use crate::{
    model::{EventSink, Library},
    version_management::ContentKind,
};
use serde_json::json;
use std::{io::Write, sync::Arc};
use tokio_util::sync::CancellationToken;

fn fixture(isolated: bool) -> (tempfile::TempDir, Launcher, String, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
    let instance = launcher.create_instance("Games", "1.20.1").unwrap();
    let game = instance.game_dir(launcher.root());
    let mut profile = crate::test_http::version_fixture("1.20.1");
    profile.libraries.push(
        serde_json::from_value::<Library>(json!({"name":"net.fabricmc:fabric-loader:0.16.10"}))
            .unwrap(),
    );
    storage::write_json(
        &crate::version::metadata_path(&game, "1.20.1").unwrap(),
        &profile,
    )
    .unwrap();
    let working = if isolated {
        game.join("versions/1.20.1")
    } else {
        game
    };
    if isolated {
        fs::create_dir_all(working.join("mods")).unwrap();
    }
    (temp, launcher, instance.id, working)
}
fn archive(path: &Path, entries: &[(&str, &str)]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, data) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}
fn local(path: &Path) -> LocalImport {
    LocalImport {
        path: path.to_owned(),
        kind: None,
    }
}
fn sink() -> EventSink {
    Arc::new(|_| {})
}

#[test]
fn content_directory_uses_the_actual_profile_and_does_not_create_missing_folders() {
    let (_temp, launcher, id, working) = fixture(true);
    assert_eq!(
        launcher
            .version_content_directory(&id, "1.20.1", ContentKind::Mods)
            .unwrap(),
        working.join("mods")
    );
    assert!(
        launcher
            .version_content_directory(&id, "1.20.1", ContentKind::Resourcepacks)
            .is_err()
    );
    assert!(!working.join("resourcepacks").exists());
    assert!(
        launcher
            .version_content_directory(&id, "untracked", ContentKind::Mods)
            .is_err()
    );
}
async fn install(launcher: &Launcher, plan: &ResourcePlan) -> Result<()> {
    launcher
        .install_resources(
            &plan.target.instance,
            &plan.target.version,
            &plan.id,
            &CancellationToken::new(),
            &sink(),
        )
        .await
}

#[test]
fn detects_payloads_instead_of_assuming_every_zip_is_a_modpack() {
    let temp = tempfile::tempdir().unwrap();
    for (name, entries, kind, prefix) in [
        (
            "pack.zip",
            vec![("pack.mcmeta", "{}"), ("assets/a/file", "a")],
            ResourceKind::Resourcepacks,
            None,
        ),
        (
            "data.zip",
            vec![("wrapper/pack.mcmeta", "{}"), ("wrapper/data/a/file", "a")],
            ResourceKind::Datapacks,
            Some("wrapper/"),
        ),
        (
            "world.zip",
            vec![
                ("My World/level.dat", "NBT"),
                ("My World/region/r.0.0.mca", "region"),
            ],
            ResourceKind::Saves,
            Some("My World/"),
        ),
        (
            "shader.zip",
            vec![("shaders/shaders.properties", "# shader")],
            ResourceKind::Shaderpacks,
            None,
        ),
        (
            "mod.jar",
            vec![("fabric.mod.json", "{}")],
            ResourceKind::Mods,
            None,
        ),
    ] {
        let path = temp.path().join(name);
        archive(&path, &entries);
        let detected = imports::inspect(&path).unwrap();
        assert_eq!(detected.format, "resource");
        assert_eq!(detected.kinds, [kind]);
        assert_eq!(detected.prefix.as_deref(), prefix);
    }
    let path = temp.path().join("modpack.zip");
    archive(
        &path,
        &[("manifest.json", r#"{"manifestType":"minecraftModpack"}"#)],
    );
    assert_eq!(imports::inspect(&path).unwrap().format, "modpack");
    archive(
        &path,
        &[
            ("manifest.json", "{}"),
            ("pack.mcmeta", "{}"),
            ("assets/a/a", "a"),
        ],
    );
    assert_eq!(
        imports::inspect(&path).unwrap().kinds,
        [ResourceKind::Resourcepacks]
    );
}

#[tokio::test]
async fn imports_all_resource_kinds_to_the_selected_profile_and_persists_sources() {
    let (temp, launcher, id, working) = fixture(true);
    fs::create_dir_all(working.join("saves/Existing")).unwrap();
    fs::write(working.join("saves/Existing/level.dat"), b"existing").unwrap();
    let files = [
        (
            "cat.jar",
            vec![("fabric.mod.json", r#"{"id":"cat","version":"1.0"}"#)],
            "mods/cat.jar",
        ),
        (
            "textures.zip",
            vec![("pack.mcmeta", "{}"), ("assets/a/file", "a")],
            "resourcepacks/textures.zip",
        ),
        (
            "data.zip",
            vec![("pack.mcmeta", "{}"), ("data/a/file", "a")],
            "saves/Existing/datapacks/data.zip",
        ),
        (
            "shader.zip",
            vec![("shaders/a.vsh", "shader")],
            "shaderpacks/shader.zip",
        ),
        (
            "Map.zip",
            vec![
                ("Wrapper/level.dat", "map"),
                ("Wrapper/region/r.0.0.mca", "region"),
            ],
            "saves/Map",
        ),
    ];
    let mut inputs = Vec::new();
    for (name, contents, _) in &files {
        let path = temp.path().join(name);
        archive(&path, contents);
        inputs.push(local(&path));
    }
    let plan = launcher
        .prepare_local_resources(
            &id,
            "1.20.1",
            &inputs,
            Some("Existing"),
            &CancellationToken::new(),
        )
        .unwrap();
    assert!(plan.conflicts.is_empty());
    assert!(!plan.target.shared);
    install(&launcher, &plan).await.unwrap();
    for (source, _, destination) in files {
        assert!(working.join(destination).exists());
        assert!(temp.path().join(source).exists());
    }
    assert_eq!(
        fs::read(working.join("saves/Map/level.dat")).unwrap(),
        b"map"
    );
    assert!(!working.join("saves/Map/Wrapper").exists());
    assert_eq!(records(&working).unwrap().len(), 5);
    assert!(launcher.resource_plans(&id, "1.20.1").unwrap().is_empty());
    let counts = launcher.version_content_counts(&id, "1.20.1").unwrap();
    assert_eq!(
        (
            counts.mods,
            counts.resourcepacks,
            counts.datapacks,
            counts.shaderpacks,
            counts.saves
        ),
        (1, 1, 1, 1, 2)
    );
    let metadata = launcher
        .version_content_metadata(&id, "1.20.1", ContentKind::Mods, "cat.jar")
        .unwrap();
    assert!(
        metadata
            .sources
            .iter()
            .any(|s| s.source == ".nekocrate/resources.json")
    );
}

#[tokio::test]
async fn shared_directory_is_explicit_and_identical_files_are_reused() {
    let (temp, launcher, id, working) = fixture(false);
    let source = temp.path().join("cat.jar");
    archive(&source, &[("fabric.mod.json", r#"{"id":"cat"}"#)]);
    let plan = launcher
        .prepare_local_resources(
            &id,
            "1.20.1",
            &[local(&source)],
            None,
            &CancellationToken::new(),
        )
        .unwrap();
    assert!(plan.target.shared);
    install(&launcher, &plan).await.unwrap();
    let second = launcher
        .prepare_local_resources(
            &id,
            "1.20.1",
            &[local(&source)],
            None,
            &CancellationToken::new(),
        )
        .unwrap();
    assert!(second.entries[0].reuse);
    assert!(second.conflicts.is_empty());
    install(&launcher, &second).await.unwrap();
    assert!(working.join("mods/cat.jar").is_file());
}

#[tokio::test]
async fn conflict_and_changed_source_never_overwrite_existing_files() {
    let (temp, launcher, id, working) = fixture(true);
    let source = temp.path().join("cat.jar");
    archive(&source, &[("fabric.mod.json", r#"{"id":"cat"}"#)]);
    let plan = launcher
        .prepare_local_resources(
            &id,
            "1.20.1",
            &[local(&source)],
            None,
            &CancellationToken::new(),
        )
        .unwrap();
    archive(&source, &[("fabric.mod.json", r#"{"id":"changed"}"#)]);
    assert!(
        install(&launcher, &plan)
            .await
            .unwrap_err()
            .to_string()
            .contains("变化")
    );
    assert!(!working.join("mods/cat.jar").exists());
    fs::write(working.join("mods/cat.jar"), b"user-file").unwrap();
    let conflict = launcher
        .prepare_local_resources(
            &id,
            "1.20.1",
            &[local(&source)],
            None,
            &CancellationToken::new(),
        )
        .unwrap();
    assert!(!conflict.conflicts.is_empty());
    assert!(install(&launcher, &conflict).await.is_err());
    assert_eq!(
        fs::read(working.join("mods/cat.jar")).unwrap(),
        b"user-file"
    );
}

#[test]
fn datapacks_require_a_real_world_and_ambiguous_packs_require_a_choice() {
    let (temp, launcher, id, _) = fixture(true);
    let source = temp.path().join("dual.zip");
    archive(
        &source,
        &[
            ("pack.mcmeta", "{}"),
            ("assets/a/file", "a"),
            ("data/a/file", "a"),
        ],
    );
    assert_eq!(imports::inspect(&source).unwrap().kinds.len(), 2);
    assert!(
        launcher
            .prepare_local_resources(
                &id,
                "1.20.1",
                &[local(&source)],
                None,
                &CancellationToken::new()
            )
            .is_err()
    );
    let input = LocalImport {
        path: source,
        kind: Some(ResourceKind::Datapacks),
    };
    assert!(
        launcher
            .prepare_local_resources(
                &id,
                "1.20.1",
                &[input],
                Some("../world"),
                &CancellationToken::new()
            )
            .is_err()
    );
}

#[test]
fn unsafe_archives_and_directory_symlinks_cannot_escape_the_target() {
    let (temp, launcher, id, working) = fixture(true);
    let source = temp.path().join("unsafe.zip");
    archive(&source, &[("../level.dat", "evil")]);
    assert!(imports::inspect(&source).is_err());
    #[cfg(unix)]
    {
        let external = temp.path().join("external");
        fs::create_dir(&external).unwrap();
        std::os::unix::fs::symlink(&external, working.join("resourcepacks")).unwrap();
        archive(&source, &[("pack.mcmeta", "{}"), ("assets/a/file", "a")]);
        assert!(
            launcher
                .prepare_local_resources(
                    &id,
                    "1.20.1",
                    &[local(&source)],
                    None,
                    &CancellationToken::new()
                )
                .is_err()
        );
        assert_eq!(fs::read_dir(&external).unwrap().count(), 0);
        let map = temp.path().join("Map");
        fs::create_dir(&map).unwrap();
        fs::write(map.join("level.dat"), b"world").unwrap();
        std::os::unix::fs::symlink(&external, map.join("region")).unwrap();
        assert!(
            launcher
                .prepare_local_resources(
                    &id,
                    "1.20.1",
                    &[local(&map)],
                    None,
                    &CancellationToken::new()
                )
                .is_err()
        );
    }
}

#[tokio::test]
async fn cancelled_plan_survives_restart_and_duplicate_mod_ids_are_rejected() {
    let (temp, launcher, id, working) = fixture(true);
    let source = temp.path().join("cat.jar");
    archive(&source, &[("fabric.mod.json", r#"{"id":"cat"}"#)]);
    let plan = launcher
        .prepare_local_resources(
            &id,
            "1.20.1",
            &[local(&source)],
            None,
            &CancellationToken::new(),
        )
        .unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(
        launcher
            .install_resources(&id, "1.20.1", &plan.id, &cancel, &sink())
            .await
            .is_err()
    );
    assert!(!working.join("mods/cat.jar").exists());
    let root = launcher.root().to_owned();
    drop(launcher);
    let launcher = Launcher::open(Some(root)).unwrap();
    assert_eq!(launcher.resource_plans(&id, "1.20.1").unwrap().len(), 1);
    install(&launcher, &plan).await.unwrap();
    let duplicate = temp.path().join("another-name.jar");
    fs::copy(&source, &duplicate).unwrap();
    let plan = launcher
        .prepare_local_resources(
            &id,
            "1.20.1",
            &[local(&duplicate)],
            None,
            &CancellationToken::new(),
        )
        .unwrap();
    assert!(plan.conflicts.iter().any(|s| s.contains("Mod ID 冲突")));
    assert!(install(&launcher, &plan).await.is_err());
    assert!(!working.join("mods/another-name.jar").exists());
}

fn cache(launcher: &Launcher, path: &str, args: &[(&str, String)], value: serde_json::Value) {
    let url = reqwest::Url::parse_with_params(&format!("https://api.modrinth.com/v2/{path}"), args)
        .unwrap();
    let hash = hex::encode(ring::digest::digest(
        &ring::digest::SHA256,
        url.as_str().as_bytes(),
    ));
    storage::write_json(
        &launcher
            .root()
            .join(format!("metadata/resources/{hash}.json")),
        &value,
    )
    .unwrap();
}
fn remote(id: &str, project: &str, dependencies: serde_json::Value) -> serde_json::Value {
    json!({"id":id,"project_id":project,"name":id,"version_number":"1.0","version_type":"release", "game_versions":["1.20.1"],"loaders":["fabric"],"files":[{"filename":format!("{project}.jar"),"url":format!("https://cdn.modrinth.com/data/{project}/{id}.jar"),"size":10,"primary":true,"hashes":{"sha1":"0000000000000000000000000000000000000000"}}],"dependencies":dependencies})
}
fn cache_project(launcher: &Launcher, id: &str) {
    cache(
        launcher,
        &format!("project/{id}"),
        &[],
        json!({"id":id,"title":id,"project_type":"mod"}),
    );
}

#[tokio::test]
async fn modpack_catalog_is_independent_of_destination_game_and_pages_versions() {
    let (_temp, launcher, _, _) = fixture(true);
    cache(
        &launcher,
        "search",
        &[
            ("query", "cats".into()),
            ("offset", "20".into()),
            ("limit", "20".into()),
            ("facets", json!([["project_type:modpack"]]).to_string()),
        ],
        json!({"hits":[{"project_id":"cats","title":"Cats","project_type":"modpack"}],"total_hits":21}),
    );
    cache(
        &launcher,
        "project/cats",
        &[],
        json!({"id":"cats","project_type":"modpack"}),
    );
    let versions: Vec<_> = (0..51)
        .map(|n| {
            let mut v = remote(&format!("pack{n}"), "cats", json!([]));
            v["game_versions"] = json!(["1.21.1"]);
            v
        })
        .collect();
    cache(&launcher, "project/cats/version", &[], json!(versions));
    let cancel = CancellationToken::new();
    let result = launcher
        .search_modpacks(Platform::Modrinth, "cats", 20, &cancel)
        .await
        .unwrap();
    assert_eq!(result.total, 21);
    assert_eq!(result.hits[0].name, "Cats");
    let first = launcher
        .modpack_versions(Platform::Modrinth, "cats", 0, &cancel)
        .await
        .unwrap();
    assert_eq!(first.versions.len(), 50);
    assert!(first.more);
    assert_eq!(first.versions[0].games, ["1.21.1"]);
    let last = launcher
        .modpack_versions(Platform::Modrinth, "cats", 50, &cancel)
        .await
        .unwrap();
    assert_eq!(last.versions[0].id, "pack50");
    assert!(!last.more);
    assert!(
        launcher
            .search_modpacks(Platform::Modrinth, "cats", 10000, &cancel)
            .await
            .is_err()
    );
    cache_project(&launcher, "mod");
    assert!(
        launcher
            .modpack_versions(Platform::Modrinth, "mod", 0, &cancel)
            .await
            .unwrap_err()
            .to_string()
            .contains("不是整合包")
    );
}

#[tokio::test]
async fn online_modpack_uses_verified_archive_cache_and_waits_for_installation_review() {
    use sha1::{Digest, Sha1};
    let (temp, launcher, id, working) = fixture(true);
    let source = temp.path().join("cats.mrpack");
    archive(
        &source,
        &[
            (
                "modrinth.index.json",
                r#"{"formatVersion":1,"game":"minecraft","name":"Cat pack","versionId":"1","dependencies":{"minecraft":"1.21.1","fabric-loader":"0.16.10"},"files":[]}"#,
            ),
            ("overrides/config/cat.txt", "meow"),
        ],
    );
    let bytes = fs::read(source).unwrap();
    let hash = hex::encode(Sha1::digest(&bytes));
    let cached = launcher
        .root()
        .join("cache/objects/sha1")
        .join(&hash[..2])
        .join(&hash);
    fs::create_dir_all(cached.parent().unwrap()).unwrap();
    fs::write(cached, &bytes).unwrap();
    let mut value = remote("pack1", "cats", json!([]));
    value["files"][0]["filename"] = json!("cats.mrpack");
    value["files"][0]["size"] = json!(bytes.len());
    value["files"][0]["hashes"]["sha1"] = json!(hash);
    cache(
        &launcher,
        "project/cats",
        &[],
        json!({"id":"cats","project_type":"modpack"}),
    );
    cache(&launcher, "version/pack1", &[], value.clone());
    let cancel = CancellationToken::new();
    let downloaded = launcher
        .download_modpack(Platform::Modrinth, "cats", "pack1", &cancel, &sink())
        .await
        .unwrap();
    assert_eq!(downloaded.preview.pack.name, "Cat pack");
    assert_eq!(downloaded.preview.pack.minecraft, "1.21.1");
    assert_eq!(fs::read(&downloaded.path).unwrap(), bytes);
    assert_eq!(launcher.instance(&id).unwrap().versions.len(), 1);
    assert!(!working.join("config/cat.txt").exists());
    let prepared = launcher
        .prepare_modpack(&id, &downloaded.path, "Cat pack", &[])
        .await
        .unwrap();
    let folder = prepared.game_dir(launcher.root()).join("versions/Cat pack");
    assert!(folder.join("modrinth.index.json").is_file());
    assert!(
        launcher
            .prepare_modpack(&id, &downloaded.path, "Cat pack", &[])
            .await
            .is_err()
    );
    assert!(
        launcher
            .download_modpack(Platform::Modrinth, "cats", "pack1", &cancel, &sink())
            .await
            .is_ok()
    );
    value["project_id"] = json!("another");
    cache(&launcher, "version/pack1", &[], value.clone());
    assert!(
        launcher
            .download_modpack(Platform::Modrinth, "cats", "pack1", &cancel, &sink())
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("不属于")
    );
    value["project_id"] = json!("cats");
    value["files"][0]["url"] = json!("http://example.com/pack.mrpack");
    cache(&launcher, "version/pack1", &[], value);
    assert!(
        launcher
            .download_modpack(Platform::Modrinth, "cats", "pack1", &cancel, &sink())
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("地址无效")
    );
    cancel.cancel();
    assert!(
        launcher
            .download_modpack(Platform::Modrinth, "cats", "pack1", &cancel, &sink())
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("取消")
    );
}

#[tokio::test]
#[ignore = "requires public Modrinth APIs and downloads one archive into a temporary directory"]
async fn live_online_modpack_search_download_and_preview() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
    let cancel = CancellationToken::new();
    let result = launcher
        .search_modpacks(Platform::Modrinth, "Fabulously Optimized", 0, &cancel)
        .await
        .unwrap();
    let project = &result.hits.first().unwrap().id;
    let versions = launcher
        .modpack_versions(Platform::Modrinth, project, 0, &cancel)
        .await
        .unwrap();
    let archive = launcher
        .download_modpack(
            Platform::Modrinth,
            project,
            &versions.versions[0].id,
            &cancel,
            &sink(),
        )
        .await
        .unwrap();
    assert_eq!(archive.preview.pack.format, "modrinth");
    assert!(!archive.preview.pack.files.is_empty());
    assert!(launcher.snapshot().database.instances.is_empty());
}
fn scope(id: &str) -> ResourceScope {
    ResourceScope {
        id: id.into(),
        version: "1.20.1".into(),
        kind: ResourceKind::Mods,
        platform: Platform::Modrinth,
    }
}

#[tokio::test]
async fn required_dependency_cycles_are_deduplicated_and_optional_dependencies_are_not_installed() {
    let (_temp, launcher, id, _working) = fixture(true);
    cache_project(&launcher, "cat");
    cache_project(&launcher, "base");
    cache(
        &launcher,
        "version/cat1",
        &[],
        remote(
            "cat1",
            "cat",
            json!([
                {"project_id":"base","version_id":"base1","dependency_type":"required"},
                {"project_id":"optional","dependency_type":"optional"}
            ]),
        ),
    );
    cache(
        &launcher,
        "version/base1",
        &[],
        remote(
            "base1",
            "base",
            json!([
                {"project_id":"cat","version_id":"cat1","dependency_type":"required"}
            ]),
        ),
    );
    let plan = launcher
        .prepare_resource(&scope(&id), "cat", "cat1", None, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(plan.entries.len(), 2);
    assert!(plan.conflicts.is_empty());
    assert_eq!(plan.entries[0].origin.dependencies.len(), 2);
}

#[tokio::test]
async fn incompatible_versions_and_conflicting_pinned_dependencies_stop_before_downloading() {
    let (_temp, launcher, id, _) = fixture(true);
    cache_project(&launcher, "cat");
    cache_project(&launcher, "base");
    cache(
        &launcher,
        "version/cat1",
        &[],
        remote(
            "cat1",
            "cat",
            json!([
                {"project_id":"base","version_id":"base1","dependency_type":"required"},
                {"project_id":"base","version_id":"base2","dependency_type":"required"},
                {"project_id":"base","dependency_type":"incompatible"}
            ]),
        ),
    );
    cache(
        &launcher,
        "version/base1",
        &[],
        remote("base1", "base", json!([])),
    );
    cache(
        &launcher,
        "version/base2",
        &[],
        remote("base2", "base", json!([])),
    );
    let plan = launcher
        .prepare_resource(&scope(&id), "cat", "cat1", None, &CancellationToken::new())
        .await
        .unwrap();
    assert!(
        plan.conflicts
            .iter()
            .any(|s| s.contains("依赖要求不同版本"))
    );
    assert!(plan.conflicts.iter().any(|s| s.contains("声明不兼容")));
    assert!(install(&launcher, &plan).await.is_err());
    let mut wrong = remote("wrong", "cat", json!([]));
    wrong["game_versions"] = json!(["1.21.1"]);
    cache(&launcher, "version/wrong", &[], wrong);
    assert!(
        !launcher
            .prepare_resource(&scope(&id), "cat", "wrong", None, &CancellationToken::new())
            .await
            .unwrap()
            .conflicts
            .is_empty()
    );
}

#[tokio::test]
async fn remote_install_uses_verified_cache_and_records_platform_ids() {
    use sha1::{Digest, Sha1};
    let (temp, launcher, id, working) = fixture(true);
    let jar = temp.path().join("cat.jar");
    archive(
        &jar,
        &[("fabric.mod.json", r#"{"id":"cat","version":"1.0"}"#)],
    );
    let data = fs::read(jar).unwrap();
    let hash = hex::encode(Sha1::digest(&data));
    let path = launcher
        .root()
        .join("cache/objects/sha1")
        .join(&hash[..2])
        .join(&hash);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, &data).unwrap();
    let mut value = remote("cat1", "cat", json!([]));
    value["files"][0]["size"] = json!(data.len());
    value["files"][0]["hashes"]["sha1"] = json!(hash);
    cache_project(&launcher, "cat");
    cache(&launcher, "version/cat1", &[], value);
    let plan = launcher
        .prepare_resource(&scope(&id), "cat", "cat1", None, &CancellationToken::new())
        .await
        .unwrap();
    install(&launcher, &plan).await.unwrap();
    assert_eq!(fs::read(working.join("mods/cat.jar")).unwrap(), data);
    let record = records(&working).unwrap().remove("mods/cat.jar").unwrap();
    assert_eq!(record.origin.platform, Platform::Modrinth);
    assert_eq!(record.origin.project.as_deref(), Some("cat"));
    assert_eq!(record.origin.version.as_deref(), Some("cat1"));
}

#[test]
fn curseforge_metadata_preserves_hashes_required_dependencies_and_loader_constraints() {
    let version=provider::parse_version(&json!({"id":45,"modId":123,"displayName":"Cat","fileName":"cat.jar","fileLength":10,"downloadUrl":"https://edge.forgecdn.net/files/cat.jar","gameVersions":["1.20.1","Forge"],"hashes":[{"algo":1,"value":"0000000000000000000000000000000000000000"}],"dependencies":[{"modId":9,"relationType":3},{"modId":10,"relationType":5}]}),Platform::Curseforge).unwrap();
    let (_temp, launcher, id, _) = fixture(true);
    let mut target = launcher.resource_target(&id, "1.20.1").unwrap();
    assert!(!provider::compatible(&version, &target, ResourceKind::Mods));
    target.loader = Some(ModLoader {
        kind: "forge".into(),
        version: "47.4.4".into(),
    });
    assert!(provider::compatible(&version, &target, ResourceKind::Mods));
    assert_eq!(version.dependencies[0].relation, "required");
    assert_eq!(version.dependencies[1].relation, "incompatible");
}

#[tokio::test]
#[ignore = "requires the public Modrinth API; no real game directory is used"]
async fn live_modrinth_search_versions_and_dependency_preview() {
    let (_temp, launcher, id, _) = fixture(true);
    let token = CancellationToken::new();
    for kind in [
        ResourceKind::Mods,
        ResourceKind::Resourcepacks,
        ResourceKind::Datapacks,
        ResourceKind::Shaderpacks,
    ] {
        let scope = ResourceScope { kind, ..scope(&id) };
        let results = launcher
            .search_resources(
                &scope,
                if kind == ResourceKind::Mods {
                    "Fabric API"
                } else {
                    ""
                },
                0,
                &token,
            )
            .await
            .unwrap();
        assert!(!results.hits.is_empty());
        let mut checked = false;
        for project in results.hits.iter().take(5) {
            let versions = launcher
                .resource_versions(&scope, &project.id, &token)
                .await
                .unwrap();
            if versions.is_empty() {
                continue;
            }
            if kind != ResourceKind::Datapacks {
                let plan = launcher
                    .prepare_resource(&scope, &project.id, &versions[0].id, None, &token)
                    .await
                    .unwrap();
                assert!(!plan.entries.is_empty());
                assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
            }
            checked = true;
            break;
        }
        assert!(checked, "no usable result for {kind:?}");
    }
}

#[test]
fn copying_on_filesystems_without_hardlinks_never_replaces_existing_files_or_keeps_corrupt_partials()
 {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let output = temp.path().join("output");
    fs::write(&source, b"verified").unwrap();
    let hash = imports::fingerprint(&source, &CancellationToken::new())
        .unwrap()
        .0;
    install::copy_new(&source, &output, &hash, &CancellationToken::new()).unwrap();
    fs::write(&source, b"changed").unwrap();
    assert!(install::copy_new(&source, &output, &hash, &CancellationToken::new()).is_err());
    assert_eq!(fs::read(&output).unwrap(), b"verified");
    fs::remove_file(&output).unwrap();
    assert!(install::copy_new(&source, &output, &hash, &CancellationToken::new()).is_err());
    assert!(!output.exists());
}

#[tokio::test]
async fn interrupted_publication_recovers_from_the_checkpoint_without_requiring_the_original() {
    let (temp, launcher, id, working) = fixture(true);
    let source = temp.path().join("Map.zip");
    archive(
        &source,
        &[("level.dat", "world"), ("region/r.0.0.mca", "region")],
    );
    let mut plan = launcher
        .prepare_local_resources(
            &id,
            "1.20.1",
            &[local(&source)],
            None,
            &CancellationToken::new(),
        )
        .unwrap();
    assert!(
        launcher.resource_plans(&id, "1.20.1").unwrap().is_empty(),
        "preview is not an installation"
    );
    let published = working.join("saves/Map");
    fs::create_dir_all(published.join("region")).unwrap();
    fs::write(published.join("level.dat"), b"world").unwrap();
    fs::write(published.join("region/r.0.0.mca"), b"region").unwrap();
    plan.entries[0].content_sha256 = Some(
        imports::fingerprint(&published, &CancellationToken::new())
            .unwrap()
            .0,
    );
    plan.started = true;
    save_plan(&plan).unwrap();
    fs::remove_file(&source).unwrap();
    install(&launcher, &plan).await.unwrap();
    assert_eq!(records(&working).unwrap().len(), 1);
    assert_eq!(fs::read(published.join("level.dat")).unwrap(), b"world");
}
