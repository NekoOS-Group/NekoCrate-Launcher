use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowMode {
    #[default]
    Default,
    Windowed,
    Fullscreen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolationMode {
    Shared,
    Version,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogBehavior {
    #[default]
    Crash,
    Startup,
    Manual,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchOptions {
    /// None retains the layout of configurations saved before explicit isolation existed.
    /// The UI offers only Shared and Version; it does not offer an automatic mode.
    pub isolation: Option<IsolationMode>,
    pub log_behavior: LogBehavior,
    pub window_mode: WindowMode,
    pub width: u32,
    pub height: u32,
    pub minimize_launcher: bool,
    /// One literal argument per entry; never evaluated by a shell.
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
    /// NAME=value entries; only applied to the child game process.
    pub environment: Vec<String>,
}
impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            isolation: None,
            log_behavior: LogBehavior::Crash,
            window_mode: WindowMode::Default,
            width: 1280,
            height: 720,
            minimize_launcher: false,
            jvm_args: Vec::new(),
            game_args: Vec::new(),
            environment: Vec::new(),
        }
    }
}
impl LaunchOptions {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.window_mode != WindowMode::Windowed
                || ((320..=7680).contains(&self.width) && (240..=4320).contains(&self.height)),
            "游戏窗口宽度应为 320–7680，高度应为 240–4320"
        );
        ensure!(
            self.jvm_args.len() <= 64
                && self.jvm_args.iter().map(String::len).sum::<usize>() <= 16384,
            "额外 JVM 参数过多或过长"
        );
        for arg in &self.jvm_args {
            ensure!(
                arg.starts_with('-') && !arg.contains(['\0', '\r', '\n']),
                "JVM 参数应每行一个，并以 - 开头"
            );
            ensure!(
                !["-Xmx", "-Xms", "-XX:MaxHeapSize=", "-XX:InitialHeapSize="]
                    .iter()
                    .any(|prefix| arg.starts_with(prefix)),
                "请通过启动内存设置调整内存"
            );
            ensure!(
                ![
                    "-cp",
                    "-classpath",
                    "--class-path",
                    "-jar",
                    "-m",
                    "--module",
                    "--",
                    "-version",
                    "--version",
                    "-help",
                    "--help",
                    "-h",
                    "-?"
                ]
                .iter()
                .any(|flag| arg == flag
                    || arg.starts_with(&format!("{flag}="))
                    || arg.starts_with(&format!("{flag} ")))
                    && !arg.starts_with("-Djava.class.path=")
                    && !arg.starts_with("-Djava.library.path="),
                "请勿在额外 JVM 参数中更改启动入口或类路径"
            );
        }
        ensure!(
            self.game_args.len() <= 128
                && self.game_args.iter().map(String::len).sum::<usize>() <= 16384,
            "游戏参数过多或过长"
        );
        for arg in &self.game_args {
            ensure!(
                !arg.is_empty() && !arg.contains(['\0', '\r', '\n']),
                "游戏参数应每行一个"
            );
            let flag = arg.split_once('=').map_or(arg.as_str(), |(flag, _)| flag);
            ensure!(
                ![
                    "--gameDir",
                    "--assetsDir",
                    "--assetIndex",
                    "--username",
                    "--uuid",
                    "--accessToken",
                    "--session",
                    "--userType",
                    "--userProperties",
                    "--clientId",
                    "--xuid"
                ]
                .contains(&flag),
                "账户和游戏目录参数由启动器管理"
            );
        }
        ensure!(
            self.environment.len() <= 64
                && self.environment.iter().map(String::len).sum::<usize>() <= 16384,
            "环境变量过多或过长"
        );
        let mut names = std::collections::HashSet::new();
        for line in &self.environment {
            let (name, value) = line
                .split_once('=')
                .ok_or_else(|| anyhow::anyhow!("环境变量应为 NAME=value，每行一项"))?;
            ensure!(
                !name.is_empty()
                    && name.bytes().enumerate().all(|(index, b)| b == b'_'
                        || b.is_ascii_alphabetic()
                        || (index > 0 && b.is_ascii_digit()))
                    && !value.contains(['\0', '\r', '\n']),
                "环境变量名称只能包含字母、数字和下划线，且不能以数字开头"
            );
            let key = if cfg!(windows) {
                name.to_ascii_uppercase()
            } else {
                name.to_owned()
            };
            ensure!(names.insert(key), "环境变量名称重复：{name}");
            ensure!(
                ![
                    "JAVA_TOOL_OPTIONS",
                    "_JAVA_OPTIONS",
                    "JDK_JAVA_OPTIONS",
                    "CLASSPATH"
                ]
                .contains(&name.to_ascii_uppercase().as_str()),
                "Java 环境参数请在 JVM 页中配置"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn game_arguments_and_environment_are_literal_but_cannot_replace_managed_settings() {
        let mut options = LaunchOptions {
            game_args: vec!["--server".into(), "host with spaces;$(echo no)".into()],
            environment: vec!["PACK=one=two $HOME".into(), "EMPTY=".into()],
            ..Default::default()
        };
        options.validate().unwrap();
        for invalid in ["--gameDir", "--accessToken=secret", "--username", "\nflag"] {
            options.game_args = vec![invalid.into()];
            assert!(options.validate().is_err(), "{invalid}");
        }
        options.game_args.clear();
        for invalid in [
            vec!["NO_EQUALS"],
            vec!["1NAME=x"],
            vec!["NAME=x\ny"],
            vec!["NAME=x", "NAME=y"],
            vec!["JAVA_TOOL_OPTIONS=-Xmx1G"],
        ] {
            options.environment = invalid.iter().map(|s| (*s).into()).collect();
            assert!(options.validate().is_err(), "{invalid:?}");
        }
        assert!(serde_json::from_str::<IsolationMode>("\"auto\"").is_err());
    }
    #[test]
    fn defaults_preserve_launch_behavior_and_parameters_remain_literal() {
        let old: LaunchOptions = serde_json::from_str("{}").unwrap();
        assert_eq!(old, LaunchOptions::default());
        assert!(!old.minimize_launcher);
        let mut options = old;
        options.jvm_args = vec![
            "-Dpack.name=My Pack".into(),
            "--add-opens=java.base/java.lang=ALL-UNNAMED".into(),
        ];
        options.validate().unwrap();
        for invalid in [
            "-Xmx8G",
            "-cp",
            "--class-path=other",
            "-jar file.jar",
            "@options",
            "-Dvalue=one\n-Xmx8G",
        ] {
            options.jvm_args = vec![invalid.into()];
            assert!(options.validate().is_err(), "{invalid}");
        }
        options.jvm_args.clear();
        options.window_mode = WindowMode::Windowed;
        options.width = 0;
        assert!(options.validate().is_err());
    }
}
