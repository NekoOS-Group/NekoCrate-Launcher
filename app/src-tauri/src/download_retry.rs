use nekocrate_core::{
    download::{DownloadFailures, RetryFile},
    model::Event,
};
use std::{collections::HashMap, sync::Mutex};

#[derive(Clone)]
pub struct Entry {
    pub file: RetryFile,
    pub instance: Option<String>,
    pub step: Option<String>,
}
#[derive(Default)]
pub struct Retries(Mutex<HashMap<String, Entry>>);
type FileScope = (Option<String>, Option<String>);
#[derive(Default)]
pub struct Scopes(Mutex<HashMap<String, FileScope>>);
impl Scopes {
    pub fn observe(&self, event: &Event) {
        for file in event
            .files
            .iter()
            .filter(|f| f.status == "error" && f.retryable)
        {
            self.0.lock().unwrap().insert(
                file.id.clone(),
                (event.instance_id.clone(), event.step.clone()),
            );
        }
    }
}
impl Retries {
    pub fn capture(&self, error: &anyhow::Error, scopes: &Scopes) {
        if let Some(failure) = error.downcast_ref::<DownloadFailures>() {
            let mut entries = self.0.lock().unwrap();
            let scopes = scopes.0.lock().unwrap();
            if entries.len() + failure.files.len() > 8192 {
                entries.clear();
            }
            for file in &failure.files {
                let (instance, step) = scopes.get(&file.id()).cloned().unwrap_or_default();
                entries.insert(
                    file.id(),
                    Entry {
                        file: file.clone(),
                        instance,
                        step,
                    },
                );
            }
        }
    }
    pub fn get(&self, id: &str) -> anyhow::Result<Entry> {
        self.0
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("此失败文件已过期，请继续原安装任务"))
    }
    pub fn invalidate_instance(&self, id: &str) {
        self.0
            .lock()
            .unwrap()
            .retain(|_, entry| entry.instance.as_deref() != Some(id));
    }
    pub fn completed(&self, id: &str) {
        self.0.lock().unwrap().remove(id);
    }
}
