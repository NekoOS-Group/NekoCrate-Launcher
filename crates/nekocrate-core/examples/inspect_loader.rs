// Read-only diagnostic; does not open a database, download, launch or write game files.
use nekocrate_core::{minecraft, model::Instance, version};
use std::path::PathBuf;
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let game = PathBuf::from(
        args.first()
            .ok_or_else(|| anyhow::anyhow!("game directory required"))?,
    );
    let root = game.join(".no-legacy-fallback");
    for id in args.iter().skip(1) {
        let resolved = version::resolve(&root, &game, id)?;
        let instance: Instance = serde_json::from_value(
            serde_json::json!({"id":"diagnostic", "name":id, "game_directory":game, "versions":[],"selected_version":id,"memory_mb":2048,"java_path":null,"created_at":0,"last_played":null}),
        )?;
        let arguments = minecraft::arguments(
            &resolved.version,
            &root,
            &instance,
            &minecraft::LaunchSession::guest(),
        )?;
        let required_java = nekocrate_core::java::JavaRequirement::for_version(&resolved.version);
        let compatibility = nekocrate_core::java_compatibility::assess(&resolved.version, &game)?;
        println!(
            "Automatic Java: {:?}; {}",
            compatibility.majors,
            compatibility.reasons.join("; ")
        );
        println!(
            "{id}: MC {} {:?}; Java {} {}; {} launch args; workdir {}; readiness {:?}",
            resolved.minecraft_version,
            resolved.loader,
            required_java.major,
            required_java.arch,
            arguments.len(),
            version::working_directory(&game, &resolved.version)?.display(),
            version::check_launch_files(&root, &game, &resolved).map_err(|e| e.to_string())
        );
    }
    Ok(())
}
