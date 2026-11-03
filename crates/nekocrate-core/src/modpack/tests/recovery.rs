use super::*;

#[tokio::test]
async fn misplaced_manifest_recovers_on_reopen_without_losing_partial_install() {
    for (manifest_name, manifest) in [
        (
            "modrinth.index.json",
            mr(vec![pack_file("mods/cat.jar", "required")]),
        ),
        (
            "manifest.json",
            json!({"manifestType":"minecraftModpack","manifestVersion":1,
            "name":"ATM9","version":"1.1.1","minecraft":{"version":"1.20.1",
            "modLoaders":[{"id":"forge-47.4.0","primary":true}]},
            "files":[{"projectID":123,"fileID":456,"required":true}],"overrides":"overrides"}),
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("launcher");
        let launcher = Launcher::open(Some(root.clone())).unwrap();
        let instance = launcher.create_instance("Games", "").unwrap();
        let game = instance.game_dir(&root);
        let path = temp.path().join("pack.zip");
        archive(&path, manifest_name, &manifest, &[]);
        launcher
            .prepare_modpack(&instance.id, &path, "ATM9", &[])
            .await
            .unwrap();
        let target = game.join("versions/ATM9");
        fs::rename(target.join(manifest_name), target.join("ATM9.json")).unwrap();
        fs::write(target.join("mods/already-downloaded.jar"), b"keep this mod").unwrap();
        fs::create_dir_all(target.join(".nekocrate/downloads")).unwrap();
        fs::write(
            target.join(".nekocrate/downloads/file.part"),
            b"partial bytes",
        )
        .unwrap();
        record::write_progress(&game, "ATM9", 23.5).unwrap();
        let state = fs::read(target.join(".nekocrate/modpack.json")).unwrap();
        assert!(
            launcher
                .version_content_counts(&instance.id, "ATM9")
                .is_err()
        );
        drop(launcher);
        let launcher = Launcher::open(Some(root)).unwrap();
        assert!(!target.join("ATM9.json").exists());
        assert_eq!(
            storage::read_json::<Value>(&target.join(manifest_name)).unwrap(),
            manifest
        );
        assert_eq!(
            fs::read(target.join(".nekocrate/modpack.json")).unwrap(),
            state
        );
        assert_eq!(
            fs::read(target.join("mods/already-downloaded.jar")).unwrap(),
            b"keep this mod"
        );
        assert_eq!(
            fs::read(target.join(".nekocrate/downloads/file.part")).unwrap(),
            b"partial bytes"
        );
        assert_eq!(
            record::describe(&game, "ATM9").unwrap().download_progress,
            23.5
        );
        let metadata = launcher.version_metadata(&instance.id, "ATM9").unwrap();
        assert!(metadata.issue.is_none());
        assert!(metadata.documents.iter().all(|d| d.issue.is_none()));
        assert!(
            launcher
                .version_content_counts(&instance.id, "ATM9")
                .is_ok()
        );
        assert!(!launcher.pack_job(&instance.id, "ATM9").unwrap().completed);
    }
}

#[tokio::test]
async fn recovery_never_rewrites_real_profiles_or_conflicting_manifests() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
    let instance = launcher.create_instance("Games", "").unwrap();
    let game = instance.game_dir(launcher.root());
    let path = temp.path().join("pack.zip");
    archive(&path, "modrinth.index.json", &mr(vec![]), &[]);
    launcher
        .prepare_modpack(&instance.id, &path, "Pack", &[])
        .await
        .unwrap();
    let job = launcher.pack_job(&instance.id, "Pack").unwrap();
    let target = game.join("versions/Pack");
    let profile = target.join("Pack.json");
    let wrong_id = json!({"id":"other-version","mainClass":"example.Main"});
    storage::write_json(&profile, &wrong_id).unwrap();
    assert!(!record::recover_manifest(&game, "Pack", &job).unwrap());
    assert_eq!(storage::read_json::<Value>(&profile).unwrap(), wrong_id);
    assert!(version::resolve(launcher.root(), &game, "Pack").is_err());
    storage::write_json(&profile, &job.pack.manifest).unwrap();
    storage::write_json(
        &target.join("modrinth.index.json"),
        &json!({"name":"different"}),
    )
    .unwrap();
    assert!(record::recover_manifest(&game, "Pack", &job).is_err());
    assert!(profile.is_file());
    storage::write_json(&target.join("modrinth.index.json"), &job.pack.manifest).unwrap();
    assert!(record::recover_manifest(&game, "Pack", &job).unwrap());
    assert!(!record::recover_manifest(&game, "Pack", &job).unwrap());
}
