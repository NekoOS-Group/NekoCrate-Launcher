//! Check official metadata services without sending account information.
use nekocrate_core::{download::Downloader, model::Settings};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let downloader = Downloader::new(std::path::Path::new("."), Settings::default())?;
    for url in [
        nekocrate_core::minecraft::MANIFEST,
        "https://api.adoptium.net/v3/info/available_releases",
        "https://api.adoptium.net/v3/assets/latest/21/hotspot?architecture=aarch64&image_type=jre&os=mac&vendor=eclipse",
    ] {
        let start = std::time::Instant::now();
        match downloader
            .client
            .get(url)
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await
        {
            Ok(response) => println!("{url}: {} ({:?})", response.status(), start.elapsed()),
            Err(error) => println!("{url}: {error:#} ({:?})", start.elapsed()),
        }
    }
    Ok(())
}
