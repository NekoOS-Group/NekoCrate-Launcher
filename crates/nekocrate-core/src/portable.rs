//! Portable packages use the same executable, with an explicit adjacent marker.
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
    if cfg!(target_os = "linux")
        && let Some(path) = std::env::var_os("APPIMAGE").map(PathBuf::from)
        && path.is_absolute()
        && path.is_file()
    {
        return Ok(path);
    }
    Ok(std::env::current_exe()?)
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
    let base = package_directory(executable, platform)?;
    let marker = base.join(MARKER);
    if !marker.try_exists()? {
        ensure!(
            platform != "macos"
                || !executable
                    .ancestors()
                    .any(|p| p.file_name().is_some_and(|n| n == "NekoCrate Portable.app")),
            "未找到便携目录。请完整解压 portable 压缩包，将 NekoCrate Portable.app 用 Finder 移到目标文件夹，并将 nekocrate.portable.json 放在它旁边后重新打开。macOS 可能正在临时隔离目录中运行此应用。\nPortable folder not found. Extract the complete package, move the app in Finder, and keep nekocrate.portable.json beside it."
        );
        return Ok(None);
    }
    let value: Value = storage::read_json(&marker).context("便携模式标记无效")?;
    ensure!(
        value.get("schema").and_then(Value::as_u64) == Some(1),
        "不支持的便携模式版本"
    );
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
    if root.file_name().is_none_or(|name| name != DATA_DIRECTORY)
        || !root.parent().is_some_and(|p| p.join(MARKER).is_file())
    {
        return Ok(None);
    }
    let marker: Value = storage::read_json(&root.parent().unwrap().join(MARKER))?;
    ensure!(
        marker.get("schema").and_then(Value::as_u64) == Some(1),
        "不支持的便携模式版本"
    );
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
            .context("便携目录不可写，请将整个便携文件夹解压到可写的位置后重试")?;
        identity
    };
    cleanup_updates(root);
    Ok(Some(identity))
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

/// Read the one executable from a signed Windows portable archive. No archive
/// path is ever extracted onto the filesystem, so traversal entries cannot write.
pub fn windows_executable(bytes: &[u8]) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let name = "NekoCrate/nekocrate-launcher.exe";
    ensure!(
        archive.file_names().filter(|n| *n == name).count() == 1,
        "便携更新包缺少唯一的启动器程序"
    );
    let mut entry = archive.by_name(name)?;
    ensure!(
        entry.is_file() && entry.size() <= 128 * 1024 * 1024,
        "便携程序大小无效"
    );
    let mut binary = Vec::new();
    entry
        .by_ref()
        .take(128 * 1024 * 1024 + 1)
        .read_to_end(&mut binary)?;
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
    Ok(binary)
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
            ("linux", "Launcher.AppImage"),
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
    fn portable_archive_reads_only_the_expected_windows_binary() {
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
        assert_eq!(
            windows_executable(&make("NekoCrate/nekocrate-launcher.exe", &binary)).unwrap(),
            binary
        );
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
