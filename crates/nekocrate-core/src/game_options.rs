use crate::{language, model::Settings, storage};
use anyhow::{Context, Result, bail};
use std::{fs, io::ErrorKind, path::Path};

#[cfg(test)]
pub fn apply_language(game: &Path, settings: &Settings) -> Result<()> {
    apply_language_for_locale(game, settings, &language::system_locale())
}

#[cfg(test)]
fn apply_language_for_locale(game: &Path, settings: &Settings, system: &str) -> Result<()> {
    apply_options(
        game,
        settings,
        &crate::launch_options::LaunchOptions::default(),
        system,
    )
}

pub fn apply_launch(
    game: &Path,
    settings: &Settings,
    options: &crate::launch_options::LaunchOptions,
) -> Result<()> {
    options.validate()?;
    apply_options(game, settings, options, &language::system_locale())
}
fn apply_options(
    game: &Path,
    settings: &Settings,
    options: &crate::launch_options::LaunchOptions,
    system: &str,
) -> Result<()> {
    use crate::launch_options::WindowMode;
    let mut values = Vec::new();
    if settings.apply_language_to_game {
        if !["system", "zh-CN", "en-US"].contains(&settings.language.as_str()) {
            bail!("不支持的游戏语言");
        }
        values.push((
            "lang",
            match language::resolve(&settings.language, system) {
                "zh-CN" => "zh_cn",
                _ => "en_us",
            }
            .to_owned(),
        ));
    }
    if options.window_mode != WindowMode::Default {
        values.push((
            "fullscreen",
            (options.window_mode == WindowMode::Fullscreen).to_string(),
        ));
        if options.window_mode == WindowMode::Windowed {
            values.push(("overrideWidth", options.width.to_string()));
            values.push(("overrideHeight", options.height.to_string()));
        }
    }
    if values.is_empty() {
        return Ok(());
    }
    let path = game.join("options.txt");
    let original = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error).context("无法读取游戏设置"),
    };
    let newline = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut output = String::new();
    let mut found = std::collections::HashSet::new();
    for line in original.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        let prefix = if body.starts_with('\u{feff}') {
            "\u{feff}"
        } else {
            ""
        };
        if let Some((key, value)) = values.iter().find(|(key, _)| {
            body.strip_prefix(prefix)
                .unwrap()
                .starts_with(&format!("{key}:"))
        }) {
            if found.insert(*key) {
                output.push_str(prefix);
                output.push_str(&format!("{key}:{value}"));
                output.push_str(&line[body.len()..]);
            }
        } else {
            output.push_str(line);
        }
    }
    for (key, value) in values.iter().filter(|(key, _)| !found.contains(key)) {
        if !output.is_empty() && !output.ends_with('\n') {
            output.push_str(newline);
        }
        output.push_str(&format!("{key}:{value}"));
        output.push_str(newline);
    }
    if output != original {
        storage::atomic_write_with_backup(
            &path,
            output.as_bytes(),
            &game.join("options.txt.nekocrate.bak"),
        )
        .context("无法保存游戏设置")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn window_settings_preserve_unrelated_options_and_default_is_noop() {
        use crate::launch_options::{LaunchOptions, WindowMode};
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("options.txt");
        let original =
            "fullscreen:true\r\noverrideWidth:900\r\noverrideWidth:800\r\nfov:0.5\r\nlang:zh_cn";
        fs::write(&path, original).unwrap();
        let options = LaunchOptions {
            window_mode: WindowMode::Windowed,
            width: 1440,
            height: 900,
            ..Default::default()
        };
        apply_launch(temp.path(), &Settings::default(), &options).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(
            text,
            "fullscreen:false\r\noverrideWidth:1440\r\nfov:0.5\r\nlang:zh_cn\r\noverrideHeight:900\r\n"
        );
        assert_eq!(
            fs::read_to_string(temp.path().join("options.txt.nekocrate.bak")).unwrap(),
            original
        );
        apply_launch(temp.path(), &Settings::default(), &LaunchOptions::default()).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
        apply_launch(
            temp.path(),
            &Settings::default(),
            &LaunchOptions {
                window_mode: WindowMode::Fullscreen,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .starts_with("fullscreen:true")
        );
    }

    #[test]
    fn language_sync_preserves_other_options_and_keeps_backup() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("options.txt");
        let original =
            "\u{feff}lang:en_us\r\nfov:0.5\r\nkey_key.chat:84\r\nresourcePacks:[\"vanilla\"]";
        fs::write(&file, original).unwrap();
        let mut settings = Settings {
            language: "zh-CN".into(),
            ..Settings::default()
        };
        apply_language(temp.path(), &settings).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), original);
        settings.apply_language_to_game = true;
        apply_language(temp.path(), &settings).unwrap();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            original.replace("en_us", "zh_cn")
        );
        let backup = temp.path().join("options.txt.nekocrate.bak");
        assert_eq!(fs::read_to_string(&backup).unwrap(), original);
        apply_language(temp.path(), &settings).unwrap();
        assert_eq!(fs::read_to_string(backup).unwrap(), original);
    }
    #[test]
    fn disabled_sync_does_not_create_options_and_enabled_sync_handles_new_instances() {
        let temp = tempfile::tempdir().unwrap();
        apply_language(temp.path(), &Settings::default()).unwrap();
        assert!(!temp.path().join("options.txt").exists());
        let settings = Settings {
            language: "zh-CN".into(),
            apply_language_to_game: true,
            ..Settings::default()
        };
        apply_language(temp.path(), &settings).unwrap();
        assert_eq!(
            fs::read_to_string(temp.path().join("options.txt")).unwrap(),
            "lang:zh_cn\n"
        );
        fs::write(
            temp.path().join("options.txt"),
            "music:0.4\nlang:en_us\nlang:ja_jp\n",
        )
        .unwrap();
        apply_language(temp.path(), &settings).unwrap();
        assert_eq!(
            fs::read_to_string(temp.path().join("options.txt")).unwrap(),
            "music:0.4\nlang:zh_cn\n"
        );
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.language, "system");
        assert!(!old.apply_language_to_game);
    }

    #[test]
    fn game_language_matches_explicit_and_system_preferences() {
        let temp = tempfile::tempdir().unwrap();
        for (preference, system, expected) in [
            ("en-US", "zh-CN", "en_us"),
            ("zh-CN", "en-US", "zh_cn"),
            ("system", "zh-Hans", "zh_cn"),
            ("system", "ja-JP", "en_us"),
            ("system", "zh-TW", "en_us"),
        ] {
            let settings = Settings {
                language: preference.into(),
                apply_language_to_game: true,
                ..Settings::default()
            };
            apply_language_for_locale(temp.path(), &settings, system).unwrap();
            assert_eq!(
                fs::read_to_string(temp.path().join("options.txt")).unwrap(),
                format!("lang:{expected}\n")
            );
        }
    }
}
