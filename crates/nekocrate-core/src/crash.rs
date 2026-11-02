//! Evidence-based, local crash hints. Never infer a culprit from an exit code alone.
use crate::{
    Launcher, diagnostics,
    model::{Event, EventSink},
    storage,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
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
    #[serde(default)]
    pub file: Option<ReportFile>,
    #[serde(default)]
    pub repairable: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReportFile {
    /// Relative to the effective game directory, or the launcher's log directory.
    pub path: String,
    pub launcher_log: bool,
}
struct Rule {
    code: &'static str,
    title: &'static str,
    advice: &'static str,
    patterns: &'static [&'static str],
}
const RULES: &[Rule] = &[
    Rule {
        code: "launch_java",
        title: "Java 无法用于本次启动",
        advice: "选择兼容的 Java，或改为自动匹配后重试。",
        patterns: &[
            "所选 java 不兼容",
            "无法执行 java",
            "java 无法运行",
            "java 检测超时",
            "无法识别 java 版本",
            "无法启动 java 进程",
            "未找到兼容的 java",
            "未找到兼容的 jdk",
            "请手动指定兼容版本",
            "java 未报告运行时架构",
        ],
    },
    Rule {
        code: "files",
        title: "启动文件缺失或损坏",
        advice: "校验并修复游戏文件；外部安装的加载器若无法修复，请使用原安装方式补全文件。",
        patterns: &[
            "启动所需文件缺失",
            "缺少或无法读取版本配置",
            "invalid or corrupt jarfile",
        ],
    },
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
    let mut file = None;
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
            // Later entries are more specific: launcher output, latest.log,
            // JVM fatal error, then the game's crash report.
            if let Some(relative) = if path == log {
                path.file_name().map(Path::new)
            } else {
                path.strip_prefix(working).ok()
            } {
                file = Some(ReportFile {
                    path: relative.to_string_lossy().into_owned(),
                    launcher_log: path == log,
                });
            }
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
        file,
        repairable: false,
    }
}
impl Launcher {
    pub(crate) fn can_repair_version(&self, id: &str, version: &str) -> bool {
        let Ok(instance) = self.instance(id) else {
            return false;
        };
        let Some(selected) = instance.versions.iter().find(|v| v.id == version) else {
            return false;
        };
        let Ok(game) = self.available_game_dir(id) else {
            return false;
        };
        selected.modpack
            || crate::loader::has_record(&game, version)
            || selected.loader.as_ref().is_some_and(|l| l.kind == "fabric")
            || crate::minecraft::check_local_install(&game, version).is_ok()
    }

    pub(crate) fn report_launch_failure(
        &self,
        id: &str,
        error: &str,
        sink: &EventSink,
    ) -> Result<()> {
        let instance = self.instance(id)?;
        let version = instance.selected_version.clone().unwrap_or_default();
        let directory = self.instance_dir(id)?.join("launcher-logs");
        fs::create_dir_all(&directory)?;
        let name = format!("{}-{}.log", storage::now(), uuid::Uuid::new_v4());
        let log = directory.join(&name);
        let message = diagnostics::redact(error);
        fs::write(&log, format!("[ERROR] {message}\n"))?;
        let report = CrashReport {
            repairable: self.can_repair_version(id, &version),
            version,
            time: storage::now(),
            exit: message.clone(),
            sources: vec![name.clone()],
            findings: analyze(&message),
            file: Some(ReportFile {
                path: name,
                launcher_log: true,
            }),
        };
        // A failed disk write must not prevent the live recovery controls.
        let saved = storage::write_json(&directory.join("last-crash.json"), &report);
        let logged = write_to_log(&report, &log, id, sink);
        sink(Event {
            instance_id: Some(id.into()),
            crash: Some(report),
            ..Event::new("launch_failed", message)
        });
        saved?;
        logged?;
        Ok(())
    }

    pub fn crash_report_path(&self, id: &str, version: &str, time: u64) -> Result<PathBuf> {
        let report = self.crash_report(id)?.context("未找到本次启动的日志文件")?;
        ensure!(
            report.version == version && report.time == time,
            "此日志已被新的启动记录替换，请重新打开日志"
        );
        let file = report.file.context("未找到本次启动的日志文件")?;
        let base = if file.launcher_log {
            self.instance_dir(id)?.join("launcher-logs")
        } else {
            let instance = self.instance(id)?;
            let game = self.available_game_dir(id)?;
            crate::version::instance_working_directory(&game, &instance, version)?
        };
        report_path(&base, &file.path)
    }

    pub fn crash_report(&self, id: &str) -> Result<Option<CrashReport>> {
        self.instance(id)?;
        let path = self.instance_dir(id)?.join("launcher-logs/last-crash.json");
        if !path.is_file() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&tail(&path)?)?))
    }
}

fn report_path(base: &Path, relative: &str) -> Result<PathBuf> {
    let relative = Path::new(relative);
    ensure!(
        !relative.as_os_str().is_empty()
            && relative
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
            && relative
                .extension()
                .is_some_and(|e| e == "log" || e == "txt"),
        "无效的日志文件路径"
    );
    let base = base.canonicalize()?;
    let path = base.join(relative).canonicalize()?;
    ensure!(
        path.starts_with(&base) && path.is_file(),
        "日志文件不在此实例目录内"
    );
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn failed_launch_has_scoped_recovery_and_saved_log_but_cancellation_does_not() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let instance = launcher.create_instance("Broken", "1.21.1").unwrap();
        let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let events = captured.clone();
        let sink: EventSink = std::sync::Arc::new(move |e| events.lock().unwrap().push(e));
        // No version JSON: this must fail before Java or network work begins.
        launcher
            .launch(
                &instance.id,
                &tokio_util::sync::CancellationToken::new(),
                sink.clone(),
            )
            .await
            .unwrap_err();
        let report = launcher.crash_report(&instance.id).unwrap().unwrap();
        assert_eq!(report.version, "1.21.1");
        assert_eq!(report.findings[0].code, "files");
        assert!(report.repairable);
        let path = launcher
            .crash_report_path(&instance.id, "1.21.1", report.time)
            .unwrap();
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains("缺少或无法读取版本配置")
        );
        assert!(
            launcher
                .crash_report_path(&instance.id, "another-version", report.time)
                .is_err()
        );
        let event = captured.lock().unwrap().last().unwrap().clone();
        assert_eq!(event.kind, "launch_failed");
        assert_eq!(event.instance_id.as_deref(), Some(instance.id.as_str()));
        assert!(event.crash.is_some());
        captured.lock().unwrap().clear();
        let cancel = tokio_util::sync::CancellationToken::new();
        cancel.cancel();
        launcher
            .launch(&instance.id, &cancel, sink)
            .await
            .unwrap_err();
        assert!(
            !captured
                .lock()
                .unwrap()
                .iter()
                .any(|e| e.kind == "launch_failed")
        );
        assert!(launcher.crash_report(&instance.id).unwrap().is_none());
    }

    #[test]
    fn report_files_cannot_escape_their_game_directory() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("game");
        fs::create_dir_all(base.join("crash-reports")).unwrap();
        fs::write(base.join("crash-reports/latest.txt"), "report").unwrap();
        fs::write(temp.path().join("outside.log"), "other").unwrap();
        assert!(report_path(&base, "crash-reports/latest.txt").is_ok());
        for path in [
            "../outside.log",
            "/outside.log",
            "crash-reports",
            "game.exe",
        ] {
            assert!(report_path(&base, path).is_err(), "{path}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(temp.path().join("outside.log"), base.join("link.log"))
                .unwrap();
            assert!(report_path(&base, "link.log").is_err());
        }
    }

    #[tokio::test]
    async fn recovery_repairs_the_requested_version_without_changing_selection() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = Launcher::open(Some(temp.path().join("launcher"))).unwrap();
        let instance = launcher.create_instance("Game", "1.21.1").unwrap();
        launcher.add_version(&instance.id, "RLCraft").unwrap();
        launcher.select_version(&instance.id, "1.21.1").unwrap();
        let game = launcher.game_dir(&instance.id).unwrap();
        storage::write_json(
            &game.join("versions/RLCraft/RLCraft.json"),
            &serde_json::json!({
                "id": "RLCraft", "mainClass": "net.minecraft.launchwrapper.Launch"
            }),
        )
        .unwrap();
        let sink: EventSink = std::sync::Arc::new(|_| {});
        let error = launcher
            .repair_version(
                &instance.id,
                "RLCraft",
                &tokio_util::sync::CancellationToken::new(),
                &sink,
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("自定义加载器"), "{error:#}");
        assert_eq!(
            launcher
                .instance(&instance.id)
                .unwrap()
                .selected_version
                .as_deref(),
            Some("1.21.1")
        );
        assert!(!launcher.can_repair_version(&instance.id, "RLCraft"));
        assert!(
            launcher
                .repair_version(
                    &instance.id,
                    "removed",
                    &tokio_util::sync::CancellationToken::new(),
                    &sink
                )
                .await
                .unwrap_err()
                .to_string()
                .contains("已被移除")
        );
    }

    #[test]
    fn java_preflight_errors_have_actionable_evidence_without_misclassifying_network_errors() {
        for message in [
            "所选 Java 不兼容：需要 Java 17",
            "无法执行 Java: No such file",
            "未找到兼容的 JDK 17，请指定 Java 路径或允许自动下载。",
        ] {
            assert_eq!(analyze(message)[0].code, "launch_java");
        }
        assert!(analyze("下载 Java 失败: network timeout").is_empty());
    }
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
        let start = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        fs::create_dir(root.path().join("logs")).unwrap();
        let latest = root.path().join("logs/latest.log");
        fs::write(&latest, "Caused by: java.lang.module.ResolutionException: Modules game and minecraft export package net.minecraft.server password=secret").unwrap();
        // Filesystem mtimes can lag the wall clock by a kernel tick. Set the
        // fixture's timestamp explicitly instead of relying on write timing.
        fs::File::open(&latest)
            .unwrap()
            .set_modified(start + std::time::Duration::from_secs(1))
            .unwrap();
        let report = collect(
            root.path(),
            &log,
            start,
            1,
            "Pack".into(),
            "Exit 1".into(),
            &[],
        );
        assert!(report.sources.contains(&"latest.log".to_owned()));
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].code, "module_conflict");
        assert_eq!(report.file.as_ref().unwrap().path, "logs/latest.log");
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
