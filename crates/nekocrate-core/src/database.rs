use crate::{model::Database, storage};
use anyhow::{Context, Result, bail, ensure};
use std::path::Path;

pub(crate) fn load(root: &Path, path: &Path) -> Result<Database> {
    if !path.exists() {
        return Ok(Database::default());
    }
    let value: serde_json::Value = crate::portable::read_database(root, path)?;
    ensure!(
        value.get("schema").and_then(|v| v.as_u64()) == Some(3),
        "不支持的数据格式版本，仅支持 schema 3"
    );
    let db: Database = serde_json::from_value(value).context("无法读取实例数据")?;
    for instance in &db.instances {
        storage::safe_id(&instance.id)?;
        if instance
            .game_directory
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
        {
            bail!("实例 {} 的关联目录必须是绝对路径", instance.name);
        }
        for version in &instance.versions {
            storage::safe_version(&version.id)?;
        }
        if instance.selected_version.is_some() && instance.selected().is_none() {
            bail!("实例 {} 的所选版本不存在", instance.name);
        }
    }
    Ok(db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn current_records_round_trip_but_invalid_directory_and_selection_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = crate::Launcher::open(Some(temp.path().into())).unwrap();
        let instance = launcher.create_instance("Current", "1.21.1").unwrap();
        let value = serde_json::to_value(launcher.snapshot().database).unwrap();
        drop(launcher);
        let file = temp.path().join("database.json");
        assert_eq!(
            load(temp.path(), &file).unwrap().instances[0].id,
            instance.id
        );
        for (field, invalid) in [
            ("game_directory", json!("../outside")),
            ("selected_version", json!("missing")),
            ("id", json!("../escape")),
        ] {
            let mut data = value.clone();
            data["instances"][0][field] = invalid;
            storage::write_json(&file, &data).unwrap();
            assert!(load(temp.path(), &file).is_err(), "{field}");
            assert_eq!(
                storage::read_json::<serde_json::Value>(&file).unwrap(),
                data
            );
        }
    }

    #[test]
    fn rejects_other_schemas_without_rewriting_or_moving_data() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("database.json");
        let game = temp.path().join("instances/old/game");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("level.dat"), b"world").unwrap();
        for schema in [json!(1), json!(2), json!(4), json!(null)] {
            let value = json!({"schema": schema, "instances": []});
            storage::write_json(&file, &value).unwrap();
            assert!(load(temp.path(), &file).is_err());
            assert_eq!(
                storage::read_json::<serde_json::Value>(&file).unwrap(),
                value
            );
            assert_eq!(std::fs::read(game.join("level.dat")).unwrap(), b"world");
            assert!(!temp.path().join("database.v1.json").exists());
            assert!(!temp.path().join("database.v2.json").exists());
        }
    }
}
