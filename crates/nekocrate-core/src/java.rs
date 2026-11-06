mod discovery;
mod history;

use crate::{
    download::{Downloader, FileSpec, check_cancel},
    model::{Event, EventSink},
    storage,
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

pub(crate) fn background_command(path: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(path);
    command.stdin(std::process::Stdio::null());
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW, including runtime probes
    command
}

pub(crate) fn game_executable(path: &Path) -> PathBuf {
    game_executable_for(path, cfg!(windows))
}

fn game_executable_for(path: &Path, windows: bool) -> PathBuf {
    if windows
        && path
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("java.exe"))
    {
        let windowed = path.with_file_name("javaw.exe");
        if windowed.is_file() {
            return windowed;
        }
    }
    path.to_owned()
}

/// Canonical Windows paths are useful for Rust filesystem operations, but Java
/// NIO rejects the verbatim namespace prefix (notably Forge's module path).
/// Convert only at the JVM argument boundary; keep the stored paths untouched.
pub(crate) fn argument_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    #[cfg(windows)]
    {
        windows_argument_path(&path)
    }
    #[cfg(not(windows))]
    {
        path.into_owned()
    }
}

#[cfg(any(windows, test))]
fn windows_argument_path(path: &str) -> String {
    if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{unc}");
    }
    if let Some(drive) = path.strip_prefix(r"\\?\") {
        let bytes = drive.as_bytes();
        if bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'\\'
        {
            return drive.to_owned();
        }
    }
    path.to_owned()
}

/// Older Windows JVMs write redirected output in the system ANSI code page.
/// Prefer valid UTF-8 (modern JVMs/mod loggers); decode legacy output before
/// redaction and saving our own UTF-8 log. This never changes JVM input paths.
pub(crate) fn decode_output(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    #[cfg(windows)]
    {
        decode_windows_output(bytes, windows_sys::Win32::Globalization::CP_ACP)
    }
    #[cfg(not(windows))]
    {
        String::from_utf8_lossy(bytes)
    }
}

#[cfg(windows)]
fn decode_windows_output(bytes: &[u8], code_page: u32) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    use windows_sys::Win32::Globalization::{MB_ERR_INVALID_CHARS, MultiByteToWideChar};
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Cow::Borrowed(text);
    }
    let Ok(len) = i32::try_from(bytes.len()) else {
        return String::from_utf8_lossy(bytes);
    };
    // Each input byte produces at most one UTF-16 code unit in Windows' ANSI
    // encodings. The source and initialized destination stay alive for the call.
    let mut wide = vec![0u16; bytes.len()];
    let written = unsafe {
        MultiByteToWideChar(
            code_page,
            MB_ERR_INVALID_CHARS,
            bytes.as_ptr(),
            len,
            wide.as_mut_ptr(),
            len,
        )
    };
    if written > 0 {
        Cow::Owned(String::from_utf16_lossy(&wide[..written as usize]))
    } else {
        String::from_utf8_lossy(bytes)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JavaInfo {
    pub path: String,
    pub major: u32,
    pub version: String,
    pub arch: String,
    pub jdk: bool,
    #[serde(default)]
    pub vendor: String,
}
pub struct JavaRequirement {
    pub major: u32,
    pub arch: &'static str,
    pub jdk: bool,
    pub compatibility: Option<crate::java_compatibility::Compatibility>,
    pub bytecode_ceiling: Option<(u32, String)>,
    pub(crate) working_directory: Option<PathBuf>,
    pub(crate) history: Option<history::LaunchContext>,
}
impl JavaRequirement {
    fn matches(&self, java: &JavaInfo) -> bool {
        self.rejection(java).is_none()
    }

    fn rejection(&self, java: &JavaInfo) -> Option<String> {
        if java.arch != self.arch {
            return Some(format!(
                "原生库需要 {}，候选架构是 {}",
                self.arch, java.arch
            ));
        }
        if self.jdk && !java.jdk {
            return Some("此版本的具体组件需要完整 JDK，候选为 JRE".into());
        }
        if let Some((maximum, component)) = &self.bytecode_ceiling
            && java.major > *maximum
        {
            return Some(format!("{component} 最高 Java {maximum}"));
        }
        if let Some(compatibility) = &self.compatibility {
            if let Some(reason) = compatibility.rejection(java.major, &java.version) {
                return Some(reason);
            }
        } else if java.major != self.major {
            // Explicit runtime installation requests remain exact.
            return Some(format!(
                "要求 Java {}，候选为 Java {}",
                self.major, java.major
            ));
        }
        if ["-ea", "-internal"]
            .iter()
            .any(|tag| java.version.to_ascii_lowercase().contains(tag))
        {
            return Some("预览或内部构建不参与稳定版自动匹配".into());
        }
        None
    }

    fn allowed_majors(&self) -> &[u32] {
        self.compatibility
            .as_ref()
            .map_or(std::slice::from_ref(&self.major), |c| c.majors.as_slice())
    }

    fn description(&self) -> String {
        let majors = self.allowed_majors();
        let range = if majors.len() > 1 && majors.windows(2).all(|p| p[1] == p[0] + 1) {
            if majors.last() == Some(&64) {
                format!("{}+", majors[0])
            } else {
                format!("{}–{}", majors[0], majors.last().unwrap())
            }
        } else {
            majors
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        };
        format!(
            "{} {range} ({})",
            if self.jdk { "JDK" } else { "Java" },
            self.arch
        )
    }

    fn select<'a>(
        &self,
        available: &'a [JavaInfo],
        previous: Option<&Path>,
    ) -> Option<&'a JavaInfo> {
        available
            .iter()
            .filter(|java| self.matches(java))
            .max_by_key(|java| {
                (
                    previous.is_some_and(|path| path == Path::new(&java.path)),
                    java.major == self.major,
                    std::cmp::Reverse(java.major.abs_diff(self.major)),
                    version_numbers(&java.version),
                    java.jdk,
                )
            })
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
            jdk: false,
            compatibility: None,
            bytecode_ceiling: crate::java_compatibility::bytecode_ceiling(version),
            working_directory: None,
            history: None,
        }
    }

    pub async fn for_instance(
        version: &crate::model::Version,
        game: &Path,
        explicit: Option<&str>,
    ) -> Result<Self> {
        Self::for_working_directory(
            version,
            &crate::version::working_directory(game, version)?,
            explicit,
        )
        .await
    }
    pub async fn for_working_directory(
        version: &crate::model::Version,
        working: &Path,
        _explicit: Option<&str>,
    ) -> Result<Self> {
        let mut requirement = Self::for_version(version);
        let version = version.clone();
        let working = working.to_owned();
        let assessed_version = version.clone();
        let assessed_working = working.clone();
        let compatibility = tokio::task::spawn_blocking(move || {
            crate::java_compatibility::assess_in_directory(&assessed_version, &assessed_working)
        })
        .await??;
        requirement.jdk = compatibility.requires_jdk;
        requirement.history = Some(history::LaunchContext::new(
            &version,
            &working,
            &compatibility,
        ));
        requirement.working_directory = Some(working);
        requirement.compatibility = Some(compatibility);
        Ok(requirement)
    }
}
pub async fn inspect(path: &Path) -> Result<JavaInfo> {
    let mut command = background_command(path);
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
    let output = decode_output(&output.stderr).into_owned() + &decode_output(&output.stdout);
    parse_probe(path, &output)
}

fn parse_probe(path: &Path, output: &str) -> Result<JavaInfo> {
    let property = |name: &str| {
        output.lines().find_map(|line| {
            let (key, value) = line.trim().split_once(" = ")?;
            (key == name).then_some(value.trim())
        })
    };
    let fallback = regex::Regex::new(r#"(?:version\s+"|openjdk\s+)([0-9][^"\s]*)"#)?;
    let captures = fallback.captures(output);
    let version = property("java.version")
        .or_else(|| captures.as_ref().and_then(|c| c.get(1)).map(|m| m.as_str()))
        .context("无法识别 Java 版本")?;
    let number = crate::java_compatibility::java_version_number(version)
        .with_context(|| format!("Java 版本格式无效：{version} · {}", path.display()))?;
    anyhow::ensure!((5..=64).contains(&number[0]), "Java 版本超出支持范围");
    let version = property("java.runtime.version")
        .filter(|runtime| {
            !version.contains("-ea")
                && !version.contains("-internal")
                && crate::java_compatibility::java_version_number(runtime).as_ref() == Some(&number)
        })
        .unwrap_or(version);
    let arch = match property("os.arch").context("Java 未报告运行时架构")? {
        "amd64" | "x86_64" | "x64" => "x86_64",
        "arm64" | "aarch64" => "aarch64",
        "x86" | "i386" | "i486" | "i586" | "i686" => "x86",
        other => other,
    };
    // java.home resolves launch wrappers and aliases to the actual JVM image.
    let executable = property("java.home")
        .map(PathBuf::from)
        .map(|home| home.join("bin").join(executable_name()))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| path.to_owned());
    let executable = executable.canonicalize().unwrap_or(executable);
    Ok(JavaInfo {
        path: executable.to_string_lossy().into_owned(),
        major: number[0],
        version: version.into(),
        arch: arch.into(),
        jdk: runtime_is_jdk(&executable),
        vendor: property("java.vendor").unwrap_or_default().into(),
    })
}
fn runtime_is_jdk(path: &Path) -> bool {
    let executable = path.canonicalize().unwrap_or_else(|_| path.to_owned());
    let Some(bin) = executable.parent() else {
        return false;
    };
    let javac = if cfg!(windows) { "javac.exe" } else { "javac" };
    bin.join(javac).is_file()
        || bin
            .parent()
            .filter(|home| home.ends_with("jre"))
            .and_then(|jre| jre.parent())
            .is_some_and(|home| home.join("bin").join(javac).is_file())
}

fn executable_name() -> &'static str {
    if cfg!(windows) { "java.exe" } else { "java" }
}
fn find_java(root: &Path) -> Option<PathBuf> {
    discovery::scan(root, 10).into_iter().next()
}
pub async fn discover(root: &Path) -> Vec<JavaInfo> {
    discovery::discover(root, &[], false).await.runtimes
}

#[cfg(test)]
async fn inspect_candidates(candidates: Vec<PathBuf>) -> Vec<JavaInfo> {
    let cache = tempfile::tempdir().unwrap();
    discovery::probe_candidates(cache.path(), candidates)
        .await
        .runtimes
}

// Installation and launch share one polling implementation; keep the large
// discovery/download state machine out of every enclosing install future.
#[inline(never)]
pub fn ensure<'a>(
    root: &'a Path,
    requirement: JavaRequirement,
    explicit: Option<&'a str>,
    auto: bool,
    downloader: &'a Downloader,
    cancel: &'a CancellationToken,
    sink: &'a EventSink,
) -> futures_util::future::BoxFuture<'a, Result<JavaInfo>> {
    Box::pin(async move {
        if let Some(compatibility) = &requirement.compatibility {
            sink(Event::new(
                "info",
                format!("Java 匹配范围：{}", requirement.description()),
            ));
            for reason in &compatibility.reasons {
                sink(Event::new("info", reason.clone()));
            }
        }
        if let Some(path) = explicit.filter(|p| !p.is_empty()) {
            let info = tokio::select! {
                info = inspect(Path::new(path)) => info?,
                _ = cancel.cancelled() => bail!("Java 搜索已取消"),
            };
            if let Some(reason) = requirement.rejection(&info) {
                bail!("所选 Java 不兼容：{reason} · {}", info.path);
            }
            sink(Event::new(
                "info",
                format!("使用指定 Java：{} · {}", info.version, info.path),
            ));
            return Ok(info);
        }
        let previous = requirement
            .history
            .as_ref()
            .and_then(|history| history.previous(root));
        let mut extra: Vec<_> = requirement.working_directory.iter().cloned().collect();
        if let Some(home) = previous
            .as_deref()
            .and_then(Path::parent)
            .and_then(Path::parent)
        {
            extra.push(home.to_owned());
        }
        let mut reported = std::collections::HashSet::new();
        for expanded in [false, true] {
            let available = tokio::select! {
                result = discovery::discover(root, &extra, expanded) => result,
                _ = cancel.cancelled() => bail!("Java 搜索已取消"),
            };
            for failure in available.failures {
                if reported.insert(failure.clone()) {
                    sink(Event::new("warning", failure));
                }
            }
            for candidate in &available.runtimes {
                let reason = requirement.rejection(candidate);
                let detail = format!(
                    "Java 候选：{} · {} · {}",
                    candidate.version,
                    candidate.path,
                    reason
                        .as_ref()
                        .map_or("满足硬性要求".into(), |r| format!("排除：{r}"))
                );
                if reported.insert(detail.clone()) {
                    sink(Event::new("info", detail));
                }
            }
            if let Some(java) = requirement.select(&available.runtimes, previous.as_deref()) {
                let why = if previous.as_deref() == Some(Path::new(&java.path)) {
                    "此前正常运行"
                } else if java.major == requirement.major {
                    "本地推荐版本"
                } else {
                    "满足已知兼容约束"
                };
                sink(Event::new(
                    "info",
                    format!("使用本地 Java：{} · {}（{why}）", java.version, java.path),
                ));
                return Ok(java.clone());
            }
            if !expanded {
                sink(Event::new(
                    "info",
                    "常见位置未找到匹配 Java，正在补查实例附带运行时",
                ));
            }
        }
        if !auto {
            bail!(
                "未找到兼容的 {}，请指定 Java 路径或允许自动下载。",
                requirement.description()
            );
        }
        check_cancel(cancel)?;
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
        // Query only release metadata first. A mod can require a particular patch;
        // validate the real release before downloading or installing its archive.
        let releases: serde_json::Value = downloader
            .json(
                "https://api.adoptium.net/v3/info/available_releases",
                cancel,
            )
            .await?;
        let lts = releases["available_lts_releases"]
            .as_array()
            .context("Java 发行目录缺失")?;
        let mut majors: Vec<u32> = releases["available_releases"]
            .as_array()
            .context("Java 发行目录缺失")?
            .iter()
            .filter_map(|n| n.as_u64().and_then(|n| u32::try_from(n).ok()))
            .filter(|major| requirement.allowed_majors().contains(major))
            .collect();
        crate::sorting::sort_by_cached_key(&mut majors, |major| {
            (
                *major != requirement.major,
                !lts.iter().any(|v| v.as_u64() == Some((*major).into())),
                major.abs_diff(requirement.major),
            )
        });
        let mut selected = None;
        for major in majors {
            let image_type = download_image_type(major, requirement.jdk);
            let url = format!(
                "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?architecture={arch}&image_type={image_type}&os={os}&vendor=eclipse"
            );
            let assets: serde_json::Value = downloader.json(&url, cancel).await?;
            for asset in assets.as_array().context("Java 发行信息格式无效")? {
                let version = asset["version"]["openjdk_version"]
                    .as_str()
                    .or_else(|| asset["version"]["semver"].as_str())
                    .context("Java 发行版本缺失")?;
                let candidate = JavaInfo {
                    path: String::new(),
                    major,
                    version: version.into(),
                    arch: requirement.arch.into(),
                    jdk: image_type == "jdk",
                    vendor: "Eclipse Adoptium".into(),
                };
                if let Some(reason) = requirement.rejection(&candidate) {
                    sink(Event::new(
                        "info",
                        format!("跳过 Java 发行版 {version}：{reason}"),
                    ));
                    continue;
                }
                selected = Some((major, image_type, asset["binary"]["package"].clone()));
                break;
            }
            if selected.is_some() {
                break;
            }
        }
        let (major, image_type, package) = selected
            .context("发行目录没有满足游戏、加载器和模组要求的 Java，请手动指定兼容版本")?;
        sink(Event::new(
            "stage",
            format!(
                "正在安装 {} {major} ({})",
                if image_type == "jdk" { "JDK" } else { "Java" },
                requirement.arch
            ),
        ));
        let link = package["link"]
            .as_str()
            .context("暂无适合此系统的 Java，请指定本地 Java 路径")?;
        let name = package["name"].as_str().context("Java 下载信息缺失")?;
        let archive = root.join("cache/java").join(storage::safe_relative(name)?);
        let archive_spec = FileSpec {
            url: link.into(),
            path: archive.clone(),
            hash: package["checksum"]
                .as_str()
                .context("Java 校验值缺失")?
                .into(),
            sha256: true,
            size: package["size"].as_u64().unwrap_or(0),
        };
        downloader.file(&archive_spec, cancel, sink).await?;
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
        let validation = inspect(&staging.join(&relative)).await.and_then(|java| {
            anyhow::ensure!(java.major == major, "Java 安装包主版本与发行信息不符");
            if let Some(reason) = requirement.rejection(&java) {
                bail!("Java 安装包不满足启动要求：{reason}");
            }
            Ok(())
        });
        if let Err(error) = validation {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(error);
        }
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
        if let Err(error) = downloader.discard_installer(&archive_spec).await {
            sink(Event::new(
                "warning",
                format!("清理 Java 安装包失败：{error:#}"),
            ));
        }
        Ok(java)
    })
}
fn download_image_type(major: u32, needs_jdk: bool) -> &'static str {
    // A modern JDK also serves vanilla games and avoids installing a second JVM
    // when a modded profile later needs full JDK classes/debug information.
    if major >= 9 || needs_jdk {
        "jdk"
    } else {
        "jre"
    }
}

fn version_numbers(version: &str) -> (Vec<u32>, Vec<u32>) {
    let number = crate::java_compatibility::java_version_number(version).unwrap_or_default();
    let value = version.split('"').nth(1).unwrap_or(version);
    let build = value
        .split_once('+')
        .or_else(|| value.split_once("-b"))
        .map(|(_, tail)| tail);
    let build = build
        .map(|build| {
            build
                .split(|c: char| !c.is_ascii_digit())
                .filter_map(|s| s.parse::<u32>().ok())
                .collect()
        })
        .unwrap_or_default();
    (number, build)
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
    #[cfg(unix)]
    #[tokio::test]
    async fn aliases_are_probed_once_and_jdk_detection_uses_the_real_installation() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("Cellar/openjdk/26.0.2.1/bin");
        std::fs::create_dir_all(&bin).unwrap();
        let java = bin.join("java");
        std::fs::write(&java, "#!/bin/sh\nprintf 'openjdk version \"26.0.2.1\"\\n    os.arch = aarch64\\n' >&2\nprintf x >> \"$(dirname \"$0\")/probes\"\n").unwrap();
        std::fs::set_permissions(&java, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(bin.join("javac"), "").unwrap();
        let opt = temp.path().join("opt");
        std::fs::create_dir_all(&opt).unwrap();
        let alias = opt.join("java");
        let second = opt.join("openjdk-java");
        symlink(&java, &alias).unwrap();
        symlink(&java, &second).unwrap();
        assert!(super::inspect(&alias).await.unwrap().jdk);
        let available = super::inspect_candidates(vec![java.clone(), alias, second]).await;
        assert_eq!(available.len(), 1);
        assert_eq!(available[0].major, 26);
        assert!(available[0].jdk);
        // The direct alias inspection above writes in opt; candidate discovery
        // probes the canonical-sorted Cellar path exactly once.
        assert_eq!(std::fs::read(bin.join("probes")).unwrap(), b"x");
        let jre_bin = temp.path().join("another-jre/bin");
        std::fs::create_dir_all(&jre_bin).unwrap();
        std::fs::copy(&java, jre_bin.join("java")).unwrap();
        let distinct = super::inspect_candidates(vec![java, jre_bin.join("java")]).await;
        assert_eq!(
            distinct.len(),
            2,
            "real separate JRE/JDK installations remain selectable"
        );
        assert!(distinct.iter().any(|j| !j.jdk));
    }

    #[test]
    fn new_modern_installations_use_jdk_while_local_jre_remains_reusable() {
        for major in [17, 21, 26] {
            assert_eq!(super::download_image_type(major, false), "jdk");
            assert_eq!(super::download_image_type(major, true), "jdk");
            let requirement = super::JavaRequirement {
                major,
                arch: "aarch64",
                jdk: false,
                compatibility: None,
                bytecode_ceiling: None,
                working_directory: None,
                history: None,
            };
            assert!(requirement.matches(&super::JavaInfo {
                path: "java".into(),
                major,
                version: format!("{major}.0.1"),
                arch: "aarch64".into(),
                jdk: false,
                vendor: String::new(),
            }));
        }
        assert_eq!(super::download_image_type(8, false), "jre");
    }

    #[test]
    fn windowed_java_is_selected_only_from_the_same_runtime_and_keeps_a_fallback() {
        let temp = tempfile::tempdir().unwrap();
        let java = temp.path().join("java.exe");
        let javaw = temp.path().join("javaw.exe");
        std::fs::write(&java, b"console runtime").unwrap();
        assert_eq!(super::game_executable_for(&java, true), java);
        std::fs::write(&javaw, b"windowed runtime").unwrap();
        assert_eq!(super::game_executable_for(&java, true), javaw);
        assert_eq!(super::game_executable_for(&java, false), java);
        let custom = temp.path().join("custom-java.exe");
        assert_eq!(super::game_executable_for(&custom, true), custom);
        assert_eq!(super::game_executable_for(&javaw, true), javaw);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_background_process_has_no_console_and_keeps_output() {
        if std::env::var_os("NEKOCRATE_TEST_NO_CONSOLE").is_some() {
            assert!(unsafe { windows_sys::Win32::System::Console::GetConsoleWindow() }.is_null());
            println!("nekocrate-piped-output");
            return;
        }
        let output = super::background_command(&std::env::current_exe().unwrap())
            .args([
                "--exact",
                "java::tests::windows_background_process_has_no_console_and_keeps_output",
                "--nocapture",
            ])
            .env("NEKOCRATE_TEST_NO_CONSOLE", "1")
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("nekocrate-piped-output"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_jvm_logs_decode_legacy_chinese_without_damaging_utf8() {
        let text = r"E:\游戏\逆转未来\.minecraft";
        let gbk = b"E:\\\xd3\xce\xcf\xb7\\\xc4\xe6\xd7\xaa\xce\xb4\xc0\xb4\\.minecraft";
        assert_eq!(super::decode_windows_output(gbk, 936), text);
        assert_eq!(super::decode_windows_output(text.as_bytes(), 936), text);
        assert_eq!(super::decode_windows_output(b"", 936), "");
        assert_eq!(
            super::decode_windows_output(b"INFO: ready", 936),
            "INFO: ready"
        );
        assert_eq!(
            super::decode_windows_output(b"broken \x81", 936),
            "broken \u{fffd}"
        );
    }

    #[test]
    fn windows_java_paths_preserve_unicode_and_normalize_only_filesystem_prefixes() {
        for (input, expected) in [
            (
                r"\\?\E:\Games\MC\2026.5-逆转未来\猫咪 盒子\.minecraft",
                r"E:\Games\MC\2026.5-逆转未来\猫咪 盒子\.minecraft",
            ),
            (
                r"\\?\E:\逆转未来\.minecraft\libraries/cpw/mods/bootstraplauncher/1.1.2/bootstraplauncher-1.1.2.jar",
                r"E:\逆转未来\.minecraft\libraries/cpw/mods/bootstraplauncher/1.1.2/bootstraplauncher-1.1.2.jar",
            ),
            (
                r"\\?\UNC\服务器\共享目录\MC\.minecraft",
                r"\\服务器\共享目录\MC\.minecraft",
            ),
            (r"E:\Games\普通 路径", r"E:\Games\普通 路径"),
            (r"\\server\share\MC", r"\\server\share\MC"),
            (r"\\.\pipe\example", r"\\.\pipe\example"),
            (r"\\?\Volume{example}\MC", r"\\?\Volume{example}\MC"),
            (
                "/home/player/\\\\?\\E:\\game",
                "/home/player/\\\\?\\E:\\game",
            ),
        ] {
            assert_eq!(super::windows_argument_path(input), expected);
        }
        #[cfg(not(windows))]
        assert_eq!(
            super::argument_path(std::path::Path::new(r"\\?\E:\literal-unix-name")),
            r"\\?\E:\literal-unix-name"
        );
    }

    use super::*;
    #[tokio::test]
    async fn modern_loaders_allow_local_jre_and_manual_selection_uses_the_same_rules() {
        let temp = tempfile::tempdir().unwrap();
        let mut version = crate::test_http::version_fixture("1.20.1");
        version.java_version = Some(crate::model::JavaVersion { major: 17 });
        version.main_class = "cpw.mods.bootstraplauncher.BootstrapLauncher".into();
        let auto = JavaRequirement::for_working_directory(&version, temp.path(), None)
            .await
            .unwrap();
        let manual = JavaRequirement::for_working_directory(&version, temp.path(), Some("java"))
            .await
            .unwrap();
        assert!(!auto.jdk);
        for major in [8, 17, 21, 26] {
            let java = JavaInfo {
                path: "java".into(),
                major,
                version: format!("{major}.0.1"),
                arch: auto.arch.into(),
                jdk: false,
                vendor: String::new(),
            };
            assert_eq!(auto.rejection(&java), manual.rejection(&java));
            assert_eq!(auto.matches(&java), major >= 17);
        }
    }

    #[tokio::test]
    async fn selection_prefers_previous_success_then_recommended_local_then_compatible_local() {
        let temp = tempfile::tempdir().unwrap();
        let mut version = crate::test_http::version_fixture("1.20.1");
        version.java_version = Some(crate::model::JavaVersion { major: 17 });
        let mut requirement = JavaRequirement::for_instance(&version, temp.path(), None)
            .await
            .unwrap();
        let available: Vec<_> = [17, 21, 26]
            .into_iter()
            .map(|major| JavaInfo {
                path: format!("/jdk{major}/bin/java"),
                major,
                version: format!("{major}.0.8"),
                arch: requirement.arch.into(),
                jdk: false,
                vendor: String::new(),
            })
            .collect();
        assert_eq!(requirement.select(&available, None).unwrap().major, 17);
        assert_eq!(requirement.select(&available[1..], None).unwrap().major, 21);
        assert_eq!(
            requirement
                .select(&available, Some(Path::new(&available[2].path)))
                .unwrap()
                .major,
            26
        );
        requirement.bytecode_ceiling = Some((21, "known loader".into()));
        assert_eq!(
            requirement
                .select(&available, Some(Path::new(&available[2].path)))
                .unwrap()
                .major,
            17
        );
        requirement.arch = "impossible-architecture";
        assert!(requirement.select(&available, None).is_none());
    }

    #[test]
    fn probe_parses_properties_unquoted_openjdk_and_legacy_java_versions() {
        let path = Path::new("/nonexistent/java");
        assert_eq!(
            parse_probe(path, "java.version = 21.0.7\nos.arch = amd64\n")
                .unwrap()
                .major,
            21
        );
        assert_eq!(
            parse_probe(path, "openjdk 26.0.2 2026-08-18\nos.arch = arm64\n")
                .unwrap()
                .arch,
            "aarch64"
        );
        assert_eq!(
            parse_probe(path, "java version \"1.8.0_321\"\nos.arch = x86\n")
                .unwrap()
                .major,
            8
        );
        assert!(
            parse_probe(path, "java.version = 21.0.7").is_err(),
            "Do not guess the host architecture when probing an emulated JVM"
        );
    }

    #[test]
    fn probe_reads_reported_vendor_without_guessing_from_the_path() {
        for vendor in [
            "Eclipse Adoptium",
            "Azul Systems, Inc.",
            "Oracle Corporation",
            "Custom JVM",
        ] {
            let info = parse_probe(
                Path::new("/Temurin/bin/java"),
                &format!("java.version = 21.0.7\nos.arch = amd64\njava.vendor = {vendor}\n"),
            )
            .unwrap();
            assert_eq!(info.vendor, vendor);
        }
        let old: JavaInfo = serde_json::from_value(serde_json::json!({
            "path": "/java", "major": 17, "version": "17.0.1", "arch": "aarch64", "jdk": true
        }))
        .unwrap();
        assert!(old.vendor.is_empty());
    }

    #[test]
    fn probe_and_ordering_preserve_extended_release_before_build_number() {
        let java = parse_probe(Path::new("/nonexistent/java"),
            "java.version = 27.0.1.0.2\njava.runtime.version = 27.0.1.0.2+1\nos.arch = aarch64\njava.vendor = Eclipse Adoptium\n").unwrap();
        assert_eq!(java.major, 27);
        assert_eq!(java.version, "27.0.1.0.2+1");
        assert!(version_numbers("27.0.1.0.2+1") > version_numbers("27.0.1+99"));
        assert!(version_numbers("27.0.1.0.3+1") > version_numbers("27.0.1.0.2+99"));
        assert!(version_numbers("27.0.1.0.2+9") > version_numbers("27.0.1.0.2+1"));
        assert_eq!(
            version_numbers("27.0.1.0.2.0+9"),
            version_numbers("27.0.1.0.2+9")
        );
        assert!(version_numbers("1.8.0_504-b02") > version_numbers("1.8.0_504-b01"));
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
        // Auto-download is disabled; local selection must still succeed.
        let cancel = CancellationToken::new();
        let found = ensure(
            temp.path(),
            JavaRequirement {
                major: 17,
                arch: crate::minecraft::arch_name(),
                jdk: true,
                compatibility: None,
                bytecode_ceiling: None,
                working_directory: None,
                history: None,
            },
            None,
            false,
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
        // A wider compatible range does not displace an existing recommended JVM.
        assert_eq!(requirement.allowed_majors().last(), Some(&26));
        let selected = ensure(
            temp.path(),
            requirement,
            None,
            false,
            &downloader,
            &cancel,
            &sink,
        )
        .await
        .unwrap();
        assert_eq!(selected.major, 17);
        assert!(!events.lock().unwrap().iter().any(|e| e.kind == "stage"));
        // Manual selection must not bypass a known bytecode incompatibility.
        let mut value = serde_json::to_value(profile).unwrap();
        value["mainClass"] = "cpw.mods.bootstraplauncher.BootstrapLauncher".into();
        value["libraries"] = serde_json::json!([{"name":"org.ow2.asm:asm:9.7.1"}]);
        let version = crate::version::resolve_chain(&[value]).unwrap().version;
        let error = ensure(
            temp.path(),
            JavaRequirement::for_version(&version),
            Some(newer.join("java").to_str().unwrap()),
            false,
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
