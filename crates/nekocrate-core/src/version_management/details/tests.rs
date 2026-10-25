use super::*;
use serde_json::json;
use std::io::Write;

fn fixture() -> (tempfile::TempDir, Launcher, String, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
    let instance = launcher.create_instance("Game", "pack").unwrap();
    let game = launcher.game_dir(&instance.id).unwrap();
    storage::write_json(
        &crate::version::metadata_path(&game, "pack").unwrap(),
        &crate::test_http::version_fixture("pack"),
    )
    .unwrap();
    (temp, launcher, instance.id, game)
}
fn archive(path: &Path, entries: &[(&str, &str)]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut archive = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, text) in entries {
        archive
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(text.as_bytes()).unwrap();
    }
    archive.finish().unwrap();
}
fn value<'a>(source: &'a ContentSource, label: &str) -> &'a str {
    &source
        .fields
        .iter()
        .find(|f| f.label == label)
        .unwrap()
        .value
}

#[test]
fn mod_details_read_descriptions_authors_licenses_and_per_mod_dependencies() {
    let (_temp, launcher, id, game) = fixture();
    let path = game.join("mods/cross-loader.jar.disabled");
    archive(
        &path,
        &[
            (
                "fabric.mod.json",
                r#"{"id":"example","name":"Example","version":"1.2","description":"Two lines\nLocal description","authors":["Cat",{"name":"Neko"}],"license":["MIT"],"depends":{"minecraft":"~1.20.1","java":">=17"},"suggests":{"optional":"*"}}"#,
            ),
            (
                "META-INF/neoforge.mods.toml",
                "license='MIT'\nmodLoader='javafml'\nloaderVersion='[1,)'\n[[mods]]\nmodId='first'\ndisplayName='First'\nversion='2'\nauthors='A'\ndescription='First description'\n[[mods]]\nmodId='second'\ndisplayName='Second'\nversion='3'\n[[dependencies.second]]\nmodId='other'\nversionRange='[4,)'\ntype='optional'\n",
            ),
        ],
    );
    let before = fs::read(&path).unwrap();
    let result = launcher
        .version_content_metadata(&id, "pack", ContentKind::Mods, "cross-loader.jar.disabled")
        .unwrap();
    assert_eq!(result.sources.len(), 3);
    assert_eq!(
        result.sources[0].description.as_deref(),
        Some("Two lines\nLocal description")
    );
    assert_eq!(value(&result.sources[0], "作者"), "Cat, Neko");
    assert_eq!(value(&result.sources[0], "许可证"), "MIT");
    assert!(
        result.sources[0]
            .dependencies
            .iter()
            .any(|d| d.id == "java" && d.requirement == ">=17")
    );
    assert!(result.sources[1].dependencies.is_empty());
    assert_eq!(result.sources[2].dependencies[0].id, "other");
    assert_eq!(result.sources[2].dependencies[0].relation, "可选");
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn loader_constraints_merge_without_conflating_language_and_platform_versions() {
    for (platform, loader_range, platform_range, expected) in [
        ("forge", "[47,)", "[47,)", "Forge / javafml [47,)"),
        ("forge", "[47,)", "[47.2,)", "Forge [47.2,) · javafml [47,)"),
        (
            "neoforge",
            "[1,)",
            "[20.4,)",
            "NeoForge [20.4,) · javafml [1,)",
        ),
    ] {
        let parsed = parse_mod(
            "META-INF/mods.toml",
            &json!({
                "modLoader":"javafml", "loaderVersion":loader_range,
                "mods":[{"modId":"cat"}],
                "dependencies":{"cat":[
                    {"modId":platform,"versionRange":platform_range,"mandatory":true},
                    {"modId":"minecraft","versionRange":"[1.20.1]","mandatory":true},
                    {"modId":"optional","versionRange":"[2,)","mandatory":false}
                ]}
            }),
        )
        .unwrap();
        let source = &parsed[0];
        assert_eq!(value(source, "加载器"), expected);
        assert!(!source.fields.iter().any(|f| f.label == "加载器版本范围"));
        assert_eq!(value(source, "适用游戏版本"), "[1.20.1]");
        assert_eq!(source.dependencies.len(), 1);
        assert_eq!(source.dependencies[0].relation, "可选");
    }
    for relation in ["optional", "incompatible", "discouraged"] {
        let parsed = parse_mod(
            "META-INF/neoforge.mods.toml",
            &json!({
                "modLoader":"javafml", "loaderVersion":"[1,)",
                "mods":[{"modId":"cat"}],
                "dependencies":{"cat":[{"modId":"forge","versionRange":"[47,)","type":relation}]}
            }),
        )
        .unwrap();
        assert_eq!(value(&parsed[0], "加载器"), "javafml [1,)");
        assert_eq!(parsed[0].dependencies.len(), 1);
        assert_ne!(parsed[0].dependencies[0].relation, "必需");
    }
}

#[test]
fn content_folder_resolves_listed_files_without_parsing_and_checks_their_scope() {
    let (_temp, launcher, id, game) = fixture();
    fs::create_dir_all(game.join("mods")).unwrap();
    // Even a corrupt/disabled JAR can be located without parsing its metadata.
    fs::write(game.join("mods/cat.jar.disabled"), b"not a zip").unwrap();
    assert_eq!(
        launcher
            .version_content_path(&id, "pack", ContentKind::Mods, "cat.jar.disabled", None)
            .unwrap(),
        game.join("mods/cat.jar.disabled").canonicalize().unwrap()
    );
    let world = game.join("saves/世界");
    fs::create_dir_all(&world).unwrap();
    fs::write(world.join("level.dat"), b"world marker").unwrap();
    storage::write_json(&world.join("datapacks/猫/pack.mcmeta"), &json!({"pack":{}})).unwrap();
    assert_eq!(
        launcher
            .version_content_path(
                &id,
                "pack",
                ContentKind::Datapacks,
                "saves/世界/datapacks/猫",
                None
            )
            .unwrap(),
        world.join("datapacks/猫").canonicalize().unwrap()
    );
    for path in [
        "../cat.jar.disabled",
        "/tmp/other.jar",
        "C:/other.jar",
        "missing.jar",
    ] {
        assert!(
            launcher
                .version_content_path(&id, "pack", ContentKind::Mods, path, None)
                .is_err()
        );
    }
    assert!(
        launcher
            .version_content_path(
                &id,
                "untracked",
                ContentKind::Mods,
                "cat.jar.disabled",
                None
            )
            .is_err()
    );
    #[cfg(unix)]
    {
        let outside = game.parent().unwrap().join("outside.jar");
        fs::write(&outside, b"outside").unwrap();
        std::os::unix::fs::symlink(&outside, game.join("mods/escape.jar")).unwrap();
        assert!(
            launcher
                .version_content_path(&id, "pack", ContentKind::Mods, "escape.jar", None)
                .is_err()
        );
    }
}

#[test]
fn legacy_and_quilt_metadata_keep_their_own_descriptions() {
    let legacy = parse_mod("mcmod.info", &json!({"modList":[{"modid":"old", "name":"Old", "description":"Legacy description", "authorList":["Old Cat"], "mcversion":"1.12.2", "dependencies":["required-after:forge@[14,)"]}]})).unwrap();
    assert_eq!(legacy[0].description.as_deref(), Some("Legacy description"));
    assert_eq!(value(&legacy[0], "作者"), "Old Cat");
    assert_eq!(value(&legacy[0], "适用游戏版本"), "1.12.2");
    let quilt = parse_mod("quilt.mod.json", &json!({"quilt_loader":{"id":"quiltmod","version":"2", "metadata":{"name":"Quilt Mod", "description":"Quilt description", "contributors":{"Cat":"Author"}},"depends":[{"id":"minecraft","versions":">=1.20"}]}})).unwrap();
    assert_eq!(quilt[0].title.as_deref(), Some("Quilt Mod"));
    assert_eq!(quilt[0].description.as_deref(), Some("Quilt description"));
    assert_eq!(value(&quilt[0], "作者"), "Cat");
    assert_eq!(value(&quilt[0], "适用游戏版本"), ">=1.20");
    assert!(quilt[0].dependencies.is_empty());
}

#[test]
fn zipped_resource_packs_and_world_datapacks_expose_rich_descriptions_and_format_ranges() {
    let (_temp, launcher, id, game) = fixture();
    archive(
        &game.join("resourcepacks/Cat.zip"),
        &[(
            "pack.mcmeta",
            r#"{"pack":{"description":{"text":"§aCat ","extra":[{"text":"<b>plain text</b>","clickEvent":{"action":"run_command","value":"ignored"}}]},"min_format":[65,0],"max_format":[65,2]}}"#,
        )],
    );
    let pack = launcher
        .version_content_metadata(&id, "pack", ContentKind::Resourcepacks, "Cat.zip")
        .unwrap();
    assert_eq!(
        pack.sources[0].description.as_deref(),
        Some("Cat <b>plain text</b>")
    );
    assert_eq!(value(&pack.sources[0], "最低格式"), "[65,0]");
    let world = game.join("saves/世界");
    fs::create_dir_all(&world).unwrap();
    fs::write(world.join("level.dat"), b"world marker").unwrap();
    storage::write_json(
        &world.join("datapacks/Recipes/pack.mcmeta"),
        &json!({"pack":{"pack_format":15,"description":"Recipes"}}),
    )
    .unwrap();
    let data = launcher
        .version_content_metadata(
            &id,
            "pack",
            ContentKind::Datapacks,
            "saves/世界/datapacks/Recipes",
        )
        .unwrap();
    assert_eq!(data.sources[0].description.as_deref(), Some("Recipes"));
    assert_eq!(data.size, None);
    assert_eq!(value(&data.sources[0], "包格式"), "15");
}

fn level_bytes() -> Vec<u8> {
    use fastnbt::Value::{Byte, Compound, Int, Long, String as Str};
    fastnbt::to_bytes(&Compound(std::collections::HashMap::from([(
        "Data".into(),
        Compound(std::collections::HashMap::from([
            ("LevelName".into(), Str("猫咪世界".into())),
            (
                "Version".into(),
                Compound(std::collections::HashMap::from([(
                    "Name".into(),
                    Str("1.20.1".into()),
                )])),
            ),
            ("GameType".into(), Int(1)),
            ("Difficulty".into(), Byte(2)),
            ("hardcore".into(), Byte(0)),
            ("allowCommands".into(), Byte(1)),
            ("LastPlayed".into(), Long(1730000000000)),
            (
                "WorldGenSettings".into(),
                Compound(std::collections::HashMap::from([(
                    "seed".into(),
                    Long(9223372036854775806),
                )])),
            ),
            ("SpawnX".into(), Int(1)),
            ("SpawnY".into(), Int(64)),
            ("SpawnZ".into(), Int(-20)),
        ])),
    )])))
    .unwrap()
}
fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn world_details_read_only_level_metadata_and_fall_back_to_backup_without_rewriting() {
    let (_temp, launcher, id, game) = fixture();
    let world = game.join("saves/World");
    fs::create_dir_all(world.join("region")).unwrap();
    fs::write(world.join("region/unread.bin"), b"not NBT").unwrap();
    let compressed = gzip(&level_bytes());
    fs::write(world.join("level.dat"), &compressed).unwrap();
    let result = launcher
        .version_content_metadata(&id, "pack", ContentKind::Saves, "World")
        .unwrap();
    let source = &result.sources[0];
    assert_eq!(source.title.as_deref(), Some("猫咪世界"));
    assert_eq!(value(source, "世界种子"), "9223372036854775806");
    assert_eq!(value(source, "游戏模式"), "创造");
    assert_eq!(value(source, "出生点"), "1, 64, -20");
    assert_eq!(
        source
            .fields
            .iter()
            .find(|f| f.label == "最后游玩")
            .unwrap()
            .format,
        Some("timestamp")
    );
    assert_eq!(fs::read(world.join("level.dat")).unwrap(), compressed);
    fs::write(world.join("level.dat"), b"broken").unwrap();
    fs::write(world.join("level.dat_old"), &compressed).unwrap();
    let result = launcher
        .version_content_metadata(&id, "pack", ContentKind::Saves, "World")
        .unwrap();
    assert!(result.sources[0].issue.is_some());
    assert_eq!(result.sources[1].source, "level.dat_old");
    assert_eq!(result.sources[1].title.as_deref(), Some("猫咪世界"));
    assert_eq!(fs::read(world.join("level.dat")).unwrap(), b"broken");
}

#[test]
fn inspection_rejects_unlisted_paths_and_limits_broken_archive_members() {
    let (_temp, launcher, id, game) = fixture();
    archive(
        &game.join("mods/broken.jar"),
        &[
            ("fabric.mod.json", "broken"),
            (
                "META-INF/mods.toml",
                "[[mods]]\nmodId='healthy'\ndescription='Still readable'\n",
            ),
        ],
    );
    let result = launcher
        .version_content_metadata(&id, "pack", ContentKind::Mods, "broken.jar")
        .unwrap();
    assert!(result.sources[0].issue.is_some());
    assert_eq!(
        result.sources[1].description.as_deref(),
        Some("Still readable")
    );
    for path in [
        "../broken.jar",
        "/tmp/other.jar",
        "C:/other.jar",
        "missing.jar",
    ] {
        assert!(
            launcher
                .version_content_metadata(&id, "pack", ContentKind::Mods, path)
                .is_err()
        );
    }
    assert!(
        launcher
            .version_content_metadata(&id, "untracked", ContentKind::Mods, "broken.jar")
            .is_err()
    );
    archive(
        &game.join("resourcepacks/huge.zip"),
        &[("pack.mcmeta", &" ".repeat(TEXT_LIMIT as usize + 1))],
    );
    let result = launcher
        .version_content_metadata(&id, "pack", ContentKind::Resourcepacks, "huge.zip")
        .unwrap();
    assert_eq!(result.sources[0].issue.as_deref(), Some("元数据内容过大"));
    #[cfg(unix)]
    {
        let outside = game.parent().unwrap().join("outside.jar");
        archive(&outside, &[("fabric.mod.json", "{}")]);
        std::os::unix::fs::symlink(&outside, game.join("mods/escape.jar")).unwrap();
        assert!(
            launcher
                .version_content_metadata(&id, "pack", ContentKind::Mods, "escape.jar")
                .is_err()
        );
    }
}

#[test]
fn game_requirements_are_fields_and_legacy_dependencies_keep_clickable_mod_ids() {
    let fabric = parse_mod(
        "fabric.mod.json",
        &json!({
            "id":"cat", "depends":{"minecraft":["~1.20", "~1.21"], "cats":">=2"},
            "breaks":{"minecraft":"1.20.2"}
        }),
    )
    .unwrap();
    assert_eq!(value(&fabric[0], "适用游戏版本"), "~1.20 | ~1.21");
    assert!(fabric[0].dependencies.iter().any(|d| d.id == "cats"));
    assert!(
        fabric[0]
            .dependencies
            .iter()
            .any(|d| d.id == "minecraft" && d.relation == "冲突")
    );
    let legacy = parse_mod(
        "mcmod.info",
        &json!([{
            "modid":"old", "mcversion":"1.12", "requiredMods":["cats@[2,)"],
            "dependencies":["required-after:minecraft@[1.12.2]", "after:fish@[1,)"]
        }]),
    )
    .unwrap();
    assert_eq!(value(&legacy[0], "适用游戏版本"), "[1.12.2]");
    assert_eq!(legacy[0].dependencies[0].id, "fish");
    assert_eq!(legacy[0].dependencies[0].requirement, "[1,)");
    assert_eq!(legacy[0].dependencies[1].id, "cats");
    assert_eq!(legacy[0].dependencies[1].relation, "必需");
}

#[test]
fn content_list_indexes_all_declared_mods_and_aliases_in_one_read() {
    let (_temp, launcher, id, game) = fixture();
    archive(
        &game.join("mods/cats.jar"),
        &[
            (
                "fabric.mod.json",
                r#"{"id":"cat","name":"Cats","version":"2","provides":["kitten"]}"#,
            ),
            (
                "META-INF/mods.toml",
                "[[mods]]\nmodId='catforge'\ndisplayName='Forge Cats'\n[[mods]]\nmodId='fish'\n",
            ),
        ],
    );
    archive(
        &game.join("mods/broken-first.jar.disabled"),
        &[
            ("fabric.mod.json", "broken"),
            (
                "mcmod.info",
                r#"[{"modid":"oldcat","name":"Old Cat"},{"modid":"oldfish"}]"#,
            ),
        ],
    );
    archive(
        &game.join("mods/quilt.jar"),
        &[(
            "quilt.mod.json",
            r#"{"quilt_loader":{"id":"qcat","metadata":{"name":"Quilt Cat"},"provides":[{"id":"qalias"}]}}"#,
        )],
    );
    let list = launcher
        .version_content(&id, "pack", ContentKind::Mods)
        .unwrap();
    let cats = list
        .files
        .iter()
        .find(|f| f.file_name == "cats.jar")
        .unwrap();
    assert_eq!(cats.name, "Cats");
    assert_eq!(cats.version.as_deref(), Some("2"));
    assert_eq!(cats.mod_ids, ["cat", "kitten", "catforge", "fish"]);
    let old = list
        .files
        .iter()
        .find(|f| f.file_name == "broken-first.jar.disabled")
        .unwrap();
    assert!(!old.enabled);
    assert_eq!(old.mod_ids, ["oldcat", "oldfish"]);
    assert_eq!(
        list.files
            .iter()
            .find(|f| f.file_name == "quilt.jar")
            .unwrap()
            .mod_ids,
        ["qcat", "qalias"]
    );
}

#[test]
fn raw_metadata_reads_original_text_without_extraction_and_only_reveals_real_files() {
    let (_temp, launcher, id, game) = fixture();
    let text = "[\n  {\"modid\": \"cat\", \"name\": \"Cat\"}\n]";
    archive(&game.join("mods/cat.jar"), &[("mcmod.info", text)]);
    let metadata = launcher
        .version_content_metadata(&id, "pack", ContentKind::Mods, "cat.jar")
        .unwrap();
    assert!(!metadata.sources[0].revealable);
    assert_eq!(
        launcher
            .version_content_source(&id, "pack", ContentKind::Mods, "cat.jar", "mcmod.info")
            .unwrap(),
        text
    );
    assert!(
        launcher
            .version_content_path(
                &id,
                "pack",
                ContentKind::Mods,
                "cat.jar",
                Some("mcmod.info")
            )
            .is_err()
    );
    assert_eq!(
        fs::read_dir(game.join("mods")).unwrap().count(),
        1,
        "no extraction"
    );
    for source in [
        "../mcmod.info",
        "assets/secret.txt",
        "/etc/hosts",
        "META-INF/MANIFEST.MF",
    ] {
        assert!(
            launcher
                .version_content_source(&id, "pack", ContentKind::Mods, "cat.jar", source)
                .is_err()
        );
    }
    assert!(
        launcher
            .version_content_source(&id, "untracked", ContentKind::Mods, "cat.jar", "mcmod.info")
            .is_err()
    );
    storage::write_json(
        &game.join("resourcepacks/Cat/pack.mcmeta"),
        &json!({"pack":{"description":"Cat"}}),
    )
    .unwrap();
    let pack = launcher
        .version_content_metadata(&id, "pack", ContentKind::Resourcepacks, "Cat")
        .unwrap();
    assert!(pack.sources[0].revealable);
    assert_eq!(
        launcher
            .version_content_path(
                &id,
                "pack",
                ContentKind::Resourcepacks,
                "Cat",
                Some("pack.mcmeta")
            )
            .unwrap(),
        game.join("resourcepacks/Cat/pack.mcmeta")
            .canonicalize()
            .unwrap()
    );
    archive(
        &game.join("mods/huge.jar"),
        &[("mcmod.info", &" ".repeat(TEXT_LIMIT as usize + 1))],
    );
    assert!(
        launcher
            .version_content_source(&id, "pack", ContentKind::Mods, "huge.jar", "mcmod.info")
            .is_err()
    );
    let world = game.join("saves/Cat");
    fs::create_dir_all(&world).unwrap();
    fs::write(world.join("level.dat"), level_bytes()).unwrap();
    let raw = launcher
        .version_content_source(&id, "pack", ContentKind::Saves, "Cat", "level.dat")
        .unwrap();
    assert!(raw.contains("9223372036854775806"));
    #[cfg(unix)]
    {
        let outside = game.parent().unwrap().join("private.json");
        fs::write(&outside, b"private").unwrap();
        let pack = game.join("resourcepacks/Escape");
        fs::create_dir_all(&pack).unwrap();
        std::os::unix::fs::symlink(&outside, pack.join("pack.mcmeta")).unwrap();
        assert!(
            launcher
                .version_content_source(
                    &id,
                    "pack",
                    ContentKind::Resourcepacks,
                    "Escape",
                    "pack.mcmeta"
                )
                .is_err()
        );
        assert!(
            launcher
                .version_content_path(
                    &id,
                    "pack",
                    ContentKind::Resourcepacks,
                    "Escape",
                    Some("pack.mcmeta")
                )
                .is_err()
        );
    }
}
