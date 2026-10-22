use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

fn initial_directory(path: &str) -> Result<PathBuf> {
    let expanded = nekocrate_core::storage::expand_user_path(Path::new(if path.is_empty() {
        "~"
    } else {
        path
    }))?;
    if let Some(directory) = expanded.ancestors().find(|directory| directory.is_dir()) {
        return Ok(directory.canonicalize()?);
    }
    nekocrate_core::storage::expand_user_path(Path::new("~"))
}

#[cfg(target_os = "macos")]
pub async fn pick(app: &tauri::AppHandle, path: &str) -> Result<Option<PathBuf>> {
    let directory = initial_directory(path)?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let result = objc2::rc::autoreleasepool(|_| pick_macos(&directory));
        let _ = sender.send(result);
    })?;
    receiver.await.context("目录选择窗口未返回结果")?
}

#[cfg(target_os = "macos")]
fn pick_macos(directory: &Path) -> Result<Option<PathBuf>> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
    use objc2_foundation::{NSString, NSURL};

    let main_thread = MainThreadMarker::new().context("目录选择窗口必须在主线程打开")?;
    let panel = NSOpenPanel::openPanel(main_thread);
    panel.setCanChooseDirectories(true);
    panel.setCanChooseFiles(false);
    panel.setAllowsMultipleSelection(false);
    panel.setCanCreateDirectories(false);
    // This panel only: do not change the user's Finder preferences.
    panel.setShowsHiddenFiles(true);
    panel.setMessage(Some(&NSString::from_str("选择现有 Minecraft 游戏目录")));
    panel.setPrompt(Some(&NSString::from_str("选择目录")));
    let initial =
        NSURL::fileURLWithPath_isDirectory(&NSString::from_str(&directory.to_string_lossy()), true);
    panel.setDirectoryURL(Some(&initial));
    if panel.runModal() != NSModalResponseOK {
        return Ok(None);
    }
    let path = panel
        .URL()
        .and_then(|url| url.path())
        .context("无法读取所选目录路径")?;
    Ok(Some(PathBuf::from(path.to_string())))
}

#[cfg(not(target_os = "macos"))]
pub async fn pick(app: &tauri::AppHandle, path: &str) -> Result<Option<PathBuf>> {
    use tauri_plugin_dialog::DialogExt;

    let directory = initial_directory(path)?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("选择现有 Minecraft 游戏目录")
        .set_directory(directory)
        .pick_folder(move |path| {
            let _ = sender.send(path);
        });
    receiver
        .await
        .context("目录选择窗口未返回结果")?
        .map(|path| path.into_path().context("无法读取所选目录路径"))
        .transpose()
}
