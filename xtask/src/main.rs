use std::{env, ffi::OsString, path::Path, process::Command};

fn signing_script() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("scripts/macos_sign.py")
}

fn signing(command: &str, path: Option<&std::ffi::OsStr>) -> bool {
    let mut process = Command::new("python3");
    process.arg(signing_script()).arg(command);
    if let Some(path) = path {
        process.arg(path);
    }
    match process.status() {
        Ok(status) => status.success(),
        Err(error) => {
            eprintln!("无法运行本地签名脚本：{error}。请确认 Python 3 已安装并位于 PATH 中。");
            false
        }
    }
}

fn signed_run(args: &[OsString]) -> ! {
    let Some((binary, args)) = args.split_first() else {
        eprintln!("缺少待签名的可执行文件");
        std::process::exit(1);
    };
    if !signing("sign", Some(binary)) {
        std::process::exit(1);
    }
    let mut command = Command::new(binary);
    command.args(args);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = command.exec();
        eprintln!("无法启动已签名的程序：{error}");
        std::process::exit(1);
    }
    #[cfg(not(unix))]
    std::process::exit(command.status().ok().and_then(|s| s.code()).unwrap_or(1));
}

fn add_signed_runner(args: &mut Vec<OsString>, executable: &Path) {
    // Tauri's first `--` separates Cargo options; a second one separates app
    // arguments. Cargo config uses an array so Unicode/spaces remain intact.
    let start = match args.iter().position(|arg| arg == "--") {
        Some(index) => index + 1,
        None => {
            args.push("--".into());
            args.len()
        }
    };
    let runner = serde_json::json!([executable, "--signed-run"]);
    let configs = ["aarch64-apple-darwin", "x86_64-apple-darwin"]
        .into_iter()
        .flat_map(|target| {
            [
                OsString::from("--config"),
                OsString::from(format!("target.{target}.runner={runner}")),
            ]
        });
    args.splice(start..start, configs);
}

fn main() {
    let mut args: Vec<_> = env::args_os().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--signed-run") {
        signed_run(&args[1..]);
    }
    let app = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("开发工具应位于仓库内")
        .join("app");
    if !app.join("node_modules/.bin/tauri").exists()
        && !app.join("node_modules/.bin/tauri.cmd").exists()
    {
        eprintln!("请先在仓库根目录运行 npm --prefix app ci 安装前端依赖。");
        std::process::exit(1);
    }
    if args.first().is_some_and(|arg| arg == "--") {
        args.remove(0);
    }
    let help = args.iter().take_while(|arg| *arg != "--").any(|arg| {
        ["--help", "-h", "--version", "-V"]
            .iter()
            .any(|flag| arg == flag)
    });
    if cfg!(target_os = "macos") && !help {
        if !signing("ensure", None) {
            std::process::exit(1);
        }
        add_signed_runner(&mut args, &env::current_exe().expect("无法定位开发工具"));
    }
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let result = Command::new(npm)
        .current_dir(app)
        .args(["run", "tauri", "--", "dev"])
        .args(args)
        .status();
    match result {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => {
            eprintln!("无法启动界面：{error}。请确认 Node.js 和 npm 已安装并位于 PATH 中。");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_config_preserves_tauri_cargo_and_application_arguments() {
        let executable = Path::new("/tmp/猫咪 project/nekocrate-dev");
        let mut args: Vec<OsString> = ["--release", "--", "--locked", "--", "--hello", "two words"]
            .into_iter()
            .map(Into::into)
            .collect();
        add_signed_runner(&mut args, executable);
        assert_eq!(&args[..2], &[OsString::from("--release"), "--".into()]);
        assert_eq!(
            &args[6..],
            &[
                OsString::from("--locked"),
                "--".into(),
                "--hello".into(),
                "two words".into()
            ]
        );
        let config = args[3].to_str().unwrap().split_once('=').unwrap().1;
        assert_eq!(
            serde_json::from_str::<Vec<String>>(config).unwrap(),
            vec![executable.to_str().unwrap(), "--signed-run"]
        );
        let mut args = vec![];
        add_signed_runner(&mut args, executable);
        assert_eq!(args[0], "--");
        assert_eq!(args.len(), 5);
    }
}
