//! Local, deterministic protocol fixtures. No public endpoint or real credential is used.
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

pub async fn serve(replies: Vec<(u16, String)>) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    serve_with_headers(
        replies
            .into_iter()
            .map(|(status, body)| (status, String::new(), body))
            .collect(),
    )
    .await
}

pub async fn serve_with_headers(
    replies: Vec<(u16, String, String)>,
) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut requests = Vec::new();
        for (status, headers, body) in replies {
            let (mut stream, _) =
                tokio::time::timeout(std::time::Duration::from_secs(5), listener.accept())
                    .await
                    .unwrap()
                    .unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut chunk = [0u8; 4096];
                let count = stream.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(end) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|s| s.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            requests.push(String::from_utf8(bytes).unwrap());
            let response = format!(
                "HTTP/1.1 {status} Test\r\n{headers}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
        requests
    });
    (address, task)
}

pub fn version_fixture(id: &str) -> crate::model::Version {
    serde_json::from_value(serde_json::json!({
        "id": id, "type": "release", "mainClass": "net.minecraft.client.main.Main",
        "downloads": {}, "libraries": [], "javaVersion": {"majorVersion": 8},
        "assetIndex": {"id": id, "url": "http://127.0.0.1/fixture", "sha1": "", "size": 0},
        "arguments": {"jvm": ["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"],
            "game": ["--gameDir", "${game_directory}", "--version", "${version_name}",
                "--username", "${auth_player_name}", "--uuid", "${auth_uuid}",
                "--accessToken", "${auth_access_token}", "--userType", "${user_type}",
                "--clientId", "${clientid}", "--xuid", "${auth_xuid}"]}
    }))
    .unwrap()
}
