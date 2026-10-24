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
    pub arch: String,
    pub jdk: bool,
}
pub struct JavaRequirement {
    pub major: u32,
    pub arch: &'static str,
    pub jdk: bool,
    pub compatibility: Option<crate::java_compatibility::Compatibility>,
    pub bytecode_ceiling: Option<(u32, String)>,
}
impl JavaRequirement {
    fn matches(&self, java: &JavaInfo) -> bool {
        self.allowed_majors().contains(&java.major)
            && java.arch == self.arch
            && (!self.jdk || java.jdk)
            && !["-ea", "-internal"]
                .iter()
                .any(|tag| java.version.contains(tag))
    }

    fn allowed_majors(&self) -> &[u32] {
        self.compatibility
            .as_ref()
            .map_or(std::slice::from_ref(&self.major), |c| c.majors.as_slice())
    }

    fn description(&self) -> String {
        format!(
            "{} {} ({})",
            if self.jdk { "JDK" } else { "Java" },
            self.allowed_majors()
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", "),
            self.arch
        )
    }

    pub fn for_version(version: &crate::model::Version) -> Self {
        let intel_mac = cfg!(all(target_os = "macos", target_arch = "aarch64"))
            && (version
                .libraries
                .iter()
                .any(|l| l.name.starts_with("org.lwjgl.lwjgl:"))
                || (version
                    .libraries
                    .iter()
                    .any(|l| l.name.starts_with("org.lwjgl:lwjgl:"))
                    && !version
                        .libraries
                        .iter()
                        .any(|l| l.name.contains("natives-macos-arm64"))));
        Self {
            major: version.java_version.as_ref().map_or(8, |j| j.major),
            arch: if intel_mac {
                "x86_64"
            } else {
                crate::minecraft::arch_name()
            },
            // Mod agents can inspect JDK classes and their local variable tables.
            // Temurin's stripped jlink JRE fails e.g. Preloading Tricks 3.5.6.
            jdk: version.java_version.as_ref().is_some_and(|j| j.major >= 9)
                && version.main_class != "net.minecraft.client.main.Main",
            compatibility: None,
            bytecode_ceiling: crate::java_compatibility::bytecode_ceiling(version),
        }
    }

    pub async fn for_instance(
        version: &crate::model::Version,
        game: &Path,
        explicit: Option<&str>,
    ) -> Result<Self> {
        let mut requirement = Self::for_version(version);
        if explicit.is_some_and(|path| !path.is_empty()) {
            return Ok(requirement);
        }
        let version = version.clone();
        let game = game.to_owned();
        requirement.compatibility = Some(
            tokio::task::spawn_blocking(move || crate::java_compatibility::assess(&version, &game))
                .await??,
        );
        Ok(requirement)
    }
}
pub async fn inspect(path: &Path) -> Result<JavaInfo> {
    let mut command = tokio::process::Command::new(path);
    command
        .args(["-XshowSettings:properties", "-version"])
        .kill_on_drop(true);
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
    let arch = version
        .lines()
        .find_map(|line| line.trim().strip_prefix("os.arch = "))
        .unwrap_or(crate::minecraft::arch_name());
    let arch = match arch.trim() {
        "amd64" | "x86_64" => "x86_64",
        "arm64" | "aarch64" => "aarch64",
        "x86" | "i386" | "i686" => "x86",
        other => other,
    };
    Ok(JavaInfo {
        path: path.to_string_lossy().into(),
        major,
        version: version
            .lines()
            .find(|line| line.contains("version \""))
            .unwrap_or("")
            .into(),
        arch: arch.into(),
        jdk: path.parent().is_some_and(|bin| {
            bin.join(if cfg!(windows) { "javac.exe" } else { "javac" })
                .is_file()
        }),
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
                if (e.file_name().to_string_lossy().contains("jdk")
                    || base.contains("JavaVirtualMachines"))
                    && let Some(p) = find_java(&e.path())
                {
                    candidates.push(p);
                }
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    let mut found = vec![];
    for path in candidates {
        if let Ok(info) = inspect(&path).await
            && !found.iter().any(|v: &JavaInfo| v.path == info.path)
        {
            found.push(info);
        }
    }
    found
}
pub async fn ensure(
    root: &Path,
    requirement: JavaRequirement,
    explicit: Option<&str>,
    auto: bool,
    downloader: &Downloader,
    cancel: &CancellationToken,
    sink: &EventSink,
) -> Result<JavaInfo> {
    let major = requirement.major;
    if let Some(path) = explicit.filter(|p| !p.is_empty()) {
        let info = inspect(Path::new(path)).await?;
        if let Some((maximum, component)) = &requirement.bytecode_ceiling
            && info.major > *maximum
        {
            bail!(
                "所选 Java {} 超出 {} 的字节码支持范围（最高 Java {}）。请切换为 Auto 或选择兼容的 Java。",
                info.major,
                component,
                maximum
            );
        }
        if !explicit_major_matches(major, info.major) {
            bail!(
                "此游戏需要 Java {major}，所选路径是 Java {}。请切换 Java 或清空路径启用自动匹配。",
                info.major
            );
        }
        if info.arch != requirement.arch {
            bail!(
                "此游戏的原生库需要 {} Java，所选 Java 是 {}。请切换 Java 或使用自动匹配。",
                requirement.arch,
                info.arch
            );
        }
        return Ok(info);
    }
    let available = discover(root).await;
    if let Some(compatibility) = &requirement.compatibility {
        sink(Event::new(
            "info",
            format!("Java 自动匹配范围：{}", requirement.description()),
        ));
        for reason in &compatibility.reasons {
            sink(Event::new("info", reason.clone()));
        }
    }
    if let Some(java) = available
        .iter()
        .filter(|j| requirement.matches(j))
        .max_by_key(|j| (j.major, version_numbers(&j.version)))
    {
        sink(Event::new(
            "info",
            format!("使用本地 Java：{} · {}", java.version, java.path),
        ));
        return Ok(java.clone());
    }
    let mut descriptions: Vec<_> = available
        .iter()
        .map(|java| {
            format!(
                "{} {} ({})",
                if java.jdk { "JDK" } else { "JRE" },
                java.major,
                java.arch
            )
        })
        .collect();
    descriptions.sort();
    descriptions.dedup();
    sink(Event::new(
        "info",
        if descriptions.is_empty() {
            format!(
                "需要 {}；未检测到可用的本地 Java",
                requirement.description()
            )
        } else {
            format!(
                "需要 {}；本地 Java 均不匹配：{}",
                requirement.description(),
                descriptions.join(", ")
            )
        },
    ));
    if !auto {
        bail!(
            "未找到兼容的 {}，请指定 Java 路径或允许自动下载。",
            requirement.description()
        );
    }
    let major = *requirement
        .allowed_majors()
        .last()
        .context("没有兼容的 Java 版本")?;
    check_cancel(cancel)?;
    sink(Event::new(
        "stage",
        format!(
            "正在安装 {} {major} ({})",
            if requirement.jdk { "JDK" } else { "Java" },
            requirement.arch
        ),
    ));
    let os = if cfg!(target_os = "macos") {
        "mac"
    } else if cfg!(windows) {
        "windows"
    } else {
        "linux"
    };
    let arch = match requirement.arch {
        "aarch64" => "aarch64",
        "x86_64" => "x64",
        _ => bail!("此架构暂不支持自动安装 Java，请手动配置"),
    };
    let image_type = if requirement.jdk { "jdk" } else { "jre" };
    let url = format!(
        "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?architecture={arch}&image_type={image_type}&os={os}&vendor=eclipse"
    );
    let assets: serde_json::Value = downloader.json(&url, cancel).await?;
    let package = &assets[0]["binary"]["package"];
    let link = package["link"]
        .as_str()
        .context("暂无适合此系统的 Java，请指定本地 Java 路径")?;
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
        .join(format!("{image_type}-{major}-{os}-{arch}"));
    if runtime.exists() {
        std::fs::remove_dir_all(&runtime)?;
    }
    std::fs::rename(&staging, &runtime)?;
    let java = inspect(&runtime.join(relative)).await?;
    if java.major != major || !requirement.matches(&java) {
        bail!("安装后的 Java 版本与要求不匹配");
    }
    Ok(java)
}
fn version_numbers(version: &str) -> Vec<u32> {
    version
        .split('"')
        .nth(1)
        .unwrap_or(version)
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| s.parse().ok())
        .collect()
}
fn explicit_major_matches(required: u32, selected: u32) -> bool {
    // Keep automatic selection on the declared major. A user can deliberately
    // choose a newer modern JVM for modpacks, while legacy Java 8 stays exact.
    selected == required || (required >= 17 && selected > required)
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modern_loader_auto_selection_requires_jdk_but_explicit_newer_java_is_allowed() {
        let mut version = crate::test_http::version_fixture("1.20.1");
        version.java_version = Some(crate::model::JavaVersion { major: 17 });
        assert!(!JavaRequirement::for_version(&version).jdk);
        version.main_class = "cpw.mods.bootstraplauncher.BootstrapLauncher".into();
        let requirement = JavaRequirement::for_version(&version);
        assert!(requirement.jdk);
        let mut local = JavaInfo {
            path: "java".into(),
            major: 26,
            version: "26.0.2.1".into(),
            arch: requirement.arch.into(),
            jdk: true,
        };
        assert!(!requirement.matches(&local));
        local.major = 17;
        assert!(requirement.matches(&local));
        local.jdk = false;
        assert!(!requirement.matches(&local));
        assert!(explicit_major_matches(17, 21));
        assert!(!explicit_major_matches(21, 17));
        assert!(!explicit_major_matches(8, 17));
        version.java_version = Some(crate::model::JavaVersion { major: 8 });
        assert!(!JavaRequirement::for_version(&version).jdk);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn auto_selection_reuses_a_matching_local_jdk_without_download() {
        use std::{
            os::unix::fs::PermissionsExt,
            sync::{Arc, Mutex},
        };
        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("runtimes/existing-jdk/bin");
        std::fs::create_dir_all(&bin).unwrap();
        let java = bin.join("java");
        std::fs::write(
            &java,
            format!(
                "#!/bin/sh\nprintf 'openjdk version \"17.0.1\"\\n    os.arch = {}\\n' >&2\n",
                crate::minecraft::arch_name()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&java, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(bin.join("javac"), "").unwrap();
        let downloader = Downloader::new(temp.path(), crate::model::Settings::default()).unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = events.clone();
        let sink: EventSink = Arc::new(move |event| captured.lock().unwrap().push(event));
        // Downloading is impossible with a cancelled token. Local reuse still succeeds.
        let cancel = CancellationToken::new();
        cancel.cancel();
        let found = ensure(
            temp.path(),
            JavaRequirement {
                major: 17,
                arch: crate::minecraft::arch_name(),
                jdk: true,
                compatibility: None,
                bytecode_ceiling: None,
            },
            None,
            true,
            &downloader,
            &cancel,
            &sink,
        )
        .await
        .unwrap();
        assert_eq!(found.major, 17);
        assert!(found.jdk);
        assert!(
            events
                .lock()
                .unwrap()
                .iter()
                .any(|e| e.message.starts_with("使用本地 Java："))
        );
        assert!(!events.lock().unwrap().iter().any(|e| e.kind == "stage"));
        let newer = temp.path().join("runtimes/newer-jdk/bin");
        std::fs::create_dir_all(&newer).unwrap();
        std::fs::write(
            newer.join("java"),
            format!(
                "#!/bin/sh\nprintf 'openjdk version \"26.0.2\"\\n    os.arch = {}\\n' >&2\n",
                crate::minecraft::arch_name()
            ),
        )
        .unwrap();
        std::fs::set_permissions(newer.join("java"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        std::fs::write(newer.join("javac"), "").unwrap();
        let mut profile = crate::test_http::version_fixture("1.20.1");
        profile.java_version = Some(crate::model::JavaVersion { major: 17 });
        profile.compatible_java_majors = (17..=26).collect();
        let requirement = JavaRequirement::for_instance(&profile, temp.path(), None)
            .await
            .unwrap();
        // The very same verified range also chooses 26 for a new download.
        assert_eq!(requirement.allowed_majors().last(), Some(&26));
        let selected = ensure(
            temp.path(),
            requirement,
            None,
            true,
            &downloader,
            &cancel,
            &sink,
        )
        .await
        .unwrap();
        assert_eq!(selected.major, 26);
        assert!(!events.lock().unwrap().iter().any(|e| e.kind == "stage"));
        // Manual selection must not bypass a known bytecode incompatibility.
        let mut value = serde_json::to_value(profile).unwrap();
        value["mainClass"] = "cpw.mods.bootstraplauncher.BootstrapLauncher".into();
        value["libraries"] = serde_json::json!([{"name":"org.ow2.asm:asm:9.7.1"}]);
        let version = crate::version::resolve_chain(&[value]).unwrap().version;
        let error = ensure(
            temp.path(),
            JavaRequirement::for_version(&version),
            Some(&selected.path),
            true,
            &downloader,
            &cancel,
            &sink,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("ASM 9.7.1"));
        assert!(error.to_string().contains("最高 Java 24"));
    }
}
