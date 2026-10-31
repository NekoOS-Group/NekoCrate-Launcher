//! Windows portable updates never invoke NSIS. A copy of the current executable
//! waits for its parent to exit, verifies the signed package again, then swaps it.
use anyhow::{Context, Result, ensure};
use base64::Engine;
use nekocrate_core::{portable, storage};
#[cfg(target_os = "windows")]
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(target_os = "windows")]
#[derive(Serialize, Deserialize)]
struct Request {
    parent_pid: u32,
    executable: String,
    version: String,
    signature: String,
}

fn verify(bytes: &[u8], signature: &str, version: &str, public: &str) -> Result<()> {
    let decode = |text: &str| -> Result<String> {
        Ok(String::from_utf8(
            base64::engine::general_purpose::STANDARD.decode(text.trim())?,
        )?)
    };
    let public = minisign_verify::PublicKey::decode(&decode(public)?)?;
    let signature = minisign_verify::Signature::decode(&decode(signature)?)?;
    public.verify(bytes, &signature, true)?;
    ensure!(
        signature
            .trusted_comment()
            .split('\t')
            .any(|field| field == format!("version:{version}")),
        "便携更新签名与版本不一致"
    );
    ensure!(
        semver::Version::parse(version)? > semver::Version::parse(env!("CARGO_PKG_VERSION"))?,
        "不能安装旧版本或相同版本"
    );
    Ok(())
}

fn target(stage: &Path, name: &str) -> Result<PathBuf> {
    let relative = storage::safe_relative(name)?;
    ensure!(
        relative.components().count() == 1
            && relative
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("exe")),
        "便携更新目标无效"
    );
    let updates = stage.parent().context("便携更新目录无效")?;
    let root = updates.parent().context("便携更新目录无效")?;
    let base = root.parent().context("便携更新目录无效")?;
    ensure!(
        updates.file_name().is_some_and(|n| n == "updates")
            && portable::identity(root).is_some()
            && base.join(portable::MARKER).is_file(),
        "便携更新不在当前便携目录内"
    );
    for path in [stage, updates, root] {
        let meta = fs::symlink_metadata(path)?;
        ensure!(
            meta.is_dir() && !meta.file_type().is_symlink(),
            "便携更新目录不能是符号链接"
        );
    }
    let target = base.join(relative);
    let meta = fs::symlink_metadata(&target)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink(),
        "便携更新目标不是普通文件"
    );
    Ok(target)
}

#[cfg(target_os = "windows")]
pub fn stage(root: &Path, update: &tauri_plugin_updater::Update, bytes: &[u8]) -> Result<()> {
    use std::{
        io::Write,
        os::windows::process::CommandExt,
        process::{Command, Stdio},
    };
    update.verify(bytes)?;
    // Validate archive structure before stopping the running launcher.
    portable::windows_executable(bytes)?;
    let executable = std::env::current_exe()?;
    ensure!(
        executable
            .parent()
            .context("程序路径无效")?
            .canonicalize()?
            == root.parent().context("便携目录无效")?.canonicalize()?,
        "便携更新目标不匹配"
    );
    let updates = root.join("updates");
    if !updates.exists() {
        fs::create_dir(&updates)?;
    }
    ensure!(
        !fs::symlink_metadata(&updates)?.file_type().is_symlink(),
        "更新目录不能是符号链接"
    );
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let stage = updates.join(format!("{}-{nonce}", std::process::id()));
    fs::create_dir(&stage)?;
    let request = Request {
        parent_pid: std::process::id(),
        executable: executable
            .file_name()
            .context("程序路径无效")?
            .to_str()
            .context("程序文件名无效")?
            .into(),
        version: update.version.clone(),
        signature: update.signature.clone(),
    };
    target(&stage, &request.executable)?;
    let mut package = fs::File::create(stage.join("package.zip"))?;
    package.write_all(bytes)?;
    package.sync_all()?;
    storage::write_json(&stage.join("request.json"), &request)?;
    let helper = stage.join("helper.exe");
    fs::copy(executable, &helper)?;
    Command::new(helper)
        .arg("--apply-portable-update")
        .creation_flags(0x08000000)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("无法启动便携更新程序")?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn wait_for_parent(pid: u32) -> Result<()> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, GetLastError, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    ensure!(pid != std::process::id(), "更新程序不能等待自身退出");
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            ensure!(
                GetLastError() == ERROR_INVALID_PARAMETER,
                "无法等待旧启动器退出"
            );
            return Ok(());
        }
        let result = WaitForSingleObject(handle, 60_000);
        CloseHandle(handle);
        ensure!(result == WAIT_OBJECT_0, "等待旧启动器退出超时，未替换程序");
    }
    Ok(())
}

fn swap(target: &Path, candidate: &Path, backup: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide = |path: &Path| {
            path.as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>()
        };
        let (target, candidate, backup) = (wide(target), wide(candidate), wide(backup));
        let result = unsafe {
            windows_sys::Win32::Storage::FileSystem::ReplaceFileW(
                target.as_ptr(),
                candidate.as_ptr(),
                backup.as_ptr(),
                0,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        if result == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        fs::rename(target, backup)?;
        if let Err(error) = fs::rename(candidate, target) {
            fs::rename(backup, target)?;
            return Err(error.into());
        }
    }
    Ok(())
}

fn replace(
    stage: &Path,
    target: &Path,
    bytes: &[u8],
    restart: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    use std::io::Write;
    let candidate = stage.join("next.exe");
    let backup = stage.join("previous.exe");
    ensure!(!backup.exists(), "旧程序备份已存在，未覆盖");
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&candidate)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    swap(target, &candidate, &backup).context("无法替换便携程序，请检查是否仍有进程占用")?;
    let result = restart(target);
    if let Err(error) = result {
        if target.exists() {
            fs::remove_file(target)?;
        }
        fs::rename(&backup, target)
            .context("更新失败且无法恢复旧程序，请保留 updates 目录中的 previous.exe")?;
        return Err(error.context("便携更新失败，已恢复旧程序"));
    }
    let _ = fs::remove_file(backup);
    let _ = fs::remove_file(stage.join("package.zip"));
    fs::write(stage.join("complete"), b"ok")?;
    Ok(())
}

/// Exercise the Windows file replacement API on disposable files using the
/// already-built release binary, without another desktop compilation in CI.
#[cfg(target_os = "windows")]
pub fn verify_native_swap(root: &Path) -> Result<()> {
    let stage = root.join("update-smoke");
    fs::create_dir_all(&stage)?;
    let executable = stage.join("test.exe");
    fs::write(&executable, b"old")?;
    ensure!(
        replace(&stage, &executable, b"new", |_| anyhow::bail!(
            "simulated spawn failure"
        ))
        .is_err(),
        "未触发回滚"
    );
    ensure!(fs::read(&executable)? == b"old", "旧程序未恢复");
    replace(&stage, &executable, b"new", |_| Ok(()))?;
    ensure!(fs::read(executable)? == b"new", "程序替换失败");
    fs::remove_dir_all(stage)?;
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn run_helper() -> bool {
    use std::{os::windows::process::CommandExt, process::Command};
    if !std::env::args_os().any(|arg| arg == "--apply-portable-update") {
        return false;
    }
    let result = (|| -> Result<()> {
        let helper = std::env::current_exe()?;
        let stage = helper.parent().context("更新程序路径无效")?;
        let request: Request = storage::read_json(&stage.join("request.json"))?;
        let target = target(stage, &request.executable)?;
        wait_for_parent(request.parent_pid)?;
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json"))?;
        let bytes = fs::read(stage.join("package.zip"))?;
        verify(
            &bytes,
            &request.signature,
            &request.version,
            config["plugins"]["updater"]["pubkey"]
                .as_str()
                .context("更新公钥无效")?,
        )?;
        let executable = portable::windows_executable(&bytes)?;
        replace(stage, &target, &executable, |target| {
            Command::new(target)
                .current_dir(target.parent().unwrap())
                .creation_flags(0x08000000)
                .spawn()?;
            Ok(())
        })
    })();
    if let Err(error) = result {
        // Keep diagnostics next to the failed update, without opening the GUI or
        // touching the installed launcher's data directory.
        if let Ok(exe) = std::env::current_exe()
            && let Some(stage) = exe.parent()
        {
            let _ = fs::write(stage.join("error.log"), format!("{error:#}"));
            rfd::MessageDialog::new()
                .set_title("便携更新未完成 · Portable update failed")
                .set_description(format!("{error:#}"))
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_helper_requires_signature_and_exact_newer_version() {
        let package = include_bytes!("../tests/fixtures/updater/test.app.tar.gz");
        let signature = include_str!("../tests/fixtures/updater/test.app.tar.gz.sig");
        let public = include_str!("../tests/fixtures/updater/test.pub");
        verify(package, signature, "9.9.9", public).unwrap();
        assert!(verify(b"modified", signature, "9.9.9", public).is_err());
        assert!(verify(package, signature, "9.9.10", public).is_err());
    }
    #[test]
    fn failed_restart_restores_original_and_success_preserves_adjacent_data() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(portable::DATA_DIRECTORY);
        fs::create_dir(&root).unwrap();
        fs::write(temp.path().join(portable::MARKER), r#"{"schema":1}"#).unwrap();
        portable::initialize(&root).unwrap();
        let stage = root.join("updates/fixture");
        fs::create_dir_all(&stage).unwrap();
        let executable = temp.path().join("launcher.exe");
        fs::write(&executable, b"old").unwrap();
        assert_eq!(target(&stage, "launcher.exe").unwrap(), executable);
        assert!(target(&stage, "../other.exe").is_err());
        assert!(
            replace(&stage, &executable, b"new", |_| anyhow::bail!(
                "spawn failure"
            ))
            .is_err()
        );
        assert_eq!(fs::read(&executable).unwrap(), b"old");
        fs::create_dir(temp.path().join(".minecraft")).unwrap();
        fs::write(temp.path().join(".minecraft/save"), b"save").unwrap();
        replace(&stage, &executable, b"new", |_| Ok(())).unwrap();
        assert_eq!(fs::read(executable).unwrap(), b"new");
        assert_eq!(
            fs::read(temp.path().join(".minecraft/save")).unwrap(),
            b"save"
        );
    }
}
