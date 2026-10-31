use anyhow::{Context, Result};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

static ACTIVE: AtomicBool = AtomicBool::new(false);
struct DragGuard;
impl Drop for DragGuard {
    fn drop(&mut self) {
        ACTIVE.store(false, Ordering::Release);
    }
}

/// Hand the existing file to the OS. Copy-only keeps the original game intact.
pub async fn start(window: tauri::WebviewWindow, paths: Vec<PathBuf>) -> Result<bool> {
    anyhow::ensure!(!ACTIVE.swap(true, Ordering::AcqRel), "已有文件正在拖动");
    let guard = DragGuard;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let sender = Arc::new(Mutex::new(Some(sender)));
    let failure = sender.clone();
    let native = window.clone();
    window.run_on_main_thread(move || {
        #[cfg(target_os = "linux")]
        let result = linux::start(&native, paths, sender);
        #[cfg(not(target_os = "linux"))]
        let result = drag::start_drag(
            &native,
            drag::DragItem::Files(paths),
            drag::Image::Raw(include_bytes!("../icons/drag.png").to_vec()),
            move |result, _| {
                if let Some(sender) = sender.lock().unwrap().take() {
                    let _ = sender.send(Ok(matches!(result, drag::DragResult::Dropped)));
                }
            },
            drag::Options {
                mode: drag::DragMode::Copy,
                skip_animatation_on_cancel_or_failure: true,
            },
        );
        if let Err(error) = result
            && let Some(sender) = failure.lock().unwrap().take()
        {
            let _ = sender.send(Err(anyhow::anyhow!(error.to_string())));
        }
    })?;
    let result = receiver.await.context("系统文件拖放已结束")?;
    drop(guard);
    result
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use gtk::{gdk, gio, glib, prelude::*};
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    type Completion = Arc<Mutex<Option<tokio::sync::oneshot::Sender<Result<bool>>>>>;

    pub fn start(
        window: &tauri::WebviewWindow,
        paths: Vec<PathBuf>,
        sender: Completion,
    ) -> Result<()> {
        // GTK objects stay on the main thread. GIO escapes spaces, #, % and
        // non-ASCII names in text/uri-list; raw file:// paths are not valid URIs.
        let window = window.gtk_window()?;
        let uris: Vec<_> = paths
            .into_iter()
            .map(|path| gio::File::for_path(path).uri())
            .collect();
        window.drag_source_set(gdk::ModifierType::BUTTON1_MASK, &[], gdk::DragAction::COPY);
        window.drag_source_add_uri_targets();
        let handlers = Rc::new(RefCell::new(Vec::new()));
        handlers
            .borrow_mut()
            .push(window.connect_drag_data_get(move |_, _, selection, _, _| {
                selection.set_uris(&uris.iter().map(|uri| uri.as_str()).collect::<Vec<_>>());
            }));
        let failed = Rc::new(Cell::new(false));
        let failure = failed.clone();
        handlers
            .borrow_mut()
            .push(window.connect_drag_failed(move |_, _, _| {
                failure.set(true);
                glib::Propagation::Stop
            }));
        let cleanup = handlers.clone();
        handlers
            .borrow_mut()
            .push(window.connect_drag_end(move |window, context| {
                if let Some(sender) = sender.lock().unwrap().take() {
                    let _ = sender.send(Ok(!failed.get() && !context.selected_action().is_empty()));
                }
                for handler in cleanup.borrow_mut().drain(..) {
                    window.disconnect(handler);
                }
                window.drag_source_unset();
            }));
        let context = window.drag_source_get_target_list().and_then(|targets| {
            // GTK expects button number 1, not the BUTTON1_MASK bitfield.
            window.drag_begin_with_coordinates(&targets, gdk::DragAction::COPY, 1, None, -1, -1)
        });
        if let Some(context) = context {
            let loader = gtk::gdk_pixbuf::PixbufLoader::new();
            if loader.write(include_bytes!("../icons/drag.png")).is_ok()
                && loader.close().is_ok()
                && let Some(icon) = loader.pixbuf()
            {
                context.drag_set_icon_pixbuf(&icon, 0, 0);
            }
            Ok(())
        } else {
            for handler in handlers.borrow_mut().drain(..) {
                window.disconnect(handler);
            }
            window.drag_source_unset();
            anyhow::bail!("系统无法开始文件拖放")
        }
    }
}
