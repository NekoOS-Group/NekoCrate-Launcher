use super::{JavaInfo, discovery};
use crate::{java_compatibility::Compatibility, model::Version, storage};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub(crate) struct LaunchContext {
    key: String,
    profile: String,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct SuccessfulLaunch {
    schema: u32,
    profile: String,
    java_path: PathBuf,
    runtime: String,
}

impl LaunchContext {
    pub(super) fn new(version: &Version, working: &Path, compatibility: &Compatibility) -> Self {
        let mut key = ring::digest::Context::new(&ring::digest::SHA256);
        key.update(
            working
                .canonicalize()
                .unwrap_or_else(|_| working.to_owned())
                .as_os_str()
                .as_encoded_bytes(),
        );
        key.update(b"\0");
        key.update(version.id.as_bytes());
        Self {
            key: hex::encode(key.finish()),
            profile: compatibility.fingerprint.clone(),
        }
    }

    fn file(&self, root: &Path) -> PathBuf {
        root.join("metadata/java-launches")
            .join(format!("{}.json", self.key))
    }

    pub(super) fn previous(&self, root: &Path) -> Option<PathBuf> {
        let receipt: SuccessfulLaunch = crate::portable::read_json(root, &self.file(root)).ok()?;
        (receipt.schema == 1
            && receipt.profile == self.profile
            && discovery::fingerprint(&receipt.java_path).as_ref() == Some(&receipt.runtime))
        .then_some(receipt.java_path)
    }

    pub(crate) fn receipt(&self, java: &JavaInfo) -> Option<SuccessfulLaunch> {
        Some(SuccessfulLaunch {
            schema: 1,
            profile: self.profile.clone(),
            java_path: PathBuf::from(&java.path),
            runtime: discovery::fingerprint(Path::new(&java.path))?,
        })
    }

    pub(crate) fn save(&self, root: &Path, receipt: &SuccessfulLaunch) -> anyhow::Result<()> {
        // Written only after a normal game exit; spawning an installer or a JVM
        // which immediately crashes is not evidence of a successful game launch.
        storage::atomic_write(&self.file(root), &crate::portable::to_vec(root, receipt)?)
    }

    pub(crate) fn include_options(&mut self, options: &impl Serialize) -> anyhow::Result<()> {
        let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
        digest.update(self.profile.as_bytes());
        digest.update(&serde_json::to_vec(options)?);
        self.profile = hex::encode(digest.finish());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_is_scoped_to_profile_runtime_and_launch_options() {
        let temp = tempfile::tempdir().unwrap();
        let working = temp.path().join("game");
        std::fs::create_dir_all(&working).unwrap();
        let java = temp.path().join("jdk/bin/java");
        std::fs::create_dir_all(java.parent().unwrap()).unwrap();
        std::fs::write(&java, "original runtime").unwrap();
        let mut version = crate::test_http::version_fixture("example");
        let compatibility =
            crate::java_compatibility::assess_in_directory(&version, &working).unwrap();
        let mut context = LaunchContext::new(&version, &working, &compatibility);
        context.include_options(&vec!["-Dtest=one"]).unwrap();
        assert!(context.previous(temp.path()).is_none());
        let info = JavaInfo {
            path: java.to_string_lossy().into_owned(),
            major: 21,
            version: "21.0.7".into(),
            arch: "aarch64".into(),
            jdk: true,
            vendor: String::new(),
        };
        let receipt = context.receipt(&info).unwrap();
        context.save(temp.path(), &receipt).unwrap();
        assert_eq!(context.previous(temp.path()), Some(java.clone()));
        let mut changed_options = LaunchContext::new(&version, &working, &compatibility);
        changed_options
            .include_options(&vec!["-Dtest=two"])
            .unwrap();
        assert!(changed_options.previous(temp.path()).is_none());
        version.main_class.push_str("Changed");
        let changed = crate::java_compatibility::assess_in_directory(&version, &working).unwrap();
        let mut changed_profile = LaunchContext::new(&version, &working, &changed);
        changed_profile
            .include_options(&vec!["-Dtest=one"])
            .unwrap();
        assert!(changed_profile.previous(temp.path()).is_none());
        std::fs::write(&java, "replaced runtime image").unwrap();
        assert!(context.previous(temp.path()).is_none());
    }
}
