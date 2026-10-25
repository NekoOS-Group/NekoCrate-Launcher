use nekocrate_core::{Launcher, model::*};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
use tauri::{Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;
use tokio_util::sync::CancellationToken;

mod folder_scan;
mod import_picker;
mod jobs;
mod pausable;
mod startup;
mod updater;

struct AppState {
    launcher: Launcher,
    jobs: jobs::Jobs,
    updater: updater::UpdateManager,
    folder_scan: folder_scan::FolderScanManager,
}
impl AppState {
    async fn job(&self) -> anyhow::Result<(CancellationToken, jobs::JobGuard)> {
        let guard = self.jobs.task().await?;
        Ok((guard.cancel.clone(), guard))
    }
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    CopyText {
        text: String,
    },
    Snapshot,
    Versions,
    FabricVersions {
        game_version: String,
    },
    LoaderVersions {
        game_version: String,
        kind: String,
    },
    AddLoaderVersion {
        id: String,
        game_version: String,
        loader: ModLoader,
    },
    AddFabricVersion {
        id: String,
        game_version: String,
        loader_version: String,
    },
    PreviewModpack {
        path: PathBuf,
    },
    PrepareModpack {
        id: String,
        path: PathBuf,
        name: String,
        optional: Vec<String>,
    },
    ModpackStatus {
        id: String,
        version: String,
    },
    SupplyModpackFiles {
        id: String,
        version: String,
        paths: Vec<PathBuf>,
    },
    CurseforgeKeyStatus,
    SaveCurseforgeKey {
        key: ProxyPassword,
    },
    Java,
    MemoryOptions {
        id: String,
    },
    CheckUpdate,
    LauncherUpdateState,
    DownloadLauncherUpdate {
        version: String,
    },
    InstallLauncherUpdate,
    CancelLauncherUpdate,
    ProxyPassword,
    SaveSettings {
        settings: Settings,
    },
    Create {
        name: String,
        #[serde(default)]
        icon: Option<u8>,
        #[serde(default)]
        version: String,
        #[serde(default)]
        directory: Option<String>,
    },
    Update {
        id: String,
        name: String,
        memory_mb: u32,
        #[serde(default)]
        memory_mode: MemoryMode,
        java_path: Option<String>,
        #[serde(default)]
        icon: Option<u8>,
    },
    Clone {
        id: String,
    },
    Delete {
        id: String,
    },
    MoveInstance {
        id: String,
        before_id: Option<String>,
    },
    Import {
        source: String,
        name: String,
        #[serde(default)]
        version: String,
    },
    ScanFolders {
        scan_id: String,
        paths: Vec<String>,
    },
    CancelFolderScan {
        scan_id: String,
    },
    ImportFolders {
        paths: Vec<String>,
    },
    SuggestedInstances,
    PickImportDirectory {
        #[serde(default)]
        path: String,
    },
    PickInstanceLocation {
        #[serde(default)]
        path: String,
    },
    AddVersion {
        id: String,
        version: String,
    },
    SelectVersion {
        id: String,
        version: String,
    },
    RenameVersion {
        id: String,
        version: String,
        name: String,
    },
    DeleteVersion {
        id: String,
        version: String,
    },
    VersionContentCounts {
        id: String,
        version: String,
    },
    VersionContent {
        id: String,
        version: String,
        kind: nekocrate_core::version_management::ContentKind,
    },
    VersionContentIndex {
        id: String,
        version: String,
    },
    VersionContentSummaries {
        id: String,
        version: String,
        files: Vec<String>,
    },
    VersionContentMetadata {
        id: String,
        version: String,
        kind: nekocrate_core::version_management::ContentKind,
        file: String,
    },
    OpenContentFolder {
        id: String,
        version: String,
        kind: nekocrate_core::version_management::ContentKind,
        file: String,
        #[serde(default)]
        source: Option<String>,
    },
    VersionContentSource {
        id: String,
        version: String,
        kind: nekocrate_core::version_management::ContentKind,
        file: String,
        source: String,
    },
    VersionMetadata {
        id: String,
        version: String,
    },
    VersionStorage {
        id: String,
        version: String,
        #[serde(default)]
        relative: String,
    },
    Install {
        id: String,
        #[serde(default)]
        repair: bool,
    },
    Launch {
        id: String,
    },
    Stop {
        id: String,
    },
    Cancel,
    Pause,
    Resume,
    BeginLogin {
        session_id: String,
    },
    WaitLogin {
        session_id: String,
    },
    CancelLogin {
        session_id: String,
    },
    RefreshAccount,
    AccountSkin {
        id: String,
    },
    SelectAccount {
        id: String,
    },
    RemoveAccount {
        id: String,
    },
    OpenFolder {
        id: Option<String>,
    },
    InstanceDirectory {
        id: String,
    },
}
#[tauri::command]
async fn command(
    request: Request,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    dispatch(request, app, state).await
}

// Keep the large operation state machine out of Tauri's by-value IPC/spawn
// wrappers. On Windows dev0.7 those wrappers used 355K + 474K + 237K of the
// main thread's 1 MiB stack before even polling the first snapshot request.
// The non-inlined allocation boundary also bounds temporary stack copies.
#[inline(never)]
fn dispatch<'a>(
    request: Request,
    app: tauri::AppHandle,
    state: State<'a, AppState>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send + 'a>> {
    Box::pin(async move {
        if state.updater.installing()
            && !matches!(request, Request::Snapshot | Request::LauncherUpdateState)
        {
            return Err("正在安装启动器更新，请稍候".into());
        }
        let handle = app.clone();
        let sink: EventSink = Arc::new(move |event| {
            let _ = handle.emit("launcher-event", event);
        });
        let launcher = &state.launcher;
        // Log operation names, never account/proxy/API-key request payloads.
        let diagnostic_operation = match &request {
            Request::Versions => Some("versions"),
            Request::LoaderVersions { .. } | Request::FabricVersions { .. } => {
                Some("loader_versions")
            }
            Request::AddLoaderVersion { .. } | Request::AddFabricVersion { .. } => {
                Some("add_loader_version")
            }
            Request::Install { .. } => Some("install"),
            Request::Launch { .. } => Some("launch"),
            Request::CheckUpdate => Some("check_update"),
            Request::DownloadLauncherUpdate { .. } => Some("download_launcher_update"),
            Request::InstallLauncherUpdate => Some("install_launcher_update"),
            Request::BeginLogin { .. } | Request::WaitLogin { .. } | Request::RefreshAccount => {
                Some("login")
            }
            _ => None,
        };
        let result: Result<Value, anyhow::Error> = async {
        Ok(match request{
            Request::CopyText{text}=>{app.clipboard().write_text(text)?;json!(true)},
            Request::Snapshot=>{
                let recovery = launcher.clone();
                let notify = sink.clone();
                tauri::async_runtime::spawn(async move {
                    if recovery.recover_download_progress().await.unwrap_or(false) {
                        notify(Event::new("download_progress_recovered", ""));
                    }
                });
                json!({"state":launcher.snapshot(),"manifest":launcher.cached_versions(),"busy":state.jobs.busy()})
            },
            Request::Versions=>{let (cancel,_guard)=state.job().await?;json!(launcher.versions(&cancel).await?)},
            Request::FabricVersions{game_version}=>json!(launcher.fabric_versions(&game_version,&CancellationToken::new()).await?),
            Request::LoaderVersions{game_version,kind}=>json!(launcher.loader_versions(&game_version,&kind,&CancellationToken::new()).await?),
            Request::AddLoaderVersion{id,game_version,loader}=>{
                let (cancel,_guard)=state.job().await?;
                json!(launcher.add_loader_version(&id,&game_version,loader,&cancel).await?)
            },
            Request::AddFabricVersion{id,game_version,loader_version}=>{
                let (cancel,_guard)=state.job().await?;
                json!(launcher.add_fabric_version(&id,&game_version,&loader_version,&cancel,&sink).await?)
            },
            Request::PreviewModpack{path}=>{
                let launcher = launcher.clone();
                json!(tauri::async_runtime::spawn_blocking(move || launcher.preview_modpack(&path)).await??)
            },
            Request::PrepareModpack{id,path,name,optional}=>{let (_cancel,_guard)=state.job().await?;json!(launcher.prepare_modpack(&id,&path,&name,&optional).await?)},
            Request::ModpackStatus{id,version}=>json!(launcher.modpack_status(&id,&version).await?),
            Request::SupplyModpackFiles{id,version,paths}=>{let (_cancel,_guard)=state.job().await?;json!(launcher.supply_modpack_files(&id,&version,paths).await?)},
            Request::CurseforgeKeyStatus=>json!(launcher.curseforge_key_configured()?),
            Request::SaveCurseforgeKey{key}=>{ launcher.save_curseforge_key(&key.0)?; json!(true) },
            Request::Java=>json!(nekocrate_core::java::discover(launcher.root()).await),
            Request::MemoryOptions{id}=>{
                let launcher = launcher.clone();
                json!(tauri::async_runtime::spawn_blocking(move || launcher.memory_options(&id)).await??)
            },
            Request::CheckUpdate=>json!(updater::check(launcher).await?),
            Request::LauncherUpdateState=>json!(state.updater.progress()),
            Request::DownloadLauncherUpdate{version}=>{state.updater.download(&app,launcher,&version).await?;json!(state.updater.progress())},
            Request::CancelLauncherUpdate=>{state.updater.cancel();json!(true)},
            Request::InstallLauncherUpdate=>{
                let (_cancel,_guard)=state.job().await?;
                if !launcher.snapshot().running.is_empty() { anyhow::bail!("请先退出正在运行的游戏，再安装启动器更新"); }
                state.updater.install(&app,launcher).await?;
                json!(true)
            },
            Request::ProxyPassword=>json!(launcher.proxy_password()?),
            Request::SaveSettings{settings}=>{launcher.save_settings(settings)?;json!(true)},
            Request::Create{name,version,directory,icon}=>{
                let (_cancel,_guard)=state.job().await?;
                let launcher = launcher.clone();
                json!(tauri::async_runtime::spawn_blocking(move || {
                    if icon.is_some_and(|i| i >= INSTANCE_ICON_COUNT) { anyhow::bail!("请选择有效的实例图标"); }
                    let instance = launcher.create_instance_at(&name,&version,directory.as_deref().map(std::path::Path::new))?;
                    if let Some(icon) = icon {
                        launcher.update_instance_memory(&instance.id, &instance.name, nekocrate_core::memory::MemoryConfig { mode: instance.memory_mode, custom_mb: instance.memory_mb }, instance.java_path.clone(), Some(icon))?;
                    }
                    launcher.instance(&instance.id)
                }).await??)
            },
            Request::Update{id,name,memory_mb,memory_mode,java_path,icon}=>{launcher.update_instance_memory(&id,&name,nekocrate_core::memory::MemoryConfig{mode:memory_mode,custom_mb:memory_mb},java_path,icon)?;json!(true)},
            Request::Clone{id}=>{let (_cancel,_guard)=state.job().await?;json!(launcher.clone_instance(&id).await?)},
            Request::Delete{id}=>{let (_cancel,_guard)=state.job().await?;launcher.delete_instance(&id)?;json!(true)},
            Request::MoveInstance{id,before_id}=>{launcher.move_instance(&id,before_id.as_deref())?;json!(true)},
            Request::Import{source,name,version}=>{let (_cancel,_guard)=state.job().await?;json!(launcher.import_instance(PathBuf::from(source),name,version).await?)},
            Request::ScanFolders{scan_id,paths}=>{
                let guard = state.folder_scan.start(scan_id);
                let launcher = launcher.clone();
                json!(tauri::async_runtime::spawn_blocking(move || {
                    launcher.scan_folders(&paths.into_iter().map(PathBuf::from).collect::<Vec<_>>(), &guard.cancel)
                }).await??)
            },
            Request::CancelFolderScan{scan_id}=>{state.folder_scan.cancel(Some(&scan_id));json!(true)},
            Request::ImportFolders{paths}=>{
                let (_cancel,_guard)=state.job().await?;
                json!(launcher.import_folders(paths.into_iter().map(PathBuf::from).collect()).await?)
            },
            Request::SuggestedInstances=>{
                let launcher = launcher.clone();
                json!(tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<_> {
                    // AppImage's running binary is inside a temporary mount.
                    let executable = if cfg!(target_os = "linux") {
                        std::env::var_os("APPIMAGE").map(PathBuf::from)
                            .filter(|path| path.is_absolute() && path.is_file())
                    } else { None }.map(Ok).unwrap_or_else(std::env::current_exe)?;
                    Ok(launcher.suggested_instances(&executable))
                }).await??)
            },
            Request::PickImportDirectory{path}=>json!(import_picker::pick(&app,&path,launcher.ui_language() == "en-US",false).await?),
            Request::PickInstanceLocation{path}=>json!(import_picker::pick(&app,&path,launcher.ui_language() == "en-US",true).await?),
            Request::AddVersion{id,version}=>{let (_cancel,_guard)=state.job().await?;json!(launcher.add_version(&id,&version)?)},
            Request::SelectVersion{id,version}=>{let (_cancel,_guard)=state.job().await?;launcher.select_version(&id,&version)?;json!(true)},
            Request::RenameVersion{id,version,name}=>{launcher.rename_version(&id,&version,&name)?;json!(true)},
            Request::DeleteVersion{id,version}=>{
                let (_cancel,_guard)=state.job().await?;
                let launcher = launcher.clone();
                json!(tauri::async_runtime::spawn_blocking(move || launcher.delete_version(&id,&version)).await??)
            },
            Request::VersionContentCounts{id,version}=>{
                let launcher = launcher.clone();
                json!(tauri::async_runtime::spawn_blocking(move || launcher.version_content_counts(&id,&version)).await??)
            },
            Request::VersionContent{id,version,kind}=>{
                let launcher = launcher.clone();
                json!(tauri::async_runtime::spawn_blocking(move || launcher.version_content(&id,&version,kind)).await??)
            },
            Request::VersionContentIndex{id,version}=>{
                let launcher = launcher.clone();
                json!(tauri::async_runtime::spawn_blocking(move || launcher.version_content_index(&id,&version)).await??)
            },
            Request::VersionContentSummaries{id,version,files}=>{
                let launcher = launcher.clone();
                json!(tauri::async_runtime::spawn_blocking(move || launcher.version_content_summaries(&id,&version,&files)).await??)
            },
            Request::VersionContentMetadata{id,version,kind,file}=>{
                let launcher = launcher.clone();
                json!(tauri::async_runtime::spawn_blocking(move || launcher.version_content_metadata(&id,&version,kind,&file)).await??)
            },
            Request::OpenContentFolder{id,version,kind,file,source}=>{
                let launcher = launcher.clone();
                let path = tauri::async_runtime::spawn_blocking(move || launcher.version_content_path(&id,&version,kind,&file,source.as_deref())).await??;
                app.opener().reveal_item_in_dir(path)?;
                json!(true)
            },
            Request::VersionContentSource{id,version,kind,file,source}=>{
                let launcher = launcher.clone();
                json!(tauri::async_runtime::spawn_blocking(move || launcher.version_content_source(&id,&version,kind,&file,&source)).await??)
            },
            Request::VersionMetadata{id,version}=>{
                let launcher = launcher.clone();
                json!(tauri::async_runtime::spawn_blocking(move || launcher.version_metadata(&id,&version)).await??)
            },
            Request::VersionStorage{id,version,relative}=>{
                let launcher = launcher.clone();
                json!(tauri::async_runtime::spawn_blocking(move || launcher.version_storage(&id,&version,&relative)).await??)
            },
            Request::Install{id,repair}=>{
                let (cancel,_guard)=state.job().await?;
                let receiver=_guard.enable_pause();
                let operation = async {
                    if repair { launcher.repair(&id,&cancel,&sink).await }
                    else { launcher.install(&id,&cancel,&sink).await }
                };
                let operation = nekocrate_core::loader::with_pause(receiver.clone(), operation);
                pausable::run(operation,&cancel,receiver).await?;
                json!(true)
            },
            Request::Launch{id}=>{let (cancel,_guard)=state.job().await?;json!(launcher.launch(&id,&cancel,sink).await?)},
            Request::Stop{id}=>{launcher.stop_game(&id)?;json!(true)},
            Request::Cancel=>{state.jobs.cancel_task();json!(true)},
            Request::Pause | Request::Resume=>{
                state.jobs.set_paused(matches!(request,Request::Pause))?;
                json!(true)
            },
            Request::BeginLogin{session_id}=>{
                let guard=state.jobs.begin_login(session_id.clone()).await?;
                let c=guard.login(launcher.begin_login()).await?;
                let mut value=json!(c);
                value["session_id"]=json!(session_id);
                guard.store_challenge(c)?;
                value
            },
            Request::WaitLogin{session_id}=>{
                let (challenge,guard)=state.jobs.wait_login(&session_id)?;
                json!(guard.login(launcher.finish_login(challenge,&guard.cancel)).await?)
            },
            Request::CancelLogin{session_id}=>{state.jobs.cancel_login(&session_id).await;json!(true)},
            Request::RefreshAccount=>{let (_cancel,_guard)=state.job().await?;json!(launcher.refresh_account().await?)},
            Request::AccountSkin{id}=>json!(launcher.account_skin(&id).await?),
            Request::SelectAccount{id}=>{let (_cancel,_guard)=state.job().await?;launcher.select_account(&id)?;json!(true)},
            Request::RemoveAccount{id}=>{let (_cancel,_guard)=state.job().await?;launcher.remove_account(&id)?;json!(true)},
            Request::OpenFolder{id}=>{let path=match id{Some(id)=>launcher.game_dir(&id)?,None=>launcher.root().to_owned()};app.opener().open_path(path.to_string_lossy(),None::<&str>)?;json!(true)},
            Request::InstanceDirectory{id}=>json!(launcher.game_dir(&id)?),
        })
    }.await;
        result.map_err(|error| {
            let error = nekocrate_core::network::explain_error(error);
            if let Some(operation) = diagnostic_operation
                && !error.to_string().contains("取消")
                && let Err(log_error) =
                    nekocrate_core::diagnostics::record_error(launcher.root(), operation, &error)
            {
                eprintln!("Could not write launcher diagnostics: {log_error}");
            }
            nekocrate_core::diagnostics::redact(&format!("{error:#}"))
        })
    })
}

fn command_future_size() -> usize {
    fn size<'a, F>(
        _command: impl FnOnce(Request, tauri::AppHandle, State<'a, AppState>) -> F,
    ) -> usize {
        std::mem::size_of::<F>()
    }
    size(command)
}

pub fn run() {
    let verifying = std::env::args_os().any(|arg| arg == "--verify-frontend");
    if !verifying {
        startup::install();
    }
    let context = tauri::generate_context!();
    // Exercise the actual embedded asset map without opening a window or user data.
    // This runs on each release runner using the already-built executable.
    if verifying {
        let assets = context.assets();
        let valid_entry = assets
            .get(&"index.html".into())
            .is_some_and(|html| !html.is_empty());
        let has_script = assets
            .iter()
            .any(|(key, bytes)| key.as_ref().ends_with(".js") && !bytes.is_empty());
        let has_style = assets
            .iter()
            .any(|(key, bytes)| key.as_ref().ends_with(".css") && !bytes.is_empty());
        if !(valid_entry && has_script && has_style) {
            eprintln!(
                "Embedded frontend is incomplete: expected index.html, JavaScript and CSS at the frontend root"
            );
            std::process::exit(1);
        }
        // Use the same already-built release executable on each native runner.
        // Asset checks alone do not exercise Tauri's first IPC dispatch.
        if command_future_size() > 4096 {
            eprintln!(
                "IPC command future is too large for Windows stack safety: {} bytes",
                command_future_size()
            );
            std::process::exit(1);
        }
        println!(
            "Embedded frontend and IPC future size verified ({} bytes)",
            command_future_size()
        );
        return;
    }
    startup::phase("initializing_tauri");
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            startup::phase("opening_launcher_data");
            let launcher = Launcher::open(None)
                .inspect_err(|error| startup::failure("startup_data", error))?;
            app.manage(AppState {
                launcher,
                jobs: jobs::Jobs::default(),
                updater: updater::UpdateManager::default(),
                folder_scan: folder_scan::FolderScanManager::default(),
            });
            startup::phase("launcher_ready");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![command])
        .build(context);
    let app = match app {
        Ok(app) => app,
        Err(error) => {
            startup::failure("startup_tauri", &error.into());
            std::process::exit(1);
        }
    };
    app.run(|app, event| {
        if let tauri::RunEvent::Exit = event
            && let Some(state) = app.try_state::<AppState>()
        {
            state.launcher.stop_all();
            state.folder_scan.cancel(None);
            state.jobs.cancel_all();
        }
    });
}

#[cfg(test)]
mod startup_tests {
    use super::*;

    #[test]
    fn ipc_entry_does_not_embed_the_large_operation_future() {
        let size = command_future_size();
        assert!(
            size <= 4096,
            "IPC command future: {size} bytes; keep the dispatch state on the heap"
        );
    }
}
