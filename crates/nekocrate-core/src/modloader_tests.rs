use crate::{minecraft::LaunchSession, model::*, version::*, *};
use serde_json::{Value, json};

fn base(id: &str) -> Value {
    serde_json::to_value(test_http::version_fixture(id)).unwrap()
}
fn save(game: &Path, value: &Value) {
    storage::write_json(
        &metadata_path(game, value["id"].as_str().unwrap()).unwrap(),
        value,
    )
    .unwrap();
}
#[cfg(unix)]
fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

#[test]
fn inherited_profiles_merge_overrides_natives_and_client_without_child_jar() {
    let temp = tempfile::tempdir().unwrap();
    let game = temp.path();
    let mut parent = base("1.21.1");
    parent["libraries"] = json!([
        {"name":"org.ow2.asm:asm:9.6","url":"https://maven.fabricmc.net/"},
        {"name":"org.lwjgl:lwjgl:3.3.3:natives-macos-arm64", "rules":[{"action":"allow","os":{"name":"osx"}}]},
        {"name":"org.lwjgl:lwjgl:3.3.3:natives-windows", "rules":[{"action":"allow","os":{"name":"windows"}}]}
    ]);
    save(game, &parent);
    save(
        game,
        &json!({"id":"Fabric pack","inheritsFrom":"1.21.1","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
        "libraries":[{"name":"org.ow2.asm:asm:9.8","url":"https://maven.fabricmc.net/"},
            {"name":"net.fabricmc:fabric-loader:0.16.14","url":"https://maven.fabricmc.net/"}],
        "arguments":{"jvm":["-Dcustom=one value"], "game":[]}}),
    );
    let resolved = resolve(game, game, "Fabric pack").unwrap();
    assert_eq!(resolved.minecraft_version, "1.21.1");
    assert_eq!(resolved.loader.unwrap().version, "0.16.14");
    assert_eq!(resolved.version.jar.as_deref(), Some("1.21.1"));
    assert!(
        !resolved
            .version
            .libraries
            .iter()
            .any(|l| l.name == "org.ow2.asm:asm:9.6")
    );
    assert_eq!(resolved.version.libraries.len(), 4);
    assert_eq!(resolved.version.arguments.unwrap().jvm.len(), 4);
    assert_eq!(
        client_path(game, &resolve(game, game, "Fabric pack").unwrap().version).unwrap(),
        game.join("versions/1.21.1/1.21.1.jar")
    );
    assert!(!game.join("versions/Fabric pack/Fabric pack.jar").exists());
}

#[test]
fn invalid_inheritance_coordinates_and_jar_paths_fail_without_writes() {
    let temp = tempfile::tempdir().unwrap();
    let game = temp.path();
    save(game, &json!({"id":"a", "inheritsFrom":"b"}));
    save(game, &json!({"id":"b", "inheritsFrom":"a"}));
    assert!(
        resolve(game, game, "a")
            .unwrap_err()
            .to_string()
            .contains("循环")
    );
    save(game, &json!({"id":"a", "inheritsFrom":"../escape"}));
    assert!(resolve(game, game, "a").is_err());
    let mut value = base("a");
    value["jar"] = json!("../../outside");
    save(game, &value);
    assert!(resolve(game, game, "a").is_err());
    for name in [
        "group:../escape:1",
        "group:name:/tmp",
        "group:name:1@/tmp",
        "group:name",
    ] {
        assert!(maven_path(name, None).is_err());
    }
    assert_eq!(
        maven_path("org.test:library:1.2:client@zip", None).unwrap(),
        "org/test/library/1.2/library-1.2-client.zip"
    );
}

#[tokio::test]
async fn fabric_maven_sidecars_fill_missing_checksums_and_invalid_responses_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let downloader = download::Downloader::new(
        temp.path(),
        Settings {
            proxy_mode: "none".into(),
            retries: 0,
            ..Settings::default()
        },
    )
    .unwrap();
    let checksum = "1234567890abcdef1234567890abcdef12345678";
    let (url, server) = test_http::serve(vec![(200, format!("{checksum}  loader.jar\n"))]).await;
    let mut profile =
        json!({"libraries":[{"name":"net.fabricmc:fabric-loader:0.16.14","url":url}]});
    fabric::prepare_libraries(&mut profile, &downloader, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        profile["libraries"][0]["downloads"]["artifact"]["sha1"],
        checksum
    );
    assert!(
        server.await.unwrap()[0]
            .starts_with("GET /net/fabricmc/fabric-loader/0.16.14/fabric-loader-0.16.14.jar.sha1 ")
    );
    // Prepared profiles are stable and can be reused without another request.
    fabric::prepare_libraries(&mut profile, &downloader, &CancellationToken::new())
        .await
        .unwrap();
    let (url, server) = test_http::serve(vec![(200, "<html>not a checksum</html>".into())]).await;
    assert!(
        downloader
            .maven_sha1(&format!("{url}/invalid.jar"), &CancellationToken::new())
            .await
            .is_err()
    );
    server.await.unwrap();
}

#[test]
fn neoforge_is_identified_from_official_launch_arguments_without_neoforge_classpath_entry() {
    // Shape produced by NeoForge CreateLauncherProfile (1.21.1): its loader's
    // own jar may be found by FML, not listed as a classpath library.
    let child = json!({"id":"Neo pack", "inheritsFrom":"1.21.1", "mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher",
        "arguments":{"game":["--fml.neoForgeVersion","21.1.172","--fml.fmlVersion","4.0.39","--fml.mcVersion","1.21.1","--launchTarget","forgeclient"],
            "jvm":["-DlibraryDirectory=${library_directory}","-p","${library_directory}/bootstrap.jar${classpath_separator}${library_directory}/modules.jar","--add-modules","ALL-MODULE-PATH"]},
        "libraries":[{"name":"net.neoforged.fancymodloader:loader:4.0.39"}]});
    let resolved = resolve_chain(&[base("1.21.1"), child]).unwrap();
    assert_eq!(
        resolved.loader,
        Some(ModLoader {
            kind: "neoforge".into(),
            version: "21.1.172".into()
        })
    );
    assert_eq!(resolved.minecraft_version, "1.21.1");
}

#[test]
fn flattened_legacy_forge_supports_empty_downloads_quotes_and_hmcl_isolation() {
    let temp = tempfile::tempdir().unwrap();
    let game = temp.path();
    let mut value = base("RLCraft");
    value["arguments"] = Value::Null;
    value["minecraftArguments"] = json!(
        "--tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker --gameDir ${game_directory} --username \"${auth_player_name}\" --custom 'two words'"
    );
    value["mainClass"] = json!("net.minecraft.launchwrapper.Launch");
    value["libraries"] =
        json!([{"name":"net.minecraftforge:forge:1.12.2-14.23.5.2860","downloads":{}}]);
    save(game, &value);
    let resolved = resolve(game, game, "RLCraft").unwrap();
    assert_eq!(resolved.minecraft_version, "1.12.2");
    assert_eq!(resolved.loader.as_ref().unwrap().version, "14.23.5.2860");
    assert!(
        resolved.version.libraries[0]
            .downloads
            .artifact
            .as_ref()
            .unwrap()
            .path
            .as_deref()
            .unwrap()
            .ends_with("forge-1.12.2-14.23.5.2860.jar")
    );
    let instance: Instance = serde_json::from_value(json!({"id":"fixture","name":"Fixture","game_directory":game,"versions":[],"selected_version":null,"memory_mb":2048,"java_path":null,"created_at":0,"last_played":null})).unwrap();
    let pack = game.join("versions/RLCraft");
    fs::create_dir_all(pack.join("mods")).unwrap();
    let mut session = LaunchSession::guest();
    session.username = "Player name; $(echo nope)".into();
    let args = minecraft::arguments(&resolved.version, game, &instance, &session).unwrap();
    assert_eq!(
        args[args.iter().position(|a| a == "--gameDir").unwrap() + 1],
        pack.to_string_lossy()
    );
    assert!(args.contains(&session.username));
    assert!(args.contains(&"two words".to_owned()));
    assert_eq!(args.iter().filter(|a| a.as_str() == "-cp").count(), 1);
    storage::write_json(
        &pack.join("hmclversion.cfg"),
        &json!({"usesGlobal":false,"gameDirType":0}),
    )
    .unwrap();
    assert_eq!(working_directory(game, &resolved.version).unwrap(), game);
}

#[cfg(unix)]
fn fake_java(path: &Path, major: u32) {
    use std::os::unix::fs::PermissionsExt;
    write(
        path,
        format!(
            r#"#!/bin/sh
if [ "$2" = "-version" ]; then
  printf '%s\n' 'openjdk version "{major}.0.1"' '    os.arch = {}' >&2
  exit 0
fi
printf '%s\n' "$@" > captured-arguments.txt
"#,
            minecraft::arch_name()
        )
        .as_bytes(),
    );
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn installed_fabric_forge_and_neoforge_launch_in_place_without_install_or_network() {
    let cases = [
        (
            "fabric",
            "0.16.14",
            "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "net.fabricmc:fabric-loader:0.16.14",
            false,
        ),
        (
            "forge",
            "14.23.5.2860",
            "net.minecraft.launchwrapper.Launch",
            "net.minecraftforge:forge:1.12.2-14.23.5.2860",
            true,
        ),
        (
            "forge",
            "47.4.4",
            "cpw.mods.bootstraplauncher.BootstrapLauncher",
            "net.minecraftforge:fmlloader:1.20.1-47.4.4",
            false,
        ),
        (
            "neoforge",
            "21.1.172",
            "cpw.mods.bootstraplauncher.BootstrapLauncher",
            "net.neoforged:neoforge:21.1.172",
            false,
        ),
    ];
    for (kind, loader_version, main, library, legacy) in cases {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("original game/.minecraft");
        let parent_id = if legacy { "1.12.2" } else { "1.21.1" };
        let mut parent = base(parent_id);
        if legacy {
            parent["arguments"] = Value::Null;
            parent["minecraftArguments"] = json!("--gameDir ${game_directory}");
        }
        save(&game, &parent);
        let child = json!({"id":"My pack","inheritsFrom":parent_id,"mainClass":main,"libraries":[{"name":library}],
            "minecraftArguments":if legacy {json!("--gameDir ${game_directory} --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker")} else {Value::Null},
            "arguments":if legacy {Value::Null} else {json!({"game":[],"jvm":["-Dprimary=${primary_jar_name}"]})}});
        save(&game, &child);
        let original = fs::read(metadata_path(&game, "My pack").unwrap()).unwrap();
        write(
            &game.join(format!("versions/{parent_id}/{parent_id}.jar")),
            b"base jar",
        );
        write(
            &game
                .join("libraries")
                .join(maven_path(library, None).unwrap()),
            b"loader jar",
        );
        let workdir = game.join("versions/My pack");
        fs::create_dir_all(workdir.join("mods")).unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let instance = launcher
            .import_instance(game.clone(), "Existing".into(), "My pack".into())
            .await
            .unwrap();
        assert!(instance.selected().unwrap().installed);
        assert_eq!(
            instance.selected().unwrap().loader,
            Some(ModLoader {
                kind: kind.into(),
                version: loader_version.into()
            })
        );
        let java = temp.path().join("Java with spaces");
        fake_java(&java, 8);
        launcher
            .update_instance(
                &instance.id,
                "Existing",
                2048,
                Some(java.to_string_lossy().into()),
                None,
            )
            .unwrap();
        let sink: EventSink = Arc::new(|_| {});
        launcher
            .launch(&instance.id, &CancellationToken::new(), sink)
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !launcher.snapshot().running.is_empty() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let captured = fs::read_to_string(workdir.join("captured-arguments.txt")).unwrap();
        assert!(captured.lines().any(|arg| arg == main));
        assert!(
            captured.contains(
                &game
                    .join(format!("versions/{parent_id}/{parent_id}.jar"))
                    .to_string_lossy()
                    .to_string()
            )
        );
        assert_eq!(
            fs::read(metadata_path(&game, "My pack").unwrap()).unwrap(),
            original
        );
        assert!(
            !launcher
                .instance_dir(&instance.id)
                .unwrap()
                .join(".minecraft")
                .exists()
        );
        assert!(!workdir.join("My pack.jar").exists());
    }
}

#[cfg(unix)]
#[tokio::test]
async fn fabric_installs_verifies_and_repairs_from_cache_without_replacing_original_profiles() {
    use sha1::{Digest, Sha1};
    let hash = |bytes: &[u8]| hex::encode(Sha1::digest(bytes));
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
    let instance = launcher.create_instance("Fabric test", "").unwrap();
    let game = launcher.game_dir(&instance.id).unwrap();
    let mut settings = launcher.settings();
    settings.proxy_mode = "none".into();
    settings.retries = 0;
    launcher.save_settings(settings).unwrap();
    let parent_id = "1.21.1";
    let loader = "0.16.14";
    let profile_id = fabric::version_id(parent_id, loader).unwrap();
    let mut parent = base(parent_id);
    let bytes = b"fixture game and loader";
    let index = br#"{"objects":{}}"#;
    let hash = hash(bytes);
    let index_hash = hex::encode(Sha1::digest(index));
    // All requests use a deterministic local server; the second install must
    // succeed after the server has stopped, even if a library is deleted.
    let (url, server) = test_http::serve(vec![
        (200, String::from_utf8(index.to_vec()).unwrap()),
        (200, String::from_utf8(bytes.to_vec()).unwrap()),
    ])
    .await;
    parent["downloads"] =
        json!({"client":{"url":format!("{url}/client.jar"),"sha1":hash,"size":bytes.len()}});
    parent["assetIndex"] = json!({"id":parent_id,"url":format!("{url}/index.json"),"sha1":index_hash,"size":index.len()});
    let body = serde_json::to_vec_pretty(&parent).unwrap();
    write(
        &launcher.root().join("metadata/versions/1.21.1.json"),
        &body,
    );
    storage::write_json(&launcher.root().join("metadata/manifest.json"),&json!({"latest":{},"versions":[{"id":parent_id,"type":"release","url":format!("{url}/must-not-fetch"),"sha1":hex::encode(Sha1::digest(&body))}]})).unwrap();
    let library = "net.fabricmc:fabric-loader:0.16.14";
    let lib_path = maven_path(library, None).unwrap();
    let profile = json!({"id":profile_id,"inheritsFrom":parent_id,"mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient","arguments":{"jvm":[],"game":[]},"libraries":[{"name":library,"downloads":{"artifact":{"path":lib_path,"url":format!("{url}/loader.jar"),"sha1":hash,"size":bytes.len()}}}]});
    storage::write_json(
        &launcher
            .root()
            .join("metadata/fabric/profiles")
            .join(format!("{profile_id}.json")),
        &profile,
    )
    .unwrap();
    let sink: EventSink = Arc::new(|_| {});
    let cancel = CancellationToken::new();
    let added = launcher
        .add_fabric_version(&instance.id, parent_id, loader, &cancel, &sink)
        .await
        .unwrap();
    assert!(!added.selected().unwrap().installed);
    let java = temp.path().join("java");
    fake_java(&java, 8);
    launcher
        .update_instance(
            &instance.id,
            "Fabric test",
            2048,
            Some(java.to_string_lossy().into()),
            None,
        )
        .unwrap();
    launcher
        .install(&instance.id, &cancel, &sink)
        .await
        .unwrap();
    assert_eq!(server.await.unwrap().len(), 2);
    assert!(
        launcher
            .instance(&instance.id)
            .unwrap()
            .selected()
            .unwrap()
            .installed
    );
    assert_eq!(
        fs::read(game.join("versions/1.21.1/1.21.1.jar")).unwrap(),
        bytes
    );
    assert!(
        !game
            .join("versions")
            .join(&profile_id)
            .join(format!("{profile_id}.jar"))
            .exists()
    );
    let persisted: Value = storage::read_json(&metadata_path(&game, &profile_id).unwrap()).unwrap();
    assert_eq!(persisted["inheritsFrom"], parent_id);
    let files = minecraft::cached_installation_files(launcher.root(), &game, &profile_id)
        .await
        .unwrap();
    assert!(
        download::local_progress(launcher.root(), files)
            .await
            .unwrap()
            > 98.0
    );
    fs::remove_file(game.join("libraries").join(&lib_path)).unwrap();
    launcher.repair(&instance.id, &cancel, &sink).await.unwrap();
    assert_eq!(
        fs::read(game.join("libraries").join(&lib_path)).unwrap(),
        bytes
    );
}

#[cfg(unix)]
#[tokio::test]
async fn crash_event_is_emitted_after_logs_but_not_for_a_user_requested_stop() {
    for (exit, expected) in [
        ("exit 0", "game_exit"),
        ("exit 7", "game_crash"),
        ("exec sleep 30", "game_exit"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let instance = launcher.create_instance("Exit fixture", "1.21.1").unwrap();
        let game = launcher.game_dir(&instance.id).unwrap();
        save(&game, &base("1.21.1"));
        write(&game.join("versions/1.21.1/1.21.1.jar"), b"fixture");
        let java = temp.path().join("java");
        fake_java(&java, 8);
        let script = fs::read_to_string(&java).unwrap();
        fs::write(
            &java,
            format!("{script}\nprintf '%s\\n' 'last diagnostic line' >&2\n{exit}\n"),
        )
        .unwrap();
        launcher
            .update_instance(
                &instance.id,
                "Exit fixture",
                2048,
                Some(java.to_string_lossy().into()),
                None,
            )
            .unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = events.clone();
        let sink: EventSink = Arc::new(move |e| captured.lock().unwrap().push(e));
        launcher
            .launch(&instance.id, &CancellationToken::new(), sink)
            .await
            .unwrap();
        if exit.starts_with("exec") {
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                while !events
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|e| e.message == "last diagnostic line")
                {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            launcher.stop_game(&instance.id).unwrap();
        }
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !events
                .lock()
                .unwrap()
                .iter()
                .any(|e| e.kind == "game_exit" || e.kind == "game_crash")
            {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let events = events.lock().unwrap();
        let event = events.last().unwrap();
        assert_eq!(event.kind, expected);
        assert!(
            events
                .iter()
                .position(|e| e.message == "last diagnostic line")
                .unwrap()
                < events.len() - 1
        );
        if exit.starts_with("exec") {
            assert_eq!(event.message, "游戏已停止");
        }
    }
}
