use anyhow::{Context, Result, ensure};
use base64::Engine;
use nekocrate_core::{
    Launcher,
    game_export::PortablePayload,
    model::{Event, EventSink},
    updates,
};
use serde_json::Value;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub async fn portable(
    app: &tauri::AppHandle,
    launcher: &Launcher,
    platform: &str,
    cancel: &CancellationToken,
    sink: &EventSink,
) -> Result<PortablePayload> {
    sink(Event::workflow("export_portable", "获取便携版启动器"));
    let settings = launcher.settings();
    let release = tokio::select! { _ = cancel.cancelled() => anyhow::bail!("操作已取消"), result = updates::check(launcher.root(), &settings) => result? };
    let client = nekocrate_core::network::client_builder(launcher.root(), &settings)?
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .build()?;
    let endpoint = release.updater_url.context("该版本没有便携版更新索引")?;
    let response = tokio::select! { _ = cancel.cancelled() => anyhow::bail!("操作已取消"), response = client.get(endpoint).send() => response? };
    let mut response = response.error_for_status()?;
    let mut index = Vec::new();
    loop {
        let chunk = tokio::select! { _ = cancel.cancelled() => anyhow::bail!("操作已取消"), chunk = response.chunk() => chunk? };
        let Some(chunk) = chunk else { break };
        ensure!(index.len() + chunk.len() <= 1024 * 1024, "便携版索引过大");
        index.extend_from_slice(&chunk);
    }
    let manifest: Value = serde_json::from_slice(&index)?;
    ensure!(
        manifest["version"].as_str() == Some(&release.version),
        "便携版索引版本不匹配"
    );
    let package = standalone_package(&manifest, platform, &release.latest_version)?;
    let url = package["url"].as_str().context("此平台暂无便携版")?;
    let signature = package["signature"].as_str().context("便携版缺少签名")?;
    let mut response = tokio::select! { _ = cancel.cancelled() => anyhow::bail!("操作已取消"), response = client.get(url).send() => response?.error_for_status()? };
    let total = response.content_length().unwrap_or(0);
    ensure!(total <= 128 * 1024 * 1024, "便携版文件过大");
    let mut bytes = Vec::new();
    loop {
        let chunk = tokio::select! { _ = cancel.cancelled() => anyhow::bail!("操作已取消"), chunk = response.chunk() => chunk? };
        let Some(chunk) = chunk else { break };
        ensure!(
            bytes.len() + chunk.len() <= 128 * 1024 * 1024,
            "便携版文件过大"
        );
        bytes.extend_from_slice(&chunk);
        let mut event = Event::new("progress", "获取便携版启动器");
        event.step = Some("export_portable".into());
        event.bytes = bytes.len() as u64;
        event.total_bytes = total;
        event.total = 1;
        sink(event);
    }
    let public = app
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|p| p["pubkey"].as_str())
        .context("未配置启动器公钥")?;
    verify(&bytes, signature, public, &release.version)?;
    Ok(PortablePayload {
        platform: platform.into(),
        bytes,
    })
}
fn standalone_package<'a>(manifest: &'a Value, platform: &str, tag: &str) -> Result<&'a Value> {
    let (target, suffix) = match platform {
        "windows" => ("windows-x86_64-portable-exe", "-win-amd64.exe"),
        "macos" => ("darwin-aarch64-unified", "-mac-appsilicon.app.tar.gz"),
        "linux" => ("linux-x86_64-elf", "-linux-amd64.elf"),
        _ => anyhow::bail!("不支持的便携版平台"),
    };
    let package = &manifest["platforms"][target];
    let url = package["url"].as_str().context("此平台暂无便携版")?;
    ensure!(
        updates::is_update_artifact(url, tag) && url.ends_with(suffix),
        "便携版下载地址无效"
    );
    Ok(package)
}

fn verify(bytes: &[u8], signature: &str, public: &str, version: &str) -> Result<()> {
    let decode = |s: &str| -> Result<String> {
        Ok(String::from_utf8(
            base64::engine::general_purpose::STANDARD.decode(s.trim())?,
        )?)
    };
    let key = minisign_verify::PublicKey::decode(&decode(public)?)?;
    let signature = minisign_verify::Signature::decode(&decode(signature)?)?;
    key.verify(bytes, &signature, true)?;
    ensure!(
        signature
            .trusted_comment()
            .split('\t')
            .any(|part| part == format!("version:{version}")),
        "便携版签名与版本不匹配"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn export_accepts_only_current_standalone_targets() {
        let mut manifest = serde_json::json!({"platforms":{}});
        let url = |name: &str| {
            format!(
                "https://github.com/NekoOS-Group/NekoCrate-Launcher/releases/download/dev9.9.9/NekoCrate-Launcher-dev9.9.9-{name}"
            )
        };
        manifest["platforms"]["darwin-aarch64"] =
            serde_json::json!({"url":url("mac-appsilicon.app.tar.gz")});
        assert!(standalone_package(&manifest, "macos", "dev9.9.9").is_err());
        manifest["platforms"]["darwin-aarch64-portable"] =
            serde_json::json!({"url":url("mac-appsilicon-portable.app.tar.gz")});
        assert!(standalone_package(&manifest, "macos", "dev9.9.9").is_err());
        manifest["platforms"]["darwin-aarch64-unified"] =
            serde_json::json!({"url":url("mac-appsilicon.app.tar.gz")});
        assert_eq!(
            standalone_package(&manifest, "macos", "dev9.9.9").unwrap()["url"],
            url("mac-appsilicon.app.tar.gz")
        );
        assert!(standalone_package(&manifest, "macos", "dev9.9.8").is_err());
        for suffix in ["win-amd64.exe", "win-amd64-portable.exe"] {
            manifest["platforms"]["windows-x86_64-portable-exe"] =
                serde_json::json!({"url":url(suffix)});
            assert_eq!(
                standalone_package(&manifest, "windows", "dev9.9.9").is_ok(),
                suffix == "win-amd64.exe"
            );
        }
        manifest["platforms"]["windows-x86_64-portable-exe"] =
            serde_json::json!({"url":url("win-amd64-setup.exe")});
        assert!(standalone_package(&manifest, "windows", "dev9.9.9").is_err());
    }
    #[test]
    fn exported_portable_requires_the_published_signature_and_version() {
        let bytes = include_bytes!("../tests/fixtures/updater/test.elf");
        let public = include_str!("../tests/fixtures/updater/elf-test.pub");
        let signature = include_str!("../tests/fixtures/updater/test.elf.sig");
        verify(bytes, signature, public, "9.9.9").unwrap();
        assert!(verify(b"corrupt executable", signature, public, "9.9.9").is_err());
        assert!(verify(bytes, signature, public, "9.9.10").is_err());
    }
}
