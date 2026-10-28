//! Small launch-time compatibility overlays; never rewrite imported profiles.
use crate::{
    download::{Downloader, FileSpec},
    model::{Artifact, Event, EventSink, Version},
};
use anyhow::{Context, Result, bail};
use std::path::Path;
use tokio_util::sync::CancellationToken;

mod legacy_colors;
pub(crate) use legacy_colors::prepare as prepare_legacy_colors;

pub async fn prepare(
    game: &Path,
    version: &mut Version,
    downloader: &Downloader,
    cancel: &CancellationToken,
    sink: &EventSink,
) -> Result<()> {
    let replacements = jna_overlay(version, cfg!(target_os = "macos"));
    if replacements.is_empty() {
        return Ok(());
    }
    sink(Event::new("stage", "应用 macOS JNA 兼容修复"));
    for artifact in replacements {
        downloader
            .file(
                &FileSpec {
                    path: crate::minecraft::library_path(game, &artifact)?,
                    url: artifact.url,
                    hash: artifact.sha1,
                    sha256: false,
                    size: artifact.size,
                },
                cancel,
                sink,
            )
            .await?;
    }
    Ok(())
}

pub async fn prepare_openal(
    game: &Path,
    version: &Version,
    downloader: &Downloader,
    cancel: &CancellationToken,
    sink: &EventSink,
) -> Result<()> {
    let arch = crate::java::JavaRequirement::for_version(version).arch;
    if !needs_openal_overlay(version, cfg!(target_os = "macos"), arch) {
        return Ok(());
    }
    sink(Event::new("stage", "应用 macOS OpenAL 兼容修复"));
    // The old OpenAL Soft 1.15.1 fails AudioUnitInitialize on modern macOS,
    // falling back to Apple's implementation without EFX (AL_INVALID_ENUM).
    // Use Mojang's pinned x64 OpenAL Soft 1.23.0 binary, keeping LWJGL 2 itself.
    let path = "org/lwjgl/lwjgl-openal/3.3.2/lwjgl-openal-3.3.2-natives-macos.jar";
    let jar = game.join("libraries").join(path);
    downloader
        .file(
            &FileSpec {
                path: jar.clone(),
                url: format!("https://libraries.minecraft.net/{path}"),
                hash: "56fa16d15039142634e3a6d97d638b56679be821".into(),
                sha256: false,
                size: 515159,
            },
            cancel,
            sink,
        )
        .await?;
    let dest = game.join("versions").join(&version.id).join("natives");
    let cancel = cancel.clone();
    tokio::task::spawn_blocking(move || extract_openal(&jar, &dest, &cancel)).await?
}

fn needs_openal_overlay(version: &Version, macos: bool, arch: &str) -> bool {
    macos
        && arch == "x86_64"
        && version.libraries.iter().any(|library| {
            library.name == "org.lwjgl.lwjgl:lwjgl-platform:2.9.2-nightly-20140822"
                && crate::minecraft::is_allowed_on(library, arch)
                && library
                    .downloads
                    .classifiers
                    .get("natives-osx")
                    .is_some_and(|artifact| {
                        artifact.sha1 == "79f5ce2fea02e77fe47a3c745219167a542121d7"
                    })
        })
}

fn extract_openal(jar: &Path, dest: &Path, cancel: &CancellationToken) -> Result<()> {
    use std::io::Read;
    crate::download::check_cancel(cancel)?;
    let mut archive = zip::ZipArchive::new(std::fs::File::open(jar)?)?;
    let mut entry = archive
        .by_name("macos/x64/org/lwjgl/openal/libopenal.dylib")
        .context("OpenAL 原生库文件缺失")?;
    const MAX_SIZE: u64 = 8 * 1024 * 1024;
    if entry.is_symlink() || entry.size() > MAX_SIZE {
        bail!("OpenAL 原生库文件无效");
    }
    let mut bytes = vec![];
    entry.by_ref().take(MAX_SIZE + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_SIZE {
        bail!("OpenAL 原生库文件无效");
    }
    crate::download::check_cancel(cancel)?;
    let target = dest.join("openal.dylib");
    if std::fs::read(&target).ok().as_deref() != Some(bytes.as_slice()) {
        crate::storage::atomic_write_with_backup(
            &target,
            &bytes,
            &dest.join("openal.dylib.nekocrate.bak"),
        )?;
    }
    Ok(())
}

fn jna_overlay(version: &mut Version, macos: bool) -> Vec<Artifact> {
    if !macos {
        return vec![];
    }
    let mut replacements = vec![];
    // JNA #1452/#1456: macOS dlerror messages can exceed the old fixed buffer,
    // causing an assertion abort before Java can handle a missing-library probe.
    // Pin the pair shipped in official Minecraft 1.20.2; hashes come from its
    // Mojang profile 96dbebfb2894bd7ffbddbc8c509c185473c85922.
    for library in &mut version.libraries {
        let Some(rest) = library.name.strip_prefix("net.java.dev.jna:") else {
            continue;
        };
        let Some((name, old)) = rest.split_once(':') else {
            continue;
        };
        let Ok(old) = semver::Version::parse(old) else {
            continue;
        };
        if old < semver::Version::new(5, 8, 0) || old >= semver::Version::new(5, 13, 0) {
            continue;
        }
        let (hash, size) = match name {
            "jna" => ("1200e7ebeedbe0d10062093f32925a912020e747", 1879325),
            "jna-platform" => ("88e9a306715e9379f3122415ef4ae759a352640d", 1363209),
            _ => continue,
        };
        let path = format!("net/java/dev/jna/{name}/5.13.0/{name}-5.13.0.jar");
        let artifact = Artifact {
            url: format!("https://libraries.minecraft.net/{path}"),
            path: Some(path),
            sha1: hash.into(),
            size,
        };
        library.name = format!("net.java.dev.jna:{name}:5.13.0");
        library.downloads.artifact = Some(artifact.clone());
        replacements.push(artifact);
    }
    replacements
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn openal_overlay_is_scoped_to_affected_mac_intel_natives_and_keeps_other_files() {
        let mut value = serde_json::to_value(crate::test_http::version_fixture("1.12.2")).unwrap();
        value["libraries"] = serde_json::json!([{
            "name":"org.lwjgl.lwjgl:lwjgl-platform:2.9.2-nightly-20140822",
            "downloads":{"classifiers":{"natives-osx":{
                "path":"native.jar", "sha1":"79f5ce2fea02e77fe47a3c745219167a542121d7"
            }}}
        }]);
        let mut version = crate::version::resolve_chain(&[value]).unwrap().version;
        assert!(needs_openal_overlay(&version, true, "x86_64"));
        assert!(!needs_openal_overlay(&version, false, "x86_64"));
        assert!(!needs_openal_overlay(&version, true, "aarch64"));
        version.libraries[0]
            .downloads
            .classifiers
            .get_mut("natives-osx")
            .unwrap()
            .sha1 = "custom".into();
        assert!(!needs_openal_overlay(&version, true, "x86_64"));
        use std::io::Write;
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("native.jar");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&jar).unwrap());
        zip.start_file(
            "macos/x64/org/lwjgl/openal/libopenal.dylib",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"fixed audio").unwrap();
        zip.finish().unwrap();
        let dest = temp.path().join("natives");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("openal.dylib"), b"old audio").unwrap();
        std::fs::write(dest.join("liblwjgl.dylib"), b"original graphics").unwrap();
        let cancel = CancellationToken::new();
        extract_openal(&jar, &dest, &cancel).unwrap();
        extract_openal(&jar, &dest, &cancel).unwrap();
        assert_eq!(
            std::fs::read(dest.join("openal.dylib")).unwrap(),
            b"fixed audio"
        );
        assert_eq!(
            std::fs::read(dest.join("openal.dylib.nekocrate.bak")).unwrap(),
            b"old audio"
        );
        assert_eq!(
            std::fs::read(dest.join("liblwjgl.dylib")).unwrap(),
            b"original graphics"
        );
        cancel.cancel();
        assert!(extract_openal(&jar, &dest, &cancel).is_err());
    }
    #[test]
    fn macos_overlay_replaces_only_affected_jna_without_altering_original_metadata() {
        let mut value = serde_json::to_value(crate::test_http::version_fixture("1.20.1")).unwrap();
        value["libraries"] = serde_json::json!([
            {"name":"net.java.dev.jna:jna:5.12.1"},
            {"name":"net.java.dev.jna:jna-platform:5.12.1"},
            {"name":"org.ow2.asm:asm:9.7.1"}
        ]);
        let original = crate::version::resolve_chain(&[value]).unwrap().version;
        let mut current = original.clone();
        assert!(jna_overlay(&mut current, false).is_empty());
        assert_eq!(current.libraries[0].name, original.libraries[0].name);
        assert_eq!(jna_overlay(&mut current, true).len(), 2);
        assert_eq!(current.libraries[0].name, "net.java.dev.jna:jna:5.13.0");
        assert_eq!(original.libraries[0].name, "net.java.dev.jna:jna:5.12.1");
        assert_eq!(current.libraries[2].name, original.libraries[2].name);
        assert!(jna_overlay(&mut current, true).is_empty());
    }
}
