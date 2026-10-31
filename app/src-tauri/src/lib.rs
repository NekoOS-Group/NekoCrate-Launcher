use nekocrate_core::{Launcher, model::*};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
use tauri::{Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tokio_util::sync::CancellationToken;

mod clipboard;
mod download_retry;
mod file_drag;
mod folder_scan;
mod import_picker;
mod jobs;
mod pausable;
#[cfg(any(target_os = "windows", test))]
mod portable_update;
mod resource_manager;
mod startup;
mod updater;

struct AppState {
    launcher: Launcher,
    jobs: jobs::Jobs,
    download_retries: download_retry::Retries,
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
    ResourceManager {
        action: resource_manager::Action,
    },
    DownloadDeclaredMod {
        id: String,
        version: String,
        key: String,
    },
    RetryDownloadFile {
        file: String,
    },
    CrashReport {
        id: String,
    },
    SetModEnabled {
        id: String,
        version: String,
        file: String,
        enabled: bool,
    },
    ModConfigFiles {
        id: String,
        version: String,
        file: String,
    },
    ReadModConfig {
        id: String,
        version: String,
        file: String,
        config: String,
    },
    SaveModConfig {
        id: String,
        version: String,
        file: String,
        config: String,
        revision: String,
        text: String,
    },
    SearchModpacks {
        platform: nekocrate_core::resources::Platform,
        query: String,
        offset: usize,
    },
    ModpackVersions {
        platform: nekocrate_core::resources::Platform,
        project: String,
        offset: usize,
    },
    DownloadModpack {
        platform: nekocrate_core::resources::Platform,
        project: String,
        version: String,
    },
    ResourceTarget {
        id: String,
        version: String,
    },
    SearchResources {
        scope: nekocrate_core::resources::ResourceScope,
        query: String,
        offset: usize,
    },
    ResourceVersions {
        scope: nekocrate_core::resources::ResourceScope,
        project: String,
    },
    PrepareResource {
        scope: nekocrate_core::resources::ResourceScope,
        project: String,
        version: String,
        world: Option<String>,
    },
    InspectResources {
        paths: Vec<PathBuf>,
    },
    PrepareLocalResources {
        id: String,
        version: String,
        inputs: Vec<nekocrate_core::resources::LocalImport>,
        world: Option<String>,
    },
    CopyResource {
        source: nekocrate_core::resources::ResourceCopySource,
        id: String,
        version: String,
    },
    CopyResources {
        sources: Vec<nekocrate_core::resources::ResourceCopySource>,
        id: String,
        version: String,
    },
    ExportResources {
        sources: Vec<nekocrate_core::resources::ResourceCopySource>,
    },
    ExportResource {
        source: nekocrate_core::resources::ResourceCopySource,
    },
    ResourcePlans {
        id: String,
        version: String,
    },
    InstallResources {
        id: String,
        version: String,
        task: String,
    },
    CopyText {
        text: String,
    },
    Snapshot,
    RefreshDirectories,
    SaveUiPreference {
        key: String,
        value: String,
    },
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
        #[serde(default)]
        launch: Option<nekocrate_core::launch_options::LaunchOptions>,
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
    MoveVersion {
        id: String,
        version: String,
        before_id: Option<String>,
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
        file: Option<String>,
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
        id: Option<String>,
    },
    SetGuestName {
        name: String,
    },
    RenameAccount {
        id: String,
        name: String,
    },
    RemoveAccount {
        id: String,
    },
    OpenFolder {
        id: Option<String>,
    },
    LaunchDirectory {
        id: String,
        isolation: Option<nekocrate_core::launch_options::IsolationMode>,
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
        let retry_scopes = Arc::new(download_retry::Scopes::default());
        let scopes = retry_scopes.clone();
        let sink: EventSink = Arc::new(move |event| {
            scopes.observe(&event);
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
            Request::DownloadDeclaredMod { .. } => Some("download_mod"),
            Request::Launch { .. } => Some("launch"),
            Request::CheckUpdate => Some("check_update"),
            Request::DownloadLauncherUpdate { .. } => Some("download_launcher_update"),
            Request::InstallLauncherUpdate => Some("install_launcher_update"),
            Request::BeginLogin { .. } | Request::WaitLogin { .. } | Request::RefreshAccount => {
                Some("login")
            }
            Request::RenameAccount { .. } => Some("rename_account"),
            _ => None,
        };
        let result: Result<Value, anyhow::Error> = async {
        Ok(match request{
            Request::ResourceManager{action}=>resource_manager::handle(action,&state,&app,&sink).await?,
            Request::DownloadDeclaredMod{id,version,key}=>{
                let (cancel,guard)=state.job().await?;
                let receiver=guard.enable_pause();
                let operation=launcher.download_declared_mod(&id,&version,&key,&cancel,&sink);
                json!(pausable::run(operation,&cancel,receiver).await?)
            },
            Request::SetModEnabled{id,version,file,enabled}=>{let launcher=launcher.clone(); json!(tauri::async_runtime::spawn_blocking(move || launcher.set_mod_enabled(&id,&version,&file,enabled)).await??)},
            Request::ModConfigFiles{id,version,file}=>{let launcher=launcher.clone(); json!(tauri::async_runtime::spawn_blocking(move || launcher.mod_config_files(&id,&version,&file)).await??)},
            Request::ReadModConfig{id,version,file,config}=>{let launcher=launcher.clone(); json!(tauri::async_runtime::spawn_blocking(move || launcher.read_mod_config(&id,&version,&file,&config)).await??)},
            Request::SaveModConfig{id,version,file,config,revision,text}=>{let launcher=launcher.clone(); json!(tauri::async_runtime::spawn_blocking(move || launcher.save_mod_config(&id,&version,&file,&config,&revision,&text)).await??)},
            Request::CrashReport{id}=>{let launcher=launcher.clone(); json!(tauri::async_runtime::spawn_blocking(move || launcher.crash_report(&id)).await??)},
            Request::RetryDownloadFile{file}=>{
                let (cancel,_guard)=state.job().await?;
                let entry=state.download_retries.get(&file)?;

                let scoped = match &entry.instance { Some(id)=>Event::scoped(&sink,id,entry.step.as_deref()), None=>sink.clone() };
                launcher.retry_download_file(&entry.file,entry.instance.as_deref(),&cancel,&scoped).await?;
                state.download_retries.completed(&file);
                json!(true)
            },
            Request::CopyText{text}=>{clipboard::write_text(&app, text)?;json!(true)},
            Request::RefreshDirectories=>{let launcher=launcher.clone();tauri::async_runtime::spawn_blocking(move || launcher.refresh_directories()).await??;json!(true)},
            Request::SaveUiPreference{key,value}=>{launcher.save_ui_preference(&key,&value)?;json!(true)},
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
            Request::ResourceTarget{id,version}=>{
                let launcher=launcher.clone();
                json!(tauri::async_runtime::spawn_blocking(move || launcher.resource_target(&id,&version)).await??)
            },
            Request::SearchModpacks{platform,query,offset}=>json!(launcher.search_modpacks(platform,&query,offset,&CancellationToken::new()).await?),
            Request::ModpackVersions{platform,project,offset}=>json!(launcher.modpack_versions(platform,&project,offset,&CancellationToken::new()).await?),
            Request::DownloadModpack{platform,project,version}=>{
                let (cancel,guard)=state.job().await?;
                let receiver=guard.enable_pause();
                let operation=launcher.download_modpack(platform,&project,&version,&cancel,&sink);
                json!(pausable::run(operation,&cancel,receiver).await?)
            },
            Request::SearchResources{scope,query,offset}=>json!(launcher.search_resources(&scope,&query,offset,&CancellationToken::new()).await?),
            Request::ResourceVersions{scope,project}=>json!(launcher.resource_versions(&scope,&project,&CancellationToken::new()).await?),
            Request::PrepareResource{scope,project,version,world}=>{
                let (cancel,_guard)=state.job().await?;
                json!(launcher.prepare_resource(&scope,&project,&version,world.as_deref(),&cancel).await?)
            },
            Request::InspectResources{paths}=>{
                let launcher=launcher.clone();
                json!(tauri::async_runtime::spawn_blocking(move || launcher.inspect_resources(&paths)).await??)
            },
            Request::PrepareLocalResources{id,version,inputs,world}=>{
                let (cancel,_guard)=state.job().await?;
                let launcher=launcher.clone();
                json!(tauri::async_runtime::spawn_blocking(move || launcher.prepare_local_resources(&id,&version,&inputs,world.as_deref(),&cancel)).await??)
            },
            Request::CopyResource{source,id,version}=>{
                let (cancel,_guard)=state.job().await?;
                let worker=launcher.clone();
                let token=cancel.clone();
                let plan=tauri::async_runtime::spawn_blocking(move || worker.prepare_resource_copy(&source,&id,&version,&token)).await??;
                if let Some(plan)=plan {
                    launcher.install_resources(&plan.target.instance,&plan.target.version,&plan.id,&cancel,&sink).await?;
                    json!(true)
                } else { json!(false) }
            },
            Request::CopyResources{sources,id,version}=>{
                let (cancel,_guard)=state.job().await?;
                let worker=launcher.clone(); let token=cancel.clone();
                let plan=tauri::async_runtime::spawn_blocking(move || worker.prepare_resource_copies(&sources,&id,&version,&token)).await??;
                if let Some(plan)=plan { launcher.install_resources(&plan.target.instance,&plan.target.version,&plan.id,&cancel,&sink).await?;json!(true) } else {json!(false)}
            },
            Request::ExportResources{sources}=>{
                anyhow::ensure!(!sources.is_empty() && sources.len() <= 256,"每次最多拖动 256 项资源");
                let worker=launcher.clone();
                let paths=tauri::async_runtime::spawn_blocking(move || sources.iter().map(|source| worker.resource_export_path(source)).collect::<anyhow::Result<std::collections::BTreeSet<_>>>()).await??;
                let window=app.get_webview_window("main").ok_or_else(|| anyhow::anyhow!("启动器窗口已关闭"))?;
                json!(file_drag::start(window,paths.into_iter().collect()).await?)
            },
            Request::ExportResource{source}=>{
                let worker=launcher.clone();
                let path=tauri::async_runtime::spawn_blocking(move || worker.resource_export_path(&source)).await??;
                let window=app.get_webview_window("main").ok_or_else(|| anyhow::anyhow!("启动器窗口已关闭"))?;
                json!(file_drag::start(window,vec![path]).await?)
            },
            Request::ResourcePlans{id,version}=>{
                let launcher=launcher.clone();
                json!(tauri::async_runtime::spawn_blocking(move || launcher.resource_plans(&id,&version)).await??)
            },
            Request::InstallResources{id,version,task}=>{
                let (cancel,guard)=state.job().await?;
                let receiver=guard.enable_pause();
                let operation=launcher.install_resources(&id,&version,&task,&cancel,&sink);
                pausable::run(operation,&cancel,receiver).await?;
                json!(true)
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
            Request::Update{id,name,memory_mb,memory_mode,java_path,icon,launch}=>{launcher.update_instance_options(&id,&name,nekocrate_core::memory::MemoryConfig{mode:memory_mode,custom_mb:memory_mb},java_path,icon,launch)?;json!(true)},
            Request::Clone{id}=>{let (_cancel,_guard)=state.job().await?;json!(launcher.clone_instance(&id).await?)},
            Request::Delete{id}=>{
                let (_cancel,_guard)=state.job().await?;
                let launcher = launcher.clone();
                let scope = id.clone();
                tauri::async_runtime::spawn_blocking(move || launcher.delete_instance(&id)).await??;
                state.download_retries.invalidate_instance(&scope);
                json!(true)
            },
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
            Request::MoveVersion{id,version,before_id}=>{launcher.move_version(&id,&version,before_id.as_deref())?;json!(true)},
            Request::RenameVersion{id,version,name}=>{launcher.rename_version(&id,&version,&name)?;state.download_retries.invalidate_instance(&id);json!(true)},
            Request::DeleteVersion{id,version}=>{
                let (_cancel,_guard)=state.job().await?;
                let launcher = launcher.clone();
                let scope = id.clone();
                let result = tauri::async_runtime::spawn_blocking(move || launcher.delete_version(&id,&version)).await??;
                state.download_retries.invalidate_instance(&scope);
                json!(result)
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
                let reveal = file.is_some();
                let path = tauri::async_runtime::spawn_blocking(move || match file {
                    Some(file) => launcher.version_content_path(&id,&version,kind,&file,source.as_deref()),
                    None => launcher.version_content_directory(&id,&version,kind),
                }).await??;
                if reveal { app.opener().reveal_item_in_dir(path)?; }
                else { app.opener().open_path(path.to_string_lossy(),None::<&str>)?; }
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
                json!(pausable::run(operation,&cancel,receiver).await?)
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
            Request::SelectAccount{id}=>{let (_cancel,_guard)=state.job().await?;if let Some(id)=id {launcher.select_account(&id)?;} else {launcher.select_guest()?;}json!(true)},
            Request::SetGuestName{name}=>json!(launcher.set_guest_name(&name)?),
            Request::RenameAccount{id,name}=>json!(Box::pin(launcher.rename_account(&id,&name)).await?),
            Request::RemoveAccount{id}=>{let (_cancel,_guard)=state.job().await?;launcher.remove_account(&id)?;json!(true)},
            Request::OpenFolder{id}=>{let path=match id{Some(id)=>launcher.game_dir(&id)?,None=>launcher.root().to_owned()};app.opener().open_path(path.to_string_lossy(),None::<&str>)?;json!(true)},
            Request::LaunchDirectory{id,isolation}=>{let launcher = launcher.clone(); json!(tauri::async_runtime::spawn_blocking(move || launcher.launch_directory(&id,isolation)).await??)},
            Request::InstanceDirectory{id}=>json!(launcher.game_dir(&id)?),
        })
    }.await;
        result.map_err(|error| {
            state.download_retries.capture(&error, &retry_scopes);
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
    #[cfg(target_os = "windows")]
    if portable_update::run_helper() {
        return;
    }
    if std::env::args_os().any(|arg| arg == "--verify-portable") {
        let launcher = Launcher::open(None).expect("portable data initialization failed");
        assert!(launcher.is_portable(), "portable marker was not detected");
        let snapshot = launcher.snapshot();
        assert_eq!(snapshot.database.instances.len(), 1);
        assert_eq!(
            snapshot.database.instances[0].game_dir(launcher.root()),
            launcher.root().parent().unwrap().join(".minecraft")
        );
        #[cfg(target_os = "windows")]
        portable_update::verify_native_swap(launcher.root()).expect("portable update swap failed");
        println!("Portable data and adjacent Minecraft directory verified");
        return;
    }
    let verifying = std::env::args_os().any(|arg| arg == "--verify-frontend");
    if !verifying {
        startup::install();
    }
    let mut context = tauri::generate_context!();
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
    startup::phase("opening_launcher_data");
    let launcher = match Launcher::open(None) {
        Ok(launcher) => launcher,
        Err(error) => {
            startup::failure("startup_data", &error);
            eprintln!("{error:#}");
            rfd::MessageDialog::new()
                .set_title("猫咪盒子启动器 · NekoCrate Launcher")
                .set_description(format!("{error:#}"))
                .set_level(rfd::MessageLevel::Error)
                .show();
            return;
        }
    };
    if launcher.is_portable() {
        context.config_mut().app.app_directories_override = Some(
            tauri::utils::config::AppDirectoriesOverride::Root(launcher.root().to_owned()),
        );
        #[cfg(target_os = "macos")]
        for window in &mut context.config_mut().app.windows {
            // WKWebView does not support relocating its data directory. A private
            // store avoids writing portable UI state into the installed app store.
            window.incognito = true;
        }
    }
    startup::phase("initializing_tauri");
    let app = tauri::Builder::default()
        .plugin(clipboard::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            app.manage(AppState {
                launcher,
                jobs: jobs::Jobs::default(),
                download_retries: download_retry::Retries::default(),
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
