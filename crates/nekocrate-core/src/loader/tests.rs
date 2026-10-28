use super::*;
use serde_json::json;

#[tokio::test]
async fn preparation_is_discoverable_isolated_and_does_not_overwrite_profiles() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("data"))).unwrap();
    let instance = launcher.create_instance("Loaders", "").unwrap();
    let game = instance.game_dir(launcher.root());
    storage::write_json(
        &launcher.root().join("metadata/loaders/forge.json"),
        &json!({"1.20.1":["1.20.1-47.4.4"]}),
    )
    .unwrap();
    storage::write_json(
        &launcher
            .root()
            .join("metadata/loaders/forge-promotions.json"),
        &json!({"promos":{"1.20.1-recommended":"47.4.4"}}),
    )
    .unwrap();
    let loader = ModLoader {
        kind: "forge".into(),
        version: "47.4.4".into(),
    };
    let cancel = CancellationToken::new();
    let added = launcher
        .add_loader_version(&instance.id, "1.20.1", loader.clone(), &cancel)
        .await
        .unwrap();
    let id = added.selected_version.as_deref().unwrap();
    let dir = game.join("versions").join(id);
    assert!(dir.join("mods").is_dir());
    assert!(!added.selected().unwrap().installed);
    assert!(
        launcher
            .version_metadata(&instance.id, id)
            .unwrap()
            .issue
            .is_none()
    );
    let content = launcher
        .version_content(
            &instance.id,
            id,
            crate::version_management::ContentKind::Mods,
        )
        .unwrap();
    assert!(content.directory.starts_with(&dir));
    let mut record = read_record(&game, id).unwrap();
    record.progress = 43.0;
    storage::write_json(&record_path(&game, id).unwrap(), &record).unwrap();
    let other = Launcher::open(Some(temp.path().join("other-data"))).unwrap();
    let imported = other
        .import_instance(game.clone(), "Linked".into(), "".into())
        .await
        .unwrap();
    assert!(
        imported
            .versions
            .iter()
            .any(|v| v.id == id && v.download_progress == 43.0 && !v.installed)
    );
    let retried = launcher
        .add_loader_version(&instance.id, "1.20.1", loader, &cancel)
        .await
        .unwrap();
    assert_eq!(retried.versions.iter().filter(|v| v.id == id).count(), 1);
    assert_eq!(read_record(&game, id).unwrap().progress, 43.0);
    let different = ModLoader {
        kind: "forge".into(),
        version: "47.4.3".into(),
    };
    let occupied = game
        .join("versions")
        .join(profile_id("1.20.1", &different).unwrap());
    std::fs::create_dir_all(&occupied).unwrap();
    std::fs::write(occupied.join("keep.txt"), b"user files").unwrap();
    assert!(
        launcher
            .add_loader_version(&instance.id, "1.20.1", different, &cancel)
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read(occupied.join("keep.txt")).unwrap(),
        b"user files"
    );
}

#[test]
fn records_reject_moved_targets_and_unsafe_versions() {
    let temp = tempfile::tempdir().unwrap();
    let loader = ModLoader {
        kind: "neoforge".into(),
        version: "21.1.172".into(),
    };
    let id = profile_id("1.21.1", &loader).unwrap();
    let record = InstallRecord {
        version: "wrong".into(),
        minecraft: "1.21.1".into(),
        loader,
        completed: false,
        progress: 0.0,
    };
    storage::write_json(&record_path(temp.path(), &id).unwrap(), &record).unwrap();
    assert!(read_record(temp.path(), &id).is_err());
    assert!(profile_id("../../elsewhere", &record.loader).is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn modern_processors_resume_repair_and_publish_only_after_success() {
    use sha1::{Digest, Sha1};
    use std::{fs, io::Write, os::unix::fs::PermissionsExt};
    let hash = |bytes: &[u8]| hex::encode(Sha1::digest(bytes));
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("data"))).unwrap();
    let instance = launcher.create_instance("处理器测试", "").unwrap();
    let game = instance.game_dir(launcher.root());
    let mut settings = launcher.settings();
    settings.proxy_mode = "none".into();
    settings.download_source = "official".into();
    settings.retries = 0;
    launcher.save_settings(settings).unwrap();
    let mut base = serde_json::to_value(crate::test_http::version_fixture("1.20.1")).unwrap();
    let bytes = b"fixture client";
    let index = br#"{"objects":{}}"#;
    base["downloads"] =
        json!({"client":{"url":"http://127.0.0.1:9/client","sha1":hash(bytes),"size":bytes.len()}});
    base["assetIndex"] = json!({"id":"1.20.1","url":"http://127.0.0.1:9/index","sha1":hash(index),"size":index.len()});
    let base_path = launcher.root().join("metadata/versions/1.20.1.json");
    storage::write_json(&base_path, &base).unwrap();
    storage::write_json(&launcher.root().join("metadata/manifest.json"),&json!({"latest":{},"versions":[{"id":"1.20.1","type":"release","url":"http://127.0.0.1:9/metadata","sha1":hash(&fs::read(&base_path).unwrap())}]})).unwrap();
    let client = game.join("versions/1.20.1/1.20.1.jar");
    fs::create_dir_all(client.parent().unwrap()).unwrap();
    fs::write(&client, bytes).unwrap();
    let assets = game.join("assets/indexes/1.20.1.json");
    fs::create_dir_all(assets.parent().unwrap()).unwrap();
    fs::write(assets, index).unwrap();
    storage::write_json(
        &launcher.root().join("metadata/loaders/forge.json"),
        &json!({"1.20.1":["1.20.1-47.4.4"]}),
    )
    .unwrap();
    storage::write_json(
        &launcher
            .root()
            .join("metadata/loaders/forge-promotions.json"),
        &json!({"promos":{}}),
    )
    .unwrap();
    let token = CancellationToken::new();
    let added = launcher
        .add_loader_version(
            &instance.id,
            "1.20.1",
            ModLoader {
                kind: "forge".into(),
                version: "47.4.4".into(),
            },
            &token,
        )
        .await
        .unwrap();
    let id = added.selected_version.unwrap();
    let cache = launcher
        .root()
        .join("metadata/loader-installers/forge/1.20.1-47.4.4");
    fs::create_dir_all(&cache).unwrap();
    let processor_path = "fixture/processor/1/processor-1.jar";
    let mut processor = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    processor
        .start_file(
            "META-INF/MANIFEST.MF",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    processor
        .write_all(b"Manifest-Version: 1.0\nMain-Class: FixtureProcessor\n")
        .unwrap();
    let processor_bytes = processor.finish().unwrap().into_inner();
    let generated_path = "net/minecraftforge/forge/1.20.1-47.4.4/forge-1.20.1-47.4.4-client.jar";
    let output = format!("{{LIBRARY_DIR}}/{generated_path}");
    let profile = json!({"id":"forge","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher","arguments":{"game":["--fml.forgeVersion","47.4.4","--fml.mcVersion","1.20.1"]},"libraries":[{"name":"net.minecraftforge:forge:1.20.1-47.4.4:client","downloads":{"artifact":{"path":generated_path,"url":"","sha1":hash(b"generated"),"size":9}}}]});
    let install = json!({"spec":1,"minecraft":"1.20.1","json":"version.json","libraries":[{"name":"fixture:processor:1","downloads":{"artifact":{"path":processor_path,"url":"","sha1":hash(&processor_bytes),"size":processor_bytes.len()}}}],"processors":[{"jar":"fixture:processor:1","classpath":[],"args":[output],"outputs":{output:hash(b"generated")}}]});
    let jar = cache.join("installer.jar");
    let mut archive = zip::ZipWriter::new(fs::File::create(&jar).unwrap());
    for (name, data) in [
        (
            "install_profile.json".to_owned(),
            serde_json::to_vec(&install).unwrap(),
        ),
        ("version.json".into(), serde_json::to_vec(&profile).unwrap()),
        (format!("maven/{processor_path}"), processor_bytes),
    ] {
        archive
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(&data).unwrap();
    }
    archive.finish().unwrap();
    let installer_bytes = fs::read(&jar).unwrap();
    let installer_hash = hash(&installer_bytes);
    storage::write_json(&cache.join("installer.sha1.json"), &installer_hash).unwrap();
    let java = temp.path().join("java");
    fs::write(
        &java,
        format!(
            r#"#!/bin/sh
if [ "$2" = "-version" ]; then
  printf '%s\n' 'openjdk version "8.0.1"' '    os.arch = {}' >&2
  exit 0
fi
printf 'run\n' >> processor-runs.txt
if [ -f fail-processor ]; then echo 'fixture processor failed' >&2; exit 2; fi
for arg in "$@"; do target="$arg"; done
mkdir -p "$(dirname "$target")"
printf 'generated' > "$target"
"#,
            crate::minecraft::arch_name()
        ),
    )
    .unwrap();
    fs::set_permissions(&java, fs::Permissions::from_mode(0o700)).unwrap();
    launcher
        .update_instance(
            &instance.id,
            "处理器测试",
            2048,
            Some(java.to_string_lossy().into()),
            None,
        )
        .unwrap();
    fs::write(game.join("fail-processor"), b"fail").unwrap();
    let sink: EventSink = Arc::new(|_| {});
    let error = launcher
        .install(&instance.id, &token, &sink)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("fixture processor failed"),
        "{error:#}"
    );
    assert!(!version::metadata_path(&game, &id).unwrap().exists());
    assert!(!read_record(&game, &id).unwrap().completed);
    assert!(jar.is_file(), "failed installation retains its installer");
    fs::remove_file(game.join("fail-processor")).unwrap();
    launcher.install(&instance.id, &token, &sink).await.unwrap();
    let installer_object = launcher
        .root()
        .join("cache/objects/sha1")
        .join(&installer_hash[..2])
        .join(&installer_hash);
    assert!(!jar.exists());
    assert!(!installer_object.exists());
    assert!(read_record(&game, &id).unwrap().completed);
    assert_eq!(
        version::resolve(launcher.root(), &game, &id)
            .unwrap()
            .minecraft_version,
        "1.20.1"
    );
    let runs = fs::read_to_string(game.join("processor-runs.txt")).unwrap();
    // A later install/repair needs the installer again. Supply the fixture as a
    // freshly available download object, never reach the public Forge service.
    fs::create_dir_all(installer_object.parent().unwrap()).unwrap();
    fs::write(&installer_object, &installer_bytes).unwrap();
    launcher.install(&instance.id, &token, &sink).await.unwrap();
    assert!(!jar.exists());
    assert!(!installer_object.exists());
    assert_eq!(
        fs::read_to_string(game.join("processor-runs.txt")).unwrap(),
        runs,
        "completed processors must not run again"
    );
    fs::write(game.join("libraries").join(generated_path), b"corrupted").unwrap();
    fs::write(&installer_object, &installer_bytes).unwrap();
    launcher.repair(&instance.id, &token, &sink).await.unwrap();
    assert!(!jar.exists());
    assert!(!installer_object.exists());
    assert_eq!(
        fs::read(game.join("libraries").join(generated_path)).unwrap(),
        b"generated"
    );
    assert_eq!(
        fs::read_to_string(game.join("processor-runs.txt")).unwrap(),
        format!("{runs}run\n")
    );
}
