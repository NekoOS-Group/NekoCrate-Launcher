//! Bounded archive reads and portable paths shared by pack and loader installers.
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
use zip::ZipArchive;
pub(crate) const MAX_ENTRY: u64 = 1024 * 1024 * 1024;
const MAX_TOTAL: u64 = 8 * MAX_ENTRY;
pub(crate) fn relative(name: &str) -> Result<PathBuf> {
    if name.is_empty() || name.contains('\\') || name.starts_with('/') || name.len() > 1024 {
        bail!("整合包包含不安全的路径：{name}");
    }
    for part in name.trim_end_matches('/').split('/') {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with(['.', ' '])
            || part
                .chars()
                .any(|c| c.is_control() || ":*?\"<>|".contains(c))
            || ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.as_bytes()[3].is_ascii_digit())
        {
            bail!("整合包包含不安全的路径：{name}");
        }
    }
    Ok(PathBuf::from(name))
}
pub(crate) fn open(path: &Path) -> Result<ZipArchive<File>> {
    let mut zip = ZipArchive::new(File::open(path).context("无法读取整合包文件")?)
        .context("不是有效的整合包压缩文件")?;
    if zip.len() > 100_000 {
        bail!("整合包文件数量过多");
    }
    let mut total = 0_u64;
    let mut names = HashSet::new();
    for index in 0..zip.len() {
        let entry = zip.by_index(index)?;
        relative(entry.name())?;
        if !names.insert(entry.name().trim_end_matches('/').to_lowercase()) {
            bail!("整合包存在重复或大小写冲突的路径");
        }
        if entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            bail!("整合包不允许符号链接");
        }
        total = total.checked_add(entry.size()).context("整合包大小无效")?;
        if entry.size() > MAX_ENTRY || total > MAX_TOTAL {
            bail!("整合包解压大小超过安全限制");
        }
    }
    Ok(zip)
}
pub(crate) fn json(zip: &mut ZipArchive<File>, name: &str) -> Result<Value> {
    let entry = zip.by_name(name)?;
    if entry.size() > 8 * 1024 * 1024 {
        bail!("整合包清单过大");
    }
    Ok(serde_json::from_reader(entry.take(8 * 1024 * 1024 + 1))?)
}
pub(crate) fn component(value: &str) -> Result<String> {
    if value.is_empty()
        || value.len() > 100
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-+".contains(&c))
    {
        bail!("整合包游戏或加载器版本号无效");
    }
    Ok(value.to_owned())
}
