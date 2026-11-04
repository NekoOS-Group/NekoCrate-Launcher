//! Shared execution, with exactly one durable checkpoint owned by each domain.
//!
//! A plan never writes a second journal. `restore` proves a saved result can be
//! reused; `checkpoint` commits the result to the same record that discovery and
//! recovery read. File-level work uses Downloader's verified-file receipts.
use crate::{
    download::check_cancel,
    model::{Event, EventSink},
};
use anyhow::Result;
use futures_util::future::BoxFuture;
use tokio_util::sync::CancellationToken;

pub(crate) mod game;
pub(crate) mod official;
#[cfg(test)]
mod tests;

pub(crate) type StepFuture<'a> = BoxFuture<'a, Result<Outcome>>;

#[derive(Clone, Copy)]
pub(crate) struct Step {
    pub id: &'static str,
    label: Option<&'static str>,
}
impl Step {
    pub const fn new(id: &'static str, label: &'static str) -> Self {
        Self {
            id,
            label: Some(label),
        }
    }
    pub const fn silent(id: &'static str) -> Self {
        Self { id, label: None }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Complete,
    Waiting,
}

pub(crate) trait Handler: Send {
    /// Read the authoritative checkpoint and validate/hydrate its output. Only
    /// true lets the executor skip the action; files alone are not trusted.
    fn restore<'a>(&'a mut self, _step: Step) -> BoxFuture<'a, Result<bool>> {
        Box::pin(async { Ok(false) })
    }
    fn execute<'a>(&'a mut self, step: Step) -> StepFuture<'a>;
    /// Persist action output and its completion together in the domain record.
    /// Steps whose durable proof is their output/receipt need no extra flag.
    fn checkpoint(&mut self, _step: Step) -> Result<()> {
        Ok(())
    }
}

pub(crate) struct Plan<'a>(pub &'a [Step]);
impl Plan<'_> {
    #[inline(never)]
    pub fn run<'a>(
        &'a self,
        handler: &'a mut dyn Handler,
        cancel: &'a CancellationToken,
        sink: &'a EventSink,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            for step in self.0.iter().copied() {
                check_cancel(cancel)?;
                if let Some(label) = step.label {
                    sink(Event::workflow(step.id, label));
                }
                check_cancel(cancel)?;
                if handler.restore(step).await? {
                    continue;
                }
                check_cancel(cancel)?;
                // Never detach or race-drop a step: cancellation cooperates
                // with downloads/processors and lets publication finish.
                if handler.execute(step).await? == Outcome::Waiting {
                    return Ok(Outcome::Waiting);
                }
                handler.checkpoint(step)?;
            }
            Ok(Outcome::Complete)
        })
    }
}
