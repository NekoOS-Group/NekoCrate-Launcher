use super::*;
use flate2::read::GzDecoder;
use serde::Deserialize;

#[derive(Deserialize)]
struct Root {
    #[serde(rename = "Data")]
    data: World,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct World {
    level_name: Option<String>,
    version: Option<WorldVersion>,
    data_version: Option<i32>,
    last_played: Option<i64>,
    game_type: Option<i32>,
    difficulty: Option<i8>,
    #[serde(rename = "hardcore")]
    hardcore: Option<i8>,
    #[serde(rename = "allowCommands")]
    allow_commands: Option<i8>,
    random_seed: Option<i64>,
    world_gen_settings: Option<WorldGen>,
    spawn_x: Option<i32>,
    spawn_y: Option<i32>,
    spawn_z: Option<i32>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WorldVersion {
    name: Option<String>,
}
#[derive(Deserialize)]
struct WorldGen {
    seed: Option<i64>,
}

pub(super) fn read(path: &Path) -> Vec<ContentSource> {
    let mut sources = Vec::new();
    for name in ["level.dat", "level.dat_old"] {
        if !path.join(name).exists() {
            continue;
        }
        let mut source = ContentSource::new(name);
        match parse(path, name, &mut source) {
            Ok(()) => {
                sources.push(source);
                return sources;
            }
            Err(error) => {
                source.issue = Some(error.to_string());
                sources.push(source);
            }
        }
    }
    sources
}
fn bytes(path: &Path, name: &str) -> Result<Vec<u8>> {
    let bytes = bounded(
        fs::File::open(contained(path, &path.join(name))?)?,
        16 * TEXT_LIMIT,
    )?;
    if bytes.starts_with(&[0x1f, 0x8b]) {
        bounded(GzDecoder::new(bytes.as_slice()), 16 * TEXT_LIMIT)
    } else {
        Ok(bytes)
    }
}

pub(super) fn raw(path: &Path, name: &str) -> Result<String> {
    let value: fastnbt::Value = fastnbt::from_bytes_with_opts(
        &bytes(path, name)?,
        fastnbt::DeOpts::new().max_seq_len(65536),
    )?;
    let text = serde_json::to_string_pretty(&value)?;
    if text.len() as u64 > 16 * TEXT_LIMIT {
        bail!("元数据内容过大");
    }
    Ok(text)
}

fn parse(path: &Path, name: &str, source: &mut ContentSource) -> Result<()> {
    let bytes = bytes(path, name)?;
    // Deserialize only the display fields. Player inventories, registries and
    // worldgen settings are skipped; never open chunks or acquire session.lock.
    let root: Root =
        fastnbt::from_bytes_with_opts(&bytes, fastnbt::DeOpts::new().max_seq_len(65536))?;
    let data = root.data;
    source.title = data.level_name;
    if let Some(name) = data.version.and_then(|v| v.name) {
        source.field("游戏版本", name);
    }
    if let Some(version) = data.data_version {
        source.field("数据版本", version);
    }
    if let Some(time) = data.last_played.filter(|v| *v > 0) {
        source.fields.push(ContentField {
            label: "最后游玩".into(),
            value: time.to_string(),
            format: Some("timestamp"),
        });
    }
    if let Some(mode) = data.game_type {
        match mode {
            0 => source.enum_field("游戏模式", "生存"),
            1 => source.enum_field("游戏模式", "创造"),
            2 => source.enum_field("游戏模式", "冒险"),
            3 => source.enum_field("游戏模式", "旁观"),
            _ => source.field("游戏模式", mode),
        }
    }
    if let Some(difficulty) = data.difficulty {
        match difficulty {
            0 => source.enum_field("难度", "和平"),
            1 => source.enum_field("难度", "简单"),
            2 => source.enum_field("难度", "普通"),
            3 => source.enum_field("难度", "困难"),
            _ => source.field("难度", difficulty),
        }
    }
    for (label, value) in [
        ("极限模式", data.hardcore),
        ("允许作弊", data.allow_commands),
    ] {
        if let Some(value) = value {
            source.enum_field(label, if value == 0 { "否" } else { "是" });
        }
    }
    if let Some(seed) = data
        .world_gen_settings
        .and_then(|v| v.seed)
        .or(data.random_seed)
    {
        // Keep 64-bit seeds as strings across IPC so JavaScript cannot round them.
        source.field("世界种子", seed);
    }
    if let (Some(x), Some(y), Some(z)) = (data.spawn_x, data.spawn_y, data.spawn_z) {
        source.field("出生点", format!("{x}, {y}, {z}"));
    }
    Ok(())
}
