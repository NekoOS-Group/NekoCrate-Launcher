use super::*;
use std::time::Duration;

impl Launcher {
    pub fn curseforge_key_configured(&self) -> Result<bool> {
        Ok(self.curseforge_key()?.is_some())
    }
    pub fn save_curseforge_key(&self, key: &str) -> Result<()> {
        let entry = crate::auth::secret_entry(self.root(), "curseforge-api-key")?;
        if key.trim().is_empty() {
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => (),
                Err(e) => return Err(e).context("无法清除 CurseForge API Key"),
            }
        } else {
            if key.len() > 4096 || key.chars().any(char::is_control) {
                bail!("CurseForge API Key 格式无效");
            }
            entry
                .set_password(key.trim())
                .context("无法保存 CurseForge API Key")?;
        }
        Ok(())
    }
    pub(super) fn curseforge_key(&self) -> Result<Option<String>> {
        match crate::auth::secret_entry(self.root(), "curseforge-api-key")?.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e).context("无法读取 CurseForge API Key"),
        }
    }
}
async fn request(
    client: &reqwest::Client,
    key: &str,
    path: &str,
    cancel: &CancellationToken,
) -> Result<Value> {
    let mut header =
        reqwest::header::HeaderValue::from_str(key).context("CurseForge API Key 格式无效")?;
    header.set_sensitive(true);
    tokio::select! {
        _ = cancel.cancelled() => bail!("操作已取消"),
        response = async {
            let mut response = client.get(format!("https://api.curseforge.com/v1/{path}")).header("x-api-key", header).timeout(Duration::from_secs(30)).send().await?;
            if matches!(response.status().as_u16(), 401 | 403) { bail!("CurseForge API Key 无效或没有访问权限，请在网络设置中更新"); }
            response = response.error_for_status()?;
            let mut data = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                if data.len() + chunk.len() > 8 * 1024 * 1024 { bail!("CurseForge 元数据过大"); }
                data.extend(chunk);
            }
            Ok::<_,anyhow::Error>(serde_json::from_slice::<Value>(&data)?["data"].clone())
        } => response,
    }
}
pub(super) async fn resolve(
    pack: &mut Pack,
    key: Option<String>,
    client: &reqwest::Client,
    cancel: &CancellationToken,
    mut checkpoint: impl FnMut(&Pack) -> Result<()>,
) -> Result<()> {
    if pack.format != "curseforge" || pack.files.iter().all(|f| f.path.is_some()) {
        return Ok(());
    }
    let key = key.context("尚未配置 CurseForge API Key，请在网络设置中配置后继续导入")?;
    for index in 0..pack.files.len() {
        let mut item = pack.files[index].clone();
        if item.path.is_some() {
            continue;
        }
        let project = item.project.context("缺少 CurseForge 项目 ID")?;
        let file_id = item.file.context("缺少 CurseForge 文件 ID")?;
        let file = request(
            client,
            &key,
            &format!("mods/{project}/files/{file_id}"),
            cancel,
        )
        .await?;
        if file["id"].as_u64() != Some(file_id) || file["modId"].as_u64() != Some(project) {
            bail!("CurseForge 返回了不匹配的文件");
        }
        let info = request(client, &key, &format!("mods/{project}"), cancel).await?;
        if info["id"].as_u64() != Some(project) || info["gameId"] != 432 {
            bail!("CurseForge 项目不是 Minecraft 项目");
        }
        let folder = match info["classId"].as_u64() {
            Some(6) => "mods",
            Some(12) => "resourcepacks",
            _ => bail!("此 CurseForge 文件类型暂不支持导入：{project}"),
        };
        item.display_name = info["name"].as_str().map(str::to_owned);
        let name = file["fileName"].as_str().context("CurseForge 文件名缺失")?;
        archive::relative(name)?;
        if name.contains('/') {
            bail!("CurseForge 文件名无效");
        }
        item.path = Some(format!("{folder}/{name}"));
        item.size = file["fileLength"]
            .as_u64()
            .context("CurseForge 文件大小缺失")?;
        let hashes = file["hashes"]
            .as_array()
            .context("CurseForge 文件缺少校验值")?;
        let sha1 = hashes
            .iter()
            .find(|h| h["algo"] == 1)
            .context("CurseForge 文件缺少 SHA-1 校验值")?;
        item.sha1 = archive::hash(sha1, "value", 40)?;
        if let Some(url) = file["downloadUrl"]
            .as_str()
            .filter(|u| !u.is_empty() && info["allowModDistribution"] != false)
        {
            archive::https(url)?;
            item.urls = vec![url.into()];
        }
        if let Some(url) = info["links"]["websiteUrl"].as_str() {
            let parsed = reqwest::Url::parse(url)?;
            if parsed.scheme() == "https" && parsed.host_str() == Some("www.curseforge.com") {
                item.manual_url = Some(format!("{}/files/{file_id}", url.trim_end_matches('/')));
            }
        }
        pack.files[index] = item;
        checkpoint(pack)?;
    }
    Ok(())
}
