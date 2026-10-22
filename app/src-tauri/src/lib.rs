use nekocrate_core::{Launcher, auth::DeviceChallenge, model::*};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tokio_util::sync::CancellationToken;

mod import_picker;

struct AppState {
    launcher: Launcher,
    job: Arc<Mutex<Option<CancellationToken>>>,
    challenge: Mutex<Option<DeviceChallenge>>,
}
struct JobGuard(Arc<Mutex<Option<CancellationToken>>>);
impl Drop for JobGuard {
    fn drop(&mut self) {
        *self.0.lock().unwrap() = None;
    }
}
impl AppState {
    fn job(&self) -> Result<(CancellationToken, JobGuard), String> {
        let mut slot = self.job.lock().unwrap();
        if slot.is_some() {
            return Err("有任务正在运行，请等待或取消当前任务".into());
        }
        let cancel = CancellationToken::new();
        *slot = Some(cancel.clone());
        Ok((cancel, JobGuard(self.job.clone())))
    }
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    Snapshot,
    Versions,
    Java,
    SaveSettings {
        settings: Settings,
    },
    Create {
        name: String,
        #[serde(default)]
        version: String,
    },
    Update {
        id: String,
        name: String,
        memory_mb: u32,
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
    Import {
        source: String,
        name: String,
        #[serde(default)]
        version: String,
    },
    PickImportDirectory {
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
    Install {
        id: String,
    },
    Launch {
        id: String,
    },
    Stop {
        id: String,
    },
    Cancel,
    BeginLogin,
    WaitLogin,
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
    let handle = app.clone();
    let sink: EventSink = Arc::new(move |event| {
        let _ = handle.emit("launcher-event", event);
    });
    let launcher = &state.launcher;
    let result: Result<Value, anyhow::Error> = async {
        Ok(match request{
            Request::Snapshot=>json!({"state":launcher.snapshot(),"manifest":launcher.cached_versions(),"busy":state.job.lock().unwrap().is_some()}),
            Request::Versions=>{let (cancel,_guard)=state.job().map_err(anyhow::Error::msg)?;json!(launcher.versions(&cancel).await?)},
            Request::Java=>json!(nekocrate_core::java::discover(launcher.root()).await),
            Request::SaveSettings{settings}=>{launcher.save_settings(settings)?;json!(true)},
            Request::Create{name,version}=>json!(launcher.create_instance(&name,&version)?),
            Request::Update{id,name,memory_mb,java_path,icon}=>{launcher.update_instance(&id,&name,memory_mb,java_path,icon)?;json!(true)},
            Request::Clone{id}=>{let (_cancel,_guard)=state.job().map_err(anyhow::Error::msg)?;json!(launcher.clone_instance(&id).await?)},
            Request::Delete{id}=>{launcher.delete_instance(&id)?;json!(true)},
            Request::Import{source,name,version}=>{let (_cancel,_guard)=state.job().map_err(anyhow::Error::msg)?;json!(launcher.import_instance(PathBuf::from(source),name,version).await?)},
            Request::PickImportDirectory{path}=>json!(import_picker::pick(&app,&path).await?),
            Request::AddVersion{id,version}=>json!(launcher.add_version(&id,&version)?),
            Request::SelectVersion{id,version}=>{launcher.select_version(&id,&version)?;json!(true)},
            Request::Install{id}=>{let (cancel,_guard)=state.job().map_err(anyhow::Error::msg)?;launcher.install(&id,&cancel,&sink).await?;json!(true)},
            Request::Launch{id}=>{let (cancel,_guard)=state.job().map_err(anyhow::Error::msg)?;json!(launcher.launch(&id,&cancel,sink).await?)},
            Request::Stop{id}=>{launcher.stop_game(&id)?;json!(true)},
            Request::Cancel=>{if let Some(token)=state.job.lock().unwrap().as_ref(){token.cancel();}json!(true)},
            Request::BeginLogin=>{let c=launcher.begin_login().await?;let value=json!(c);*state.challenge.lock().unwrap()=Some(c);value},
            Request::WaitLogin=>{let (cancel,_guard)=state.job().map_err(anyhow::Error::msg)?;let c=state.challenge.lock().unwrap().take().ok_or_else(||anyhow::Error::msg("请先获取登录验证码"))?;json!(launcher.finish_login(c,&cancel).await?)},
            Request::RefreshAccount=>{let (_cancel,_guard)=state.job().map_err(anyhow::Error::msg)?;json!(launcher.refresh_account().await?)},
            Request::AccountSkin{id}=>json!(launcher.account_skin(&id).await?),
            Request::SelectAccount{id}=>{launcher.select_account(&id)?;json!(true)},
            Request::RemoveAccount{id}=>{launcher.remove_account(&id)?;json!(true)},
            Request::OpenFolder{id}=>{let path=match id{Some(id)=>launcher.game_dir(&id)?,None=>launcher.root().to_owned()};app.opener().open_path(path.to_string_lossy(),None::<&str>)?;json!(true)},
            Request::InstanceDirectory{id}=>json!(launcher.game_dir(&id)?),
        })
    }.await;
    result.map_err(|e| format!("{e:#}"))
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let launcher = Launcher::open(None)?;
            app.manage(AppState {
                launcher,
                job: Arc::new(Mutex::new(None)),
                challenge: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![command])
        .build(tauri::generate_context!())
        .expect("猫咪盒子启动器启动失败")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                let state = app.state::<AppState>();
                state.launcher.stop_all();
                if let Some(token) = state.job.lock().unwrap().as_ref() {
                    token.cancel();
                }
            }
        });
}
