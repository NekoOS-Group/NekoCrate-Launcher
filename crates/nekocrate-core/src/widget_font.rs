//! Vanilla Unihex glyphs for the optional splash widget; no full font in the binary.
use crate::{Launcher, network, storage};
use anyhow::{Result, ensure};
use sha1::{Digest, Sha1};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader, Cursor, Read},
    time::Duration,
};
const HASH: &str = "666999116c467f10622db1527a06ddc0a6efad2a";
const SIZE: usize = 1_496_916;
const FILE: &str = "unifont_all_no_pua-15.1.05.hex";
fn valid(bytes: &[u8]) -> bool {
    bytes.len() == SIZE && hex::encode(Sha1::digest(bytes)) == HASH
}
fn extract(bytes: Vec<u8>, text: &str) -> Result<BTreeMap<String, String>> {
    let wanted: BTreeSet<char> = text.chars().collect();
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    let input = archive.by_name(FILE)?;
    ensure!(input.size() <= 8 * 1024 * 1024, "字体内容过大");
    let mut result = BTreeMap::new();
    for line in BufReader::new(input.take(8 * 1024 * 1024)).lines() {
        let line = line?;
        let Some((code, bits)) = line.split_once(':') else {
            continue;
        };
        if let Some(c) = u32::from_str_radix(code, 16).ok().and_then(char::from_u32)
            && wanted.contains(&c)
            && matches!(bits.len(), 32 | 64 | 96 | 128)
            && bits.bytes().all(|b| b.is_ascii_hexdigit())
        {
            result.insert(c.to_string(), bits.to_string());
        }
        if result.len() == wanted.len() {
            break;
        }
    }
    Ok(result)
}
pub async fn glyphs(launcher: &Launcher, text: String) -> Result<BTreeMap<String, String>> {
    ensure!(text.chars().count() <= 500, "标语过长");
    let cache = launcher.root().join("cache/fonts/unifont-15.1.05.zip");
    let mut candidates = vec![cache.clone()];
    candidates.extend(
        launcher
            .snapshot()
            .database
            .instances
            .iter()
            .map(|instance| {
                instance
                    .game_dir(launcher.root())
                    .join(format!("assets/objects/66/{HASH}"))
            }),
    );
    let mut font = None;
    for path in candidates {
        if tokio::fs::metadata(&path)
            .await
            .is_ok_and(|meta| meta.len() == SIZE as u64)
            && let Ok(bytes) = tokio::fs::read(path).await
            && valid(&bytes)
        {
            font = Some(bytes);
            break;
        }
    }
    let bytes = if let Some(bytes) = font {
        bytes
    } else {
        let client = network::client_builder(launcher.root(), &launcher.settings())?
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let mut response = client
            .get(format!(
                "https://resources.download.minecraft.net/66/{HASH}"
            ))
            .send()
            .await?
            .error_for_status()?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(bytes.len() + chunk.len() <= SIZE, "字体资源过大");
            bytes.extend_from_slice(&chunk);
        }
        ensure!(valid(&bytes), "字体资源校验失败");
        // A read-only/full cache does not prevent rendering a verified response.
        let _ = storage::atomic_write(&cache, &bytes);
        bytes
    };
    tokio::task::spawn_blocking(move || extract(bytes, &text)).await?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extracts_requested_unicode_without_expanding_the_archive_to_disk() {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file(FILE, zip::write::SimpleFileOptions::default())
            .unwrap();
        use std::io::Write;
        writeln!(
            archive,
            "0041:0000182442427E424242420000000000\n4F60:{}",
            "00".repeat(32)
        )
        .unwrap();
        let result = extract(archive.finish().unwrap().into_inner(), "你你缺").unwrap();
        assert_eq!(result.len(), 1);
        assert!(result.contains_key("你"));
        assert!(!valid(b"invalid"));
    }
}
