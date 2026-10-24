//! Heap presets are estimates, not guarantees about a modpack's runtime needs.
//! Keep this policy in the core so CLI launches and UI previews use the same rules.
use crate::{
    model::{Instance, MemoryMode},
    storage, version,
};
use anyhow::{Result, bail};
use serde::Serialize;
use serde_json::Value;
use std::{fs, io::Read, path::Path};

#[derive(Clone, Copy, Debug)]
pub struct MemoryConfig {
    pub mode: MemoryMode,
    pub custom_mb: u32,
}
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct SystemMemory {
    pub total_mb: u64,
    pub available_mb: u64,
}
impl SystemMemory {
    pub fn read() -> Self {
        let mut system = sysinfo::System::new();
        system.refresh_memory_specifics(sysinfo::MemoryRefreshKind::nothing().with_ram());
        Self {
            total_mb: system.total_memory() / 1024 / 1024,
            available_mb: system.available_memory() / 1024 / 1024,
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct MemoryPresets {
    pub minimum_mb: u32,
    pub recommended_mb: u32,
    pub declared_minimum_mb: Option<u32>,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct MemoryOptions {
    pub auto_mb: u32,
    #[serde(flatten)]
    pub presets: MemoryPresets,
    #[serde(flatten)]
    pub system: SystemMemory,
}

/// HMCL's available-memory curve, with a 512 MiB lower bound and deterministic
/// fallback when the OS cannot report RAM. Sample again immediately before launch.
/// https://github.com/HMCL-dev/HMCL/blob/main/HMCL/src/main/java/org/jackhuang/hmcl/game/HMCLGameRepository.java
pub fn automatic(system: SystemMemory, fallback: u32) -> u32 {
    if system.total_mb == 0 {
        return fallback.min(4096);
    }
    let usable = system.available_mb.min(system.total_mb).saturating_sub(512);
    let budget = if usable <= 8192 {
        usable * 4 / 5
    } else {
        (8192 * 4 + (usable - 8192)) / 5
    };
    budget.clamp(512, 16384) as u32
}

fn baseline(game: &str, modded: bool, mods: usize) -> MemoryPresets {
    let old = game
        .strip_prefix("1.")
        .and_then(|tail| tail.split('.').next()?.parse::<u32>().ok())
        .is_some_and(|minor| minor <= 12);
    let (minimum_mb, recommended_mb) = match (modded || mods > 0, mods, old) {
        (true, 400.., _) => (8192, 12288),
        (true, 200.., _) => (6144, 8192),
        (true, 100.., _) => (4096, 6144),
        (true, _, true) => (2048, 4096),
        (true, _, false) => (3072, 6144),
        (false, _, true) => (1024, 2048),
        _ => (2048, 4096),
    };
    MemoryPresets {
        minimum_mb,
        recommended_mb,
        declared_minimum_mb: None,
    }
}

fn metadata(path: &Path) -> Option<Value> {
    // Read only known small metadata files, not archives or mod contents.
    let file = fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 4 * 1024 * 1024 {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

pub fn presets(root: &Path, instance: &Instance) -> MemoryPresets {
    let game = instance.game_dir(root);
    let selected = instance.selected();
    let resolved = selected.and_then(|s| version::resolve(root, &game, &s.id).ok());
    let game_version = resolved
        .as_ref()
        .map(|r| r.minecraft_version.as_str())
        .or_else(|| selected.and_then(|s| s.minecraft_version.as_deref()))
        .or_else(|| selected.map(|s| s.id.as_str()))
        .unwrap_or("");
    let modded = resolved.as_ref().is_some_and(|r| r.loader.is_some())
        || selected.is_some_and(|s| s.loader.is_some());
    let working = resolved
        .as_ref()
        .and_then(|r| version::working_directory(&game, &r.version).ok())
        .unwrap_or_else(|| game.clone());
    let mods = if modded {
        fs::read_dir(working.join("mods"))
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
                    && entry.path().is_file()
            })
            .take(400)
            .count()
    } else {
        0
    };
    let mut result = baseline(game_version, modded, mods);
    let mut locations = vec![working.clone()];
    if let Some(selected) = selected.filter(|s| storage::safe_version(&s.id).is_ok()) {
        let profile = game.join("versions").join(&selected.id);
        if profile != working {
            locations.push(profile);
        }
    }
    // MCBBS pack manifests (including HMCL's retained manifest) declare minMemory
    // in MiB. Other launchers' personal Xmx/Xms settings are not pack requirements.
    for directory in locations {
        for name in ["mcbbs.packmeta", "modpack.cfg"] {
            let Some(value) = metadata(&directory.join(name)) else {
                continue;
            };
            let declaration = value
                .pointer("/launchInfo/minMemory")
                .or_else(|| value.pointer("/manifest/launchInfo/minMemory"))
                .and_then(Value::as_u64)
                .filter(|n| (512..=65536).contains(n));
            if let Some(minimum) = declaration {
                result.declared_minimum_mb =
                    Some(result.declared_minimum_mb.unwrap_or(0).max(minimum as u32));
                result.minimum_mb = result.minimum_mb.max(minimum as u32);
                result.recommended_mb = result.recommended_mb.max(result.minimum_mb);
            }
        }
    }
    result
}

pub fn options(root: &Path, instance: &Instance) -> MemoryOptions {
    let presets = presets(root, instance);
    let system = SystemMemory::read();
    MemoryOptions {
        auto_mb: automatic(system, presets.recommended_mb),
        presets,
        system,
    }
}

impl MemoryOptions {
    pub fn allocation(&self, config: MemoryConfig, java_arch: &str) -> Result<u32> {
        let requested = match config.mode {
            MemoryMode::Auto => self.auto_mb,
            MemoryMode::Recommended => self.presets.recommended_mb,
            MemoryMode::Minimum => self.presets.minimum_mb,
            MemoryMode::Custom => config.custom_mb,
        };
        if !(512..=65536).contains(&requested) {
            bail!("内存应为 512–65536 MiB");
        }
        // Xmx reserves address space; do not silently reduce a manual or pack preset.
        // A 32-bit VM cannot honour most modern packs' estimates.
        if matches!(java_arch, "x86" | "i386" | "i686" | "arm") {
            if config.mode == MemoryMode::Auto {
                return Ok(requested.min(768));
            }
            if requested > 1536 {
                bail!("所选内存超过 32 位 Java 的可用范围，请选择 64 位 Java 或自动内存");
            }
        }
        Ok(requested)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Launcher, minecraft,
        model::{Argument, InstanceVersion, ModLoader},
    };
    use serde_json::json;

    #[test]
    fn automatic_curve_boundaries_fallback_and_java_bitness() {
        let system = |available_mb| SystemMemory {
            total_mb: 65536,
            available_mb,
        };
        assert_eq!(automatic(system(8192), 4096), 6144);
        assert_eq!(automatic(system(8704), 4096), 6553);
        assert_eq!(automatic(system(16384), 4096), 8089);
        assert_eq!(automatic(system(65536), 4096), 16384);
        assert_eq!(automatic(system(0), 4096), 512);
        assert_eq!(automatic(system(256), 4096), 512);
        assert_eq!(automatic(SystemMemory::default(), 8192), 4096);
        let options = MemoryOptions {
            auto_mb: 8089,
            presets: baseline("1.20.1", true, 220),
            system: system(16384),
        };
        for (mode, expected) in [
            (MemoryMode::Auto, 8089),
            (MemoryMode::Recommended, 8192),
            (MemoryMode::Minimum, 6144),
            (MemoryMode::Custom, 5120),
        ] {
            assert_eq!(
                options
                    .allocation(
                        MemoryConfig {
                            mode,
                            custom_mb: 5120
                        },
                        "aarch64"
                    )
                    .unwrap(),
                expected
            );
        }
        assert_eq!(
            options
                .allocation(
                    MemoryConfig {
                        mode: MemoryMode::Auto,
                        custom_mb: 5120
                    },
                    "x86"
                )
                .unwrap(),
            768
        );
        assert!(
            options
                .allocation(
                    MemoryConfig {
                        mode: MemoryMode::Custom,
                        custom_mb: 5120
                    },
                    "x86"
                )
                .is_err()
        );
    }

    #[tokio::test]
    async fn default_and_legacy_instances_use_auto_and_modes_survive_copy_and_reopen() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let launcher = Launcher::open(Some(root.clone())).unwrap();
        let instance = launcher.create_instance("Memory", "").unwrap();
        assert_eq!(instance.memory_mode, MemoryMode::Auto);
        let mut legacy = serde_json::to_value(&instance).unwrap();
        legacy.as_object_mut().unwrap().remove("memory_mode");
        legacy["memory_mb"] = json!(5120);
        let legacy: Instance = serde_json::from_value(legacy).unwrap();
        assert_eq!(legacy.memory_mode, MemoryMode::Auto);
        assert_eq!(legacy.memory_mb, 5120);
        for mode in [
            MemoryMode::Custom,
            MemoryMode::Minimum,
            MemoryMode::Recommended,
            MemoryMode::Auto,
        ] {
            launcher
                .update_instance_memory(
                    &instance.id,
                    "Memory",
                    MemoryConfig {
                        mode,
                        custom_mb: 5120,
                    },
                    None,
                    None,
                )
                .unwrap();
            assert_eq!(launcher.instance(&instance.id).unwrap().memory_mode, mode);
        }
        let copied = launcher.clone_instance(&instance.id).await.unwrap();
        assert_eq!(copied.memory_mode, MemoryMode::Auto);
        assert_eq!(copied.memory_mb, 5120);
        drop(launcher);
        let launcher = Launcher::open(Some(root)).unwrap();
        assert_eq!(
            launcher.instance(&instance.id).unwrap().memory_mode,
            MemoryMode::Auto
        );
        assert_eq!(launcher.instance(&instance.id).unwrap().memory_mb, 5120);
        launcher
            .update_instance_memory(
                &instance.id,
                "Memory",
                MemoryConfig {
                    mode: MemoryMode::Custom,
                    custom_mb: 6144,
                },
                None,
                None,
            )
            .unwrap();
        assert_eq!(launcher.instance(&instance.id).unwrap().memory_mb, 6144);
    }

    #[test]
    fn presets_count_only_active_mods_in_the_actual_version_directory_and_read_pack_declarations() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("data"))).unwrap();
        let mut instance = launcher.create_instance("Pack", "pack").unwrap();
        instance.versions = vec![InstanceVersion {
            id: "pack".into(),
            minecraft_version: Some("1.20.1".into()),
            loader: Some(ModLoader {
                kind: "forge".into(),
                version: "47".into(),
            }),
            ..Default::default()
        }];
        let game = instance.game_dir(launcher.root());
        let isolated = game.join("versions/pack");
        fs::create_dir_all(isolated.join("mods")).unwrap();
        fs::create_dir_all(game.join("mods")).unwrap();
        for n in 0..210 {
            fs::write(isolated.join(format!("mods/{n}.jar")), "").unwrap();
        }
        for n in 0..400 {
            fs::write(game.join(format!("mods/{n}.jar")), "").unwrap();
        }
        fs::write(isolated.join("mods/disabled.jar.disabled"), "").unwrap();
        let v = crate::test_http::version_fixture("pack");
        storage::write_json(&isolated.join("pack.json"), &v).unwrap();
        let estimate = presets(launcher.root(), &instance);
        assert_eq!((estimate.minimum_mb, estimate.recommended_mb), (6144, 8192));
        storage::write_json(
            &isolated.join("modpack.cfg"),
            &json!({"manifest":{"launchInfo":{"minMemory":10240}}}),
        )
        .unwrap();
        let declared = presets(launcher.root(), &instance);
        assert_eq!(
            (
                declared.minimum_mb,
                declared.recommended_mb,
                declared.declared_minimum_mb
            ),
            (10240, 10240, Some(10240))
        );
        storage::write_json(
            &isolated.join("modpack.cfg"),
            &json!({"manifest":{"launchInfo":{"minMemory":u64::MAX}}}),
        )
        .unwrap();
        assert_eq!(
            presets(launcher.root(), &instance).declared_minimum_mb,
            None
        );
        assert_eq!(
            (
                baseline("1.12.2", false, 0).minimum_mb,
                baseline("1.12.2", false, 0).recommended_mb
            ),
            (1024, 2048)
        );
    }

    #[test]
    fn selected_heap_replaces_conflicting_imported_flags_without_changing_other_arguments() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("data"))).unwrap();
        let instance = launcher.create_instance("Memory", "1.21.1").unwrap();
        let mut v = crate::test_http::version_fixture("1.21.1");
        let jvm = &mut v.arguments.as_mut().unwrap().jvm;
        jvm.extend(
            [
                "-Xmx99G",
                "-Xms16G",
                "-XX:MaxHeapSize=999999999",
                "-XX:InitialHeapSize=999999999",
                "-Dpack.example=keep",
            ]
            .map(|s| Argument::Plain(s.into())),
        );
        let args = minecraft::arguments_with_memory(
            &v,
            launcher.root(),
            &instance,
            &minecraft::LaunchSession::guest(),
            6144,
        )
        .unwrap();
        assert_eq!(
            args.iter()
                .filter(|arg| arg.starts_with("-Xmx"))
                .collect::<Vec<_>>(),
            vec!["-Xmx6144M"]
        );
        assert_eq!(
            args.iter()
                .filter(|arg| arg.starts_with("-Xms"))
                .collect::<Vec<_>>(),
            vec!["-Xms256M"]
        );
        assert!(
            !args.iter().any(|arg| arg.starts_with("-XX:MaxHeapSize=")
                || arg.starts_with("-XX:InitialHeapSize="))
        );
        assert!(args.iter().any(|arg| arg == "-Dpack.example=keep"));
    }
}
