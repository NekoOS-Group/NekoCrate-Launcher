use crate::{
    download::{Downloader, FileSpec, check_cancel},
    model::{Event, EventSink},
    storage,
};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Serialize)]
pub struct JavaInfo {
    pub path: String,
    pub major: u32,
    pub version: String,
}
pub async fn inspect(path: &Path) -> Result<JavaInfo> {
    let mut command = tokio::process::Command::new(path);
    command.arg("-version").kill_on_drop(true);
    let output = tokio::time::timeout(std::time::Duration::from_secs(10), command.output())
        .await
        .context("Java 检测超时")?
        .context("无法执行 Java")?;
    if !output.status.success() {
        bail!("Java 无法运行：{}", path.display());
    }
    let version = String::from_utf8_lossy(&output.stderr).to_string()
        + &String::from_utf8_lossy(&output.stdout);
    let re = regex::Regex::new(r#"version\s+"(\d+)(?:\.(\d+))?"#)?;
    let captures = re
        .captures(&version)
        .context("无法识别 Java 版本，请选择标准 Java 可执行文件")?;
    let first: u32 = captures[1].parse()?;
    let major = if first == 1 {
        captures
            .get(2)
            .context("Java 版本不完整")?
            .as_str()
            .parse()?
    } else {
        first
    };
    Ok(JavaInfo {
        path: path.to_string_lossy().into(),
        major,
        version: version.lines().next().unwrap_or("").into(),
    })
}
fn executable_name() -> &'static str {
    if cfg!(windows) { "java.exe" } else { "java" }
}
fn find_java(root: &Path) -> Option<PathBuf> {
    walkdir::WalkDir::new(root)
        .max_depth(6)
        .into_iter()
        .filter_map(|e| e.ok())
        .find(|e| {
            e.file_type().is_file()
                && e.file_name() == executable_name()
                && e.path().parent().is_some_and(|p| p.ends_with("bin"))
        })
        .map(|e| e.into_path())
}
pub async fn discover(root: &Path) -> Vec<JavaInfo> {
    let mut candidates = vec![];
    if let Ok(home) = std::env::var("JAVA_HOME") {
        candidates.push(PathBuf::from(home).join("bin").join(executable_name()));
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for p in std::env::split_paths(&paths) {
            let p = p.join(executable_name());
            if p.is_file() {
                candidates.push(p);
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir(root.join("runtimes")) {
        for e in entries.flatten() {
            if let Some(p) = find_java(&e.path()) {
                candidates.push(p);
            }
        }
    }
    #[cfg(target_os = "macos")]
    for base in ["/Library/Java/JavaVirtualMachines", "/opt/homebrew/opt"] {
        if let Ok(entries) = std::fs::read_dir(base) {
            for e in entries.flatten() {
                if e.file_name().to_string_lossy().contains("jdk")
                    || base.contains("JavaVirtualMachines")
                {
                    if let Some(p) = find_java(&e.path()) {
                        candidates.push(p);
                    }
                }
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    let mut found = vec![];
    for path in candidates {
        if let Ok(info) = inspect(&path).await {
            if !found.iter().any(|v: &JavaInfo| v.path == info.path) {
                found.push(info);
            }
        }
    }
    found
}
pub async fn ensure(
    root: &Path,
    major: u32,
    explicit: Option<&str>,
    auto: bool,
    downloader: &Downloader,
    cancel: &CancellationToken,
    sink: &EventSink,
) -> Result<JavaInfo> {
    if let Some(path) = explicit.filter(|p| !p.is_empty()) {
        let info = inspect(Path::new(path)).await?;
        if info.major != major {
            bail!(
                "此游戏需要 Java {major}，所选路径是 Java {}。请切换 Java 或清空路径启用自动匹配。",
                info.major
            );
        }
        return Ok(info);
    }
    if let Some(java) = discover(root).await.into_iter().find(|j| j.major == major) {
        return Ok(java);
    }
    if !auto {
        bail!("未找到 Java {major}，请指定 Java 路径或在设置中允许自动下载。");
    }
    check_cancel(cancel)?;
    sink(Event::new("stage", format!("正在安装 Java {major}")));
    let os = if cfg!(target_os = "macos") {
        "mac"
    } else if cfg!(windows) {
        "windows"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86_64") {
        "x64"
    } else {
        bail!("此架构暂不支持自动安装 Java，请手动配置")
    };
    let url = format!(
        "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?architecture={arch}&image_type=jre&os={os}&vendor=eclipse"
    );
    let assets: serde_json::Value = downloader.json(&url, cancel).await?;
    let package = &assets[0]["binary"]["package"];
    let link = package["link"]
        .as_str()
        .context("暂无适合此系统的 Java JRE，请指定本地 Java 路径")?;
    let name = package["name"].as_str().context("Java 下载信息缺失")?;
    let archive = root.join("cache/java").join(storage::safe_relative(name)?);
    downloader
        .file(
            &FileSpec {
                url: link.into(),
                path: archive.clone(),
                hash: package["checksum"]
                    .as_str()
                    .context("Java 校验值缺失")?
                    .into(),
                sha256: true,
                size: package["size"].as_u64().unwrap_or(0),
            },
            cancel,
            sink,
        )
        .await?;
    check_cancel(cancel)?;
    let staging = root
        .join("runtimes")
        .join(format!(".install-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&staging)?;
    let extraction = extract_runtime(&archive, &staging, cancel);
    if let Err(e) = extraction {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    let relative = find_java(&staging)
        .context("解压后未找到 Java 可执行文件")?
        .strip_prefix(&staging)?
        .to_path_buf();
    let runtime = root
        .join("runtimes")
        .join(format!("java-{major}-{os}-{arch}"));
    if runtime.exists() {
        std::fs::remove_dir_all(&runtime)?;
    }
    std::fs::rename(&staging, &runtime)?;
    let java = inspect(&runtime.join(relative)).await?;
    if java.major != major {
        bail!("安装后的 Java 版本与要求不匹配");
    }
    Ok(java)
}
fn extract_runtime(archive: &Path, dest: &Path, cancel: &CancellationToken) -> Result<()> {
    let file = std::fs::File::open(archive)?;
    if archive.extension().is_some_and(|e| e == "zip") {
        let mut zip = zip::ZipArchive::new(file)?;
        for i in 0..zip.len() {
            check_cancel(cancel)?;
            let mut entry = zip.by_index(i)?;
            let path = entry.enclosed_name().context("Java 压缩包含越界路径")?;
            if entry.is_symlink() {
                bail!("Java ZIP 中含不支持的符号链接");
            }
            let target = dest.join(path);
            if entry.is_dir() {
                std::fs::create_dir_all(target)?;
            } else {
                std::fs::create_dir_all(target.parent().unwrap())?;
                std::io::copy(&mut entry, &mut std::fs::File::create(&target)?)?;
            }
        }
    } else {
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
        for entry in tar.entries()? {
            check_cancel(cancel)?;
            let mut entry = entry?;
            let kind = entry.header().entry_type();
            // Extract regular files/directories only. Temurin's executable and dylibs are regular files.
            if !(kind.is_file() || kind.is_dir()) {
                continue;
            }
            if !entry.unpack_in(dest)? {
                bail!("Java 压缩包包含越界路径");
            }
        }
    }
    Ok(())
}
