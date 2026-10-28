//! deAWT for the official pre-1.6 vanilla entry point on Apple Silicon.
//! The profile overlay exists for one launch only; no game JAR/JSON is patched.
use crate::{
    download::{Downloader, FileSpec, check_cancel},
    model::{Event, EventSink, Version},
    version::{self, ResolvedVersion},
};
use anyhow::{Context, Result};
use std::{fs::File, path::Path};
use tokio_util::sync::CancellationToken;

// Official release asset and GitHub's SHA-256 digest, verified 2026-10-28.
// https://github.com/betacraftuk/legacyfix/releases/tag/26.10 (GPL-3.0)
const RELEASE: &str = "26.10";
const SHA256: &str = "ea89b3418b049ffcf4090605710c29a5eacd6f1fd181b74d253b5612f17b548e";
const SIZE: u64 = 1_092_490;
const WRAPPER: &str = "net.minecraft:launchwrapper:1.5";

// Keep deAWT, its mouse fix and the mandatory game-directory bridge. Disable
// unrelated default patches, especially networking and launcher-file rewriting.
// These IDs are tied to the pinned release and must be reviewed when updating it.
const DISABLED: &[&str] = &[
    "proxy",
    "launcher",
    "level-proxy",
    "authlib-patch",
    "one-six-main",
    "bitdepth",
    "disable-controllers",
    "classic-resize",
    "c0.0.15a-server-port",
    "classic-performance",
    "a1.1.1",
    "texture-pack-folder",
    "indev-sound",
    "screenshot",
    "demo-patch",
    "auth-on-server-join",
    "bouncycastle",
];

fn candidate(resolved: &ResolvedVersion, apple_silicon: bool) -> bool {
    let v = &resolved.version;
    apple_silicon
        && resolved.loader.is_none()
        && v.main_class == "net.minecraft.launchwrapper.Launch"
        && v.libraries.iter().any(|l| l.name == WRAPPER)
        && v.libraries.iter().any(|l| l.name.starts_with("org.lwjgl.lwjgl:lwjgl:"))
        && v.minecraft_arguments.as_deref().is_some_and(|args| {
            args.starts_with("${auth_player_name} ${auth_session}")
                && !args.contains("--tweakClass")
        })
        // A manually configured agent/tweaker takes ownership of compatibility.
        && !v.libraries.iter().any(|l| l.name.to_ascii_lowercase().contains("legacyfix"))
        && v.arguments.as_ref().is_none_or(|a| {
            let args = serde_json::to_string(a).unwrap_or_default().to_ascii_lowercase();
            !args.contains("legacyfix") && !args.contains("-dlf.") && !args.contains("--tweakclass")
        })
}

fn has_applet(client: &Path) -> Result<bool> {
    let archive =
        zip::ZipArchive::new(File::open(client)?).context("无法检查旧版游戏的窗口兼容性")?;
    Ok(archive
        .index_for_name("net/minecraft/client/MinecraftApplet.class")
        .is_some()
        && archive
            .index_for_name("net/minecraft/client/Minecraft.class")
            .is_some())
}

fn artifact(game: &Path) -> FileSpec {
    FileSpec {
        path: game.join(format!(
            "libraries/uk/betacraft/legacyfix/{RELEASE}/legacyfix-{RELEASE}.jar"
        )),
        url: format!(
            "https://github.com/betacraftuk/legacyfix/releases/download/{RELEASE}/legacyfix-{RELEASE}.jar"
        ),
        hash: SHA256.into(),
        sha256: true,
        size: SIZE,
    }
}

pub(crate) async fn prepare(
    game: &Path,
    resolved: &mut ResolvedVersion,
    downloader: &Downloader,
    cancel: &CancellationToken,
    sink: &EventSink,
) -> Result<Vec<String>> {
    if !candidate(
        resolved,
        cfg!(all(target_os = "macos", target_arch = "aarch64")),
    ) {
        return Ok(vec![]);
    }
    check_cancel(cancel)?;
    let client = version::client_path(game, &resolved.version)?;
    if !tokio::task::spawn_blocking(move || has_applet(&client)).await?? {
        return Ok(vec![]);
    }
    let directory = version::working_directory(game, &resolved.version)?;
    let file = artifact(game);
    prepare_file(
        &file,
        &directory,
        &mut resolved.version,
        downloader,
        cancel,
        sink,
    )
    .await
}

async fn prepare_file(
    file: &FileSpec,
    directory: &Path,
    version: &mut Version,
    downloader: &Downloader,
    cancel: &CancellationToken,
    sink: &EventSink,
) -> Result<Vec<String>> {
    sink(Event::new("stage", "准备旧版 Minecraft 颜色兼容补丁"));
    downloader
        .file(file, cancel, sink)
        .await
        .context("无法准备旧版 Minecraft 颜色兼容补丁")?;
    check_cancel(cancel)?;
    let mut args = vec![format!(
        "-javaagent:{}",
        crate::java::argument_path(&file.path)
    )];
    args.extend(DISABLED.iter().map(|id| format!("-Dlf.{id}.disable=true")));
    args.push(format!(
        "-Dlf.gameDir={}",
        crate::java::argument_path(directory)
    ));

    // LF replaces VanillaTweaker's injector in LaunchWrapper 1.5. In that mode
    // its java.io directory bridge cannot transform bootstrap classes, and the
    // game silently falls back to the official directory. Use the game's own
    // main with the agent instead: instrumentation then preserves this exact
    // instance directory. Keep the positional username/session and other args.
    version.libraries.retain(|l| l.name != WRAPPER);
    version.main_class = "net.minecraft.client.Minecraft".into();
    sink(Event::new(
        "info",
        format!("已启用旧版 Minecraft 颜色兼容补丁：LegacyFix {RELEASE}"),
    ));
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::Settings, test_http};
    use std::{io::Write, sync::Arc};

    fn legacy() -> ResolvedVersion {
        let mut v = serde_json::to_value(test_http::version_fixture("renamed-1.0")).unwrap();
        v["mainClass"] = "net.minecraft.launchwrapper.Launch".into();
        v["minecraftArguments"] =
            "${auth_player_name} ${auth_session} --gameDir ${game_directory}".into();
        v["libraries"] = serde_json::json!([
            {"name": WRAPPER}, {"name": "org.lwjgl.lwjgl:lwjgl:2.9.0"}
        ]);
        crate::version::resolve_chain(&[v]).unwrap()
    }

    #[test]
    fn detection_requires_legacy_vanilla_on_apple_silicon_and_an_actual_applet() {
        let mut v = legacy();
        assert!(candidate(&v, true));
        assert!(!candidate(&v, false));
        v.version.main_class = "net.minecraft.client.main.Main".into();
        assert!(!candidate(&v, true));
        v = legacy();
        v.version
            .arguments
            .as_mut()
            .unwrap()
            .jvm
            .push(crate::model::Argument::Plain(
                "-javaagent:/somewhere/legacyfix.jar".into(),
            ));
        assert!(!candidate(&v, true));
        v = legacy();
        v.version
            .minecraft_arguments
            .as_mut()
            .unwrap()
            .push_str(" --tweakClass custom.Loader");
        assert!(!candidate(&v, true));
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("client.jar");
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        zip.start_file(
            "net/minecraft/client/Minecraft.class",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"fixture").unwrap();
        zip.finish().unwrap();
        assert!(!has_applet(&path).unwrap());
        let mut zip =
            zip::ZipWriter::new_append(File::options().read(true).write(true).open(&path).unwrap())
                .unwrap();
        zip.start_file(
            "net/minecraft/client/MinecraftApplet.class",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"fixture").unwrap();
        zip.finish().unwrap();
        assert!(has_applet(&path).unwrap());
    }

    #[tokio::test]
    async fn download_reuse_and_corruption_never_enable_an_unverified_agent() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("a game with spaces/.minecraft");
        let settings = Settings {
            retries: 0,
            proxy_mode: "none".into(),
            ..Default::default()
        };
        let downloader = Downloader::new(temp.path(), settings).unwrap();
        let sink: EventSink = Arc::new(|_| {});
        let cancel = CancellationToken::new();
        let (url, server) = test_http::serve(vec![(200, "verified fixture".into())]).await;
        let file = FileSpec {
            url,
            hash: hex::encode(ring::digest::digest(
                &ring::digest::SHA256,
                b"verified fixture",
            )),
            size: 16,
            ..artifact(&game)
        };
        let original = legacy().version;
        let mut v = original.clone();
        let args = prepare_file(&file, &game, &mut v, &downloader, &cancel, &sink)
            .await
            .unwrap();
        assert_eq!(server.await.unwrap().len(), 1);
        assert_eq!(args[0], format!("-javaagent:{}", file.path.display()));
        assert!(args.contains(&format!("-Dlf.gameDir={}", game.display())));
        assert!(args.contains(&"-Dlf.proxy.disable=true".into()));
        assert!(!args.contains(&"-Dlf.mouse.disable=true".into()));
        assert_eq!(v.main_class, "net.minecraft.client.Minecraft");
        assert_eq!(original.main_class, "net.minecraft.launchwrapper.Launch");
        assert_eq!(v.minecraft_arguments, original.minecraft_arguments);
        assert_eq!(v.libraries.len(), original.libraries.len() - 1);
        // The HTTP listener has gone away; reuse must still succeed, including
        // after completion discarded the download staging copy.
        downloader.finish_installation(&sink).await;
        prepare_file(
            &file,
            &game,
            &mut original.clone(),
            &downloader,
            &cancel,
            &sink,
        )
        .await
        .unwrap();

        let (url, server) = test_http::serve(vec![(200, "corrupt response".into())]).await;
        let corrupt = FileSpec {
            url,
            path: game.join("bad.jar"),
            hash: "0".repeat(64),
            ..file.clone()
        };
        let mut v = original.clone();
        assert!(
            prepare_file(&corrupt, &game, &mut v, &downloader, &cancel, &sink)
                .await
                .is_err()
        );
        server.await.unwrap();
        assert!(!corrupt.path.exists());
        assert_eq!(v.main_class, original.main_class);
        cancel.cancel();
        assert!(
            prepare_file(&file, &game, &mut v, &downloader, &cancel, &sink)
                .await
                .is_err()
        );
        assert_eq!(v.main_class, original.main_class);
    }
}
