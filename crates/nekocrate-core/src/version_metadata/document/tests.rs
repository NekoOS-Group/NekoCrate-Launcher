use super::*;
use serde_json::json;

#[test]
fn flattened_pack_profiles_do_not_repeat_game_and_loader_as_components() {
    for (id, game, forge) in [
        ("逆转未来1.1.4", "1.20.1", "47.4.4"),
        ("RLCraft", "1.12.2", "14.23.5.2860"),
    ] {
        let mut value = serde_json::to_value(crate::test_http::version_fixture(id)).unwrap();
        value["patches"] = json!([
            {"id":"game", "version":game}, {"id":"forge", "version":forge}
        ]);
        value["libraries"] = json!([{"name":format!("net.minecraftforge:forge:{game}-{forge}")}]);
        let mut document = empty(format!("{id}.json").into(), id.into(), "版本配置");
        parse_profile(&mut document, &value);
        assert!(
            document
                .fields
                .iter()
                .any(|f| f.label == "游戏版本" && f.value == game)
        );
        assert!(
            document
                .fields
                .iter()
                .any(|f| f.label == "Mod 加载器" && f.value == format!("Forge {forge}"))
        );
        assert!(!document.sections.iter().any(|s| s.label == "版本组件"));
    }
}

#[test]
fn profile_exposes_declared_java_entrypoint_and_libraries_without_calling_them_mods() {
    let mut document = empty("child.json".into(), "child".into(), "版本配置");
    parse_profile(
        &mut document,
        &json!({
            "id":"child", "inheritsFrom":"1.20.1", "javaVersion":{"majorVersion":17},
            "mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
            "libraries":[{"name":"net.fabricmc:fabric-loader:0.16.9"}, {"name":"org.ow2.asm:asm:9.7"}]
        }),
    );
    for (label, value) in [
        ("Java 版本", "17"),
        ("Mod 加载器", "Fabric 0.16.9"),
        (
            "启动入口",
            "net.fabricmc.loader.impl.launch.knot.KnotClient",
        ),
    ] {
        assert!(
            document
                .fields
                .iter()
                .any(|f| f.label == label && f.value == value)
        );
    }
    assert_eq!(document.sections.len(), 1);
    assert_eq!(document.sections[0].label, "运行依赖");
    assert_eq!(document.sections[0].entries.len(), 2);
}

#[test]
fn parses_pack_mods_separately_from_configs_and_curseforge_id_references() {
    let mut mr = empty("modrinth.index.json".into(), String::new(), "整合包清单");
    parse_pack(
        &mut mr,
        &json!({"game":"minecraft", "dependencies":{"minecraft":"1.20.1", "forge":"47.4.4"}, "files":[
            {"path":"mods/alexscaves-2.0.2.jar"}, {"path":"config/example.toml"},
            {"path":"resourcepacks/style.zip"}
        ]}),
    );
    assert_eq!(mr.kind, "Modrinth");
    assert_eq!(mr.fields[0].label, "清单格式");
    assert_eq!(mr.fields[0].value, "Modrinth");
    assert!(
        mr.fields
            .iter()
            .any(|f| f.label == "游戏版本" && f.value == "1.20.1")
    );
    assert_eq!(mr.sections[0].label, "声明的 Mods");
    assert_eq!(mr.sections[0].target, Some("mods"));
    assert_eq!(mr.sections[0].entries.len(), 1);
    assert_eq!(mr.sections[0].entries[0].name, "alexscaves-2.0.2.jar");
    assert_eq!(mr.sections[1].label, "声明的资源包");
    assert_eq!(mr.sections[1].target, Some("resourcepacks"));
    assert_eq!(mr.sections[1].entries.len(), 1);
    assert_eq!(mr.sections[2].label, "其他文件");
    assert_eq!(mr.sections[2].target, None);
    assert_eq!(mr.sections[2].entries.len(), 1);
    let mut cf = empty("manifest.json".into(), String::new(), "整合包清单");
    parse_pack(
        &mut cf,
        &json!({"manifestType":"minecraftModpack", "files":[
            {"projectID":284754,"fileID":2725062},
            {"projectID":427597,"fileID":3136255,"fileName":"dynamictrees.jar"}
        ]}),
    );
    assert_eq!(cf.sections[0].entries[0].name, "CurseForge #284754");
    assert_eq!(cf.fields[0].value, "CurseForge");
    assert_eq!(cf.sections[0].label, "待解析文件");
    assert_eq!(cf.sections[0].target, Some("unresolved"));
    assert_eq!(cf.sections[0].entries[1].name, "dynamictrees.jar");
}

#[test]
fn hmcl_records_identify_the_embedded_manifest_format() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("modpack.cfg");
    for (format, manifest) in [
        (
            "Modrinth",
            json!({"game":"minecraft", "dependencies":{"minecraft":"1.20.1"}, "files":[]}),
        ),
        (
            "CurseForge",
            json!({"manifestType":"minecraftModpack", "files":[]}),
        ),
    ] {
        fs::write(&path, json!({"manifest":manifest}).to_string()).unwrap();
        let document = read_pack(path.clone());
        assert_eq!(document.kind, format!("HMCL · {format}"));
        assert_eq!(document.fields[0].label, "清单格式");
        assert_eq!(document.fields[0].value, format);
        assert!(document.sections.is_empty());
    }
}

#[test]
fn malformed_or_oversized_files_are_bounded_and_do_not_hide_other_sources() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("modpack.cfg");
    fs::write(&path, b"{broken").unwrap();
    let document = read_pack(path.clone());
    assert_eq!(document.json.as_deref(), Some("{broken"));
    assert!(document.issue.is_some());
    fs::write(&path, b"[]").unwrap();
    assert!(read_pack(path.clone()).issue.is_some());
    fs::File::create(&path)
        .unwrap()
        .set_len(8 * 1024 * 1024 + 1)
        .unwrap();
    let document = read_pack(path);
    assert!(document.issue.as_deref().unwrap().contains("8 MiB"));
    assert!(document.json.is_none());
}
