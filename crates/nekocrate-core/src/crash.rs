//! Evidence-based, local crash hints. Never infer a culprit from an exit code alone.
use crate::{
    Launcher, diagnostics,
    model::{Event, EventSink},
    storage,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    time::SystemTime,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Finding {
    pub code: String,
    pub title: String,
    pub advice: String,
    pub evidence: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrashReport {
    pub version: String,
    pub time: u64,
    pub exit: String,
    pub sources: Vec<String>,
    pub findings: Vec<Finding>,
}
struct Rule {
    code: &'static str,
    title: &'static str,
    advice: &'static str,
    patterns: &'static [&'static str],
}
const RULES: &[Rule] = &[
    Rule {
        code: "module_conflict",
        title: "Java 模块冲突",
        advice: "检查启动类路径中是否重复包含游戏本体或依赖 JAR，并核对加载器的 ignoreList；错误中引用该包的 Mod 不一定是冲突来源。",
        patterns: &[
            "java.lang.module.resolutionexception",
            "java.lang.layerinstantiationexception",
        ],
    },
    Rule {
        code: "heap",
        title: "Java 堆内存不足",
        advice: "检查实例的最大内存；结合系统可用内存适当增加，或减少资源包与 Mod。",
        patterns: &[
            "outofmemoryerror: java heap space",
            "outofmemoryerror: gc overhead limit exceeded",
        ],
    },
    Rule {
        code: "native_memory",
        title: "系统或原生内存不足",
        advice: "关闭占用内存的程序；检查是否给 Java 分配了过多堆内存，为原生库和系统保留空间。",
        patterns: &[
            "native memory allocation (mmap) failed",
            "unable to create native thread",
            "there is insufficient memory for the java runtime",
        ],
    },
    Rule {
        code: "java",
        title: "Java 版本不兼容",
        advice: "检查报错要求的 Java 版本，并在实例设置中选择兼容版本。",
        patterns: &[
            "unsupportedclassversionerror",
            "has been compiled by a more recent version of the java runtime",
        ],
    },
    Rule {
        code: "bytecode",
        title: "加载器或字节码解析器不支持此 Java 字节码",
        advice: "检查加载器与 Mod 的兼容范围；仅更新 Java 不一定能解决此问题。",
        patterns: &["unsupported class file major version"],
    },
    Rule {
        code: "dependencies",
        title: "Mod 依赖缺失或版本冲突",
        advice: "根据日志中的 Mod 名称和版本范围补全前置，或换成兼容当前游戏与加载器的版本。",
        patterns: &[
            "missing or unsupported mandatory dependencies",
            "incompatible mod set!",
            "requires version",
            "which is missing!",
        ],
    },
    Rule {
        code: "duplicate",
        title: "存在重复的 Mod",
        advice: "在 Mods 页检查重复 Mod ID，保留适用的版本并禁用多余文件。",
        patterns: &[
            "duplicatemodsfoundexception",
            "duplicate mods found",
            "found a duplicate mod",
        ],
    },
    Rule {
        code: "config",
        title: "配置文件读取失败",
        advice: "查看日志指出的配置文件；修正格式或先备份再恢复默认配置。",
        patterns: &[
            "configloadingexception",
            "failed loading config file",
            "failed to load config file",
        ],
    },
    Rule {
        code: "mixin",
        title: "Mixin 注入失败",
        advice: "检查下方涉及的 Mod 与加载器版本，优先排查最近添加或更新的 Mod；注入失败本身不能确定唯一责任 Mod。",
        patterns: &[
            "mixinapplyerror",
            "mixin apply failed",
            "invalidmixinexception",
            "injectionerror",
        ],
    },
    Rule {
        code: "graphics",
        title: "图形上下文创建失败",
        advice: "检查显卡驱动和 OpenGL 支持；若使用远程桌面，尝试在本机启动。",
        patterns: &[
            "glfw error 65542",
            "glfw error 65543",
            "failed to create the opengl context",
            "pixel format not accelerated",
        ],
    },
    Rule {
        code: "native_crash",
        title: "JVM 或原生库异常退出",
        advice: "查看原生崩溃日志的 Problematic frame，核对 Java 架构、显卡驱动及原生库；不能仅凭退出码判断是哪个 Mod。",
        patterns: &[
            "exception_access_violation",
            "sigsegv",
            "a fatal error has been detected by the java runtime environment",
        ],
    },
];
pub fn analyze(text: &str) -> Vec<Finding> {
    let lines: Vec<_> = text.lines().collect();
    let mut findings: Vec<_> = RULES.iter().filter_map(|rule| {
        let evidence: Vec<_> = lines.iter().filter(|line| {
            let lower = line.to_ascii_lowercase();
            rule.patterns.iter().any(|pattern| lower.contains(pattern))
                // A version requirement printed during normal discovery is not a crash reason.
                && (!lower.contains("requires version") || lower.contains("missing") || lower.contains("incompatible"))
        }).take(3).map(|line| diagnostics::redact(line).chars().take(600).collect()).collect();
        (!evidence.is_empty()).then(|| Finding { code:rule.code.into(), title:rule.title.into(), advice:rule.advice.into(), evidence })
    }).take(5).collect();
    // Preserve the actual exception even when it has no dedicated rule. This is
    // evidence for investigation, not an invented attribution to a mod.
    if findings.is_empty() {
        let mut evidence: Vec<_> = lines
            .iter()
            .rev()
            .filter(|line| {
                let line = line.trim();
                line.starts_with("Caused by:")
                    || line.starts_with("Exception in thread ")
                    || (line.starts_with("java.")
                        && (line.contains("Exception:") || line.contains("Error:")))
            })
            .take(3)
            .map(|line| diagnostics::redact(line).chars().take(600).collect())
            .collect();
        evidence.reverse();
        if !evidence.is_empty() {
            findings.push(Finding {
                code: "unclassified_exception".into(),
                title: "发现尚未分类的运行异常".into(),
                advice:
                    "以下为本次退出前的异常信息；尚不能确定具体原因，请结合异常调用栈继续排查。"
                        .into(),
                evidence,
            });
        }
    }
    findings
}

/// Diagnostics share the game log, so filtering, copying and saved logs all
/// contain the same evidence. A disk error must not hide it from the live UI.
pub(crate) fn write_to_log(
    report: &CrashReport,
    log: &Path,
    id: &str,
    sink: &EventSink,
) -> std::io::Result<()> {
    let line = |level: &str, message: &str| Event {
        instance_id: Some(id.into()),
        ..Event::new("diagnosis", format!("[{level}] [Diagnosis] {message}"))
    };
    let mut events = vec![line("ERROR", &report.exit)];
    if report.findings.is_empty() {
        events.push(line(
            "WARN",
            "未找到明确的已知原因，请结合完整日志继续排查。",
        ));
    }
    for finding in &report.findings {
        events.push(line("ERROR", &finding.title));
        events.push(line("WARN", &finding.advice));
        for evidence in &finding.evidence {
            events.push(Event {
                instance_id: Some(id.into()),
                // Evidence is game output; do not translate it as launcher UI.
                ..Event::new("log", format!("[ERROR] [Diagnosis] {evidence}"))
            });
        }
    }
    let result = (|| {
        let mut file = fs::OpenOptions::new().append(true).open(log)?;
        for event in &events {
            writeln!(file, "{}", event.message)?;
        }
        file.flush()
    })();
    for event in events {
        sink(event);
    }
    result
}
fn tail(path: &Path) -> std::io::Result<String> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    let mut file = fs::File::open(path)?;
    let length = file.metadata()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(1024 * 1024)))?;
    let mut bytes = Vec::new();
    file.take(1024 * 1024).read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
pub(crate) fn collect(
    working: &Path,
    log: &Path,
    started: SystemTime,
    pid: u32,
    version: String,
    exit: String,
    secrets: &[String],
) -> CrashReport {
    let mut paths = vec![
        log.to_owned(),
        working.join("logs/latest.log"),
        working.join(format!("hs_err_pid{pid}.log")),
    ];
    let reports = working.join("crash-reports");
    if !fs::symlink_metadata(&reports).is_ok_and(|m| m.file_type().is_symlink())
        && let Ok(entries) = fs::read_dir(reports)
    {
        let latest = entries
            .take(4096)
            .flatten()
            .filter_map(|entry| {
                let metadata = entry.metadata().ok()?;
                let modified = metadata.modified().ok()?;
                (metadata.is_file()
                    && modified >= started
                    && entry.path().extension().is_some_and(|e| e == "txt"))
                .then_some((modified, entry.path()))
            })
            .max_by_key(|(modified, _)| *modified);
        if let Some((_, path)) = latest {
            paths.push(path);
        }
    }
    let mut text = String::new();
    let mut sources = Vec::new();
    for path in paths {
        if path != log
            && !fs::metadata(&path)
                .and_then(|m| m.modified())
                .is_ok_and(|m| m >= started)
        {
            continue;
        }
        if let Ok(content) = tail(&path) {
            text.push_str(&content);
            text.push('\n');
            sources.push(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    for secret in secrets.iter().filter(|s| !s.is_empty()) {
        text = text.replace(secret, "[REDACTED]");
    }
    CrashReport {
        version,
        time: storage::now(),
        exit,
        sources,
        findings: analyze(&text),
    }
}
impl Launcher {
    pub fn crash_report(&self, id: &str) -> Result<Option<CrashReport>> {
        self.instance(id)?;
        let path = self.instance_dir(id)?.join("launcher-logs/last-crash.json");
        if !path.is_file() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&tail(&path)?)?))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn module_conflicts_and_unclassified_exceptions_keep_the_real_error() {
        let message = "Exception in thread \"main\" java.lang.module.ResolutionException: Modules _1._20._1 and minecraft export package net.minecraft.server to module sdm_economy";
        let findings = analyze(message);
        assert_eq!(findings[0].code, "module_conflict");
        assert_eq!(findings[0].evidence, [message]);
        let unknown = analyze(
            "Exception in thread \"main\" java.lang.IllegalStateException: fixture\nCaused by: sample.UnfamiliarException: reason",
        );
        assert_eq!(unknown[0].code, "unclassified_exception");
        assert_eq!(unknown[0].evidence.len(), 2);
    }
    #[test]
    fn reads_fresh_game_log_and_appends_diagnosis_to_saved_and_live_logs() {
        let root = tempfile::tempdir().unwrap();
        let log = root.path().join("game.log");
        fs::write(&log, "game output\n").unwrap();
        let start = SystemTime::now();
        fs::create_dir(root.path().join("logs")).unwrap();
        fs::write(root.path().join("logs/latest.log"), "Caused by: java.lang.module.ResolutionException: Modules game and minecraft export package net.minecraft.server password=secret").unwrap();
        let report = collect(
            root.path(),
            &log,
            start,
            1,
            "Pack".into(),
            "Exit 1".into(),
            &[],
        );
        assert_eq!(report.findings[0].code, "module_conflict");
        assert!(report.sources.contains(&"latest.log".to_owned()));
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let saved = events.clone();
        let sink: EventSink = std::sync::Arc::new(move |event| saved.lock().unwrap().push(event));
        write_to_log(&report, &log, "instance", &sink).unwrap();
        let text = fs::read_to_string(&log).unwrap();
        assert!(text.starts_with("game output\n"));
        assert!(text.contains("[ERROR] [Diagnosis] Java 模块冲突"));
        assert!(!text.contains("secret"));
        let events = events.lock().unwrap();
        assert!(
            events
                .iter()
                .all(|event| event.instance_id.as_deref() == Some("instance")
                    && text.contains(&event.message))
        );
    }
    #[test]
    fn distinguishes_heap_and_native_memory_and_keeps_evidence() {
        let heap = analyze("Caused by: java.lang.OutOfMemoryError: Java heap space");
        assert_eq!(heap[0].code, "heap");
        assert_eq!(heap[0].evidence.len(), 1);
        let native = analyze("Native memory allocation (mmap) failed to map 100 bytes");
        assert_eq!(native[0].code, "native_memory");
        assert!(analyze("Process exited with code -1\nA mod requires version 1.20.1").is_empty());
        assert_eq!(
            analyze("Unsupported class file major version 70")[0].code,
            "bytecode"
        );
    }
    #[test]
    fn diagnostics_redact_tokens_and_ignore_old_reports() {
        let root = tempfile::tempdir().unwrap();
        let log = root.path().join("game.log");
        fs::write(&log, "Mixin apply failed Bearer abc.def password=secret").unwrap();
        fs::create_dir(root.path().join("crash-reports")).unwrap();
        fs::write(
            root.path().join("crash-reports/old.txt"),
            "OutOfMemoryError: Java heap space",
        )
        .unwrap();
        let report = collect(
            root.path(),
            &log,
            SystemTime::now() + std::time::Duration::from_secs(1),
            1,
            "test".into(),
            "-1".into(),
            &[],
        );
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].code, "mixin");
        let encoded = serde_json::to_string(&report).unwrap();
        assert!(!encoded.contains("abc.def"));
        assert!(!encoded.contains("secret"));
    }
}
