use anyhow::Result;
use clap::{Parser, Subcommand};
use nekocrate_core::{Launcher, model::EventSink};
use std::{path::PathBuf, sync::Arc};
use tokio_util::sync::CancellationToken;

#[derive(Parser)]
#[command(
    name = "nekocrate",
    version,
    about = "猫咪盒子启动器 · NekoCrate Launcher"
)]
struct Cli {
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    Status,
    Versions,
    Create {
        name: String,
        #[arg(long, default_value = "")]
        version: String,
    },
    AddVersion {
        id: String,
        version: String,
    },
    SelectVersion {
        id: String,
        version: String,
    },
    Install {
        id: String,
    },
    Repair {
        id: String,
    },
    Launch {
        id: String,
    },
    Clone {
        id: String,
    },
    Delete {
        id: String,
    },
    Import {
        source: PathBuf,
        name: String,
        #[arg(long, default_value = "")]
        version: String,
    },
    /// Associate an existing instance with its original directory without moving files.
    Relink {
        id: String,
        source: PathBuf,
    },
    Java,
    Login,
    Refresh,
    SelectAccount {
        id: String,
    },
    RemoveAccount {
        id: String,
    },
    Configure {
        #[arg(long)]
        client_id: Option<String>,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        concurrency: Option<usize>,
    },
}
#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();
    let launcher = Launcher::open(args.data_dir)?;
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        signal.cancel();
    });
    let sink: EventSink = Arc::new(|e| {
        if !["transfer", "file", "file_plan"].contains(&e.kind.as_str())
            && (e.kind != "progress" || e.completed % 100 == 0 || e.completed == e.total)
        {
            eprintln!(
                "[{}] {} {}{}",
                e.kind,
                e.message,
                if e.total > 0 {
                    format!("{}/", e.completed)
                } else {
                    String::new()
                },
                if e.total > 0 {
                    e.total.to_string()
                } else {
                    String::new()
                }
            );
        }
    });
    match args.command {
        Commands::Status => println!("{}", serde_json::to_string_pretty(&launcher.snapshot())?),
        Commands::Versions => {
            let m = launcher.versions(&cancel).await?;
            for v in m.versions.iter().take(60) {
                println!("{}\t{}", v.id, v.kind);
            }
        }
        Commands::Create { name, version } => println!(
            "{}",
            serde_json::to_string_pretty(&launcher.create_instance(&name, &version)?)?
        ),
        Commands::AddVersion { id, version } => println!(
            "{}",
            serde_json::to_string_pretty(&launcher.add_version(&id, &version)?)?
        ),
        Commands::SelectVersion { id, version } => launcher.select_version(&id, &version)?,
        Commands::Install { id } => {
            launcher.install(&id, &cancel, &sink).await?;
        }
        Commands::Repair { id } => {
            launcher.repair(&id, &cancel, &sink).await?;
        }
        Commands::Clone { id } => println!(
            "{}",
            serde_json::to_string_pretty(&launcher.clone_instance(&id).await?)?
        ),
        Commands::Import {
            source,
            name,
            version,
        } => println!(
            "{}",
            serde_json::to_string_pretty(&launcher.import_instance(source, name, version).await?)?
        ),
        Commands::Delete { id } => launcher.delete_instance(&id)?,
        Commands::Relink { id, source } => println!(
            "{}",
            serde_json::to_string_pretty(&launcher.relink_instance(&id, source).await?)?
        ),
        Commands::Java => println!(
            "{}",
            serde_json::to_string_pretty(&nekocrate_core::java::discover(launcher.root()).await)?
        ),
        Commands::Login => {
            let challenge = launcher.begin_login().await?;
            println!(
                "请打开 {} 并输入验证码 {}",
                challenge.verification_uri, challenge.user_code
            );
            let a = launcher.finish_login(challenge, &cancel).await?;
            println!("已登录：{}（Java 权益已验证）", a.username);
        }
        Commands::Refresh => {
            println!("已刷新：{}", launcher.refresh_account().await?.username);
        }
        Commands::SelectAccount { id } => launcher.select_account(&id)?,
        Commands::RemoveAccount { id } => launcher.remove_account(&id)?,
        Commands::Configure {
            client_id,
            source,
            concurrency,
        } => {
            let mut s = launcher.settings();
            if let Some(v) = client_id {
                s.microsoft_client_id = v;
            }
            if let Some(v) = source {
                s.download_source = v;
            }
            if let Some(v) = concurrency {
                s.concurrency = v;
            }
            launcher.save_settings(s)?;
            println!("配置已保存");
        }
        Commands::Launch { id } => {
            launcher.launch(&id, &cancel, sink).await?;
            while launcher.snapshot().running.contains(&id) {
                tokio::select! {_=cancel.cancelled()=>{launcher.stop_all();break},_=tokio::time::sleep(std::time::Duration::from_millis(500))=>{}}
            }
        }
    }
    Ok(())
}
