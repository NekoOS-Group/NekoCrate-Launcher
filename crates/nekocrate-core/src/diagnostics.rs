//! Persistent launcher diagnostics, separate from Minecraft stdout/stderr.
use std::{
    fs,
    io::Write,
    path::Path,
    sync::{Mutex, OnceLock},
    time::Instant,
};

const MAX_BYTES: u64 = 1024 * 1024;
static WRITE_LOCK: Mutex<()> = Mutex::new(());
static START: OnceLock<Instant> = OnceLock::new();

/// Keep host/path context, strip credentials and signed queries before truncation.
pub fn redact(message: &str) -> String {
    static URLS: OnceLock<regex::Regex> = OnceLock::new();
    static FIELDS: OnceLock<regex::Regex> = OnceLock::new();
    static AUTH: OnceLock<regex::Regex> = OnceLock::new();
    let urls =
        URLS.get_or_init(|| regex::Regex::new(r#"(?i)(?:https?|socks5h?)://[^\s<>"')]+"#).unwrap());
    let value = urls.replace_all(message, |captures: &regex::Captures<'_>| {
        let Ok(mut url) = reqwest::Url::parse(&captures[0]) else {
            return "[REDACTED URL]".to_owned();
        };
        let _ = url.set_username("");
        let _ = url.set_password(None);
        url.set_query(None);
        url.set_fragment(None);
        url.to_string()
    });
    let fields = FIELDS.get_or_init(|| regex::Regex::new(
        r#"(?i)(["']?\b(?:access_token|refresh_token|id_token|client_secret|password|proxy_password|api[_-]?key|x-api-key|device_code|user_code)["']?\s*[:=]\s*)(?:"[^"\r\n]*"|'[^'\r\n]*'|[^\s,;}]+)"#,
    ).unwrap());
    let value = fields.replace_all(&value, "${1}[REDACTED]");
    let auth = AUTH
        .get_or_init(|| regex::Regex::new(r"(?i)\b(Bearer|Basic)\s+[A-Za-z0-9._~+/=-]+").unwrap());
    auth.replace_all(&value, "${1} [REDACTED]").into_owned()
}

pub fn record_error(root: &Path, operation: &str, error: &anyhow::Error) -> std::io::Result<()> {
    record(root, "ERROR", operation, "error", &format!("{error:#}"))
}

pub fn record_startup(root: &Path, phase: &str) -> std::io::Result<()> {
    record(root, "INFO", "startup", "message", phase)
}

fn record(
    root: &Path,
    level: &str,
    operation: &str,
    field: &str,
    message: &str,
) -> std::io::Result<()> {
    let started = *START.get_or_init(Instant::now);
    let local_time = chrono::Local::now();
    let detail: String = redact(message).chars().take(16384).collect();
    let record = serde_json::json!({
        "level": level, "pid": std::process::id(), "local_time": local_time.to_rfc3339(),
        "system_unix_ms": local_time.timestamp_millis(), "elapsed_ms": started.elapsed().as_millis(),
        "version": env!("CARGO_PKG_VERSION"), "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
        "operation": operation, (field): detail,
    });
    let mut bytes = serde_json::to_vec(&record)?;
    bytes.push(b'\n');
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let directory = root.join("logs");
    fs::create_dir_all(&directory)?;
    let path = directory.join("launcher.log");
    if fs::metadata(&path).is_ok_and(|m| m.len() + bytes.len() as u64 > MAX_BYTES) {
        // Keep three previous files, independent of the (possibly wrong) date.
        for number in (1..=3).rev() {
            let destination = directory.join(format!("launcher.{number}.log"));
            if destination.exists() {
                fs::remove_file(&destination)?;
            }
            let source = if number == 1 {
                path.clone()
            } else {
                directory.join(format!("launcher.{}.log", number - 1))
            };
            if source.exists() {
                fs::rename(source, destination)?;
            }
        }
    }
    let mut options = fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_phases_are_recorded_before_the_database_is_opened() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("not-created-yet");
        record_startup(&root, "process_started").unwrap();
        record_startup(&root, "initializing_tauri").unwrap();
        let records: Vec<serde_json::Value> = fs::read_to_string(root.join("logs/launcher.log"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["level"], "INFO");
        assert_eq!(records[0]["operation"], "startup");
        assert_eq!(records[0]["pid"], std::process::id());
        assert_eq!(records[0]["message"], "process_started");
        assert_eq!(records[1]["message"], "initializing_tauri");
        assert!(records[0].get("error").is_none());
        assert_eq!(
            fs::read_dir(root).unwrap().count(),
            1,
            "only logs are created"
        );
    }

    #[test]
    fn removes_url_credentials_signed_queries_and_authentication_secrets() {
        let value = redact(
            r#"failed https://name:password@host.test/game.jar?X-Amz-Signature=secret#token Bearer jwt.secret.value {"access_token":"access-secret","refresh_token":"refresh-secret", "password": "with spaces"} x-api-key=key-secret device_code=device-secret"#,
        );
        assert!(value.contains("https://host.test/game.jar"));
        for secret in [
            "name:",
            "jwt.secret",
            "access-secret",
            "refresh-secret",
            "with spaces",
            "key-secret",
            "device-secret",
            "X-Amz",
            "#token",
        ] {
            assert!(!value.contains(secret), "{value}");
        }
    }
    #[test]
    fn errors_survive_restart_and_rotation_is_bounded_without_using_wall_clock() {
        let dir = tempfile::tempdir().unwrap();
        let error = anyhow::anyhow!("certificate expired").context("update failed");
        record_error(dir.path(), "check_update", &error).unwrap();
        let path = dir.path().join("logs/launcher.log");
        let record: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(record["error"], "update failed: certificate expired");
        assert_eq!(record["operation"], "check_update");
        assert!(
            chrono::DateTime::parse_from_rfc3339(record["local_time"].as_str().unwrap()).is_ok()
        );
        for _ in 0..5 {
            fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_len(MAX_BYTES)
                .unwrap();
            record_error(dir.path(), "install", &error).unwrap();
        }
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 4);
        assert!(fs::metadata(path).unwrap().len() < MAX_BYTES);
    }
}
