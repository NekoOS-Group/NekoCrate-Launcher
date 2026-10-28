use crate::AppState;
use anyhow::{Context, Result};
use nekocrate_core::{model::EventSink, resource_manager::CacheKind};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tauri_plugin_opener::OpenerExt;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    Java,
    RegisterJava {
        path: PathBuf,
    },
    InstallJava {
        major: u32,
    },
    RemoveJava {
        path: PathBuf,
    },
    OpenJava {
        path: PathBuf,
    },
    Groups {
        content: nekocrate_core::version_management::ContentKind,
    },
    Cache,
    ClearCache {
        category: CacheKind,
    },
    Skins,
    GuestSkin,
    SetGuestSkin {
        id: String,
    },
    ApplyAccountSkin {
        account: String,
        id: String,
    },
    ImportSkin {
        path: PathBuf,
    },
    SkinBytes {
        id: String,
    },
    EditSkin {
        id: String,
        name: String,
        slim: bool,
    },
    DeleteSkin {
        id: String,
    },
    ExportSkin {
        id: String,
        path: PathBuf,
    },
    ExportAccountSkin {
        id: String,
        path: PathBuf,
    },
}

fn export_skin(
    launcher: &nekocrate_core::Launcher,
    id: &str,
    path: &std::path::Path,
) -> Result<()> {
    let bytes: &[u8] = match id {
        "builtin:steve" => include_bytes!("../../src/assets/steve.png"),
        "builtin:cirno" => include_bytes!("../../src/assets/cirno.png"),
        _ => return launcher.export_saved_skin(id, path),
    };
    nekocrate_core::resource_manager::export_skin_png(bytes, path)
}

async fn skin_for_upload(launcher: &nekocrate_core::Launcher, id: &str) -> Result<(Vec<u8>, bool)> {
    let (skin, bytes) = launcher.resource_skin(id).await?;
    let bytes = match bytes {
        Some(bytes) => bytes,
        None if id == "builtin:cirno" => include_bytes!("../../src/assets/cirno.png").to_vec(),
        // Builtin Steve, or an account without a custom texture.
        None => include_bytes!("../../src/assets/steve.png").to_vec(),
    };
    Ok((bytes, skin.slim))
}

#[inline(never)]
pub fn handle<'a>(
    action: Action,
    state: &'a AppState,
    app: &'a tauri::AppHandle,
    sink: &'a EventSink,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value>> + Send + 'a>> {
    Box::pin(async move {
        let launcher = state.launcher.clone();
        match action {
            Action::ApplyAccountSkin { account, id } => {
                let (bytes, slim) = skin_for_upload(&launcher, &id).await?;
                Ok(json!(
                    launcher.apply_account_skin(&account, &bytes, slim).await?
                ))
            }
            Action::GuestSkin => {
                let (skin, bytes) = launcher.guest_skin().await?;
                Ok(json!({ "skin": skin, "bytes": bytes }))
            }
            Action::ExportAccountSkin { id, path } => {
                launcher.export_account_skin(&id, &path).await?;
                Ok(Value::Null)
            }
            Action::Java => Ok(json!(launcher.resource_java().await?)),
            Action::RegisterJava { path } => Ok(json!(launcher.register_java(&path).await?)),
            Action::InstallJava { major } => {
                let (cancel, _guard) = state.job().await?;
                Ok(json!(
                    launcher.install_resource_java(major, &cancel, sink).await?
                ))
            }
            Action::OpenJava { path } => {
                let runtime = launcher
                    .resource_java()
                    .await?
                    .into_iter()
                    .find(|r| Path::new(&r.java.path) == path)
                    .context("Java 不在资源列表中")?;
                app.opener().reveal_item_in_dir(runtime.java.path)?;
                Ok(Value::Null)
            }
            other => {
                tauri::async_runtime::spawn_blocking(move || -> Result<Value> {
                    Ok(match other {
                        Action::Groups { content } => json!(launcher.resource_groups(content)),
                        Action::Cache => json!(launcher.resource_cache()?),
                        Action::ClearCache { category } => {
                            launcher.clear_resource_cache(category)?;
                            Value::Null
                        }
                        Action::RemoveJava { path } => {
                            launcher.remove_resource_java(&path)?;
                            Value::Null
                        }
                        Action::Skins => json!(launcher.saved_skins()?),
                        Action::SetGuestSkin { id } => json!(launcher.set_guest_skin(&id)?),
                        Action::ImportSkin { path } => json!(launcher.import_saved_skin(&path)?),
                        Action::SkinBytes { id } => json!(launcher.saved_skin_bytes(&id)?),
                        Action::EditSkin { id, name, slim } => {
                            launcher.edit_saved_skin(&id, &name, slim)?;
                            Value::Null
                        }
                        Action::DeleteSkin { id } => {
                            launcher.delete_saved_skin(&id)?;
                            Value::Null
                        }
                        Action::ExportSkin { id, path } => {
                            export_skin(&launcher, &id, &path)?;
                            Value::Null
                        }
                        _ => unreachable!(),
                    })
                })
                .await?
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn uploads_resolve_bundled_and_imported_skins_without_frontend_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = nekocrate_core::Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        for (id, slim, expected) in [
            (
                "builtin:steve",
                false,
                &include_bytes!("../../src/assets/steve.png")[..],
            ),
            (
                "builtin:cirno",
                true,
                &include_bytes!("../../src/assets/cirno.png")[..],
            ),
        ] {
            let (bytes, model) = skin_for_upload(&launcher, id).await.unwrap();
            assert_eq!(bytes, expected);
            assert_eq!(model, slim);
        }
        let source = include_bytes!("../../src/assets/cirno.png");
        let saved = launcher.save_skin_bytes("Imported", source, true).unwrap();
        let (bytes, slim) = skin_for_upload(&launcher, &saved.id).await.unwrap();
        assert_eq!(bytes, source);
        assert!(slim);
        for invalid in ["builtin:missing", "account:missing", "../../database.json"] {
            assert!(skin_for_upload(&launcher, invalid).await.is_err());
        }
    }

    #[test]
    fn bundled_skins_export_offline_without_creating_saved_copies() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = nekocrate_core::Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        for (name, original) in [
            ("steve", &include_bytes!("../../src/assets/steve.png")[..]),
            ("cirno", &include_bytes!("../../src/assets/cirno.png")[..]),
        ] {
            let output = temp.path().join(format!("{name}.png"));
            export_skin(&launcher, &format!("builtin:{name}"), &output).unwrap();
            assert_eq!(std::fs::read(output).unwrap(), original);
            assert!(
                export_skin(
                    &launcher,
                    &format!("builtin:{name}"),
                    &temp.path().join("bad.json")
                )
                .is_err()
            );
            assert!(
                launcher
                    .edit_saved_skin(&format!("builtin:{name}"), "changed", false)
                    .is_err()
            );
            assert!(
                launcher
                    .delete_saved_skin(&format!("builtin:{name}"))
                    .is_err()
            );
        }
        assert!(!launcher.root().join("skin-library").exists());
        assert!(!launcher.root().join("cache/skins").exists());
        assert!(
            export_skin(
                &launcher,
                "builtin:missing",
                &temp.path().join("missing.png")
            )
            .is_err()
        );
    }
}
