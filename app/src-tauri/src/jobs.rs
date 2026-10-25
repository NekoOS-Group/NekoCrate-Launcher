use anyhow::{Result, bail};
use nekocrate_core::auth::DeviceChallenge;
use std::{
    future::Future,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Default)]
pub struct Jobs(Arc<Mutex<State>>);

#[derive(Default)]
struct State {
    next_id: u64,
    active: Option<ActiveJob>,
    challenge: Option<(String, DeviceChallenge)>,
}

#[derive(Clone, PartialEq)]
enum Kind {
    Task,
    Login(String),
}

struct ActiveJob {
    id: u64,
    kind: Kind,
    cancel: CancellationToken,
    finished: watch::Receiver<bool>,
    pause: Option<watch::Sender<bool>>,
}

pub struct JobGuard {
    jobs: Jobs,
    id: u64,
    kind: Kind,
    pub cancel: CancellationToken,
    finished: watch::Sender<bool>,
}

impl Jobs {
    pub async fn task(&self) -> Result<JobGuard> {
        self.start(Kind::Task).await
    }

    pub async fn begin_login(&self, session_id: String) -> Result<JobGuard> {
        self.start(Kind::Login(session_id)).await
    }

    async fn start(&self, kind: Kind) -> Result<JobGuard> {
        let (guard, previous) = {
            let mut state = self.0.lock().unwrap();
            if state
                .active
                .as_ref()
                .is_some_and(|job| job.kind == Kind::Task)
            {
                bail!("有任务正在运行，请等待或取消当前任务");
            }
            let previous = state.active.take().map(|job| {
                job.cancel.cancel();
                job.finished
            });
            state.challenge = None;
            (self.reserve(&mut state, kind), previous)
        };
        // Reserve the successor first, but do not run it until the old login
        // has dropped its future and released core's operation lock. Keep this
        // wait even if superseded again, so rapid retries form a cleanup chain.
        if let Some(mut finished) = previous {
            let _ = finished.wait_for(|done| *done).await;
        }
        if guard.cancel.is_cancelled() {
            bail!("操作已取消");
        }
        Ok(guard)
    }

    fn reserve(&self, state: &mut State, kind: Kind) -> JobGuard {
        state.next_id += 1;
        let id = state.next_id;
        let cancel = CancellationToken::new();
        let (finished, receiver) = watch::channel(false);
        state.active = Some(ActiveJob {
            id,
            kind: kind.clone(),
            cancel: cancel.clone(),
            finished: receiver,
            pause: None,
        });
        JobGuard {
            jobs: self.clone(),
            id,
            kind,
            cancel,
            finished,
        }
    }

    pub fn wait_login(&self, session_id: &str) -> Result<(DeviceChallenge, JobGuard)> {
        let mut state = self.0.lock().unwrap();
        if state.active.is_some()
            || state
                .challenge
                .as_ref()
                .is_none_or(|(id, _)| id != session_id)
        {
            bail!("登录已取消");
        }
        let (_, challenge) = state.challenge.take().unwrap();
        let guard = self.reserve(&mut state, Kind::Login(session_id.into()));
        Ok((challenge, guard))
    }

    pub async fn cancel_login(&self, session_id: &str) {
        let finished = {
            let mut state = self.0.lock().unwrap();
            if state
                .challenge
                .as_ref()
                .is_some_and(|(id, _)| id == session_id)
            {
                state.challenge = None;
            }
            state
                .active
                .as_ref()
                .filter(|job| job.kind == Kind::Login(session_id.into()))
                .map(|job| {
                    job.cancel.cancel();
                    job.finished.clone()
                })
        };
        if let Some(mut finished) = finished {
            let _ = finished.wait_for(|done| *done).await;
        }
    }

    pub fn cancel_task(&self) {
        if let Some(job) = &self.0.lock().unwrap().active
            && job.kind == Kind::Task
        {
            job.cancel.cancel();
        }
    }

    pub fn cancel_all(&self) {
        let mut state = self.0.lock().unwrap();
        state.challenge = None;
        if let Some(job) = &state.active {
            job.cancel.cancel();
        }
    }

    pub fn busy(&self) -> bool {
        self.0.lock().unwrap().active.is_some()
    }

    pub fn set_paused(&self, paused: bool) -> Result<()> {
        let state = self.0.lock().unwrap();
        let control = state
            .active
            .as_ref()
            .and_then(|job| job.pause.as_ref())
            .ok_or_else(|| anyhow::anyhow!("当前没有可暂停的下载任务"))?;
        control
            .send(paused)
            .map_err(|_| anyhow::anyhow!("下载任务已结束"))
    }
}

impl JobGuard {
    pub async fn login<T>(&self, future: impl Future<Output = Result<T>>) -> Result<T> {
        tokio::select! {
            biased;
            _ = self.cancel.cancelled() => bail!("登录已取消"),
            result = future => result,
        }
    }

    pub fn store_challenge(&self, challenge: DeviceChallenge) -> Result<()> {
        let mut state = self.jobs.0.lock().unwrap();
        let Kind::Login(session_id) = &self.kind else {
            bail!("请先获取登录验证码");
        };
        if self.cancel.is_cancelled() || state.active.as_ref().is_none_or(|job| job.id != self.id) {
            bail!("登录已取消");
        }
        state.challenge = Some((session_id.clone(), challenge));
        Ok(())
    }

    pub fn enable_pause(&self) -> watch::Receiver<bool> {
        let (pause, receiver) = watch::channel(false);
        let mut state = self.jobs.0.lock().unwrap();
        if let Some(job) = &mut state.active
            && job.id == self.id
        {
            job.pause = Some(pause);
        }
        receiver
    }
}

impl Drop for JobGuard {
    fn drop(&mut self) {
        let mut state = self.jobs.0.lock().unwrap();
        if state.active.as_ref().is_some_and(|job| job.id == self.id) {
            state.active = None;
        }
        let _ = self.finished.send(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn challenge() -> DeviceChallenge {
        DeviceChallenge {
            device_code: "private-test-code".into(),
            user_code: "TEST-CODE".into(),
            verification_uri: "https://www.microsoft.com/link".into(),
            expires_in: 900,
            interval: 5,
            started_at: 0,
            client_id: String::new(),
        }
    }

    async fn polling(jobs: &Jobs, session: &str) -> JobGuard {
        let begin = jobs.begin_login(session.into()).await.unwrap();
        begin.store_challenge(challenge()).unwrap();
        drop(begin);
        jobs.wait_login(session).unwrap().1
    }

    #[tokio::test]
    async fn retry_cancels_old_login_and_waits_for_resource_release() {
        let jobs = Jobs::default();
        let old = polling(&jobs, "old").await;
        let old_cancel = old.cancel.clone();
        let next_jobs = jobs.clone();
        let next = tokio::spawn(async move { next_jobs.begin_login("new".into()).await.unwrap() });
        old_cancel.cancelled().await;
        assert!(
            !next.is_finished(),
            "must release core's lock before starting again"
        );
        assert!(old.store_challenge(challenge()).is_err());
        drop(old);
        let next = next.await.unwrap();
        assert!(jobs.busy(), "old guard must not clear its successor");
        jobs.cancel_login("old").await;
        assert!(!next.cancel.is_cancelled());
        next.store_challenge(challenge()).unwrap();
        drop(next);
        assert!(jobs.wait_login("old").is_err());
        let (_, new) = jobs.wait_login("new").unwrap();
        assert!(!new.cancel.is_cancelled());
    }

    #[tokio::test]
    async fn new_task_replaces_login_but_downloads_are_never_preempted() {
        let jobs = Jobs::default();
        let old = polling(&jobs, "old").await;
        let old_cancel = old.cancel.clone();
        let next_jobs = jobs.clone();
        let next = tokio::spawn(async move { next_jobs.task().await.unwrap() });
        old_cancel.cancelled().await;
        drop(old);
        let task = next.await.unwrap();
        jobs.cancel_login("old").await;
        assert!(jobs.begin_login("new".into()).await.is_err());
        assert!(jobs.task().await.is_err());
        assert!(!task.cancel.is_cancelled());
        let pause = task.enable_pause();
        jobs.set_paused(true).unwrap();
        assert!(*pause.borrow());
        jobs.set_paused(false).unwrap();
        assert!(!*pause.borrow());
        jobs.cancel_task();
        assert!(task.cancel.is_cancelled());
        drop(task);
        assert!(!jobs.busy());
    }

    #[tokio::test]
    async fn cancelled_login_drops_a_blocked_future_before_successor_runs() {
        let jobs = Jobs::default();
        let login = polling(&jobs, "old").await;
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        let held = lock.clone().lock_owned().await;
        let worker = tokio::spawn(async move {
            let result = login
                .login(async move {
                    let _held = held;
                    std::future::pending::<Result<()>>().await
                })
                .await;
            drop(login);
            result
        });
        let new = tokio::time::timeout(Duration::from_secs(1), jobs.begin_login("new".into()))
            .await
            .unwrap()
            .unwrap();
        assert!(worker.await.unwrap().is_err());
        assert!(lock.try_lock().is_ok());
        assert!(!new.cancel.is_cancelled());
    }

    #[tokio::test]
    async fn pending_code_is_invalidated_even_before_polling_starts() {
        let jobs = Jobs::default();
        let begin = jobs.begin_login("old".into()).await.unwrap();
        begin.store_challenge(challenge()).unwrap();
        drop(begin);
        let task = jobs.task().await.unwrap();
        assert!(jobs.wait_login("old").is_err());
        drop(task);
        let next = jobs.begin_login("new".into()).await.unwrap();
        next.store_challenge(challenge()).unwrap();
        drop(next);
        jobs.cancel_login("old").await;
        jobs.cancel_login("new").await;
        assert!(jobs.wait_login("new").is_err());
    }

    #[tokio::test]
    async fn rapid_retries_preserve_the_entire_cleanup_chain() {
        let jobs = Jobs::default();
        let first = polling(&jobs, "first").await;
        let second_jobs = jobs.clone();
        let second = tokio::spawn(async move { second_jobs.begin_login("second".into()).await });
        first.cancel.cancelled().await;
        let third_jobs = jobs.clone();
        let third = tokio::spawn(async move { third_jobs.begin_login("third".into()).await });
        loop {
            if jobs.0.lock().unwrap().active.as_ref().unwrap().kind == Kind::Login("third".into()) {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(!second.is_finished());
        assert!(!third.is_finished());
        drop(first);
        assert!(second.await.unwrap().is_err());
        let third = third.await.unwrap().unwrap();
        assert!(jobs.busy());
        drop(third);
        assert!(!jobs.busy());
    }
}
