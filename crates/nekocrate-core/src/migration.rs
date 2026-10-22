use crate::{model::Database, storage};
use anyhow::{Context, Result, bail};
use serde_json::json;
use std::{fs, path::Path};

pub(crate) fn load_database(root: &Path, path: &Path) -> Result<Database> {
    if !path.exists() {
        return Ok(Database::default());
    }
    let mut value: serde_json::Value = storage::read_json(path)?;
    let schema = value.get("schema").and_then(|v| v.as_u64()).unwrap_or(1);
    if !(1..=3).contains(&schema) {
        bail!("不支持的数据格式版本 {schema}");
    }
    if schema == 1 {
        let instances = value["instances"].as_array_mut().context("实例数据无效")?;
        for instance in instances {
            let old = instance.as_object_mut().context("实例数据无效")?;
            let version = old.remove("version").context("旧实例缺少版本")?;
            let installed = old.remove("installed").unwrap_or(json!(false));
            old.insert(
                "versions".into(),
                json!([{ "id": version, "installed": installed }]),
            );
            old.insert("selected_version".into(), version);
        }
        value["schema"] = json!(2);
    }
    let mut db: Database = serde_json::from_value(value).context("无法读取实例数据")?;
    // Validate all identifiers before using any of them as paths during migration.
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
    if schema == 1 {
        // Keep a stable pre-migration copy in addition to the normal rolling backup.
        let backup = root.join("database.v1.json");
        if !backup.exists() {
            fs::copy(path, backup)?;
        }
        for instance in &mut db.instances {
            let dir = root.join("instances").join(&instance.id);
            let old_game = dir.join("game");
            let game = dir.join(".minecraft");
            if old_game.exists() {
                if game.exists() {
                    bail!(
                        "实例 {} 同时存在 game 与 .minecraft，未覆盖任何游戏文件",
                        instance.name
                    );
                }
                fs::rename(old_game, &game)?;
            }
            fs::create_dir_all(&game)?;
            for version in &mut instance.versions {
                let dest = game.join("versions").join(&version.id);
                fs::create_dir_all(&dest)?;
                for (source, target) in [
                    (
                        dir.join("version.json"),
                        dest.join(format!("{}.json", version.id)),
                    ),
                    (dir.join("runtime.json"), dest.join("runtime.json")),
                    (
                        root.join("versions")
                            .join(&version.id)
                            .join(format!("{}.jar", version.id)),
                        dest.join(format!("{}.jar", version.id)),
                    ),
                ] {
                    if source.exists() && !target.exists() {
                        fs::copy(source, target)?;
                    }
                }
                if dir.join("natives").is_dir() && !dest.join("natives").exists() {
                    storage::copy_tree(&dir.join("natives"), &dest.join("natives"))?;
                }
                version.installed &= dest.join(format!("{}.json", version.id)).is_file()
                    && dest.join(format!("{}.jar", version.id)).is_file()
                    && dest.join("natives").is_dir();
            }
        }
    }
    if schema < 3 {
        // Older launchers must reject linked-directory databases rather than silently
        // discarding their paths. Existing managed games remain exactly where they are.
        let backup = root.join(format!("database.v{schema}.json"));
        if !backup.exists() {
            fs::copy(path, backup)?;
        }
        db.schema = 3;
        storage::write_json(path, &db)?;
    }
    Ok(db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Launcher;

    #[test]
    fn upgrades_schema_two_without_relocating_existing_copies() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let id = uuid::Uuid::new_v4().to_string();
        let game = root.join("instances").join(&id).join(".minecraft");
        fs::create_dir_all(game.join("saves/world")).unwrap();
        fs::write(game.join("saves/world/level.dat"), b"existing-copy").unwrap();
        let old = json!({"schema":2,"instances":[{
            "id":id,"name":"旧版导入副本","icon":2,
            "versions":[{"id":"1.21.1","installed":true}],"selected_version":"1.21.1",
            "memory_mb":2048,"java_path":null,"created_at":1,"last_played":null
        }]});
        storage::write_json(&root.join("database.json"), &old).unwrap();
        let launcher = Launcher::open(Some(root.into())).unwrap();
        assert_eq!(launcher.snapshot().database.schema, 3);
        assert_eq!(
            launcher.game_dir(&id).unwrap(),
            game.canonicalize().unwrap()
        );
        let instance = launcher.instance(&id).unwrap();
        assert_eq!(instance.game_directory, None);
        assert_eq!(instance.icon, Some(2));
        assert!(instance.selected().unwrap().installed);
        assert_eq!(
            fs::read(game.join("saves/world/level.dat")).unwrap(),
            b"existing-copy"
        );
        assert_eq!(
            storage::read_json::<serde_json::Value>(&root.join("database.v2.json")).unwrap(),
            old
        );
        drop(launcher);
        let reopened = Launcher::open(Some(root.into())).unwrap();
        assert_eq!(reopened.instance(&id).unwrap().game_directory, None);
    }

    #[test]
    fn rejects_relative_linked_directory_without_mutating_database() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("database.json");
        let invalid = json!({"schema":3,"instances":[{
            "id":uuid::Uuid::new_v4().to_string(),"name":"错误路径",
            "game_directory":"../outside","versions":[],"selected_version":null,
            "memory_mb":2048,"java_path":null,"created_at":1,"last_played":null
        }]});
        storage::write_json(&file, &invalid).unwrap();
        assert!(
            load_database(temp.path(), &file)
                .unwrap_err()
                .to_string()
                .contains("绝对路径")
        );
        assert_eq!(
            storage::read_json::<serde_json::Value>(&file).unwrap(),
            invalid
        );
    }

    #[test]
    fn upgrades_old_instance_without_losing_saves_or_installation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let id = uuid::Uuid::new_v4().to_string();
        let dir = root.join("instances").join(&id);
        fs::create_dir_all(dir.join("game/saves/world")).unwrap();
        fs::write(dir.join("game/saves/world/level.dat"), b"world-data").unwrap();
        fs::create_dir_all(dir.join("natives")).unwrap();
        fs::write(dir.join("natives/test-native"), b"native-v1").unwrap();
        storage::write_json(&dir.join("version.json"), &json!({"id": "1.21.1"})).unwrap();
        fs::create_dir_all(root.join("versions/1.21.1")).unwrap();
        fs::write(root.join("versions/1.21.1/1.21.1.jar"), b"client-v1").unwrap();
        let legacy = json!({
            "schema": 1,
            "instances": [{"id": id, "name": "旧实例", "version": "1.21.1", "memory_mb": 4096,
                "java_path": null, "created_at": 1, "installed": true, "last_played": 2}]
        });
        storage::write_json(&root.join("database.json"), &legacy).unwrap();
        let launcher = Launcher::open(Some(root.into())).unwrap();
        let instance = launcher.instance(&id).unwrap();
        assert_eq!(instance.icon, None);
        assert_eq!(instance.selected_version.as_deref(), Some("1.21.1"));
        assert!(instance.selected().unwrap().installed);
        assert_eq!(instance.memory_mb, 4096);
        let game = launcher.game_dir(&id).unwrap();
        assert_eq!(
            fs::read(game.join("saves/world/level.dat")).unwrap(),
            b"world-data"
        );
        assert_eq!(
            fs::read(game.join("versions/1.21.1/natives/test-native")).unwrap(),
            b"native-v1"
        );
        assert_eq!(
            fs::read(game.join("versions/1.21.1/1.21.1.jar")).unwrap(),
            b"client-v1"
        );
        assert_eq!(
            storage::read_json::<serde_json::Value>(&root.join("database.v1.json")).unwrap(),
            legacy
        );
        drop(launcher);
        let reopened = Launcher::open(Some(root.into())).unwrap();
        assert_eq!(reopened.snapshot().database.schema, 3);
        assert!(
            reopened
                .instance(&id)
                .unwrap()
                .selected()
                .unwrap()
                .installed
        );
    }
}
