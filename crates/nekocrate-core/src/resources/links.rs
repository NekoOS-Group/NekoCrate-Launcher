use super::{
    provider::{Provider, identifier, project},
    *,
};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

#[derive(Serialize)]
pub struct ProjectLink {
    pub platform: Platform,
    pub kind: String,
    pub project: Project,
}
fn parse(value: &str) -> Result<(Platform, String, String)> {
    anyhow::ensure!(value.len() <= 2048, "项目链接过长");
    let url = reqwest::Url::parse(value.trim())?;
    anyhow::ensure!(
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none(),
        "请选择 HTTPS 项目链接"
    );
    let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
    let (platform, category, slug) = match url.host_str() {
        Some("modrinth.com" | "www.modrinth.com") if parts.len() >= 2 => {
            (Platform::Modrinth, parts[0], parts[1])
        }
        Some("curseforge.com" | "www.curseforge.com")
            if parts.len() >= 3 && parts[0] == "minecraft" =>
        {
            (Platform::Curseforge, parts[1], parts[2])
        }
        _ => bail!("只支持 Modrinth / CurseForge 的 Minecraft 项目链接"),
    };
    identifier(slug)?;
    let kind = match (platform, category) {
        (Platform::Modrinth, "mod") | (Platform::Curseforge, "mc-mods") => "mods",
        (Platform::Modrinth, "modpack") | (Platform::Curseforge, "modpacks") => "modpack",
        (Platform::Modrinth, "resourcepack") | (Platform::Curseforge, "texture-packs") => {
            "resourcepacks"
        }
        (Platform::Modrinth, "shader") | (Platform::Curseforge, "shaders") => "shaderpacks",
        (Platform::Modrinth, "datapack") | (Platform::Curseforge, "data-packs" | "datapacks") => {
            "datapacks"
        }
        (Platform::Curseforge, "worlds") => "saves",
        _ => bail!("不支持的项目分类"),
    };
    Ok((platform, kind.into(), slug.into()))
}
impl Launcher {
    pub async fn resolve_project_link(
        &self,
        url: &str,
        cancel: &CancellationToken,
    ) -> Result<ProjectLink> {
        let (platform, kind, slug) = parse(url)?;
        let api = Provider::new(self, platform)?;
        let value: Value = if platform == Platform::Modrinth {
            let value = api.get_display(&format!("project/{slug}"), cancel).await?;
            let expected = match kind.as_str() {
                "mods" => "mod",
                "resourcepacks" => "resourcepack",
                "shaderpacks" => "shader",
                "datapacks" => "datapack",
                _ => "modpack",
            };
            anyhow::ensure!(
                value["project_type"].as_str() == Some(expected)
                    || (expected == "datapack"
                        && value["project_type"] == "mod"
                        && value["loaders"]
                            .as_array()
                            .is_some_and(|a| a.iter().any(|l| l == "datapack"))),
                "链接分类与项目内容不符"
            );
            value
        } else {
            let class = match kind.as_str() {
                "modpack" => 4471,
                _ => {
                    api.cf_class(serde_json::from_value(Value::String(kind.clone()))?, cancel)
                        .await?
                }
            };
            let result = api
                .get_display(
                    &format!("mods/search?gameId=432&classId={class}&slug={slug}"),
                    cancel,
                )
                .await?;
            result["data"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|v| {
                    v["slug"].as_str() == Some(&slug) && v["gameId"] == 432 && v["classId"] == class
                })
                .cloned()
                .context("未找到对应的资源项目")?
        };
        Ok(ProjectLink {
            platform,
            kind,
            project: project(&value, platform),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_exact_project_hosts_and_classes() {
        for (url, kind) in [
            ("https://modrinth.com/mod/sodium", "mods"),
            (
                "https://www.curseforge.com/minecraft/modpacks/all-the-mods-9/files/123?x=1",
                "modpack",
            ),
            ("https://modrinth.com/datapack/example", "datapacks"),
        ] {
            assert_eq!(parse(url).unwrap().1, kind);
        }
        for url in [
            "https://modrinth.com.evil.test/mod/sodium",
            "https://user:secret@modrinth.com/mod/sodium",
            "file:///etc/passwd",
            "https://curseforge.com/wow/addons/thing",
            "https://cdn.modrinth.com/data/file.jar",
            "https://modrinth.com/mod/%2e%2e",
            "https://modrinth.com:9999/mod/test",
        ] {
            assert!(parse(url).is_err(), "{url}");
        }
    }
}
