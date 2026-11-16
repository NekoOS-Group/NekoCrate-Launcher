//! A pack's source and resumable state travel with its version directory.
use super::*;

pub(super) fn read_job(game: &Path, version: &str) -> Result<Job> {
    let path = job_path(game, version)?;
    let mut job: Job = storage::read_json(&path)?;
    if job.version != version || job.archive != Path::new("source.zip") {
        bail!("整合包安装目标不匹配");
    }
    job.archive = archive::managed_destination(
        &game.join("versions").join(version),
        ".nekocrate/source.zip",
    )?;
    Ok(job)
}

pub(super) fn write_job(game: &Path, version: &str, job: &Job) -> Result<()> {
    let mut saved = job.clone();
    saved.version = version.into();
    saved.archive = "source.zip".into();
    storage::write_json(&job_path(game, version)?, &saved)
}

pub(super) fn write_progress(game: &Path, version: &str, progress: f64) -> Result<()> {
    let directory = game.join("versions").join(version);
    storage::write_json(
        &archive::managed_destination(&directory, ".nekocrate/progress.json")?,
        &progress,
    )
}

pub(crate) fn describe(game: &Path, version: &str) -> Result<InstanceVersion> {
    let job = read_job(game, version)?;
    let directory = game.join("versions").join(version);
    let progress = storage::read_json::<f64>(&directory.join(".nekocrate/progress.json"))
        .unwrap_or(0.0)
        .clamp(0.0, 100.0);
    let installed = job.completed && version::metadata_path(game, version)?.is_file();
    Ok(InstanceVersion {
        id: version.into(),
        name: Some(job.name),
        minecraft_version: Some(job.pack.minecraft),
        loader: job.pack.loader,
        modpack: true,
        installed,
        download_progress: if installed { 100.0 } else { progress.min(99.0) },
    })
}

impl Launcher {
    pub(crate) fn save_pack_progress(
        &self,
        instance: &str,
        version: &str,
        progress: f64,
    ) -> Result<()> {
        let instance = self.instance(instance)?;
        if instance
            .versions
            .iter()
            .any(|v| v.id == version && v.modpack)
        {
            write_progress(&instance.game_dir(self.root()), version, progress)?;
        }
        Ok(())
    }
}
