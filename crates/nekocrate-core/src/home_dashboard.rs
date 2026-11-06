//! Read-only home widgets. Never scan world chunks, mutate servers.dat, or hold
//! the installation lock while discovering local shortcuts or querying a server.
use crate::{Launcher, storage, version};
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::Path,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

#[derive(Clone, Debug, Serialize)]
pub struct SavedServer {
    pub name: String,
    pub address: String,
    pub instance: String,
    pub version: String,
    pub icon: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct RecentWorld {
    pub key: String,
    pub name: String,
    pub file: String,
    pub instance: String,
    pub version: String,
    pub game_version: Option<String>,
    pub last_played: u64,
    pub icon: Option<String>,
}
#[derive(Debug, Serialize, Default)]
pub struct LocalWidgets {
    pub servers: Vec<SavedServer>,
    pub worlds: Vec<RecentWorld>,
}
#[derive(Deserialize, Default)]
struct ServerFile {
    #[serde(default)]
    servers: Vec<ServerEntry>,
}
#[derive(Deserialize)]
struct ServerEntry {
    name: String,
    ip: String,
    icon: Option<String>,
}

fn bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "小组件元数据过大");
    Ok(bytes)
}
fn png(bytes: &[u8]) -> Option<String> {
    if bytes.len() > 96 * 1024 || bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    (width > 0 && height > 0 && width <= 256 && height <= 256)
        .then(|| format!("data:image/png;base64,{}", STANDARD.encode(bytes)))
}
fn server_icon(value: &str) -> Option<String> {
    let data = value
        .strip_prefix("data:image/png;base64,")
        .unwrap_or(value);
    if data.len() > 128 * 1024 {
        return None;
    }
    png(&STANDARD.decode(data).ok()?)
}
impl Launcher {
    pub fn home_widget_local(&self) -> LocalWidgets {
        let mut result = LocalWidgets::default();
        let mut directories = HashSet::new();
        let mut worlds = HashSet::new();
        for instance in self.snapshot().database.instances {
            let Ok(game) = self.available_game_dir(&instance.id) else {
                continue;
            };
            let mut versions = instance
                .versions
                .iter()
                .filter(|v| v.installed)
                .collect::<Vec<_>>();
            versions.sort_by_key(|v| instance.selected_version.as_deref() != Some(&v.id));
            for profile in versions {
                let Ok(working) =
                    version::instance_working_directory(&game, &instance, &profile.id)
                        .and_then(|p| fs::canonicalize(p).map_err(Into::into))
                else {
                    continue;
                };
                if !directories.insert(working.clone()) {
                    continue;
                }
                if let Ok(bytes) = bounded(&working.join("servers.dat"), 2 * 1024 * 1024)
                    && let Ok(list) = fastnbt::from_bytes_with_opts::<ServerFile>(
                        &bytes,
                        fastnbt::DeOpts::new().max_seq_len(4096),
                    )
                {
                    for item in list.servers.into_iter().take(128) {
                        if let Ok(address) = ServerAddress::parse(&item.ip) {
                            result.servers.push(SavedServer {
                                name: plain(&item.name, 80),
                                address: address.display(),
                                instance: instance.id.clone(),
                                version: profile.id.clone(),
                                icon: item.icon.as_deref().and_then(server_icon),
                            });
                        }
                    }
                }
                let Ok(entries) = fs::read_dir(working.join("saves")) else {
                    continue;
                };
                for entry in entries.flatten() {
                    if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                        continue;
                    }
                    let path = entry.path();
                    let level = ["level.dat", "level.dat_old"]
                        .into_iter()
                        .map(|name| path.join(name))
                        .find(|p| p.is_file());
                    let Some(level) = level else { continue };
                    let Ok(canonical) = fs::canonicalize(&path) else {
                        continue;
                    };
                    if !canonical.starts_with(&working) || !worlds.insert(canonical.clone()) {
                        continue;
                    }
                    let source = crate::version_management::details::world::read(&path)
                        .into_iter()
                        .find(|s| s.issue.is_none());
                    let field = |label: &str| {
                        source
                            .as_ref()
                            .and_then(|s| s.fields.iter().find(|f| f.label == label))
                            .map(|f| f.value.clone())
                    };
                    let modified = fs::metadata(level)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|m| m.as_millis() as u64)
                        .unwrap_or(0);
                    let file = entry.file_name().to_string_lossy().into_owned();
                    result.worlds.push(RecentWorld {
                        key: canonical.to_string_lossy().into_owned(),
                        name: source
                            .as_ref()
                            .and_then(|s| s.title.clone())
                            .filter(|s| !s.trim().is_empty())
                            .unwrap_or_else(|| file.clone()),
                        file,
                        instance: instance.id.clone(),
                        version: profile.id.clone(),
                        game_version: field("游戏版本"),
                        last_played: field("最后游玩")
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(modified),
                        icon: bounded(&path.join("icon.png"), 96 * 1024)
                            .ok()
                            .as_deref()
                            .and_then(png),
                    });
                }
            }
        }
        result.worlds.sort_by(|a, b| {
            b.last_played
                .cmp(&a.last_played)
                .then_with(|| a.key.cmp(&b.key))
        });
        result.worlds.truncate(12);
        result.servers.truncate(128);
        result
    }
}

#[derive(Debug, Clone)]
pub struct ServerAddress {
    pub host: String,
    pub port: u16,
}
impl ServerAddress {
    pub fn parse(input: &str) -> Result<Self> {
        let input = input
            .trim()
            .strip_prefix("minecraft://")
            .unwrap_or(input.trim());
        ensure!(
            !input.is_empty()
                && input.len() <= 255
                && !input.chars().any(|c| c.is_whitespace() || c.is_control())
                && !input.contains(['/', '?', '#', '@', '\\']),
            "请输入有效的服务器地址"
        );
        let input = if input.parse::<std::net::Ipv6Addr>().is_ok() {
            format!("[{input}]")
        } else {
            input.to_owned()
        };
        let url =
            reqwest::Url::parse(&format!("http://{input}")).context("请输入有效的服务器地址")?;
        let host = url
            .host_str()
            .context("请输入有效的服务器地址")?
            .trim_matches(['[', ']'])
            .to_owned();
        let suffix = if input.starts_with('[') {
            input.split_once(']').map(|(_, tail)| tail).unwrap_or("")
        } else {
            input.find(':').map(|index| &input[index..]).unwrap_or("")
        };
        let port = if let Some(port) = suffix.strip_prefix(':') {
            port.parse::<u16>().context("服务器端口无效")?
        } else {
            25565
        };
        ensure!(port > 0, "服务器端口无效");
        Ok(Self { host, port })
    }
    pub fn display(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if self.port == 25565 {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }
}
fn plain(value: &str, limit: usize) -> String {
    let mut skip = false;
    value
        .chars()
        .filter(|c| {
            if skip {
                skip = false;
                return false;
            }
            if *c == '§' {
                skip = true;
                return false;
            }
            !c.is_control() || *c == '\n'
        })
        .take(limit)
        .collect()
}
fn motd(value: &Value, depth: usize, output: &mut String) {
    if depth > 8 || output.len() > 4096 {
        return;
    }
    match value {
        Value::String(s) => output.extend(s.chars().take(1024)),
        Value::Array(items) => {
            for item in items.iter().take(64) {
                motd(item, depth + 1, output);
            }
        }
        Value::Object(fields) => {
            if let Some(s) = fields.get("text") {
                motd(s, depth + 1, output);
            }
            if let Some(extra) = fields.get("extra") {
                motd(extra, depth + 1, output);
            }
        }
        _ => (),
    }
}
fn varint(mut number: u32, out: &mut Vec<u8>) {
    loop {
        let mut byte = (number & 127) as u8;
        number >>= 7;
        if number != 0 {
            byte |= 128;
        }
        out.push(byte);
        if number == 0 {
            break;
        }
    }
}
async fn read_varint(reader: &mut (impl AsyncReadExt + Unpin)) -> Result<usize> {
    let mut value = 0u32;
    for index in 0..5 {
        let byte = reader.read_u8().await?;
        ensure!(index != 4 || byte & 0xf0 == 0, "服务器返回了无效长度");
        value |= ((byte & 127) as u32) << (index * 7);
        if byte & 128 == 0 {
            return Ok(value as usize);
        }
    }
    bail!("服务器返回了无效长度")
}
async fn packet(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let length = read_varint(stream).await?;
    ensure!(length > 0 && length <= 192 * 1024, "服务器响应过大");
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await?;
    Ok(bytes)
}
#[derive(Debug, Serialize)]
pub struct ServerStatus {
    pub online: u64,
    pub max: u64,
    pub latency: u64,
    pub motd: String,
    pub version: String,
    pub icon: Option<String>,
}
/// Java 1.7+ status handshake, JSON response and ping; no login/session tokens.
/// Protocol reference: mcstatus.readthedocs.io/en/stable/_modules/mcstatus/_protocol/java_client/
pub async fn server_status(address: &str) -> Result<ServerStatus> {
    let address = ServerAddress::parse(address)?;
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut stream = TcpStream::connect((address.host.as_str(), address.port)).await?;
        stream.set_nodelay(true)?;
        let mut handshake = vec![0];
        varint(u32::MAX, &mut handshake);
        varint(address.host.len() as u32, &mut handshake);
        handshake.extend(address.host.as_bytes());
        handshake.extend(address.port.to_be_bytes());
        handshake.push(1);
        let mut request = Vec::new();
        varint(handshake.len() as u32, &mut request);
        request.extend(handshake);
        request.extend([1, 0]);
        let start = Instant::now();
        stream.write_all(&request).await?;
        let bytes = packet(&mut stream).await?;
        let mut reader = bytes.as_slice();
        ensure!(read_varint(&mut reader).await? == 0, "服务器状态响应无效");
        let size = read_varint(&mut reader).await?;
        ensure!(size == reader.len(), "服务器状态响应无效");
        let json: Value = serde_json::from_slice(reader)?;
        let mut text = String::new();
        motd(&json["description"], 0, &mut text);
        let mut latency = start.elapsed().as_millis() as u64;
        let token = storage::now().to_be_bytes();
        let mut ping = vec![9, 1];
        ping.extend(token);
        let start = Instant::now();
        // A missing pong must not turn a valid status response into "offline".
        if let Ok(Ok(bytes)) = tokio::time::timeout(Duration::from_millis(800), async {
            stream.write_all(&ping).await?;
            packet(&mut stream).await
        })
        .await
            && bytes.len() == 9
            && bytes[0] == 1
            && bytes[1..] == token
        {
            latency = start.elapsed().as_millis() as u64;
        }
        Ok(ServerStatus {
            online: json["players"]["online"].as_u64().unwrap_or(0),
            max: json["players"]["max"].as_u64().unwrap_or(0),
            latency,
            motd: plain(&text, 512),
            version: plain(json["version"]["name"].as_str().unwrap_or(""), 80),
            icon: json["favicon"].as_str().and_then(server_icon),
        })
    })
    .await
    .context("服务器响应超时")?
}

pub fn apply_server_launch(
    instance: &mut crate::model::Instance,
    version: &crate::model::Version,
    server: &str,
) -> Result<()> {
    let server = ServerAddress::parse(server)?;
    let modern = version.arguments.as_ref().is_some_and(|args| {
        args.game.iter().any(|arg| match arg {
            crate::model::Argument::Plain(value) => value == "--quickPlayMultiplayer",
            crate::model::Argument::Conditional { value, .. } => {
                value.as_str() == Some("--quickPlayMultiplayer")
                    || value.as_array().is_some_and(|items| {
                        items
                            .iter()
                            .any(|v| v.as_str() == Some("--quickPlayMultiplayer"))
                    })
            }
        })
    });
    // This is an in-memory clone for this launch; persisted JVM/game options stay intact.
    let args = &mut instance.launch.game_args;
    let mut index = 0;
    while index < args.len() {
        if [
            "--server",
            "--port",
            "--quickPlayMultiplayer",
            "--quickPlaySingleplayer",
            "--quickPlayRealms",
        ]
        .iter()
        .any(|flag| args[index] == *flag || args[index].starts_with(&format!("{flag}=")))
        {
            let has_value = !args.remove(index).contains('=');
            if has_value && index < args.len() && !args[index].starts_with("--") {
                args.remove(index);
            }
        } else {
            index += 1;
        }
    }
    if modern {
        args.extend(["--quickPlayMultiplayer".into(), server.display()]);
    } else {
        args.extend([
            "--server".into(),
            server.host,
            "--port".into(),
            server.port.to_string(),
        ]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn addresses_keep_ports_and_reject_non_addresses() {
        for (input, expected) in [
            ("example.org", "example.org"),
            ("EXAMPLE.org:25565", "example.org"),
            ("minecraft://localhost:80", "localhost:80"),
            ("[::1]:25566", "[::1]:25566"),
            ("::1", "[::1]"),
        ] {
            assert_eq!(ServerAddress::parse(input).unwrap().display(), expected);
        }
        for input in [
            "",
            "https://example.org",
            "user@localhost",
            "localhost/",
            "localhost:",
            "localhost:0",
            "localhost:65536",
            "a\nb",
            "a?b",
        ] {
            assert!(ServerAddress::parse(input).is_err(), "accepted {input}");
        }
    }
    #[tokio::test]
    async fn reads_status_and_ping_without_authentication_and_bounds_packets() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let handshake = packet(&mut socket).await.unwrap();
            let mut reader = handshake.as_slice();
            assert_eq!(read_varint(&mut reader).await.unwrap(), 0);
            assert_eq!(read_varint(&mut reader).await.unwrap(), u32::MAX as usize);
            let length = read_varint(&mut reader).await.unwrap();
            assert_eq!(&reader[..length], b"127.0.0.1");
            assert_eq!(*reader.last().unwrap(), 1);
            assert_eq!(packet(&mut socket).await.unwrap(), [0]);
            let json = json!({"version":{"name":"1.21.1"},"players":{"online":7,"max":20},"description":{"text":"§aHello ","extra":[{"text":"world"}]},"favicon":"data:image/svg+xml,<svg/>"}).to_string();
            let mut response = vec![0];
            varint(json.len() as u32, &mut response);
            response.extend(json.as_bytes());
            let mut bytes = Vec::new();
            varint(response.len() as u32, &mut bytes);
            bytes.extend(response);
            socket.write_all(&bytes).await.unwrap();
            let ping = packet(&mut socket).await.unwrap();
            assert_eq!(ping.len(), 9);
            assert_eq!(ping[0], 1);
            socket
                .write_all(&[&[9][..], ping.as_slice()].concat())
                .await
                .unwrap();
        });
        let status = server_status(&address).await.unwrap();
        assert_eq!((status.online, status.max), (7, 20));
        assert_eq!(status.motd, "Hello world");
        assert_eq!(status.version, "1.21.1");
        assert!(status.icon.is_none());
        server.await.unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            packet(&mut socket).await.unwrap();
            let mut bytes = Vec::new();
            varint(1024 * 1024, &mut bytes);
            socket.write_all(&bytes).await.unwrap();
        });
        assert!(
            server_status(&address)
                .await
                .unwrap_err()
                .to_string()
                .contains("过大")
        );
        server.await.unwrap();
    }
    #[tokio::test]
    async fn local_discovery_deduplicates_shared_worlds_and_never_mutates_game_data() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let instance = launcher.create_instance("Shared", "1.21.1").unwrap();
        launcher.add_version(&instance.id, "1.20.1").unwrap();
        for id in ["1.21.1", "1.20.1"] {
            launcher.set_installed(&instance.id, id, true).unwrap();
        }
        launcher.select_version(&instance.id, "1.21.1").unwrap();
        let game = launcher.game_dir(&instance.id).unwrap();
        let servers = fastnbt::to_bytes(&json!({"servers":[{"name":"§aLocal","ip":"localhost:80"},{"name":"invalid","ip":"a/b"}]})).unwrap();
        fs::write(game.join("servers.dat"), &servers).unwrap();
        for (name, time) in [("older", 1000_i64), ("newer", 2000_i64)] {
            let path = game.join("saves").join(name);
            fs::create_dir_all(&path).unwrap();
            #[derive(Serialize)]
            struct Data {
                #[serde(rename = "LevelName")]
                name: String,
                #[serde(rename = "LastPlayed")]
                time: i64,
            }
            #[derive(Serialize)]
            struct Root {
                #[serde(rename = "Data")]
                data: Data,
            }
            fs::write(
                path.join("level.dat"),
                fastnbt::to_bytes(&Root {
                    data: Data {
                        name: format!("World {name}"),
                        time,
                    },
                })
                .unwrap(),
            )
            .unwrap();
        }
        let result = launcher.home_widget_local();
        assert_eq!(result.servers.len(), 1);
        assert_eq!(result.servers[0].address, "localhost:80");
        assert_eq!(result.worlds.len(), 2);
        assert_eq!(result.worlds[0].name, "World newer");
        assert_eq!(result.worlds[0].last_played, 2000);
        assert_eq!(result.worlds[0].version, "1.21.1");
        assert_eq!(fs::read(game.join("servers.dat")).unwrap(), servers);
        assert!(!game.join("saves/newer/session.lock").exists());
    }
    #[tokio::test]
    async fn server_launch_replaces_conflicts_only_in_the_ephemeral_clone() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().into())).unwrap();
        let mut instance = launcher.create_instance("Launch", "1.21.1").unwrap();
        instance.launch.game_args = vec![
            "--demo".into(),
            "--server=old".into(),
            "--port".into(),
            "123".into(),
            "--quickPlaySingleplayer".into(),
            "World".into(),
        ];
        let original = instance.clone();
        let mut version: crate::model::Version=serde_json::from_value(json!({"id":"1.21.1","type":"release","mainClass":"Main","downloads":{},"libraries":[],"assetIndex":{"id":"x","sha1":"","url":"","size":0},"arguments":{"jvm":[],"game":[{"rules":[],"value":["--quickPlayMultiplayer","${quickPlayMultiplayer}"]}]}})).unwrap();
        apply_server_launch(&mut instance, &version, "localhost:80").unwrap();
        assert_eq!(
            instance.launch.game_args,
            ["--demo", "--quickPlayMultiplayer", "localhost:80"]
        );
        assert!(
            launcher
                .instance(&instance.id)
                .unwrap()
                .launch
                .game_args
                .is_empty()
        );
        version.arguments = None;
        let mut instance = original;
        apply_server_launch(&mut instance, &version, "[::1]:25566").unwrap();
        assert_eq!(
            instance.launch.game_args,
            ["--demo", "--server", "::1", "--port", "25566"]
        );
    }
}
