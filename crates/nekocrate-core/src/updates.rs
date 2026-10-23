use anyhow::{Context, Result, bail};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const RELEASES_API: &str =
    "https://api.github.com/repos/NekoOS-Group/NekoCrate-Launcher/releases?per_page=100";
const RELEASES_URL: &str = "https://github.com/NekoOS-Group/NekoCrate-Launcher/releases/";

#[derive(Debug, Serialize)]
pub struct UpdateStatus {
    pub latest_version: String,
    pub available: bool,
    pub download_url: String,
}
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    draft: bool,
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

fn release_version(tag: &str) -> Option<Version> {
    // Public preview tags are compact names; installed packages use SemVer.
    if let Some(preview) = tag.strip_prefix("dev") {
        let (major, minor) = preview.split_once('.')?;
        if major.is_empty()
            || minor.is_empty()
            || !major
                .bytes()
                .chain(minor.bytes())
                .all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        return Version::parse(&format!("{major}.{minor}.0-dev.1")).ok();
    }
    Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()
}
fn select_release(
    releases: &[Release],
    current: &str,
    os: &str,
    arch: &str,
) -> Result<UpdateStatus> {
    let current = Version::parse(current).context("当前启动器版本无效")?;
    let (version, release) = releases
        .iter()
        .filter(|release| !release.draft && (!current.pre.is_empty() || !release.prerelease))
        .filter_map(|release| release_version(&release.tag_name).map(|version| (version, release)))
        .filter(|(version, _)| !current.pre.is_empty() || version.pre.is_empty())
        .max_by(|a, b| a.0.cmp_precedence(&b.0))
        .context("暂无可识别的公开版本，请稍后重试")?;
    let suffix = match (os, arch) {
        ("macos", "aarch64") => "-mac-appsilicon.dmg",
        ("windows", "x86_64") => "-win-amd64-setup.exe",
        ("linux", "x86_64") => "-linux-amd64.AppImage",
        _ => "",
    };
    if !release.html_url.starts_with(&format!("{RELEASES_URL}tag/")) {
        bail!("发布地址无效");
    }
    let url = release
        .assets
        .iter()
        .find(|asset| {
            !suffix.is_empty()
                && asset.name.ends_with(suffix)
                && asset
                    .browser_download_url
                    .starts_with(&format!("{RELEASES_URL}download/"))
        })
        .map_or(&release.html_url, |asset| &asset.browser_download_url);
    Ok(UpdateStatus {
        latest_version: release.tag_name.clone(),
        available: version.cmp_precedence(&current).is_gt(),
        download_url: url.clone(),
    })
}

pub async fn check(
    root: &std::path::Path,
    settings: &crate::model::Settings,
) -> Result<UpdateStatus> {
    let client = crate::network::client_builder(root, settings)?
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(15))
        .build()?;
    let response = client
        .get(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .context("无法连接更新服务器")?;
    if response.status() == reqwest::StatusCode::FORBIDDEN
        || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        bail!("更新检查次数受限，请稍后重试");
    }
    let releases: Vec<Release> = response
        .error_for_status()
        .context("更新服务器返回错误")?
        .json()
        .await
        .context("无法读取更新信息")?;
    select_release(
        &releases,
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release(tag: &str, prerelease: bool) -> Release {
        Release {
            tag_name: tag.into(),
            html_url: format!("{RELEASES_URL}tag/{tag}"),
            draft: false,
            prerelease,
            assets: [
                "mac-appsilicon.dmg",
                "win-amd64-setup.exe",
                "linux-amd64.AppImage",
            ]
            .into_iter()
            .map(|suffix| Asset {
                name: format!("NekoCrate-Launcher-{tag}-{suffix}"),
                browser_download_url: format!(
                    "{RELEASES_URL}download/{tag}/NekoCrate-Launcher-{tag}-{suffix}"
                ),
            })
            .collect(),
        }
    }
    #[test]
    fn detects_current_legacy_release_and_new_semantic_versions_without_downgrading() {
        let mut releases = vec![release("dev0.1", true)];
        let status = select_release(&releases, "0.1.0-dev.1", "macos", "aarch64").unwrap();
        assert!(!status.available);
        assert_eq!(status.latest_version, "dev0.1");
        releases.push(release("v0.1.0-dev.2", true));
        assert!(
            select_release(&releases, "0.1.0-dev.1", "macos", "aarch64")
                .unwrap()
                .available
        );
        assert!(
            !select_release(&releases, "0.1.0-dev.3", "macos", "aarch64")
                .unwrap()
                .available
        );
        releases.push(release("v0.1.0", false));
        releases.push(release("v0.2.0-dev.1", true));
        assert_eq!(
            select_release(&releases, "0.1.0", "linux", "x86_64")
                .unwrap()
                .latest_version,
            "v0.1.0"
        );
        let mut draft = release("v99.0.0", false);
        draft.draft = true;
        releases.push(draft);
        assert_eq!(
            select_release(&releases, "0.1.0-dev.1", "windows", "x86_64")
                .unwrap()
                .latest_version,
            "v0.2.0-dev.1"
        );
    }
    #[test]
    fn dev_tag_updates_older_builds_and_recognizes_the_installed_preview() {
        let releases = vec![release("dev0.1", true), release("dev0.2", true)];
        let dev = select_release(&releases, "0.1.0-dev.1", "macos", "aarch64").unwrap();
        assert!(dev.available);
        assert_eq!(dev.latest_version, "dev0.2");
        assert!(dev.download_url.ends_with("dev0.2-mac-appsilicon.dmg"));
        for current in ["0.2.0-dev.1", "0.2.0-dev.2"] {
            let preview = select_release(&releases, current, "windows", "x86_64").unwrap();
            assert!(!preview.available);
            assert_eq!(preview.latest_version, "dev0.2");
        }
        assert!(select_release(&releases, "0.1.0", "linux", "x86_64").is_err());
    }
    #[test]
    fn compact_preview_tags_support_dev03_and_future_versions() {
        let releases = vec![release("dev0.2", true), release("dev0.3", true)];
        let update = select_release(&releases, "0.2.0-dev.1", "macos", "aarch64").unwrap();
        assert!(update.available);
        assert_eq!(update.latest_version, "dev0.3");
        assert!(update.download_url.ends_with("dev0.3-mac-appsilicon.dmg"));
        assert!(
            !select_release(&releases, "0.3.0-dev.1", "windows", "x86_64")
                .unwrap()
                .available
        );
        assert_eq!(
            release_version("dev12.34").unwrap().to_string(),
            "12.34.0-dev.1"
        );
        for invalid in [
            "dev",
            "dev.3",
            "dev0.",
            "dev0.03",
            "dev0.3.1",
            "dev0.3-beta",
        ] {
            assert!(release_version(invalid).is_none(), "{invalid}");
        }
    }
    #[test]
    fn chooses_platform_installers_and_rejects_untrusted_release_links() {
        let mut releases = vec![release("v0.2.0", false)];
        for (os, arch, ending) in [
            ("macos", "aarch64", ".dmg"),
            ("windows", "x86_64", ".exe"),
            ("linux", "x86_64", ".AppImage"),
        ] {
            assert!(
                select_release(&releases, "0.1.0", os, arch)
                    .unwrap()
                    .download_url
                    .ends_with(ending)
            );
        }
        assert!(
            select_release(&releases, "0.1.0", "linux", "aarch64")
                .unwrap()
                .download_url
                .contains("/tag/")
        );
        releases[0].html_url = "https://invalid.example/release".into();
        assert!(select_release(&releases, "0.1.0", "macos", "aarch64").is_err());
        assert!(select_release(&[], "0.1.0", "macos", "aarch64").is_err());
    }
}
