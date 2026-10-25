use super::*;
use crate::version_management::ContentKind;

#[tokio::test]
async fn unresolved_curseforge_files_are_visible_and_resolution_survives_interruption() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
    let instance = launcher.create_instance("Games", "").unwrap();
    let game = instance.game_dir(launcher.root());
    let path = temp.path().join("atm.zip");
    let manifest = json!({
        "manifestType":"minecraftModpack", "manifestVersion":1,
        "name":"ATM9", "version":"1",
        "minecraft":{"version":"1.20.1","modLoaders":[{"id":"forge-47.4.4","primary":true}]},
        "files":(0..435).map(|id| json!({"projectID":1000+id,"fileID":2000+id,"required":true})).collect::<Vec<_>>(),
        "overrides":"overrides"
    });
    archive(&path, "manifest.json", &manifest, &[]);
    launcher
        .prepare_modpack(&instance.id, &path, "ATM9", &[])
        .await
        .unwrap();
    let counts = launcher
        .version_content_counts(&instance.id, "ATM9")
        .unwrap();
    assert_eq!(
        (counts.mods, counts.resourcepacks, counts.unresolved),
        (0, 0, 435)
    );
    let unresolved = launcher
        .version_content(&instance.id, "ATM9", ContentKind::Unresolved)
        .unwrap();
    assert_eq!(unresolved.files.len(), 435);
    let first = &unresolved.files[0];
    assert_eq!(first.declaration.as_ref().unwrap().status, "unresolved");
    assert!(
        launcher
            .version_content_metadata(
                &instance.id,
                "ATM9",
                ContentKind::Unresolved,
                &first.file_name
            )
            .is_err()
    );
    assert!(
        launcher
            .version_content_path(
                &instance.id,
                "ATM9",
                ContentKind::Unresolved,
                &first.file_name,
                None
            )
            .is_err()
    );
    let metadata = launcher.version_metadata(&instance.id, "ATM9").unwrap();
    let section = &metadata
        .documents
        .iter()
        .find(|d| d.name == "manifest.json")
        .unwrap()
        .sections[0];
    assert_eq!(section.target, Some("unresolved"));
    assert_eq!(section.entries.len(), 435);

    // The resolver checkpoints every successful response before requesting the next.
    // Emulate interruption after a Mod and a resource pack have been resolved.
    let mut job = launcher.pack_job(&instance.id, "ATM9").unwrap();
    job.pack.files[0].path = Some("mods/example.jar".into());
    job.pack.files[0].display_name = Some("Example Mod".into());
    job.pack.files[0].size = 8;
    job.pack.files[0].sha1 = "a".repeat(40);
    job.pack.files[0].urls = vec!["https://example.org/mod.jar".into()];
    job.pack.files[1].path = Some("resourcepacks/style.zip".into());
    job.pack.files[1].display_name = Some("Style".into());
    job.pack.files[1].size = 10;
    job.pack.files[1].sha1 = "b".repeat(40);
    // An author-disabled distribution remains visible with a manual-download status.
    record::write_job(&game, "ATM9", &job).unwrap();
    let counts = launcher
        .version_content_counts(&instance.id, "ATM9")
        .unwrap();
    assert_eq!(
        (counts.mods, counts.resourcepacks, counts.unresolved),
        (1, 1, 433)
    );
    let index = launcher
        .version_content_index(&instance.id, "ATM9")
        .unwrap();
    assert!(
        index.pending.is_empty(),
        "No JAR metadata reads before downloading"
    );
    assert_eq!(index.content.files[0].name, "Example Mod");
    assert_eq!(
        index.content.files[0].declaration.as_ref().unwrap().status,
        "pending"
    );
    assert_eq!(
        launcher
            .version_content(&instance.id, "ATM9", ContentKind::Resourcepacks)
            .unwrap()
            .files[0]
            .declaration
            .as_ref()
            .unwrap()
            .status,
        "manual"
    );

    let version = game.join("versions/ATM9");
    let cached = version
        .join(".nekocrate/downloads/sha1/aa")
        .join(format!("{}.part", "a".repeat(40)));
    fs::create_dir_all(cached.parent().unwrap()).unwrap();
    fs::write(&cached, b"half").unwrap();
    let partial = launcher
        .version_content_index(&instance.id, "ATM9")
        .unwrap();
    let declared = partial.content.files[0].declaration.as_ref().unwrap();
    assert_eq!(
        (declared.status, declared.downloaded, declared.total),
        ("partial", 4, 8)
    );
    assert!(partial.pending.is_empty());

    fs::write(version.join("mods/example.jar"), b"complete").unwrap();
    let downloaded = launcher
        .version_content_index(&instance.id, "ATM9")
        .unwrap();
    assert_eq!(
        downloaded.content.files.len(),
        1,
        "Downloaded rows replace declarations"
    );
    assert_eq!(
        downloaded.content.files[0]
            .declaration
            .as_ref()
            .unwrap()
            .status,
        "local"
    );
    assert_eq!(downloaded.pending, ["example.jar"]);
    let summary = launcher
        .version_content_summaries(&instance.id, "ATM9", &downloaded.pending)
        .unwrap();
    assert_eq!(summary[0].declaration.as_ref().unwrap().status, "local");
    fs::rename(
        version.join("mods/example.jar"),
        version.join("mods/example.jar.disabled"),
    )
    .unwrap();
    let disabled = launcher
        .version_content(&instance.id, "ATM9", ContentKind::Mods)
        .unwrap();
    assert_eq!(disabled.files.len(), 1);
    assert!(!disabled.files[0].enabled);
    assert_eq!(
        disabled.files[0].declaration.as_ref().unwrap().status,
        "local"
    );

    let metadata = launcher.version_metadata(&instance.id, "ATM9").unwrap();
    let document = metadata
        .documents
        .iter()
        .find(|d| d.name == "manifest.json")
        .unwrap();
    assert_eq!(
        document
            .sections
            .iter()
            .map(|s| (s.target, s.entries.len()))
            .collect::<Vec<_>>(),
        [
            (Some("mods"), 1),
            (Some("resourcepacks"), 1),
            (Some("unresolved"), 433)
        ]
    );
    assert_eq!(
        serde_json::from_str::<Value>(document.json.as_ref().unwrap()).unwrap(),
        manifest
    );
    drop(launcher);
    let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
    let counts = launcher
        .version_content_counts(&instance.id, "ATM9")
        .unwrap();
    assert_eq!(
        (counts.mods, counts.resourcepacks, counts.unresolved),
        (1, 1, 433)
    );
}

#[tokio::test]
async fn modrinth_pending_files_are_categorized_without_network_or_local_archives() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
    let instance = launcher.create_instance("Games", "").unwrap();
    let path = temp.path().join("pack.mrpack");
    archive(
        &path,
        "modrinth.index.json",
        &mr(vec![
            pack_file("mods/example.jar", "required"),
            pack_file("resourcepacks/style.zip", "required"),
            pack_file("datapacks/rules.zip", "required"),
            pack_file("mods/optional.jar", "optional"),
            pack_file("mods/server.jar", "unsupported"),
        ]),
        &[],
    );
    launcher
        .prepare_modpack(&instance.id, &path, "Pack", &[])
        .await
        .unwrap();
    let counts = launcher
        .version_content_counts(&instance.id, "Pack")
        .unwrap();
    assert_eq!(
        (
            counts.mods,
            counts.resourcepacks,
            counts.datapacks,
            counts.unresolved
        ),
        (2, 1, 1, 0)
    );
    for (kind, name) in [
        (ContentKind::Mods, "example.jar"),
        (ContentKind::Resourcepacks, "style.zip"),
        (ContentKind::Datapacks, "datapacks/rules.zip"),
    ] {
        let list = launcher
            .version_content(&instance.id, "Pack", kind)
            .unwrap();
        let file = list.files.iter().find(|f| f.file_name == name).unwrap();
        assert_eq!(file.declaration.as_ref().unwrap().status, "pending");
        assert!(
            launcher
                .version_content_path(&instance.id, "Pack", kind, name, None)
                .is_err()
        );
    }
    let mods = launcher
        .version_content_index(&instance.id, "Pack")
        .unwrap();
    assert!(mods.pending.is_empty());
    assert_eq!(
        mods.content.files[1].declaration.as_ref().unwrap().status,
        "skipped"
    );
}
