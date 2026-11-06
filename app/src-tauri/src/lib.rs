mod blocking;
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
mod game_export;
mod home_widgets;
mod import_picker;
mod jobs;
mod pausable;
#[cfg(any(target_os = "windows", test))]
mod portable_update;
mod resource_manager;
mod startup;
mod tagged_json;
mod updater;
#[cfg(not(target_os = "windows"))]
mod window_icon;

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
// Generate field validation as an inherent method. The trait implementation
// below supplies the unchanged, internally tagged JSON wire representation.
#[serde(remote = "Self", rename_all = "snake_case")]
enum Request {
    HomeWidgetFeed {
        url: String,
    },
    HomeWidgetGlyphs {
        text: String,
    },
    HomeWidgetLocal,
    HomeServerStatus {
        address: String,
    },
    ExportGame {
        id: String,
        version: Option<String>,
        platform: Option<String>,
    },
    DragGameExport {
        export: String,
    },
    SaveGameExport {
        export: String,
        name: String,
    },
    DiscardGameExport {
        export: String,
    },
    CopyVersion {
        id: String,
        version: String,
        target: String,
    },
    ReadDroppedConfig {
        path: PathBuf,
    },
    ResolveProjectLink {
        url: String,
    },
    PreviewResourceDrop {
        id: String,
        version: String,
        #[serde(default)]
        paths: Vec<PathBuf>,
        #[serde(default)]
        sources: Vec<nekocrate_core::resources::ResourceCopySource>,
        kind: Option<nekocrate_core::resources::ResourceKind>,
        world: Option<String>,
    },
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
    OpenCrashReport {
        id: String,
        version: String,
        time: u64,
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
    ResourceProject {
        platform: nekocrate_core::resources::Platform,
        project: String,
        #[serde(default)]
        refresh: bool,
    },
    ResourceProjectSummary {
        platform: nekocrate_core::resources::Platform,
        project: String,
    },
    ResourceImage {
        url: String,
    },
    ModpackCategories {
        platform: nekocrate_core::resources::Platform,
    },
    SearchModpacks {
        platform: nekocrate_core::resources::Platform,
        query: String,
        offset: usize,
        #[serde(default)]
        filters: nekocrate_core::resources::ModpackFilters,
    },
    ModpackVersion {
        platform: nekocrate_core::resources::Platform,
        project: String,
        version: String,
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
    SearchResourceCatalog {
        scope: nekocrate_core::resources::ResourceCatalog,
        query: String,
        offset: usize,
    },
    CatalogResourceVersions {
        scope: nekocrate_core::resources::ResourceCatalog,
        project: String,
    },
    QueueVersionResources {
        id: String,
        version: String,
        minecraft: String,
        loader: Option<nekocrate_core::model::ModLoader>,
        resources: Vec<nekocrate_core::resources::ResourceSelection>,
        #[serde(default)]
        local: Vec<nekocrate_core::resources::ResourceCopySource>,
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
    PrepareMissingDependencies {
        id: String,
        version: String,
        time: u64,
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
        world: Option<String>,
    },
    CopyResources {
        sources: Vec<nekocrate_core::resources::ResourceCopySource>,
        id: String,
        version: String,
        world: Option<String>,
    },
    ExportResources {
        sources: Vec<nekocrate_core::resources::ResourceCopySource>,
    },
    RemoveResources {
        sources: Vec<nekocrate_core::resources::ResourceCopySource>,
        #[serde(default)]
        confirmed: bool,
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
    ImportGame {
        request: nekocrate_core::game_import::GameImport,
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
    VersionContentPresentation {
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
        version: Option<String>,
    },
    Launch {
        id: String,
        server: Option<String>,
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
    SyncAccount {
        id: String,
    },
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
impl<'de> Deserialize<'de> for Request {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        tagged_json::deserialize(deserializer, "op", Self::deserialize)
    }
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
            Request::BeginLogin { .. } | Request::WaitLogin { .. } => Some("login"),
            Request::SyncAccount { .. } => Some("sync_account"),
            Request::RenameAccount { .. } => Some("rename_account"),
            _ => None,
        };
        let result: Result<Value, anyhow::Error> = async {
        Ok(match request{
            Request::HomeWidgetFeed{url}=>home_widgets::fetch(launcher,&url).await?,
            Request::HomeWidgetGlyphs{text}=>json!(nekocrate_core::widget_font::glyphs(launcher,text).await?),
            Request::HomeWidgetLocal=>{let launcher=launcher.clone(); blocking::json(move || Ok::<_,anyhow::Error>(launcher.home_widget_local())).await??},
            Request::HomeServerStatus{address}=>json!(nekocrate_core::home_dashboard::server_status(&address).await?),
            Request::ResourceManager{action}=>resource_manager::handle(action,&state,&app,&sink).await?,
            Request::DownloadDeclaredMod{id,version,key}=>{
                let (cancel,guard)=state.job().await?;
                let receiver=guard.enable_pause();
                let operation=launcher.download_declared_mod(&id,&version,&key,&cancel,&sink);
                json!(pausable::run(operation,&cancel,receiver).await?)
            },
            Request::SetModEnabled{id,version,file,enabled}=>{let launcher=launcher.clone(); blocking::json(move || launcher.set_mod_enabled(&id,&version,&file,enabled)).await??},
            Request::ModConfigFiles{id,version,file}=>{let launcher=launcher.clone(); blocking::json(move || launcher.mod_config_files(&id,&version,&file)).await??},
            Request::ReadModConfig{id,version,file,config}=>{let launcher=launcher.clone(); blocking::json(move || launcher.read_mod_config(&id,&version,&file,&config)).await??},
            Request::SaveModConfig{id,version,file,config,revision,text}=>{let launcher=launcher.clone(); blocking::json(move || launcher.save_mod_config(&id,&version,&file,&config,&revision,&text)).await??},
            Request::CrashReport{id}=>{let launcher=launcher.clone(); blocking::json(move || launcher.crash_report(&id)).await??},
            Request::OpenCrashReport{id,version,time}=>{
                let launcher=launcher.clone();
                let path=blocking::run(move || launcher.crash_report_path(&id,&version,time)).await??;
                app.opener().open_path(path.to_string_lossy(),None::<&str>)?;
                json!(true)
            },
            Request::RetryDownloadFile{file}=>{
                let (cancel,_guard)=state.job().await?;
                let entry=state.download_retries.get(&file)?;

                let scoped = match &entry.instance { Some(id)=>Event::scoped(&sink,id,entry.step.as_deref()), None=>sink.clone() };
                launcher.retry_download_file(&entry.file,entry.instance.as_deref(),&cancel,&scoped).await?;
                state.download_retries.completed(&file);
                json!(true)
            },
            Request::CopyText{text}=>{clipboard::write_text(&app, text)?;json!(true)},
            Request::RefreshDirectories=>{let launcher=launcher.clone();blocking::json(move || launcher.refresh_directories()).await??;json!(true)},
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
                blocking::json(move || launcher.resource_target(&id,&version)).await??
            },
            Request::ResourceProject{platform,project,refresh}=>json!(launcher.resource_project_with_refresh(platform,&project,refresh,&CancellationToken::new()).await?),
            Request::ResourceProjectSummary{platform,project}=>json!(launcher.resource_project_summary(platform,&project,&CancellationToken::new()).await?),
            Request::ResourceImage{url}=>{
                use base64::Engine;
                let (bytes,mime)=launcher.resource_image(&url,&CancellationToken::new()).await?;
                json!(format!("data:{mime};base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes)))
            },
            Request::ModpackCategories{platform}=>json!(launcher.modpack_categories(platform,&CancellationToken::new()).await?),
            Request::SearchModpacks{platform,query,offset,filters}=>json!(launcher.search_modpacks_filtered(platform,&query,offset,&filters,&CancellationToken::new()).await?),
            Request::ModpackVersion{platform,project,version}=>json!(launcher.modpack_version(platform,&project,&version,&CancellationToken::new()).await?),
            Request::ModpackVersions{platform,project,offset}=>json!(launcher.modpack_versions(platform,&project,offset,&CancellationToken::new()).await?),
            Request::DownloadModpack{platform,project,version}=>{
                let (cancel,guard)=state.job().await?;
                let receiver=guard.enable_pause();
                let operation=launcher.download_modpack(platform,&project,&version,&cancel,&sink);
                json!(pausable::run(operation,&cancel,receiver).await?)
            },
            Request::SearchResourceCatalog{scope,query,offset}=>json!(launcher.search_resource_catalog(&scope,&query,offset,&CancellationToken::new()).await?),
            Request::CatalogResourceVersions{scope,project}=>json!(launcher.catalog_resource_versions(&scope,&project,&CancellationToken::new()).await?),
            Request::QueueVersionResources{id,version,minecraft,loader,resources,local}=>{
                let launcher=launcher.clone();
                blocking::json(move || launcher.queue_version_resources(&id,&version,&minecraft,loader,resources,local)).await??;
                json!(true)
            },
            Request::SearchResources{scope,query,offset}=>json!(launcher.search_resources(&scope,&query,offset,&CancellationToken::new()).await?),
            Request::ResourceVersions{scope,project}=>json!(launcher.resource_versions(&scope,&project,&CancellationToken::new()).await?),
            Request::PrepareResource{scope,project,version,world}=>{
                let (cancel,_guard)=state.job().await?;
                json!(launcher.prepare_resource(&scope,&project,&version,world.as_deref(),&cancel).await?)
            },
            Request::PrepareMissingDependencies{id,version,time}=>{
                let (cancel,_guard)=state.job().await?;
                json!(launcher.prepare_missing_dependencies(&id,&version,time,&cancel,&sink).await?)
            },
            Request::ExportGame{id,version,platform}=>{
                let (cancel,_guard)=state.job().await?;
                let portable=if let Some(platform)=platform { Some(game_export::portable(&app,launcher,&platform,&cancel,&sink).await?) } else {None};
                let launcher=launcher.clone();
                blocking::json(move || launcher.export_game(&id,version.as_deref(),portable,&cancel,&sink)).await??
            },
            Request::DragGameExport{export}=>{
                let path=launcher.exported_game_path(&export)?;
                let window=app.get_webview_window("main").ok_or_else(|| anyhow::anyhow!("启动器窗口已关闭"))?;
                json!(file_drag::start(window,vec![path]).await?)
            },
            Request::SaveGameExport{export,name}=>{
                let source=launcher.exported_game_path(&export)?;
                let name=std::path::Path::new(&name).file_name().and_then(|s| s.to_str()).unwrap_or("Minecraft.zip");
                if let Some(file)=rfd::AsyncFileDialog::new().set_file_name(name).add_filter("ZIP",&["zip"]).save_file().await {
                    let path=file.path().to_owned();
                    blocking::json(move || Ok(std::fs::copy(source,path)?)).await??;
                    json!(true)
                } else {json!(false)}
            },
            Request::DiscardGameExport{export}=>{ launcher.discard_game_export(&export)?;json!(true) },
            Request::CopyVersion{id,version,target}=>{
                let (cancel,_guard)=state.job().await?; let launcher=launcher.clone();
                blocking::json(move || launcher.copy_version(&id,&version,&target,&cancel,&sink)).await??
            },
            Request::PreviewResourceDrop{id,version,paths,sources,kind,world}=>{let worker=launcher.clone();blocking::json(move || worker.preview_resource_drop(&id,&version,&paths,&sources,kind,world.as_deref())).await??},
            Request::ResolveProjectLink{url}=>json!(launcher.resolve_project_link(&url,&CancellationToken::new()).await?),
            Request::ReadDroppedConfig{path}=>{
                blocking::json(move || nekocrate_core::version_management::mods::read_dropped_config(&path)).await??
            },
            Request::InspectResources{paths}=>{
                let launcher=launcher.clone();
                blocking::json(move || launcher.inspect_resources(&paths)).await??
            },
            Request::PrepareLocalResources{id,version,inputs,world}=>{
                let (cancel,_guard)=state.job().await?;
                let launcher=launcher.clone();
                blocking::json(move || launcher.prepare_local_resources(&id,&version,&inputs,world.as_deref(),&cancel)).await??
            },
            Request::CopyResource{source,id,version,world}=>{
                let (cancel,_guard)=state.job().await?;
                let worker=launcher.clone();
                let token=cancel.clone();
                let plan=blocking::run(move || worker.prepare_resource_copies_to(&[source],&id,&version,world.as_deref(),&token)).await??;
                if let Some(plan)=plan {
                    launcher.install_resources(&plan.target.instance,&plan.target.version,&plan.id,&cancel,&sink).await?;
                    json!(true)
                } else { json!(false) }
            },
            Request::CopyResources{sources,id,version,world}=>{
                let (cancel,_guard)=state.job().await?;
                let worker=launcher.clone(); let token=cancel.clone();
                let plan=blocking::run(move || worker.prepare_resource_copies_to(&sources,&id,&version,world.as_deref(),&token)).await??;
                if let Some(plan)=plan { launcher.install_resources(&plan.target.instance,&plan.target.version,&plan.id,&cancel,&sink).await?;json!(true) } else {json!(false)}
            },
            Request::RemoveResources{sources,confirmed}=>{
                let launcher=launcher.clone();
                blocking::json(move || launcher.remove_game_resources_confirmed(&sources,confirmed)).await??
            },
            Request::ExportResources{sources}=>{
                anyhow::ensure!(!sources.is_empty() && sources.len() <= 256,"每次最多拖动 256 项资源");
                let worker=launcher.clone();
                let paths=blocking::run(move || sources.iter().map(|source| worker.resource_export_path(source)).collect::<anyhow::Result<std::collections::BTreeSet<_>>>()).await??;
                let window=app.get_webview_window("main").ok_or_else(|| anyhow::anyhow!("启动器窗口已关闭"))?;
                json!(file_drag::start(window,paths.into_iter().collect()).await?)
            },
            Request::ExportResource{source}=>{
                let worker=launcher.clone();
                let path=blocking::run(move || worker.resource_export_path(&source)).await??;
                let window=app.get_webview_window("main").ok_or_else(|| anyhow::anyhow!("启动器窗口已关闭"))?;
                json!(file_drag::start(window,vec![path]).await?)
            },
            Request::ResourcePlans{id,version}=>{
                let launcher=launcher.clone();
                blocking::json(move || launcher.resource_plans(&id,&version)).await??
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
                blocking::json(move || launcher.preview_modpack(&path)).await??
            },
            Request::PrepareModpack{id,path,name,optional}=>{let (_cancel,_guard)=state.job().await?;json!(launcher.prepare_modpack(&id,&path,&name,&optional).await?)},
            Request::ModpackStatus{id,version}=>json!(launcher.modpack_status(&id,&version).await?),
            Request::SupplyModpackFiles{id,version,paths}=>{let (_cancel,_guard)=state.job().await?;json!(launcher.supply_modpack_files(&id,&version,paths).await?)},
            Request::Java=>json!(nekocrate_core::java::discover(launcher.root()).await),
            Request::MemoryOptions{id}=>{
                let launcher = launcher.clone();
                blocking::json(move || launcher.memory_options(&id)).await??
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
                blocking::json(move || {
                    if icon.is_some_and(|i| i >= INSTANCE_ICON_COUNT) { anyhow::bail!("请选择有效的实例图标"); }
                    let instance = launcher.create_instance_at(&name,&version,directory.as_deref().map(std::path::Path::new))?;
                    if let Some(icon) = icon {
                        launcher.update_instance_memory(&instance.id, &instance.name, nekocrate_core::memory::MemoryConfig { mode: instance.memory_mode, custom_mb: instance.memory_mb }, instance.java_path.clone(), Some(icon))?;
                    }
                    launcher.instance(&instance.id)
                }).await??
            },
            Request::Update{id,name,memory_mb,memory_mode,java_path,icon,launch}=>{launcher.update_instance_options(&id,&name,nekocrate_core::memory::MemoryConfig{mode:memory_mode,custom_mb:memory_mb},java_path,icon,launch)?;json!(true)},
            Request::Clone{id}=>{let (_cancel,_guard)=state.job().await?;json!(launcher.clone_instance(&id).await?)},
            Request::Delete{id}=>{
                let (_cancel,_guard)=state.job().await?;
                let launcher = launcher.clone();
                let scope = id.clone();
                blocking::json(move || launcher.delete_instance(&id)).await??;
                state.download_retries.invalidate_instance(&scope);
                json!(true)
            },
            Request::MoveInstance{id,before_id}=>{launcher.move_instance(&id,before_id.as_deref())?;json!(true)},
            Request::Import{source,name,version}=>{let (_cancel,_guard)=state.job().await?;json!(launcher.import_instance(PathBuf::from(source),name,version).await?)},
            Request::ScanFolders{scan_id,paths}=>{
                let guard = state.folder_scan.start(scan_id);
                let launcher = launcher.clone();
                blocking::json(move || {
                    launcher.scan_folders(&paths.into_iter().map(PathBuf::from).collect::<Vec<_>>(), &guard.cancel)
                }).await??
            },
            Request::CancelFolderScan{scan_id}=>{state.folder_scan.cancel(Some(&scan_id));json!(true)},
            Request::ImportFolders{paths}=>{
                let (_cancel,_guard)=state.job().await?;
                json!(launcher.import_folders(paths.into_iter().map(PathBuf::from).collect()).await?)
            },
            Request::ImportGame{request}=>{
                let (cancel,_guard)=state.job().await?;
                let launcher = launcher.clone();
                blocking::json(move || launcher.import_game(request, &cancel, &sink)).await??
            },
            Request::SuggestedInstances=>{
                let launcher = launcher.clone();
                blocking::json(move || {
                    let executable = nekocrate_core::portable::executable()?;
                    Ok(launcher.suggested_instances(&executable))
                }).await??
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
                let result = blocking::json(move || launcher.delete_version(&id,&version)).await??;
                state.download_retries.invalidate_instance(&scope);
                result
            },
            Request::VersionContentCounts{id,version}=>{
                let launcher = launcher.clone();
                blocking::json(move || launcher.version_content_counts(&id,&version)).await??
            },
            Request::VersionContent{id,version,kind}=>{
                let launcher = launcher.clone();
                blocking::json(move || launcher.version_content(&id,&version,kind)).await??
            },
            Request::VersionContentIndex{id,version}=>{
                let launcher = launcher.clone();
                blocking::json(move || launcher.version_content_index(&id,&version)).await??
            },
            Request::VersionContentSummaries{id,version,files}=>{
                let launcher = launcher.clone();
                blocking::json(move || launcher.version_content_summaries(&id,&version,&files)).await??
            },
            Request::VersionContentMetadata{id,version,kind,file}=>{
                let launcher = launcher.clone();
                blocking::json(move || launcher.version_content_metadata(&id,&version,kind,&file)).await??
            },
            Request::VersionContentPresentation{id,version,kind,file}=>{
                use base64::Engine;
                let launcher = launcher.clone();
                blocking::value(move || {
                    let result = launcher.version_content_presentation(&id,&version,kind,&file)?;
                    let icon = result.icon.map(|(bytes,mime)|format!("data:{mime};base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes)));
                    Ok(json!({"icon":icon,"project":result.project}))
                }).await??
            },
            Request::OpenContentFolder{id,version,kind,file,source}=>{
                let launcher = launcher.clone();
                let reveal = file.is_some();
                let path = blocking::run(move || match file {
                    Some(file) => launcher.version_content_path(&id,&version,kind,&file,source.as_deref()),
                    None => launcher.version_content_directory(&id,&version,kind),
                }).await??;
                if reveal { app.opener().reveal_item_in_dir(path)?; }
                else { app.opener().open_path(path.to_string_lossy(),None::<&str>)?; }
                json!(true)
            },
            Request::VersionContentSource{id,version,kind,file,source}=>{
                let launcher = launcher.clone();
                blocking::json(move || launcher.version_content_source(&id,&version,kind,&file,&source)).await??
            },
            Request::VersionMetadata{id,version}=>{
                let launcher = launcher.clone();
                blocking::json(move || launcher.version_metadata(&id,&version)).await??
            },
            Request::VersionStorage{id,version,relative}=>{
                let launcher = launcher.clone();
                blocking::json(move || launcher.version_storage(&id,&version,&relative)).await??
            },
            Request::Install{id,repair,version}=>{
                let (cancel,_guard)=state.job().await?;
                let receiver=_guard.enable_pause();
                let operation = async {
                    if repair && let Some(version) = version { launcher.repair_version(&id,&version,&cancel,&sink).await }
                    else if repair { launcher.repair(&id,&cancel,&sink).await }
                    else { launcher.install(&id,&cancel,&sink).await }
                };
                let operation = nekocrate_core::loader::with_pause(receiver.clone(), operation);
                json!(pausable::run(operation,&cancel,receiver).await?)
            },
            Request::Launch{id,server}=>{let (cancel,_guard)=state.job().await?;json!(launcher.launch_to_server(&id,server.as_deref(),&cancel,sink).await?)},
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
            Request::SyncAccount{id}=>json!(launcher.sync_account(&id).await?),
            Request::AccountSkin{id}=>json!(launcher.account_skin(&id).await?),
            Request::SelectAccount{id}=>{let (_cancel,_guard)=state.job().await?;if let Some(id)=id {launcher.select_account(&id)?;} else {launcher.select_guest()?;}json!(true)},
            Request::SetGuestName{name}=>json!(launcher.set_guest_name(&name)?),
            Request::RenameAccount{id,name}=>json!(Box::pin(launcher.rename_account(&id,&name)).await?),
            Request::RemoveAccount{id}=>{let (_cancel,_guard)=state.job().await?;launcher.remove_account(&id)?;json!(true)},
            Request::OpenFolder{id}=>{let path=match id{Some(id)=>launcher.game_dir(&id)?,None=>launcher.root().to_owned()};app.opener().open_path(path.to_string_lossy(),None::<&str>)?;json!(true)},
            Request::LaunchDirectory{id,isolation}=>{let launcher = launcher.clone(); blocking::json(move || launcher.launch_directory(&id,isolation)).await??},
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
    if !cfg!(debug_assertions) {
        nekocrate_core::portable::enable();
    }
    #[cfg(target_os = "windows")]
    if portable_update::run_helper() {
        return;
    }
    if std::env::args_os().any(|arg| arg == "--verify-installed") {
        assert!(
            nekocrate_core::portable::detect(
                &std::env::current_exe().unwrap(),
                std::env::consts::OS,
            )
            .expect("installed mode configuration is invalid")
            .is_none(),
            "installed mode configuration was not applied"
        );
        let launcher = Launcher::open(None).expect("installed data initialization failed");
        assert!(
            !launcher.is_portable(),
            "installed binary unexpectedly enabled portable mode"
        );
        assert!(launcher.snapshot().database.instances.is_empty());
        println!("Installed mode remains independent of the adjacent game directory");
        return;
    }
    if std::env::args_os().any(|arg| arg == "--verify-portable") {
        let launcher = Launcher::open(None).expect("portable data initialization failed");
        assert!(launcher.is_portable(), "portable mode was not enabled");
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
    let mut context = application_context();
    // Exercise the actual embedded asset map without opening a window or user data.
    // This runs on each release runner using the already-built executable.
    if verifying {
        verify_frontend_permissions(&mut context)
            .expect("required frontend commands are missing from the compiled capabilities");
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
            "Embedded frontend, native permissions and IPC future size verified ({} bytes)",
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

fn application_context() -> tauri::Context {
    tauri::tauri_build_context!()
}

// Check the resolved release ACL, not just the source JSON. The JS integration
// test also exercises the SDK methods used by our UI, including drag listeners.
fn verify_frontend_permissions(context: &mut tauri::Context) -> anyhow::Result<()> {
    let authority = context.runtime_authority_mut();
    for command in [
        "plugin:event|listen",
        "plugin:event|unlisten",
        "plugin:window|set_title",
        "plugin:window|is_minimized",
        "plugin:window|minimize",
        "plugin:window|unminimize",
        "plugin:window|set_focus",
        "plugin:dialog|open",
        "plugin:dialog|save",
        "plugin:opener|open_url",
    ] {
        anyhow::ensure!(
            authority
                .resolve_access(command, "main", "main", &tauri::ipc::Origin::Local)
                .is_some(),
            "主窗口缺少必要权限: {command}"
        );
    }
    Ok(())
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

    #[test]
    fn compiled_capabilities_allow_the_frontend_native_operations() {
        verify_frontend_permissions(&mut application_context()).unwrap();
    }
}
