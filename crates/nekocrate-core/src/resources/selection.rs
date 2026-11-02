//! Preselected resources belong to the new version, including across restarts.
use super::*;
use crate::model::{Event, EventSink};
use tokio_util::sync::CancellationToken;

#[derive(Serialize, Deserialize)]
struct Selection {
    minecraft: String,
    loader: Option<ModLoader>,
    resources: Vec<ResourceSelection>,
    plan: Option<String>,
}
fn path(game: &Path, version: &str) -> Result<PathBuf> {
    storage::safe_version(version)?;
    safe_join(
        game,
        &format!("versions/{version}/.nekocrate/selected-resources.json"),
    )
}
pub(crate) fn has_selection(game: &Path, version: &str) -> bool {
    path(game, version).is_ok_and(|p| p.exists())
}
impl Launcher {
    pub fn queue_version_resources(
        &self,
        id: &str,
        version: &str,
        minecraft: &str,
        loader: Option<ModLoader>,
        resources: Vec<ResourceSelection>,
    ) -> Result<()> {
        let _operation = self
            .0
            .operation
            .try_lock()
            .context("有任务正在运行，请稍后重试")?;
        let _directory_guard = self.idle(id)?;
        let instance = self.instance(id)?;
        let tracked = instance
            .versions
            .iter()
            .find(|v| v.id == version)
            .context("版本不存在")?;
        anyhow::ensure!(
            tracked.minecraft_version.as_deref().unwrap_or(&tracked.id) == minecraft
                && tracked.loader == loader,
            "目标版本已变化，请重新选择资源"
        );
        anyhow::ensure!(
            !resources.is_empty() && resources.len() <= 128,
            "请预选 1–128 项资源"
        );
        for pick in &resources {
            provider::identifier(&pick.project)?;
            provider::identifier(&pick.version)?;
            anyhow::ensure!(
                pick.platform != Platform::Local
                    && matches!(
                        pick.kind,
                        ResourceKind::Mods
                            | ResourceKind::Resourcepacks
                            | ResourceKind::Shaderpacks
                    ),
                "此资源类型不支持预选安装"
            );
            anyhow::ensure!(pick.name.len() <= 512, "资源名称过长");
            anyhow::ensure!(
                pick.kind != ResourceKind::Mods || loader.is_some(),
                "请先选择 Mod 加载器"
            );
        }
        let path = path(&self.available_game_dir(id)?, version)?;
        if path.exists() {
            let previous: Selection = storage::read_json(&path)?;
            // Retrying the creation form must not discard an existing checkpoint.
            anyhow::ensure!(
                previous.minecraft == minecraft
                    && previous.loader == loader
                    && previous.resources == resources,
                "此版本已有待安装资源，请先继续原安装任务"
            );
            return Ok(());
        }
        storage::write_json(
            &path,
            &Selection {
                minecraft: minecraft.into(),
                loader,
                resources,
                plan: None,
            },
        )?;
        self.set_installed(id, version, false)
    }

    pub(crate) async fn install_selected_resources(
        &self,
        id: &str,
        version: &str,
        cancel: &CancellationToken,
        sink: &EventSink,
    ) -> Result<()> {
        let path = path(&self.available_game_dir(id)?, version)?;
        if !path.exists() {
            return Ok(());
        }
        let mut selection: Selection = storage::read_json(&path)?;
        let target = self.resource_target(id, version)?;
        anyhow::ensure!(
            target.minecraft == selection.minecraft && target.loader == selection.loader,
            "目标版本已变化，请重新选择资源"
        );
        let scoped = Event::scoped(sink, id, None);
        scoped(Event::workflow("resource_resolve", "解析预选资源依赖"));
        let task = if let Some(task) = selection.plan.as_ref() {
            task.clone()
        } else {
            let _operation = self.0.operation.lock().await;
            let _directory_guard = self.idle(id)?;
            let mut plan = self
                .resolve_resource_selection(target, &selection.resources, None, cancel)
                .await?;
            let token = cancel.clone();
            plan = tokio::task::spawn_blocking(move || {
                install::preflight(&mut plan, &token)?;
                Ok::<_, anyhow::Error>(plan)
            })
            .await??;
            anyhow::ensure!(plan.conflicts.is_empty(), "{}", plan.conflicts.join("\n"));
            plan.started = true;
            save_plan(&plan)?;
            selection.plan = Some(plan.id.clone());
            storage::write_json(&path, &selection)?;
            plan.id
        };
        self.install_resources(id, version, &task, cancel, sink)
            .await?;
        fs::remove_file(&path)?;
        let _ = fs::remove_file(path.with_extension("json.bak"));
        self.set_installed(id, version, true)
    }
}
