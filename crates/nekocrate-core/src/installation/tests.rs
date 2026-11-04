use super::*;
use crate::storage;
use std::{collections::BTreeSet, path::Path, sync::Arc};

const PLAN: Plan<'static> = Plan(&[
    Step::new("files", "files"),
    Step::new("extract", "extract"),
    Step::new("publish", "publish"),
]);
fn sink() -> EventSink {
    Arc::new(|_| {})
}

// The fixture mimics an existing domain record. The executor has no other
// persistence; actions unconditionally do work, so only restore can skip them.
struct Fixture<'a> {
    root: &'a Path,
    done: BTreeSet<String>,
    fail: bool,
    fail_checkpoint: bool,
    manual: bool,
    executed: Vec<&'static str>,
    cancel: &'a CancellationToken,
    cancel_at: Option<&'static str>,
}
fn output(step: Step) -> &'static str {
    match step.id {
        "files" => "payload",
        "extract" => "extracted",
        "publish" => "installed",
        _ => unreachable!(),
    }
}
impl Handler for Fixture<'_> {
    fn restore<'a>(&'a mut self, step: Step) -> BoxFuture<'a, Result<bool>> {
        Box::pin(async move {
            Ok(self.done.contains(step.id)
                && std::fs::read(self.root.join(output(step))).ok().as_deref() == Some(b"verified"))
        })
    }
    fn execute<'a>(&'a mut self, step: Step) -> StepFuture<'a> {
        Box::pin(async move {
            self.executed.push(step.id);
            if step.id == "extract" {
                if self.fail {
                    anyhow::bail!("fixture extraction failed");
                }
                if self.manual && !self.root.join("manual").is_file() {
                    return Ok(Outcome::Waiting);
                }
            }
            std::fs::write(self.root.join(output(step)), b"verified")?;
            if self.cancel_at == Some(step.id) {
                self.cancel.cancel();
            }
            Ok(Outcome::Complete)
        })
    }
    fn checkpoint(&mut self, step: Step) -> Result<()> {
        if self.fail_checkpoint {
            anyhow::bail!("fixture disk write failed");
        }
        self.done.insert(step.id.into());
        storage::write_json(&self.root.join("record.json"), &self.done)
    }
}
fn fixture<'a>(root: &'a Path, cancel: &'a CancellationToken) -> Fixture<'a> {
    Fixture {
        root,
        done: if root.join("record.json").exists() {
            storage::read_json(&root.join("record.json")).unwrap()
        } else {
            BTreeSet::new()
        },
        fail: false,
        fail_checkpoint: false,
        manual: false,
        executed: vec![],
        cancel,
        cancel_at: None,
    }
}

#[tokio::test]
async fn restart_actually_skips_verified_actions_and_reexecutes_damaged_outputs() {
    let temp = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    let mut first = fixture(temp.path(), &cancel);
    first.fail = true;
    assert!(PLAN.run(&mut first, &cancel, &sink()).await.is_err());
    assert_eq!(first.executed, ["files", "extract"]);
    assert!(!temp.path().join("installed").exists());
    let mut resumed = fixture(temp.path(), &cancel);
    PLAN.run(&mut resumed, &cancel, &sink()).await.unwrap();
    assert_eq!(resumed.executed, ["extract", "publish"]);
    std::fs::write(temp.path().join("payload"), b"corrupt").unwrap();
    let mut repaired = fixture(temp.path(), &cancel);
    PLAN.run(&mut repaired, &cancel, &sink()).await.unwrap();
    assert_eq!(repaired.executed, ["files"]);
    assert_eq!(
        std::fs::read(temp.path().join("payload")).unwrap(),
        b"verified"
    );
    let files: BTreeSet<_> = std::fs::read_dir(temp.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(
        files,
        [
            "payload",
            "extracted",
            "installed",
            "record.json",
            "record.json.bak"
        ]
        .map(str::to_owned)
        .into()
    );
}

#[tokio::test]
async fn manual_wait_never_checkpoints_or_publishes_and_restart_uses_the_same_record() {
    let temp = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    let mut task = fixture(temp.path(), &cancel);
    task.manual = true;
    assert_eq!(
        PLAN.run(&mut task, &cancel, &sink()).await.unwrap(),
        Outcome::Waiting
    );
    assert!(!temp.path().join("installed").exists());
    let mut resumed = fixture(temp.path(), &cancel);
    assert_eq!(resumed.done, ["files".into()].into());
    std::fs::write(temp.path().join("manual"), b"provided").unwrap();
    resumed.manual = true;
    assert_eq!(
        PLAN.run(&mut resumed, &cancel, &sink()).await.unwrap(),
        Outcome::Complete
    );
    assert_eq!(resumed.executed, ["extract", "publish"]);
}

#[tokio::test]
async fn checkpoint_failure_stops_publication_and_unrecorded_output_cannot_skip_work() {
    let temp = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    let mut task = fixture(temp.path(), &cancel);
    task.fail_checkpoint = true;
    assert!(
        PLAN.run(&mut task, &cancel, &sink())
            .await
            .unwrap_err()
            .to_string()
            .contains("disk write")
    );
    assert_eq!(task.executed, ["files"]);
    assert!(temp.path().join("payload").is_file());
    assert!(!temp.path().join("record.json").exists());
    assert!(!temp.path().join("installed").exists());
    let mut resumed = fixture(temp.path(), &cancel);
    PLAN.run(&mut resumed, &cancel, &sink()).await.unwrap();
    assert_eq!(resumed.executed, ["files", "extract", "publish"]);
}

#[tokio::test]
async fn cancellation_stops_at_boundaries_but_keeps_successful_commit_recorded() {
    for cancel_at in ["files", "publish"] {
        let temp = tempfile::tempdir().unwrap();
        let cancel = CancellationToken::new();
        let mut task = fixture(temp.path(), &cancel);
        task.cancel_at = Some(cancel_at);
        assert_eq!(
            PLAN.run(&mut task, &cancel, &sink()).await.is_ok(),
            cancel_at == "publish"
        );
        assert_eq!(
            temp.path().join("installed").exists(),
            cancel_at == "publish"
        );
        assert_eq!(
            fixture(temp.path(), &cancel).done.contains("publish"),
            cancel_at == "publish"
        );
    }
}

#[tokio::test]
async fn workflow_cancellation_does_not_enter_restore_or_execute() {
    let temp = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let sink: EventSink = Arc::new(move |_| token.cancel());
    let mut task = fixture(temp.path(), &cancel);
    assert!(PLAN.run(&mut task, &cancel, &sink).await.is_err());
    assert!(task.executed.is_empty());
    assert!(!temp.path().join("record.json").exists());
}

#[tokio::test]
async fn obsolete_execution_journal_cannot_authorize_skipping_an_action() {
    let temp = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    let journal = temp.path().join("execution.json");
    let obsolete = br#"{"schema":99,"complete":true,"steps":[{"id":"files","state":"complete"}]}"#;
    std::fs::write(&journal, obsolete).unwrap();
    let mut task = fixture(temp.path(), &cancel);
    PLAN.run(&mut task, &cancel, &sink()).await.unwrap();
    assert_eq!(task.executed, ["files", "extract", "publish"]);
    assert_eq!(std::fs::read(journal).unwrap(), obsolete);
}

#[tokio::test]
async fn game_files_resume_offline_after_native_preparation_fails() {
    use crate::{
        download::Downloader,
        model::{Artifact, Settings},
        test_http,
    };
    use sha1::{Digest, Sha1};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("launcher");
    let game = temp.path().join(".minecraft");
    let index = r#"{"objects":{}}"#;
    let (url, server) = test_http::serve(vec![(200, index.into()), (200, "client".into())]).await;
    let hash = |b: &[u8]| hex::encode(Sha1::digest(b));
    let mut version = test_http::version_fixture("test");
    version.asset_index.url = format!("{url}/index");
    version.asset_index.sha1 = hash(index.as_bytes());
    version.asset_index.size = index.len() as u64;
    version.downloads.insert(
        "client".into(),
        Artifact {
            url: format!("{url}/client"),
            sha1: hash(b"client"),
            size: 6,
            path: None,
        },
    );
    let natives = game.join("versions/test/natives");
    std::fs::create_dir_all(natives.parent().unwrap()).unwrap();
    std::fs::write(&natives, b"occupied").unwrap();
    let settings = Settings {
        proxy_mode: "none".into(),
        retries: 0,
        ..Default::default()
    };
    let cancel = CancellationToken::new();
    let downloader = Downloader::new(&root, settings.clone()).unwrap();
    assert!(
        game::files(&game, &version, &downloader, &cancel, &sink())
            .await
            .is_err()
    );
    assert_eq!(server.await.unwrap().len(), 2);
    assert!(!game.join("versions/test/.nekocrate/installation").exists());
    drop(downloader);
    std::fs::remove_file(&natives).unwrap();
    // No server remains. A new downloader must recover using disk receipts.
    game::files(
        &game,
        &version,
        &Downloader::new(&root, settings).unwrap(),
        &cancel,
        &sink(),
    )
    .await
    .unwrap();
    assert!(natives.is_dir());
    assert_eq!(
        std::fs::read(game.join("versions/test/test.jar")).unwrap(),
        b"client"
    );
    assert!(!game.join("versions/test/.nekocrate/installation").exists());
    assert!(
        !game.join("versions/test/test.json").exists(),
        "payload preparation never publishes a launch profile"
    );
}
