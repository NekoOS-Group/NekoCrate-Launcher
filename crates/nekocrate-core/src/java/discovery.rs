//! Bounded, platform-specific searches. Runtime files owned by other launchers
//! are only read; their configuration and account stores are never opened.
#[cfg(windows)]
use super::background_command;
use super::{JavaInfo, executable_name, inspect};
use crate::storage;
use futures_util::{StreamExt, stream};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::OnceLock,
    time::UNIX_EPOCH,
};

#[derive(Default)]
pub(super) struct Discovery {
    pub runtimes: Vec<JavaInfo>,
    pub failures: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct CachedProbe {
    stamp: String,
    info: JavaInfo,
}

#[derive(Default, Serialize, Deserialize)]
struct ProbeCache {
    schema: u32,
    entries: BTreeMap<String, CachedProbe>,
}

/// Include the runtime image, not just the tiny java executable: replacing
/// lib/modules or javac can change compatibility without changing bin/java.
pub(super) fn fingerprint(path: &Path) -> Option<String> {
    let path = path.canonicalize().ok()?;
    let home = path.parent()?.parent()?;
    let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
    for file in [
        path.clone(),
        home.join("release"),
        home.join("lib/modules"),
        home.join("lib/rt.jar"),
        home.join("jre/lib/rt.jar"),
        home.join(if cfg!(windows) {
            "bin/javac.exe"
        } else {
            "bin/javac"
        }),
    ] {
        digest.update(file.as_os_str().as_encoded_bytes());
        match file.metadata() {
            Ok(meta) => {
                digest.update(&meta.len().to_le_bytes());
                digest.update(
                    &meta
                        .modified()
                        .ok()?
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos()
                        .to_le_bytes(),
                );
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    digest.update(&meta.mode().to_le_bytes());
                    digest.update(&meta.ctime().to_le_bytes());
                    digest.update(&meta.ctime_nsec().to_le_bytes());
                }
                if file.file_name().is_some_and(|n| n == "release") && meta.len() < 64 * 1024 {
                    digest.update(&std::fs::read(file).ok()?);
                }
            }
            Err(_) => digest.update(b"missing"),
        }
    }
    Some(hex::encode(digest.finish()))
}

pub(super) async fn probe_candidates(root: &Path, mut candidates: Vec<PathBuf>) -> Discovery {
    // Refresh and launch may request discovery concurrently. Share the cache and
    // avoid racing writes or launching duplicate JVM probes.
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    let _guard = LOCK.get_or_init(Default::default).lock().await;
    for path in &mut candidates {
        *path = path.canonicalize().unwrap_or_else(|_| path.clone());
    }
    candidates.sort();
    candidates.dedup();
    let cache_path = root.join("metadata/java-probes.json");
    let mut cache: ProbeCache = storage::read_json(&cache_path).unwrap_or_default();
    if cache.schema != 1 {
        cache.entries.clear();
        cache.schema = 1;
    }
    let mut found = Discovery::default();
    let mut pending = Vec::new();
    for path in candidates {
        let key = path
            .canonicalize()
            .unwrap_or_else(|_| path.clone())
            .to_string_lossy()
            .into_owned();
        let stamp = fingerprint(&path);
        if let Some(entry) = cache.entries.get(&key)
            && stamp.as_ref() == Some(&entry.stamp)
            && Path::new(&entry.info.path).is_file()
        {
            found.runtimes.push(entry.info.clone());
        } else {
            cache.entries.remove(&key);
            pending.push((path, key, stamp));
        }
    }
    let mut probes = stream::iter(pending.into_iter().map(|(path, key, stamp)| async move {
        let result = inspect(&path).await;
        (path, key, stamp, result)
    }))
    .buffer_unordered(4);
    while let Some((path, key, stamp, result)) = probes.next().await {
        match result {
            Ok(info) => {
                // If the image was being replaced while probing, do not cache it.
                if let Some(stamp) = stamp.filter(|s| fingerprint(&path).as_ref() == Some(s))
                    && path.canonicalize().ok().as_deref() == Some(Path::new(&info.path))
                {
                    cache.entries.insert(
                        key,
                        CachedProbe {
                            stamp,
                            info: info.clone(),
                        },
                    );
                }
                found.runtimes.push(info);
            }
            Err(error) => found
                .failures
                .push(format!("Java 探测失败：{} · {error:#}", path.display())),
        }
    }
    crate::sorting::sort_by(&mut found.runtimes, |a, b| a.path.cmp(&b.path));
    found.runtimes.dedup_by(|a, b| a.path == b.path);
    found.failures.sort();
    cache.entries.retain(|path, _| Path::new(path).is_file());
    // Cache only metadata, and keep it bounded even after many portable moves.
    while cache.entries.len() > 512 {
        cache.entries.pop_first();
    }
    let bytes = serde_json::to_vec(&cache).unwrap_or_default();
    if std::fs::read(&cache_path).ok().as_deref() != Some(&bytes)
        && let Err(error) = storage::atomic_write(&cache_path, &bytes)
    {
        found
            .failures
            .push(format!("无法保存 Java 探测索引：{error:#}"));
    }
    found
}

pub(super) async fn discover(root: &Path, extra: &[PathBuf], expanded: bool) -> Discovery {
    let owned_root = root.to_owned();
    let extra = extra.to_vec();
    let collected =
        tokio::task::spawn_blocking(move || collect(&owned_root, &extra, expanded)).await;
    let candidates = match collected {
        Ok(paths) => paths,
        Err(error) => {
            return Discovery {
                failures: vec![format!("Java 搜索失败：{error}")],
                ..Default::default()
            };
        }
    };
    let mut registry_failures = Vec::new();
    #[cfg(windows)]
    let candidates = {
        let mut candidates = candidates;
        registry_candidates(&mut candidates, &mut registry_failures).await;
        candidates
    };
    let mut result = probe_candidates(root, candidates).await;
    result.failures.append(&mut registry_failures);
    result
}

fn collect(root: &Path, extra: &[PathBuf], expanded: bool) -> Vec<PathBuf> {
    let mut direct = crate::resource_manager::registered_java(root);
    for name in ["JAVA_HOME", "JDK_HOME"] {
        if let Some(home) = std::env::var_os(name) {
            direct.push(PathBuf::from(home).join("bin").join(executable_name()));
        }
    }
    if let Some(paths) = std::env::var_os("PATH") {
        direct.extend(
            std::env::split_paths(&paths)
                .map(|p| p.join(executable_name()))
                .filter(|p| p.is_file()),
        );
    }
    let home = directories::BaseDirs::new()
        .map(|d| d.home_dir().to_owned())
        .unwrap_or_default();
    let mut roots = known_roots(std::env::consts::OS, &home, |name| {
        std::env::var_os(name).map(PathBuf::from)
    });
    roots.push(root.join("runtimes"));
    // A portable build can reuse installed runtimes without sharing its database.
    if let Some(installed) = directories::ProjectDirs::from("dev", "NekoCrate", "NekoCrate") {
        roots.push(installed.data_local_dir().join("runtimes"));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Ok(package) = crate::portable::package_directory(&exe, std::env::consts::OS)
    {
        add_adjacent_roots(&package, &mut roots);
        add_adjacent_roots(&package.join(".minecraft"), &mut roots);
    }
    for working in extra {
        add_adjacent_roots(working, &mut roots);
        if let Some(game) = working
            .ancestors()
            .find(|p| p.file_name().is_some_and(|n| n == ".minecraft"))
        {
            add_adjacent_roots(game, &mut roots);
            if let Some(parent) = game.parent() {
                add_adjacent_roots(parent, &mut roots);
            }
        }
        if expanded {
            roots.push(working.clone());
        }
    }
    let mut seen = HashSet::new();
    for root in roots {
        if !root.is_dir() || !seen.insert(root.canonicalize().unwrap_or_else(|_| root.clone())) {
            continue;
        }
        direct.extend(scan(&root, if expanded { 12 } else { 10 }));
    }
    direct
}

fn add_adjacent_roots(base: &Path, roots: &mut Vec<PathBuf>) {
    for name in ["runtime", "runtimes", "java", "jdk", "jre"] {
        roots.push(base.join(name));
    }
    if base.join("bin").join(executable_name()).is_file() {
        roots.push(base.to_owned());
    }
    if let Ok(children) = std::fs::read_dir(base) {
        roots.extend(children.flatten().map(|e| e.path()).filter(|p| {
            p.file_name().is_some_and(|n| {
                let n = n.to_string_lossy().to_ascii_lowercase();
                ["jdk", "jre", "java-", "openjdk", "temurin", "zulu"]
                    .iter()
                    .any(|prefix| n.starts_with(prefix))
            })
        }));
    }
}

pub(super) fn scan(root: &Path, depth: usize) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .follow_links(true)
        .max_depth(depth)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            e.depth() == 0
                || (!name.starts_with(".install-")
                    && ![
                        "lib",
                        "jmods",
                        "include",
                        "legal",
                        "man",
                        ".git",
                        "node_modules",
                        "mods",
                        "assets",
                        "libraries",
                        "saves",
                        "resourcepacks",
                        "shaderpacks",
                        "logs",
                        "cache",
                        "versions",
                    ]
                    .contains(&name.as_ref()))
        })
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_type().is_file()
                && e.file_name() == executable_name()
                && e.path().parent().is_some_and(|p| p.ends_with("bin"))
        })
        .map(|e| e.into_path())
        .collect()
}

fn known_roots(os: &str, home: &Path, env: impl Fn(&str) -> Option<PathBuf>) -> Vec<PathBuf> {
    let mut roots = vec![
        home.join(".jdks"),
        home.join(".sdkman/candidates/java"),
        home.join(".hmcl/java"),
        home.join(".minecraft/runtime"),
    ];
    match os {
        "windows" => {
            let roaming = env("APPDATA").unwrap_or_else(|| home.join("AppData/Roaming"));
            let local = env("LOCALAPPDATA").unwrap_or_else(|| home.join("AppData/Local"));
            for relative in [
                ".minecraft/runtime",
                ".hmcl/java",
                "PrismLauncher/java",
                "ModrinthApp/meta/java_versions",
                "ATLauncher/runtimes/minecraft",
            ] {
                roots.push(roaming.join(relative));
            }
            for relative in [
                ".ftba/bin/runtime",
                "Programs/Eclipse Adoptium",
                "Programs/Java",
                "Packages/Microsoft.4297127D64EC6_8wekyb3d8bbwe/LocalCache/Local/runtime",
            ] {
                roots.push(local.join(relative));
            }
            roots.push(home.join("curseforge/minecraft/Install/runtime"));
            roots.push(home.join("Documents/Curse/Minecraft/Install/runtime"));
            for base in [
                env("ProgramFiles").unwrap_or_else(|| "C:/Program Files".into()),
                env("ProgramFiles(x86)").unwrap_or_else(|| "C:/Program Files (x86)".into()),
            ] {
                for vendor in [
                    "Java",
                    "Eclipse Adoptium",
                    "Eclipse Foundation",
                    "AdoptOpenJDK",
                    "Microsoft",
                    "Amazon Corretto",
                    "Zulu",
                    "BellSoft",
                    "Semeru",
                    "Minecraft Launcher/runtime",
                    "Minecraft/runtime",
                ] {
                    roots.push(base.join(vendor));
                }
            }
        }
        "macos" => {
            roots.extend([
                PathBuf::from("/Library/Java/JavaVirtualMachines"),
                home.join("Library/Java/JavaVirtualMachines"),
            ]);
            for base in ["/opt/homebrew/opt", "/usr/local/opt"] {
                add_adjacent_roots(Path::new(base), &mut roots);
            }
            for relative in [
                "minecraft/runtime",
                "hmcl/java",
                "PrismLauncher/java",
                "com.modrinth.theseus/meta/java_versions",
                "ModrinthApp/meta/java_versions",
                "ATLauncher/runtimes/minecraft",
                "ftb-app/bin/runtime",
            ] {
                roots.push(home.join("Library/Application Support").join(relative));
            }
        }
        "linux" => {
            roots.extend(
                [
                    "/usr/java",
                    "/usr/lib/jvm",
                    "/usr/lib32/jvm",
                    "/usr/lib64/jvm",
                    "/opt/java",
                    "/opt/jdk",
                ]
                .map(PathBuf::from),
            );
            let data = env("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"));
            for relative in [
                "PrismLauncher/java",
                "modrinth-app/meta/java_versions",
                "com.modrinth.theseus/meta/java_versions",
                "ATLauncher/runtimes/minecraft",
                "hmcl/java",
            ] {
                roots.push(data.join(relative));
            }
            roots.push(
                home.join(".var/app/org.prismlauncher.PrismLauncher/data/PrismLauncher/java"),
            );
        }
        _ => (),
    }
    roots
}

#[cfg(windows)]
async fn registry_candidates(paths: &mut Vec<PathBuf>, failures: &mut Vec<String>) {
    // No shell and CREATE_NO_WINDOW: probing must never flash a console window.
    // Both registry views matter even when this launcher itself is 64-bit.
    let reg = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| "C:/Windows".into())
        .join("System32/reg.exe");
    for hive in ["HKLM", "HKCU"] {
        for view in ["/reg:64", "/reg:32"] {
            let mut command = background_command(&reg);
            command
                .args(["query", &format!("{hive}\\SOFTWARE\\JavaSoft"), "/s", view])
                .kill_on_drop(true);
            match tokio::time::timeout(std::time::Duration::from_secs(3), command.output()).await {
                Ok(Ok(output)) if output.status.success() => {
                    paths.extend(
                        registry_homes(&super::decode_output(&output.stdout))
                            .into_iter()
                            .map(|p| p.join("bin/java.exe")),
                    );
                }
                Ok(Ok(_)) => (), // The registry key need not exist.
                Ok(Err(error)) => failures.push(format!("读取 Java 注册表失败：{error}")),
                Err(_) => failures.push("读取 Java 注册表超时".into()),
            }
        }
    }
}

#[cfg(any(windows, test))]
fn registry_homes(output: &str) -> Vec<PathBuf> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if !line
                .split_whitespace()
                .next()?
                .eq_ignore_ascii_case("JavaHome")
            {
                return None;
            }
            let (_, path) = line.split_once("REG_SZ")?;
            Some(PathBuf::from(path.trim()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_locations_include_official_vendor_and_other_launcher_runtimes() {
        let home = Path::new("/test-user");
        let windows = known_roots("windows", home, |key| {
            (key == "APPDATA").then(|| "/roaming".into())
        });
        for suffix in [
            ".minecraft/runtime",
            "PrismLauncher/java",
            "ModrinthApp/meta/java_versions",
        ] {
            assert!(windows.contains(&Path::new("/roaming").join(suffix)));
        }
        assert!(windows.iter().any(|p| p.ends_with("Eclipse Adoptium")));
        let mac = known_roots("macos", home, |_| None);
        assert!(mac.contains(&home.join("Library/Application Support/minecraft/runtime")));
        assert!(mac.contains(&home.join(".jdks")));
        let linux = known_roots("linux", home, |key| {
            (key == "XDG_DATA_HOME").then(|| "/data".into())
        });
        assert!(linux.contains(&PathBuf::from("/usr/lib/jvm")));
        assert!(linux.contains(&home.join(".sdkman/candidates/java")));
        assert!(linux.contains(&PathBuf::from("/data/PrismLauncher/java")));
        assert_eq!(
            registry_homes(
                "JavaHome    REG_SZ    C:\\Program Files\\Java\\jdk-21\nRuntimeLib REG_SZ ignored"
            ),
            vec![PathBuf::from(r"C:\Program Files\Java\jdk-21")]
        );
    }

    #[cfg(unix)]
    #[test]
    fn scan_finds_all_images_and_links_but_skips_unfinished_installations_and_game_data() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        for folder in [
            "jdk17/bin",
            "jdk21/Contents/Home/bin",
            ".install-incomplete/bin",
            "mods/embedded/bin",
        ] {
            std::fs::create_dir_all(temp.path().join(folder)).unwrap();
            std::fs::write(temp.path().join(folder).join("java"), "java").unwrap();
        }
        symlink(temp.path().join("jdk21"), temp.path().join("alias")).unwrap();
        symlink(temp.path(), temp.path().join("jdk17/loop")).unwrap();
        let paths = scan(temp.path(), 10);
        let identities: HashSet<_> = paths
            .into_iter()
            .map(|p| p.canonicalize().unwrap())
            .collect();
        assert_eq!(identities.len(), 2);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cached_probes_deduplicate_aliases_and_reprobe_changed_runtime_images() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("image");
        let bin = home.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let java = bin.join("java");
        std::fs::write(&java, "#!/bin/sh\nprintf 'java.version = 21.0.7\\nos.arch = aarch64\\n'\nprintf x >> \"$(dirname \"$0\")/probes\"\n").unwrap();
        std::fs::set_permissions(&java, std::fs::Permissions::from_mode(0o755)).unwrap();
        let release = home.join("release");
        std::fs::write(&release, "first").unwrap();
        let alias = temp.path().join("alias");
        symlink(&java, &alias).unwrap();
        let candidates = vec![java.clone(), alias];
        let cache = temp.path().join("cache");
        let first = probe_candidates(&cache, candidates.clone()).await;
        assert_eq!(first.runtimes.len(), 1);
        assert!(first.failures.is_empty());
        assert!(!first.runtimes[0].jdk);
        probe_candidates(&cache, candidates.clone()).await;
        assert_eq!(std::fs::read(bin.join("probes")).unwrap(), b"x");
        std::fs::write(&release, "changed release").unwrap();
        std::fs::write(bin.join("javac"), "compiler").unwrap();
        assert!(probe_candidates(&cache, candidates.clone()).await.runtimes[0].jdk);
        assert_eq!(std::fs::read(bin.join("probes")).unwrap(), b"xx");
        std::fs::write(&java, "#!/bin/sh\nexit 1\n").unwrap();
        let broken = probe_candidates(&cache, candidates).await;
        assert!(broken.runtimes.is_empty());
        assert_eq!(broken.failures.len(), 1);
        assert!(broken.failures[0].contains("无法运行"));
    }
}
