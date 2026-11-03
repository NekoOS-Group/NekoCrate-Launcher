use super::*;
use serde_json::json;
use std::io::Write;

fn png() -> Vec<u8> {
    let mut bytes = vec![0; 33];
    bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    bytes[12..16].copy_from_slice(b"IHDR");
    bytes[16..20].copy_from_slice(&64u32.to_be_bytes());
    bytes[20..24].copy_from_slice(&64u32.to_be_bytes());
    bytes
}
fn jar(path: &Path, entries: &[(&str, &[u8])]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut archive = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, bytes) in entries {
        archive
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(bytes).unwrap();
    }
    archive.finish().unwrap();
}
#[test]
fn logos_and_exact_project_links_work_across_mod_formats() {
    let temp = tempfile::tempdir().unwrap();
    let png = png();
    for (descriptor, text) in [
        (
            "fabric.mod.json",
            r#"{"icon":{"16":"tiny.png","64":"cat.png","128":"large.png"},"contact":{"homepage":"https://modrinth.com/mod/cat"}}"#,
        ),
        (
            "quilt.mod.json",
            r#"{"quilt_loader":{"metadata":{"icon":"cat.png","contact":{"homepage":"https://modrinth.com/mod/cat"}}}}"#,
        ),
        (
            "META-INF/mods.toml",
            "[[mods]]\nmodId='cat'\nlogoFile='cat.png'\ndisplayURL='https://modrinth.com/mod/cat'",
        ),
        (
            "META-INF/neoforge.mods.toml",
            "logoFile='cat.png'\n[[mods]]\nmodId='cat'\ndisplayURL='https://modrinth.com/mod/cat'",
        ),
        (
            "mcmod.info",
            r#"[{"modid":"cat","logoFile":"cat.png","url":"https://modrinth.com/mod/cat"}]"#,
        ),
    ] {
        let path = temp.path().join("cat.jar.disabled");
        jar(&path, &[(descriptor, text.as_bytes()), ("cat.png", &png)]);
        let before = fs::read(&path).unwrap();
        let result = from_archive(&path, ContentKind::Mods);
        assert_eq!(result.icon.unwrap(), (png.clone(), "image/png"));
        assert_eq!(result.project.unwrap().id, "cat");
        assert_eq!(fs::read(path).unwrap(), before);
    }
}
#[test]
fn rejects_unsafe_icons_and_unrelated_project_urls() {
    let temp = tempfile::tempdir().unwrap();
    for (name, bytes) in [
        ("../cat.png", png()),
        ("cat.svg", b"<svg/>".to_vec()),
        ("huge.png", vec![0; ICON_LIMIT as usize + 1]),
    ] {
        let descriptor = json!({"icon": name}).to_string();
        let path = temp.path().join("cat.jar");
        jar(
            &path,
            &[("fabric.mod.json", descriptor.as_bytes()), (name, &bytes)],
        );
        assert!(from_archive(&path, ContentKind::Mods).icon.is_none());
    }
    for url in [
        "https://modrinth.com.evil.test/mod/cat",
        "file:///mod/cat",
        "https://u:p@modrinth.com/mod/cat",
        "https://modrinth.com/mod/evil%2Fid",
        "https://curseforge.com/minecraft/modpacks/cat",
    ] {
        assert!(project_link(url).is_none());
    }
    let cf = project_link("https://www.curseforge.com/minecraft/mc-mods/cat/files/123").unwrap();
    assert_eq!(cf.platform, Platform::Curseforge);
    assert_eq!(cf.id, "cat");
}
#[test]
fn presentation_stays_inside_version_and_prefers_install_origin() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
    let instance = launcher.create_instance("Game", "pack").unwrap();
    let game = launcher.game_dir(&instance.id).unwrap();
    storage::write_json(
        &crate::version::metadata_path(&game, "pack").unwrap(),
        &crate::test_http::version_fixture("pack"),
    )
    .unwrap();
    jar(
        &game.join("mods/cat.jar"),
        &[(
            "fabric.mod.json",
            br#"{"contact":{"homepage":"https://modrinth.com/mod/cat"}}"#,
        )],
    );
    storage::write_json(&game.join(".nekocrate/resources.json"), &json!({"mods/cat.jar":{"kind":"mods","origin":{"platform":"curseforge","project":"123","version":"456","name":"Cat","dependencies":[]},"sha256":"","sha1":null,"installed_at":0}})).unwrap();
    let result = launcher
        .version_content_presentation(&instance.id, "pack", ContentKind::Mods, "cat.jar")
        .unwrap();
    let project = result.project.unwrap();
    assert_eq!(project.platform, Platform::Curseforge);
    assert_eq!(project.id, "123");
    assert!(
        launcher
            .version_content_presentation(&instance.id, "pack", ContentKind::Mods, "../outside.jar")
            .is_err()
    );
}

#[test]
fn resource_and_shader_artwork_support_archives_folders_and_recorded_origins() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
    let instance = launcher.create_instance("Game", "pack").unwrap();
    let game = launcher.game_dir(&instance.id).unwrap();
    storage::write_json(
        &crate::version::metadata_path(&game, "pack").unwrap(),
        &crate::test_http::version_fixture("pack"),
    )
    .unwrap();
    let png = png();
    for (kind, directory) in [
        (ContentKind::Resourcepacks, "resourcepacks"),
        (ContentKind::Shaderpacks, "shaderpacks"),
    ] {
        let folder = game.join(directory).join("unpacked");
        fs::create_dir_all(folder.join("shaders")).unwrap();
        fs::write(
            folder.join("pack.mcmeta"),
            br#"{"pack":{"pack_format":15,"description":"Cat"}}"#,
        )
        .unwrap();
        fs::write(folder.join("pack.png"), &png).unwrap();
        jar(
            &game.join(directory).join("packed.zip"),
            &[("pack.png", &png)],
        );
        for name in ["unpacked", "packed.zip"] {
            let result = launcher
                .version_content_presentation(&instance.id, "pack", kind, name)
                .unwrap();
            assert_eq!(result.icon.unwrap(), (png.clone(), "image/png"));
        }
        fs::remove_file(folder.join("pack.png")).unwrap();
        storage::write_json(&game.join(".nekocrate/resources.json"), &json!({format!("{directory}/unpacked"):{"kind":directory,"origin":{"platform":"modrinth","project":"cat","version":"v1","name":"Cat","dependencies":[]},"sha256":"","sha1":null,"installed_at":0}})).unwrap();
        let result = launcher
            .version_content_presentation(&instance.id, "pack", kind, "unpacked")
            .unwrap();
        assert!(result.icon.is_none());
        assert_eq!(result.project.unwrap().id, "cat");
    }
}

#[test]
fn directory_artwork_rejects_oversized_invalid_and_escaping_files() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("pack");
    fs::create_dir_all(&folder).unwrap();
    for bytes in [b"<svg/>".to_vec(), vec![0; ICON_LIMIT as usize + 1]] {
        fs::write(folder.join("pack.png"), bytes).unwrap();
        assert!(directory_icon(&folder, ContentKind::Resourcepacks).is_none());
    }
    #[cfg(unix)]
    {
        fs::remove_file(folder.join("pack.png")).unwrap();
        let outside = temp.path().join("outside.png");
        fs::write(&outside, png()).unwrap();
        std::os::unix::fs::symlink(outside, folder.join("pack.png")).unwrap();
        assert!(directory_icon(&folder, ContentKind::Shaderpacks).is_none());
    }
}
