use super::*;
use serde::Serialize;
#[derive(Serialize)]
pub struct ResourceDropPreview {
    pub existing: Vec<String>,
}
impl Launcher {
    /// Hover checks names and target availability only. Hashing and compatibility
    /// checks remain in the transactional install plan after the actual drop.
    pub fn preview_resource_drop(
        &self,
        id: &str,
        version: &str,
        paths: &[PathBuf],
        sources: &[ResourceCopySource],
        kind: Option<ResourceKind>,
        world: Option<&str>,
    ) -> Result<ResourceDropPreview> {
        let _idle = self.idle(id)?;
        anyhow::ensure!(
            paths.len() + sources.len() <= 256,
            "每次最多拖动 256 项资源"
        );
        let target = self.resource_target(id, version)?;
        let mut inputs: Vec<_> = paths.iter().map(|p| (p.clone(), kind)).collect();
        for source in sources {
            inputs.push((self.resource_export_path(source)?, Some(source.kind)));
        }
        let mut existing = Vec::new();
        for (path, chosen) in inputs {
            let detected = imports::inspect(&path)?;
            let Some(kind) =
                chosen.or_else(|| (detected.kinds.len() == 1).then(|| detected.kinds[0]))
            else {
                continue;
            };
            if kind == ResourceKind::Datapacks && world.is_none() {
                continue;
            }
            let name = if detected.prefix.is_some() {
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            } else {
                detected.name
            };
            let relative = destination(&target, kind, &name, world)?;
            if target.directory.join(relative).exists() {
                existing.push(name);
            }
        }
        Ok(ResourceDropPreview { existing })
    }
}
