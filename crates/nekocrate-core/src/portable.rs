//! Desktop releases default to portable mode; installed bundles carry a mode file.
//! Only launcher-owned state uses the path codec; Minecraft metadata stays native.
use crate::storage;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const MARKER: &str = "nekocrate.portable.json";
pub const RUNTIME_CONFIG: &str = "nekocrate.runtime.json";
pub const DATA_DIRECTORY: &str = ".nekocrate";
const PREFIX: &str = "nekocrate-portable:/";

#[derive(Clone, Serialize, Deserialize)]
pub struct Identity {
    pub schema: u32,
    pub id: String,
    #[serde(default)]
    pub game_linked: bool,
}

pub fn executable() -> Result<PathBuf> {
    Ok(std::env::current_exe()?)
}

static PORTABLE_DEFAULT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Called by the release desktop entry point before opening any data.
/// CLI and development builds retain their existing system-directory default.
pub fn enable() {
    PORTABLE_DEFAULT.store(true, std::sync::atomic::Ordering::Relaxed);
}

pub fn package_directory(executable: &Path, platform: &str) -> Result<PathBuf> {
    let executable = if platform == "macos" {
        executable
            .ancestors()
            .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app")))
            .unwrap_or(executable)
    } else {
        executable
    };
    Ok(executable
        .parent()
        .context("无法确定启动器所在目录")?
        .to_owned())
}

pub fn detect(executable: &Path, platform: &str) -> Result<Option<PathBuf>> {
    detect_mode(
        executable,
        platform,
        PORTABLE_DEFAULT.load(std::sync::atomic::Ordering::Relaxed),
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Installed,
    Portable,
}

#[derive(Deserialize)]
struct RuntimeConfig {
    schema: u32,
    mode: Mode,
}

fn configured_mode(executable: &Path, platform: &str) -> Result<Option<bool>> {
    let system_root = if platform == "macos" {
        storage::system_data_root()?
    } else {
        PathBuf::new()
    };
    configured_mode_in(executable, platform, &system_root)
}

fn configured_mode_in(
    executable: &Path,
    platform: &str,
    system_root: &Path,
) -> Result<Option<bool>> {
    let directory = executable.parent().context("无法确定启动器所在目录")?;
    let mut paths = vec![directory.join(RUNTIME_CONFIG)];
    if platform == "macos" {
        if let Some(bundle) = executable
            .ancestors()
            .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app")))
        {
            paths.push(bundle.join("Contents/Resources").join(RUNTIME_CONFIG));
        }
    } else if platform == "linux" && directory.file_name().is_some_and(|name| name == "bin") {
        // Only inspect the package next to THIS executable. A global /usr/lib
        // fallback would make a portable copy inherit another installation's mode.
        paths.push(
            directory
                .join("../lib/nekocrate-launcher")
                .join(RUNTIME_CONFIG),
        );
    }
    for path in paths {
        if path.try_exists()? {
            let config: RuntimeConfig =
                storage::read_json(&path).context("启动器运行模式配置无效")?;
            ensure!(config.schema == 1, "不支持的启动器运行模式配置版本");
            return Ok(Some(matches!(config.mode, Mode::Portable)));
        }
    }
    if platform == "macos" {
        return remembered_macos_mode(executable, system_root);
    }
    Ok(None)
}

#[derive(Serialize, Deserialize)]
struct InstalledApplication {
    schema: u32,
    bundle: PathBuf,
}

fn macos_bundle(executable: &Path) -> Option<&Path> {
    executable.ancestors().find(|path| {
        path.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("app"))
    })
}

fn macos_mode_record(bundle: &Path, system_root: &Path) -> PathBuf {
    let hash = ring::digest::digest(&ring::digest::SHA256, bundle.as_os_str().as_encoded_bytes());
    system_root
        .join("runtime-modes")
        .join(format!("{}.json", hex::encode(hash.as_ref())))
}

fn remembered_macos_mode(executable: &Path, system_root: &Path) -> Result<Option<bool>> {
    let Some(bundle) = macos_bundle(executable) else {
        return Ok(None);
    };
    let bundle = match bundle.canonicalize() {
        Ok(bundle) => bundle,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let path = macos_mode_record(&bundle, system_root);
    if !path.try_exists()? {
        return Ok(None);
    }
    let record: InstalledApplication =
        storage::read_json(&path).context("启动器运行模式记录无效")?;
    ensure!(
        record.schema == 1 && record.bundle == bundle,
        "启动器运行模式记录不匹配"
    );
    Ok(Some(false))
}

/// The common macOS archive has no installed-mode resource. Save the current
/// installed app's exact location outside the signed bundle before replacement.
/// Portable updates never call this, and unrelated copies keep portable defaults.
pub fn preserve_macos_installation(executable: &Path) -> Result<()> {
    preserve_macos_installation_in(executable, &storage::system_data_root()?)
}

fn preserve_macos_installation_in(executable: &Path, system_root: &Path) -> Result<()> {
    let bundle = macos_bundle(executable)
        .context("更新程序不在 macOS App 内")?
        .canonicalize()?;
    let path = macos_mode_record(&bundle, system_root);
    if remembered_macos_mode(executable, system_root)? == Some(false) {
        return Ok(());
    }
    storage::write_json(&path, &InstalledApplication { schema: 1, bundle })
        .context("无法保存启动器运行模式，更新尚未安装")
}

fn detect_mode(executable: &Path, platform: &str, portable: bool) -> Result<Option<PathBuf>> {
    let portable = match configured_mode(executable, platform)? {
        Some(false) => return Ok(None),
        Some(true) => true,
        None => portable,
    };
    let base = package_directory(executable, platform)?;
    let marker = base.join(MARKER);
    if !portable && !marker.try_exists()? {
        return Ok(None);
    }
    ensure!(
        platform != "macos"
            || !base
                .components()
                .any(|part| part.as_os_str() == "AppTranslocation"),
        "macOS 正在临时隔离目录中运行此应用。请关闭启动器，用 Finder 将 .app 移到目标游戏文件夹后重新打开。\nmacOS is running this app in a temporary location. Close it, move the .app in Finder to your game folder, then reopen it."
    );
    if !portable {
        validate_marker(&marker)?;
    }
    let root = base.join(DATA_DIRECTORY);
    if let Ok(meta) = fs::symlink_metadata(&root) {
        ensure!(
            meta.is_dir() && !meta.file_type().is_symlink(),
            "便携数据目录必须是程序旁边的普通文件夹"
        );
    }
    Ok(Some(root))
}

pub fn initialize(root: &Path) -> Result<Option<Identity>> {
    if root.file_name().is_none_or(|name| name != DATA_DIRECTORY) {
        return Ok(None);
    }
    let embedded = detect(&executable()?, std::env::consts::OS)?
        .and_then(|path| path.canonicalize().ok())
        .is_some_and(|path| path == root);
    initialize_mode(root, embedded)
}

fn initialize_mode(root: &Path, embedded: bool) -> Result<Option<Identity>> {
    if root.file_name().is_none_or(|name| name != DATA_DIRECTORY) {
        return Ok(None);
    }
    if !embedded {
        let marker = root.parent().unwrap().join(MARKER);
        if !marker.try_exists()? {
            return Ok(None);
        }
        validate_marker(&marker)?;
    }
    let path = root.join("portable.json");
    let identity = if path.try_exists()? {
        let identity: Identity = storage::read_json(&path)?;
        ensure!(identity.schema == 1, "不支持的便携数据版本");
        uuid::Uuid::parse_str(&identity.id).context("便携数据标识无效")?;
        identity
    } else {
        let identity = Identity {
            schema: 1,
            id: uuid::Uuid::new_v4().to_string(),
            game_linked: false,
        };
        storage::write_json(&path, &identity)
            .context("便携目录不可写，请将启动器移到可写的文件夹后重试")?;
        identity
    };
    cleanup_updates(root);
    Ok(Some(identity))
}

fn validate_marker(path: &Path) -> Result<()> {
    let value: Value = storage::read_json(path).context("便携模式标记无效")?;
    ensure!(
        value.get("schema").and_then(Value::as_u64) == Some(1),
        "不支持的便携模式版本"
    );
    Ok(())
}

fn cleanup_updates(root: &Path) {
    let updates = root.join("updates");
    if !fs::symlink_metadata(&updates)
        .is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink())
    {
        return;
    }
    let Ok(entries) = fs::read_dir(updates) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.split_once('-').is_some_and(|(pid, nonce)| {
            !pid.is_empty()
                && !nonce.is_empty()
                && pid.bytes().chain(nonce.bytes()).all(|c| c.is_ascii_digit())
        }) {
            continue;
        }
        if entry.file_type().is_ok_and(|kind| kind.is_dir())
            && fs::read(entry.path().join("complete")).is_ok_and(|bytes| bytes == b"ok")
        {
            // A just-exiting Windows helper may still be mapped. Retry next launch.
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

pub fn identity(root: &Path) -> Option<Identity> {
    if root.file_name().is_none_or(|name| name != DATA_DIRECTORY) {
        return None;
    }
    let value: Identity = storage::read_json(&root.join("portable.json")).ok()?;
    (value.schema == 1 && uuid::Uuid::parse_str(&value.id).is_ok()).then_some(value)
}

fn base(root: &Path) -> Option<&Path> {
    identity(root)?;
    root.parent()
}

fn path_text(value: &str, base: &Path, encode: bool) -> Result<String> {
    if encode {
        if let Ok(relative) = Path::new(value).strip_prefix(base) {
            ensure!(
                relative
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_))),
                "无效便携路径"
            );
            return Ok(format!(
                "{PREFIX}{}",
                relative.to_string_lossy().replace('\\', "/")
            ));
        }
    } else if let Some(relative) = value.strip_prefix(PREFIX) {
        let relative = if relative.is_empty() {
            PathBuf::new()
        } else {
            storage::safe_relative(relative)?
        };
        return Ok(base.join(relative).to_string_lossy().into_owned());
    }
    Ok(value.into())
}

fn transform(value: &mut Value, base: &Path, encode: bool) -> Result<()> {
    match value {
        Value::String(text) => *text = path_text(text, base, encode)?,
        Value::Array(values) => {
            for value in values {
                transform(value, base, encode)?;
            }
        }
        Value::Object(values) => {
            // Verification indexes also store paths as object keys.
            for (key, mut value) in std::mem::take(values) {
                transform(&mut value, base, encode)?;
                let key = path_text(&key, base, encode)?;
                ensure!(values.insert(key, value).is_none(), "便携路径记录重复");
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn from_slice<T: DeserializeOwned>(root: &Path, bytes: &[u8]) -> Result<T> {
    let mut value: Value = serde_json::from_slice(bytes)?;
    if let Some(base) = base(root) {
        transform(&mut value, base, false)?;
    }
    Ok(serde_json::from_value(value)?)
}
pub(crate) fn to_vec(root: &Path, value: &impl Serialize) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(value)?;
    if let Some(base) = base(root) {
        transform(&mut value, base, true)?;
    }
    Ok(serde_json::to_vec_pretty(&value)?)
}
pub(crate) fn read_json<T: DeserializeOwned>(root: &Path, path: &Path) -> Result<T> {
    from_slice(
        root,
        &fs::read(path).with_context(|| format!("无法读取 {}", path.display()))?,
    )
}
pub(crate) fn write_json(root: &Path, path: &Path, value: &impl Serialize) -> Result<()> {
    storage::atomic_write(path, &to_vec(root, value)?)
}

fn database_paths(root: &Path, value: &mut Value, encode: bool) -> Result<()> {
    if let Some(base) = base(root)
        && let Some(instances) = value.get_mut("instances").and_then(Value::as_array_mut)
    {
        for instance in instances {
            for key in ["game_directory", "java_path"] {
                if let Some(Value::String(path)) = instance.get_mut(key) {
                    *path = path_text(path, base, encode)?;
                }
            }
        }
    }
    Ok(())
}
pub(crate) fn read_database(root: &Path, path: &Path) -> Result<Value> {
    let mut value = storage::read_json(path)?;
    database_paths(root, &mut value, false)?;
    Ok(value)
}
pub(crate) fn write_database(root: &Path, path: &Path, db: &crate::model::Database) -> Result<()> {
    let mut value = serde_json::to_value(db)?;
    database_paths(root, &mut value, true)?;
    storage::write_json(path, &value)
}

/// Validate the current single-file Windows portable update payload.
pub fn windows_executable(bytes: &[u8]) -> Result<Vec<u8>> {
    ensure!(bytes.len() <= 128 * 1024 * 1024, "便携程序大小无效");
    let binary = bytes;
    ensure!(
        binary.len() <= 128 * 1024 * 1024 && binary.get(..2) == Some(b"MZ"),
        "便携程序不是 Windows 可执行文件"
    );
    let offset = binary.get(60..64).context("便携程序头无效")?;
    let offset = u32::from_le_bytes(offset.try_into()?) as usize;
    ensure!(
        binary.get(offset..offset.saturating_add(6)) == Some(b"PE\0\0\x64\x86"),
        "便携程序架构无效"
    );
    Ok(binary.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Launcher;

    fn package(base: &Path) -> PathBuf {
        fs::create_dir_all(base).unwrap();
        fs::write(base.join(MARKER), r#"{"schema":1}"#).unwrap();
        base.join(DATA_DIRECTORY)
    }

    #[test]
    fn common_macos_update_keeps_only_the_registered_app_installed() {
        let temp = tempfile::tempdir().unwrap();
        let system = temp.path().join("system-data");
        let bundle = temp.path().join("Applications/Launcher.app");
        let executable = bundle.join("Contents/MacOS/launcher");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        let configuration = bundle.join("Contents/Resources").join(RUNTIME_CONFIG);
        fs::create_dir_all(configuration.parent().unwrap()).unwrap();
        fs::write(&configuration, r#"{"schema":1,"mode":"installed"}"#).unwrap();
        assert_eq!(
            configured_mode_in(&executable, "macos", &system).unwrap(),
            Some(false)
        );
        preserve_macos_installation_in(&executable, &system).unwrap();
        // Simulate the updater replacing the entire signed app, without a mode resource.
        fs::remove_dir_all(&bundle).unwrap();
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        assert_eq!(
            configured_mode_in(&executable, "macos", &system).unwrap(),
            Some(false)
        );
        preserve_macos_installation_in(&executable, &system).unwrap();
        assert_eq!(
            fs::read_dir(system.join("runtime-modes")).unwrap().count(),
            1
        );
        let copied = temp
            .path()
            .join("Games/Launcher.app/Contents/MacOS/launcher");
        fs::create_dir_all(copied.parent().unwrap()).unwrap();
        assert_eq!(configured_mode_in(&copied, "macos", &system).unwrap(), None);
        fs::write(
            executable.parent().unwrap().join(RUNTIME_CONFIG),
            r#"{"schema":1,"mode":"portable"}"#,
        )
        .unwrap();
        assert_eq!(
            configured_mode_in(&executable, "macos", &system).unwrap(),
            Some(true)
        );
    }

    #[test]
    fn corrupt_or_unwritable_mode_records_do_not_silently_change_mode() {
        let temp = tempfile::tempdir().unwrap();
        let bundle = temp.path().join("Launcher.app");
        let executable = bundle.join("Contents/MacOS/launcher");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, b"current executable").unwrap();
        let blocked = temp.path().join("blocked");
        fs::write(&blocked, b"not a directory").unwrap();
        assert!(preserve_macos_installation_in(&executable, &blocked).is_err());
        assert_eq!(fs::read(&executable).unwrap(), b"current executable");
        let system = temp.path().join("system-data");
        preserve_macos_installation_in(&executable, &system).unwrap();
        let record = macos_mode_record(&bundle.canonicalize().unwrap(), &system);
        for invalid in [
            "invalid".to_owned(),
            serde_json::json!({"schema":1,"bundle":temp.path()}).to_string(),
        ] {
            fs::write(&record, invalid).unwrap();
            assert!(configured_mode_in(&executable, "macos", &system).is_err());
        }
    }

    #[test]
    fn one_executable_uses_installed_configuration_or_defaults_to_portable() {
        let temp = tempfile::tempdir().unwrap();
        for (platform, executable, configuration) in [
            (
                "windows",
                "Windows/renamed.exe",
                "Windows/nekocrate.runtime.json",
            ),
            (
                "linux",
                "Linux/usr/bin/nekocrate-launcher",
                "Linux/usr/lib/nekocrate-launcher/nekocrate.runtime.json",
            ),
            (
                "macos",
                "Mac/Renamed.app/Contents/MacOS/launcher",
                "Mac/Renamed.app/Contents/Resources/nekocrate.runtime.json",
            ),
        ] {
            let executable = temp.path().join(executable);
            fs::create_dir_all(executable.parent().unwrap()).unwrap();
            let configuration = temp.path().join(configuration);
            fs::create_dir_all(configuration.parent().unwrap()).unwrap();
            let base = package_directory(&executable, platform).unwrap();
            fs::create_dir_all(base.join(".minecraft")).unwrap();
            let expected = Some(base.join(DATA_DIRECTORY));
            assert_eq!(detect_mode(&executable, platform, true).unwrap(), expected);
            fs::write(&configuration, r#"{"schema":1,"mode":"installed"}"#).unwrap();
            assert!(detect_mode(&executable, platform, true).unwrap().is_none());
            // An old portable marker cannot override an explicit installed mode.
            fs::write(base.join(MARKER), r#"{"schema":1}"#).unwrap();
            assert!(detect_mode(&executable, platform, true).unwrap().is_none());
            fs::remove_file(base.join(MARKER)).unwrap();
            fs::remove_file(&configuration).unwrap();
            assert_eq!(detect_mode(&executable, platform, true).unwrap(), expected);
        }
    }

    #[test]
    fn invalid_runtime_configuration_never_silently_switches_data_directories() {
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join("launcher.exe");
        let path = temp.path().join(RUNTIME_CONFIG);
        for value in [
            "{broken",
            r#"{"schema":2,"mode":"installed"}"#,
            r#"{"schema":1,"mode":"typo"}"#,
        ] {
            fs::write(&path, value).unwrap();
            assert!(detect_mode(&executable, "windows", true).is_err());
        }
        fs::write(&path, r#"{"schema":1,"mode":"portable"}"#).unwrap();
        assert_eq!(
            detect_mode(&executable, "windows", false).unwrap(),
            Some(temp.path().join(DATA_DIRECTORY))
        );
    }

    #[test]
    fn installed_linux_configuration_does_not_change_a_separate_portable_copy() {
        let temp = tempfile::tempdir().unwrap();
        let installed = temp.path().join("usr/bin/nekocrate-launcher");
        fs::create_dir_all(installed.parent().unwrap()).unwrap();
        let configuration = temp
            .path()
            .join("usr/lib/nekocrate-launcher")
            .join(RUNTIME_CONFIG);
        fs::create_dir_all(configuration.parent().unwrap()).unwrap();
        fs::write(configuration, r#"{"schema":1,"mode":"installed"}"#).unwrap();
        let portable = temp.path().join("games/renamed.elf");
        assert!(detect_mode(&installed, "linux", true).unwrap().is_none());
        assert_eq!(
            detect_mode(&portable, "linux", true).unwrap(),
            Some(temp.path().join("games/.nekocrate"))
        );
    }

    #[test]
    fn embedded_portable_mode_follows_the_application_without_a_marker() {
        let temp = tempfile::tempdir().unwrap();
        for (platform, name) in [
            ("windows", "User renamed.exe"),
            ("linux", "user-renamed"),
            ("macos", "User renamed.app/Contents/MacOS/launcher"),
        ] {
            let executable = temp.path().join(name);
            assert!(detect_mode(&executable, platform, false).unwrap().is_none());
            assert_eq!(
                detect_mode(&executable, platform, true).unwrap(),
                Some(temp.path().join(DATA_DIRECTORY))
            );
        }
        let root = temp.path().join(DATA_DIRECTORY);
        fs::create_dir(&root).unwrap();
        let first = initialize_mode(&root, true).unwrap().unwrap();
        assert_eq!(initialize_mode(&root, true).unwrap().unwrap().id, first.id);
        assert!(!temp.path().join(MARKER).exists());
        assert!(
            initialize_mode(&temp.path().join("explicit-data"), true)
                .unwrap()
                .is_none()
        );
        let next = temp.path().join("next folder").join(DATA_DIRECTORY);
        fs::create_dir_all(&next).unwrap();
        assert_ne!(initialize_mode(&next, true).unwrap().unwrap().id, first.id);
    }

    #[test]
    fn portable_mac_refuses_a_translocated_directory() {
        assert!(detect_mode(Path::new("/private/var/folders/test/AppTranslocation/random/d/Any.app/Contents/MacOS/launcher"), "macos", true).is_err());
    }

    #[test]
    fn detects_only_explicit_packages_and_uses_outer_bundle_directory() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join(".minecraft")).unwrap();
        assert!(
            detect(&temp.path().join("launcher"), "linux")
                .unwrap()
                .is_none()
        );
        let root = package(temp.path());
        for (platform, name) in [
            ("windows", "launcher.exe"),
            ("linux", "launcher.elf"),
            ("macos", "Neko.app/Contents/MacOS/launcher"),
        ] {
            assert_eq!(
                detect(&temp.path().join(name), platform).unwrap(),
                Some(root.clone())
            );
        }
        fs::write(temp.path().join(MARKER), r#"{"schema":2}"#).unwrap();
        assert!(detect(&temp.path().join("launcher"), "linux").is_err());
    }

    #[test]
    fn package_and_installed_data_stay_independent_and_unlink_is_remembered() {
        let temp = tempfile::tempdir().unwrap();
        let root = package(&temp.path().join("portable"));
        let portable = Launcher::open(Some(root.clone())).unwrap();
        let installed = Launcher::open(Some(temp.path().join("installed"))).unwrap();
        assert!(portable.is_portable());
        assert!(!installed.is_portable());
        assert!(installed.snapshot().database.instances.is_empty());
        portable.save_ui_preference("theme", "ember").unwrap();
        assert!(installed.snapshot().database.ui_preferences.is_empty());
        let instance = portable.snapshot().database.instances.remove(0);
        let shared = installed
            .import_unlocked(
                instance.game_dir(portable.root()),
                String::new(),
                String::new(),
            )
            .unwrap();
        let guard = portable.idle(&instance.id).unwrap();
        assert!(
            installed
                .update_instance(&shared.id, "shared", 2048, None, None)
                .is_err()
        );
        drop(guard);
        installed
            .update_instance(&shared.id, "shared", 2048, None, None)
            .unwrap();
        let game = instance.game_dir(portable.root());
        portable
            .delete_instance_with_recycle(&instance.id, |path| Ok(fs::remove_dir_all(path)?))
            .unwrap();
        assert!(game.is_dir());
        drop(portable);
        let portable = Launcher::open(Some(root)).unwrap();
        assert!(portable.snapshot().database.instances.is_empty());
        assert_eq!(
            portable.snapshot().database.ui_preferences["theme"],
            "ember"
        );
    }

    #[test]
    fn portable_update_accepts_only_a_windows_executable() {
        use std::io::Write;
        let make = |name: &str, binary: &[u8]| {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(binary).unwrap();
            zip.finish().unwrap().into_inner()
        };
        let mut binary = vec![0; 70];
        binary[..2].copy_from_slice(b"MZ");
        binary[60..64].copy_from_slice(&64_u32.to_le_bytes());
        binary[64..].copy_from_slice(b"PE\0\0\x64\x86");
        assert_eq!(windows_executable(&binary).unwrap(), binary);
        assert!(windows_executable(b"MZ incomplete").is_err());
        assert!(windows_executable(&make("NekoCrate/nekocrate-launcher.exe", &binary)).is_err());
        let mut wrong_arch = binary.clone();
        wrong_arch[68..70].copy_from_slice(&[0x4c, 0x01]);
        assert!(windows_executable(&wrong_arch).is_err());
        assert!(windows_executable(&make("../nekocrate-launcher.exe", &binary)).is_err());
        assert!(windows_executable(&make("NekoCrate/nekocrate-launcher.exe", b"not PE")).is_err());
    }

    #[test]
    fn moved_package_keeps_instances_java_tasks_identity_and_external_references() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("猫咪 盒子");
        let root = package(&original);
        let launcher = Launcher::open(Some(root.clone())).unwrap();
        let instance = launcher.snapshot().database.instances.remove(0);
        let game = original.join(".minecraft");
        assert_eq!(
            instance.game_dir(launcher.root()),
            game.canonicalize().unwrap()
        );
        assert_eq!(instance.name, "猫咪 盒子");
        fs::write(game.join("keep.txt"), b"game data").unwrap();
        let root = launcher.root().to_owned();
        let game = instance.game_dir(&root);
        let java = root.join("runtimes/java/bin/java");
        launcher
            .update_instance(
                &instance.id,
                &instance.name,
                2048,
                Some(java.to_string_lossy().into()),
                None,
            )
            .unwrap();
        let id = identity(launcher.root()).unwrap().id;
        let external = temp.path().join("outside/file");
        let record = serde_json::json!({"game":game,"files":[root.join("cache/objects/file"),external],"text":"unchanged"});
        write_json(launcher.root(), &root.join("task.json"), &record).unwrap();
        assert!(
            !fs::read_to_string(root.join("database.json"))
                .unwrap()
                .contains(original.to_str().unwrap())
        );
        drop(launcher);
        let moved = temp.path().join("新位置");
        fs::rename(&original, &moved).unwrap();
        let moved = moved.canonicalize().unwrap();
        let root = moved.join(DATA_DIRECTORY);
        let launcher = Launcher::open(Some(root.clone())).unwrap();
        assert_eq!(identity(&root).unwrap().id, id);
        assert_eq!(launcher.snapshot().database.instances.len(), 1);
        let instance = launcher.instance(&instance.id).unwrap();
        assert_eq!(instance.game_dir(&root), moved.join(".minecraft"));
        assert_eq!(
            instance.java_path.unwrap(),
            root.join("runtimes/java/bin/java").to_string_lossy()
        );
        let record: Value = read_json(&root, &root.join("task.json")).unwrap();
        assert_eq!(
            record["game"],
            moved.join(".minecraft").to_string_lossy().as_ref()
        );
        assert_eq!(record["files"][1], external.to_string_lossy().as_ref());
        assert_eq!(
            fs::read(moved.join(".minecraft/keep.txt")).unwrap(),
            b"game data"
        );
        assert!(
            from_slice::<Value>(&root, br#"{"path":"nekocrate-portable:/../escape"}"#).is_err()
        );
    }
}
