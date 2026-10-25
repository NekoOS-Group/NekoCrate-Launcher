//! Install diagnostics before Tauri, plugins or the database are initialized.
use nekocrate_core::{diagnostics, storage};
use std::{path::PathBuf, sync::Once};

fn roots() -> Vec<PathBuf> {
    let mut roots: Vec<_> = storage::data_root(None).into_iter().collect();
    roots.push(std::env::temp_dir().join("NekoCrate-diagnostics"));
    roots
}

pub fn phase(phase: &str) {
    for root in roots() {
        if diagnostics::record_startup(&root, phase).is_ok() {
            return;
        }
    }
}

pub fn failure(operation: &str, error: &anyhow::Error) {
    for root in roots() {
        if diagnostics::record_error(&root, operation, error).is_ok() {
            return;
        }
    }
    eprintln!("{}", diagnostics::redact(&format!("{error:#}")));
}

pub fn install() {
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| {
        let original = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Ordinary Rust panics get a backtrace. Native stack overflows may
            // bypass this hook; the last startup phase and Windows WER remain.
            failure(
                "panic",
                &anyhow::anyhow!("{info}\n{}", std::backtrace::Backtrace::force_capture()),
            );
            original(info);
        }));
    });
    phase("process_started");
}
