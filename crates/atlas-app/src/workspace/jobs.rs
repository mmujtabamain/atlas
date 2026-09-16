//! The job centre: long-running work that belongs to the workspace, not to
//! any pane.
//!
//! A pane is a view of the household; it may be closed, moved, hidden behind a
//! tab or carried into another window while something it started is still
//! running. Work that took more than a frame therefore never lives inside a
//! pane. It is a **job** on the [`JobCenter`]: the pane (or the title bar)
//! starts it, the job runs to its end on its own, and whoever is on screen
//! afterwards — the same pane, a reopened one, nobody — reads its state from
//! the centre. The title bar shows how many jobs are running and lists them,
//! with the way back to the pane a job belongs to, a retry for a failed job
//! and a cancel for one that allows it.
//!
//! The state machine is [`atlas_workspace::JobBoard`] (pure data, tested on
//! its own). This entity adds what needs the app: the runners that a retry
//! re-executes, the cancellation flags background code polls, notifications
//! when a job ends, and the report to the team when one fails.
//!
//! Jobs today:
//!
//! | source | what | started by |
//! |---|---|---|
//! | `save` | the household written to its file | `Save` in the title bar, Save as… |
//! | `sensitivity` | the sensitivity report recomputed for a new scope | *Run sensitivity* in the Assumptions pane |
//!
//! Both survive the pane that started them; the sensitivity result is
//! installed when it arrives, unless the household was edited meanwhile, in
//! which case it is out of date and dropped (the next look computes afresh).

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use atlas_workspace::{Job, JobBoard, JobError, JobId, JobState, PaneDefinition};
use gpui_kit::*;

use crate::alerting::{self, Level};
use crate::nav::Route;
use crate::workspace::kinds;

/// How many finished jobs the list keeps before the oldest are dropped.
const KEEP_FINISHED: usize = 8;

/// What a job's background code holds: its id, and the flag a cancel sets.
#[derive(Clone, Debug)]
pub struct JobTicket {
    pub id: JobId,
    cancelled: Arc<AtomicBool>,
}

impl JobTicket {
    /// True once the person cancelled the job; long loops poll this.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// What the centre announces.
pub enum JobEvent {
    /// A job ended (completed, failed or was cancelled); the shell tells the person.
    Finished(JobId),
}

/// The operation a job performs, kept so a failed job can be retried: it is
/// handed the new job's ticket and runs the same work again.
pub type Runner = Rc<dyn Fn(JobTicket, &mut Window, &mut App)>;

/// The workspace's jobs and the means to drive them.
pub struct JobCenter {
    board: JobBoard,
    runners: HashMap<JobId, Runner>,
    cancels: HashMap<JobId, Arc<AtomicBool>>,
}

impl Default for JobCenter {
    fn default() -> Self {
        JobCenter { board: JobBoard::new(KEEP_FINISHED), runners: HashMap::new(), cancels: HashMap::new() }
    }
}

impl JobCenter {
    /// Starts a job and returns its ticket. `associated` is the screen the job
    /// belongs to, which the indicator opens on a click; `runner` is what a
    /// retry re-executes (`None` for a job that cannot be retried).
    pub fn start(&mut self, title: impl Into<String>, source: impl Into<String>, associated: Option<Route>, cancellable: bool, runner: Option<Runner>, cx: &mut Context<Self>) -> JobTicket {
        let associated = associated.map(kinds::definition_of);
        let id = self.board.start(title, source, associated, cancellable);
        let cancelled = Arc::new(AtomicBool::new(false));
        self.cancels.insert(id, cancelled.clone());
        if let Some(runner) = runner {
            self.runners.insert(id, runner);
        }
        cx.notify();
        JobTicket { id, cancelled }
    }

    /// Reports progress: `done` of `total` (or unknown), with an optional word on the current step.
    pub fn progress(&mut self, id: JobId, done: u64, total: Option<u64>, message: Option<String>, cx: &mut Context<Self>) {
        if let Err(err) = self.board.progress(id, done, total, message) {
            log::debug!("jobs: progress for {id} ignored: {err}");
        }
        cx.notify();
    }

    /// The job finished well.
    pub fn complete(&mut self, id: JobId, summary: impl Into<String>, cx: &mut Context<Self>) {
        match self.board.complete(id, summary) {
            Ok(()) => self.finished(id, cx),
            Err(err) => log::debug!("jobs: completion of {id} ignored: {err}"),
        }
    }

    /// The job failed. Failures reach the team through the alerting hook; the
    /// person sees a toast and, when `retryable`, a Retry in the jobs list.
    pub fn fail(&mut self, id: JobId, error: impl Into<String>, retryable: bool, cx: &mut Context<Self>) {
        let error = error.into();
        let title = self.board.get(id).map(|job| job.title.clone()).unwrap_or_default();
        match self.board.fail(id, error.clone(), retryable) {
            Ok(()) => {
                alerting::report(Level::Error, format!("job failed: {title}: {error}"));
                self.finished(id, cx);
            }
            Err(err) => log::debug!("jobs: failure of {id} ignored: {err}"),
        }
    }

    /// Cancels a running job that allows it: the flag its code polls is set
    /// and the job is marked cancelled at once.
    pub fn cancel(&mut self, id: JobId, cx: &mut Context<Self>) -> Result<(), JobError> {
        self.board.cancel(id)?;
        if let Some(flag) = self.cancels.get(&id) {
            flag.store(true, Ordering::Relaxed);
        }
        self.finished(id, cx);
        Ok(())
    }

    /// Runs a failed, retryable job again as a fresh job.
    pub fn retry(&mut self, id: JobId, window: &mut Window, cx: &mut Context<Self>) -> Result<JobId, JobError> {
        let runner = self.runners.get(&id).cloned().ok_or(JobError::NotRetryable(id))?;
        let new_id = self.board.retry(id)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        self.cancels.insert(new_id, cancelled.clone());
        self.runners.insert(new_id, runner.clone());
        cx.notify();
        runner(JobTicket { id: new_id, cancelled }, window, cx);
        Ok(new_id)
    }

    /// Drops the finished jobs from the list.
    pub fn clear_finished(&mut self, cx: &mut Context<Self>) {
        let finished: Vec<JobId> = self.board.finished().into_iter().map(|job| job.id).collect();
        self.board.keep_finished = 0;
        self.board.prune();
        self.board.keep_finished = KEEP_FINISHED;
        for id in finished {
            self.runners.remove(&id);
            self.cancels.remove(&id);
        }
        cx.notify();
    }

    fn finished(&mut self, id: JobId, cx: &mut Context<Self>) {
        self.cancels.remove(&id);
        self.board.prune();
        cx.emit(JobEvent::Finished(id));
        cx.notify();
    }

    /// Every job, oldest first.
    pub fn jobs(&self) -> &[Job] {
        self.board.all()
    }

    pub fn get(&self, id: JobId) -> Option<&Job> {
        self.board.get(id)
    }

    /// How many jobs are queued or running.
    pub fn running_count(&self) -> usize {
        self.board.running_count()
    }

    /// The running job from `source`, if there is one (the newest).
    pub fn running_for(&self, source: &str) -> Option<&Job> {
        self.board.running().into_iter().filter(|job| job.source == source).last()
    }

    /// Percent done, when the job's total is known.
    pub fn percent(&self, id: JobId) -> Option<u8> {
        self.board.percent(id)
    }

    /// Whether a retry is possible: failed, retryable, and its operation kept.
    pub fn can_retry(&self, id: JobId) -> bool {
        matches!(self.board.get(id).map(|job| &job.state), Some(JobState::Failed { retryable: true, .. })) && self.runners.contains_key(&id)
    }

    /// The screen a job belongs to, when it has one this build can open.
    pub fn associated_route(&self, id: JobId) -> Option<Route> {
        self.board.get(id).and_then(|job| job.associated.as_ref()).and_then(kinds::route_of)
    }

    /// One line for a job in the indicator's list: title and where it stands.
    pub fn describe(&self, job: &Job) -> String {
        match &job.state {
            JobState::Queued => format!("{} · queued", job.title),
            JobState::Running { done, total: Some(total), .. } if *total > 0 => format!("{} · {}%", job.title, (done * 100 / total).min(100)),
            JobState::Running { message: Some(message), .. } => format!("{} · {message}", job.title),
            JobState::Running { .. } => format!("{} · running", job.title),
            JobState::Completed { .. } => format!("{} · done", job.title),
            JobState::Failed { error, .. } => format!("{} · failed: {error}", job.title),
            JobState::Cancelled => format!("{} · cancelled", job.title),
        }
    }

    /// The definition of the pane a job belongs to, for the resolver.
    pub fn associated_definition(&self, id: JobId) -> Option<&PaneDefinition> {
        self.board.get(id).and_then(|job| job.associated.as_ref())
    }
}

impl EventEmitter<JobEvent> for JobCenter {}

#[cfg(test)]
mod tests {
    use super::*;
    // The module glob-imports gpui, whose `test` attribute would otherwise
    // shadow the language's inside the expansion of `gpui_kit::test`.
    use core::prelude::v1::test;
    use std::cell::Cell;

    #[gpui_kit::test]
    fn a_job_runs_to_completion_and_is_listed(cx: &mut TestAppContext) {
        let centre = cx.new(|_| JobCenter::default());
        let ticket = cx.update(|cx| centre.update(cx, |centre, cx| centre.start("Save household", "save", None, false, None, cx)));
        cx.update(|cx| {
            centre.update(cx, |centre, cx| {
                assert_eq!(centre.running_count(), 1);
                assert!(centre.running_for("save").is_some());
                centre.progress(ticket.id, 1, Some(4), Some("writing".into()), cx);
                assert_eq!(centre.percent(ticket.id), Some(25));
                assert!(centre.describe(centre.get(ticket.id).unwrap()).contains("25%"));
                centre.complete(ticket.id, "Saved", cx);
                assert_eq!(centre.running_count(), 0);
                assert!(centre.running_for("save").is_none());
                assert!(!centre.can_retry(ticket.id));
                assert!(centre.describe(centre.get(ticket.id).unwrap()).ends_with("done"));
            })
        });
    }

    #[gpui_kit::test]
    fn a_failed_job_retries_through_its_runner_and_a_cancel_sets_the_flag(cx: &mut TestAppContext) {
        let centre = cx.new(|_| JobCenter::default());
        let runs = Rc::new(Cell::new(0u32));
        let counted = runs.clone();
        let runner: Runner = Rc::new(move |_ticket, _, _| counted.set(counted.get() + 1));
        let ticket = cx.update(|cx| centre.update(cx, |centre, cx| centre.start("Recompute sensitivity", "sensitivity", Some(Route::Sensitivity), true, Some(runner), cx)));
        cx.update(|cx| {
            centre.update(cx, |centre, cx| {
                assert_eq!(centre.associated_route(ticket.id), Some(Route::Sensitivity));
                centre.fail(ticket.id, "engine said no", true, cx);
                assert!(centre.can_retry(ticket.id));
            })
        });
        struct Blank;
        impl Render for Blank {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div()
            }
        }
        let window = cx.add_window(|_, _| Blank);
        let retried = cx.update_window(window.into(), |_, window, cx| centre.update(cx, |centre, cx| centre.retry(ticket.id, window, cx).expect("retry"))).unwrap();
        assert_ne!(retried, ticket.id);
        assert_eq!(runs.get(), 1, "the retry ran the job's operation again");
        cx.update(|cx| {
            centre.update(cx, |centre, cx| {
                assert_eq!(centre.get(retried).map(|job| job.attempts), Some(2));
                assert!(centre.running_for("sensitivity").is_some());
                // Cancelling flips the flag the running code polls.
                let ticket = JobTicket { id: retried, cancelled: centre.cancels.get(&retried).cloned().unwrap() };
                assert!(!ticket.is_cancelled());
                centre.cancel(retried, cx).expect("cancellable");
                assert!(ticket.is_cancelled());
                assert_eq!(centre.running_count(), 0);
                assert!(centre.cancel(retried, cx).is_err(), "a finished job cannot be cancelled again");
                centre.clear_finished(cx);
                assert!(centre.jobs().is_empty());
            })
        });
    }
}
