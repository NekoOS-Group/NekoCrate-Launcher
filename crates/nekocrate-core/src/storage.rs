use anyhow::{Context, Result, bail};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Resolve the path without opening/locking the database, so early startup
/// failures can be logged to the same directory.
pub fn data_root(root: Option<PathBuf>) -> Result<PathBuf> {
    root.or_else(|| std::env::var_os("NEKOCRATE_DATA_DIR").map(PathBuf::from))
        .or_else(|| {
            directories::ProjectDirs::from("dev", "NekoCrate", "NekoCrate")
                .map(|p| p.data_local_dir().to_owned())
        })
        .context("无法确定数据目录")
}
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("无法读取 {}", path.display()))?)
        .with_context(|| format!("JSON 损坏：{}，请保留原文件后恢复备份", path.display()))
}
pub(crate) fn same_file_contents(a: &Path, b: &Path) -> Result<bool> {
    use ring::digest::{Context, SHA256};
    use std::io::Read;
    if !a.is_file() || !b.is_file() || fs::metadata(a)?.len() != fs::metadata(b)?.len() {
        return Ok(false);
    }
    let hash = |path: &Path| -> Result<_> {
        let mut hash = Context::new(&SHA256);
        let mut file = fs::File::open(path)?;
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            let count = match file.read(&mut buffer) {
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                result => result?,
            };
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        Ok(hash.finish())
    };
    Ok(hash(a)?.as_ref() == hash(b)?.as_ref())
}
pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    atomic_write(path, &serde_json::to_vec_pretty(value)?)
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    atomic_write_with_backup(path, bytes, &path.with_extension("json.bak"))
}
pub(crate) fn atomic_write_with_backup(path: &Path, bytes: &[u8], backup: &Path) -> Result<()> {
    use std::io::Write;
    fs::create_dir_all(path.parent().context("路径没有父目录")?)?;
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    // Keep a last-known snapshot so a damaged metadata file is recoverable.
    if path.exists() {
        fs::copy(path, backup)?;
    }
    fs::rename(&temp, path)?;
    Ok(())
}
pub fn safe_relative(value: &str) -> Result<PathBuf> {
    if value.is_empty() || value.contains('\\') || value.contains(':') || value.contains('\0') {
        bail!("非法相对路径：{value}");
    }
    let path = Path::new(value);
    if path
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        bail!("路径不能离开实例目录：{value}");
    }
    Ok(path.to_path_buf())
}
pub fn safe_version(version: &str) -> Result<()> {
    if version.len() > 80 || safe_relative(version)?.components().count() != 1 {
        bail!("游戏版本不合法");
    }
    Ok(())
}
pub fn safe_id(id: &str) -> Result<()> {
    uuid::Uuid::parse_str(id).context("实例 ID 无效")?;
    Ok(())
}

/// Expand only a leading `~` path component; never evaluate shell syntax.
pub fn expand_user_path(path: &Path) -> Result<PathBuf> {
    if let Ok(relative) = path.strip_prefix("~") {
        let home = directories::UserDirs::new().context("无法确定用户主目录")?;
        Ok(home.home_dir().join(relative))
    } else {
        Ok(path.to_owned())
    }
}

pub fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    let source = source.canonicalize().context("迁移目录不存在")?;
    fs::create_dir_all(destination)?;
    let dest = destination.canonicalize()?;
    if dest.starts_with(&source) {
        bail!("目标目录不能位于来源目录内");
    }
    for entry in walkdir::WalkDir::new(&source).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_symlink() {
            bail!(
                "迁移目录含符号链接，请先将链接替换为实际文件：{}",
                entry.path().display()
            );
        }
        let relative = entry.path().strip_prefix(&source)?;
        // Never import another launcher's credentials or session tokens.
        if relative.components().any(|c| {
            matches!(
                c.as_os_str().to_str(),
                Some(
                    "launcher_accounts.json"
                        | "launcher_profiles.json"
                        | "accounts.json"
                        | "usercache.json"
                        | ".git"
                )
            )
        }) {
            continue;
        }
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(target)?;
        } else if entry.file_type().is_file() {
            fs::create_dir_all(target.parent().unwrap())?;
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expands_home_without_evaluating_other_path_text() {
        let home = directories::UserDirs::new().unwrap();
        assert_eq!(expand_user_path(Path::new("~")).unwrap(), home.home_dir());
        assert_eq!(
            expand_user_path(Path::new("~/game/minecraft/.minecraft")).unwrap(),
            home.home_dir().join("game/minecraft/.minecraft")
        );
        for literal in [
            "~someone/game",
            "game/~/world",
            "$HOME/game",
            "game with spaces/.minecraft",
        ] {
            assert_eq!(
                expand_user_path(Path::new(literal)).unwrap(),
                PathBuf::from(literal)
            );
        }
    }
    #[test]
    fn rejects_path_traversal() {
        for bad in ["../secret", "/etc/passwd", "C:/x", "a\\b", "a/../../b", ""] {
            assert!(safe_relative(bad).is_err(), "{bad}");
        }
        assert_eq!(
            safe_relative("org/example/a.jar").unwrap(),
            PathBuf::from("org/example/a.jar")
        );
    }
    #[test]
    fn json_replacement_preserves_backup() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("db.json");
        write_json(&p, &vec![1]).unwrap();
        write_json(&p, &vec![2]).unwrap();
        assert_eq!(read_json::<Vec<u8>>(&p).unwrap(), vec![2]);
        assert_eq!(
            read_json::<Vec<u8>>(&p.with_extension("json.bak")).unwrap(),
            vec![1]
        );
    }
}
