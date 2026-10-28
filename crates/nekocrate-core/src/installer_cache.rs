//! Migration of old Java archives. Successful new installations clean themselves.
use crate::{Launcher, diagnostics};
use anyhow::{Result, ensure};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

fn owned(root: &Path, path: &Path) -> Result<()> {
    let relative = path.strip_prefix(root)?;
    let mut parent = root.to_owned();
    for component in relative.components() {
        ensure!(
            matches!(component, std::path::Component::Normal(_)),
            "无效缓存路径"
        );
        parent.push(component);
        match fs::symlink_metadata(&parent) {
            Ok(metadata) => ensure!(
                !metadata.file_type().is_symlink(),
                "安装缓存路径不能是符号链接"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
fn archives(root: &Path) -> Result<Vec<PathBuf>> {
    let directory = root.join("cache/java");
    owned(root, &directory)?;
    if !directory.try_exists()? {
        return Ok(vec![]);
    }
    let mut files = vec![];
    for entry in walkdir::WalkDir::new(directory).follow_links(false) {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if entry.file_type().is_file()
            && (name.ends_with(".tar.gz") || name.ends_with(".zip") || name.ends_with(".tgz"))
        {
            files.push(entry.into_path());
        }
    }
    Ok(files)
}
pub(crate) fn cleanup_java_archives(root: &Path) -> Result<()> {
    let files = archives(root)?;
    if files.is_empty() {
        return Ok(());
    }
    let hashes = crate::verification::recorded_sha256(root, &files);
    let pending = crate::download_tasks::pending_files(root)?;
    for path in files {
        if pending.contains(&path) {
            continue;
        }
        owned(root, &path)?;
        let hash = if let Some(hash) = hashes.get(&path) {
            hash.clone()
        } else {
            // Only pre-receipt installations need this one-time streaming hash.
            let mut file = fs::File::open(&path)?;
            let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                digest.update(&buffer[..n]);
            }
            hex::encode(digest.finish())
        };
        let object = crate::download::object_path(
            &root.join("cache/objects"),
            &crate::download::FileSpec {
                url: String::new(),
                path: path.clone(),
                hash,
                sha256: true,
                size: 0,
            },
        )?;
        owned(root, &object)?;
        if pending.contains(&object) {
            continue;
        }
        match fs::remove_file(object) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        fs::remove_file(path)?;
    }
    Ok(())
}
impl Launcher {
    pub(crate) fn clean_old_java_archives(&self) {
        let marker = self.root().join("cache/installer-cleanup-v1.json");
        if marker.is_file() {
            return;
        }
        // No background thread or verification-index read when there is no work.
        match archives(self.root()) {
            Ok(paths) if paths.is_empty() => {
                if let Err(error) = crate::storage::write_json(&marker, &true) {
                    let _ = diagnostics::record_error(self.root(), "installer_cache", &error);
                }
                return;
            }
            Err(error) => {
                let _ = diagnostics::record_error(self.root(), "installer_cache", &error);
                return;
            }
            _ => {}
        }
        let launcher = self.clone();
        let _ = std::thread::Builder::new()
            .name("installer-cache".into())
            .spawn(move || {
                let _guard = launcher.0.operation.blocking_lock();
                if let Err(error) = cleanup_java_archives(launcher.root()) {
                    let _ = diagnostics::record_error(launcher.root(), "installer_cache", &error);
                } else if let Err(error) = crate::storage::write_json(&marker, &true) {
                    let _ = diagnostics::record_error(launcher.root(), "installer_cache", &error);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_cleanup_removes_both_copies_and_keeps_partials_and_installed_java() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let body = b"old Java archive";
        let hash = hex::encode(ring::digest::digest(&ring::digest::SHA256, body));
        let files = [
            root.join("cache/java/runtime.tar.gz"),
            root.join("cache/objects/sha256")
                .join(&hash[..2])
                .join(&hash),
            root.join("cache/objects/sha256")
                .join(&hash[..2])
                .join(format!("{hash}.part")),
            root.join("runtimes/jdk-17/bin/java"),
            root.join("cache/java/incomplete.zip.part"),
        ];
        for path in &files {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, body).unwrap();
        }
        cleanup_java_archives(root).unwrap();
        assert!(!files[0].exists());
        assert!(!files[1].exists());
        for path in &files[2..] {
            assert!(path.is_file());
        }
        cleanup_java_archives(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn legacy_cleanup_does_not_follow_linked_caches() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("launcher");
        let external = temp.path().join("external");
        fs::create_dir_all(root.join("cache")).unwrap();
        fs::create_dir_all(&external).unwrap();
        fs::write(external.join("keep.zip"), b"keep").unwrap();
        std::os::unix::fs::symlink(&external, root.join("cache/java")).unwrap();
        assert!(cleanup_java_archives(&root).is_err());
        assert!(external.join("keep.zip").is_file());
    }
}
