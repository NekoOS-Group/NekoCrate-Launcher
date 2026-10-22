use std::{env, path::Path, process::Command};

fn main() {
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
    let mut args: Vec<_> = env::args_os().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--") {
        args.remove(0);
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
