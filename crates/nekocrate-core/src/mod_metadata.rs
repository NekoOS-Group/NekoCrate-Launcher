//! One bounded descriptor reader/parser shared by display and compatibility code.
//! Cache parsed documents, not loader policy: each consumer keeps its own rules.
use crate::storage;
use anyhow::{Context, Result};
use serde_json::Value;
use std::{
    fs,
    io::{Read, Seek},
    path::Path,
};

mod cache;
pub(crate) use cache::{get, read};
pub(crate) const LIMIT: u64 = 1024 * 1024;
pub(crate) const DESCRIPTORS: [&str; 5] = [
    "fabric.mod.json",
    "quilt.mod.json",
    "META-INF/neoforge.mods.toml",
    "META-INF/mods.toml",
    "mcmod.info",
];

pub(crate) struct Document {
    pub name: &'static str,
    pub value: std::result::Result<Value, String>,
}
#[derive(Default)]
pub(crate) struct Metadata {
    pub documents: Vec<Document>,
    pub nested_forge: bool,
    issue: Option<String>,
    bytes: usize,
}
impl Metadata {
    pub(crate) fn check(&self) -> Result<()> {
        if let Some(issue) = &self.issue {
            anyhow::bail!("{issue}");
        }
        Ok(())
    }
    pub(crate) fn has(&self, name: &str) -> bool {
        self.documents.iter().any(|d| d.name == name)
    }
    pub(crate) fn value(&self, name: &str) -> Result<Option<&Value>> {
        self.check()?;
        self.documents
            .iter()
            .find(|d| d.name == name)
            .map(|d| d.value.as_ref().map_err(|e| anyhow::anyhow!("{e}")))
            .transpose()
    }
}

pub(crate) fn entry<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<Option<Vec<u8>>> {
    let mut file = match zip.by_name(name) {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    anyhow::ensure!(file.size() <= limit, "元数据内容过大");
    Ok(Some(storage::read_bounded(&mut file, limit)?))
}

#[inline(never)]
pub(crate) fn parse(name: &str, bytes: &[u8]) -> Result<Value> {
    if name.ends_with(".toml") {
        // Deserialize TOML straight into the shared JSON-shaped value tree.
        // No intermediate toml::Value or second serialization traversal.
        Ok(toml::from_str(std::str::from_utf8(bytes)?)?)
    } else if matches!(name, "fabric.mod.json" | "mcmod.info") {
        Ok(crate::fabric_metadata::parse(bytes)?)
    } else {
        Ok(serde_json::from_slice(bytes)?)
    }
}

pub(crate) fn document<'a, R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
    cached: Option<&'a Metadata>,
) -> Result<Option<std::borrow::Cow<'a, Value>>> {
    if let Some(cached) = cached.filter(|_| DESCRIPTORS.contains(&name)) {
        return Ok(cached.value(name)?.map(std::borrow::Cow::Borrowed));
    }
    entry(zip, name, LIMIT)?
        .map(|bytes| parse(name, &bytes).map(std::borrow::Cow::Owned))
        .transpose()
}

pub(crate) fn infos<'a>(name: &str, value: &'a Value) -> Result<&'a [Value]> {
    match name {
        "quilt.mod.json" => Ok(std::slice::from_ref(&value["quilt_loader"])),
        "fabric.mod.json" => Ok(std::slice::from_ref(value)),
        "mcmod.info" => value
            .as_array()
            .or_else(|| value["modList"].as_array())
            .map(Vec::as_slice)
            .context("Mod 描述格式无效"),
        _ => value["mods"]
            .as_array()
            .map(Vec::as_slice)
            .context("Mod 描述缺少 mods 列表"),
    }
}

fn load(path: &Path) -> Metadata {
    let mut result = Metadata::default();
    let read = (|| -> Result<()> {
        let mut zip = zip::ZipArchive::new(fs::File::open(path)?)?;
        result.nested_forge = zip
            .index_for_name("META-INF/jarjar/metadata.json")
            .is_some();
        for name in DESCRIPTORS {
            let parsed = match entry(&mut zip, name, LIMIT) {
                Ok(Some(bytes)) => parse(name, &bytes).map_err(|error| error.to_string()),
                Ok(None) => continue,
                Err(error) => Err(error.to_string()),
            };
            result.bytes += 256
                + match &parsed {
                    Ok(value) => tree_bytes(value),
                    Err(error) => error.capacity(),
                };
            result.documents.push(Document {
                name,
                value: parsed,
            });
        }
        Ok(())
    })();
    result.issue = read.err().map(|e| e.to_string());
    result
}

// Account for retained values directly, without serializing a second copy or
// overcharging long descriptions as though every byte were a separate node.
fn tree_bytes(value: &Value) -> usize {
    std::mem::size_of::<Value>()
        + match value {
            Value::String(text) => text.capacity(),
            Value::Array(items) => {
                items.iter().map(tree_bytes).sum::<usize>()
                    + (items.capacity() - items.len()) * std::mem::size_of::<Value>()
            }
            Value::Object(items) => items
                .iter()
                .map(|(key, value)| {
                    key.capacity()
                        + tree_bytes(value)
                        + std::mem::size_of::<String>()
                        + 4 * std::mem::size_of::<usize>()
                })
                .sum(),
            _ => 0,
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, sync::Arc};

    #[test]
    fn toml_value_types_match_existing_metadata_representation() {
        let text = "name='Cat'\ncount=2\nenabled=true\nratio=1.5\ninvalid=nan\nrelease=2026-11-16T12:34:56Z\n[dependencies]\nmods=['a','b']\n";
        let expected = serde_json::to_value(toml::from_str::<toml::Value>(text).unwrap()).unwrap();
        assert_eq!(
            parse("META-INF/mods.toml", text.as_bytes()).unwrap(),
            expected
        );
    }

    #[test]
    fn descriptors_share_parses_and_reject_changed_or_broken_files() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.jar");
        let write = |body: &str| {
            let mut archive = zip::ZipWriter::new(fs::File::create(&path).unwrap());
            archive
                .start_file("fabric.mod.json", zip::write::SimpleFileOptions::default())
                .unwrap();
            archive.write_all(body.as_bytes()).unwrap();
            archive.finish().unwrap();
        };
        write(r#"{"id":"cat","name":"Cat"}"#);
        let first = read(&path);
        assert_eq!(
            first.value("fabric.mod.json").unwrap().unwrap()["id"],
            "cat"
        );
        assert!(Arc::ptr_eq(&first, &read(&path)));
        write("broken");
        assert!(get(&path).is_none());
        let broken = read(&path);
        assert!(broken.has("fabric.mod.json"));
        assert!(broken.value("fabric.mod.json").is_err());
        assert!(broken.value("mcmod.info").unwrap().is_none());
        assert!(Arc::ptr_eq(&broken, &read(&path)));
        fs::write(&path, b"not a ZIP").unwrap();
        assert!(read(&path).check().is_err());
    }
}
