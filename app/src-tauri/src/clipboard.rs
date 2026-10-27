//! Text clipboard with application lifetime (required by X11 and Wayland).
use std::sync::Mutex;
use tauri::{Manager, Runtime};

struct TextClipboard(Mutex<Result<Option<arboard::Clipboard>, arboard::Error>>);

pub fn write_text<R: Runtime>(app: &tauri::AppHandle<R>, text: String) -> anyhow::Result<()> {
    let state = app.state::<TextClipboard>();
    let mut clipboard = state.0.lock().unwrap_or_else(|error| error.into_inner());
    match &mut *clipboard {
        Ok(Some(clipboard)) => Ok(clipboard.set_text(text)?),
        Ok(None) => anyhow::bail!("Clipboard is closed"),
        Err(error) => anyhow::bail!("Clipboard unavailable: {error}"),
    }
}

pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("text-clipboard")
        .setup(|app, _| {
            // A missing clipboard service must not prevent launcher startup.
            app.manage(TextClipboard(Mutex::new(
                arboard::Clipboard::new().map(Some),
            )));
            Ok(())
        })
        .on_event(|app, event| {
            if let tauri::RunEvent::Exit = event {
                let state = app.state::<TextClipboard>();
                let mut clipboard = state.0.lock().unwrap_or_else(|error| error.into_inner());
                if let Ok(clipboard) = &mut *clipboard {
                    // Managed state is not dropped on process exit. Release the
                    // clipboard owner and its worker before the event loop exits.
                    clipboard.take();
                }
            }
        })
        .build()
}
