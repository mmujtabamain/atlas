//! Background jobs, as seen by the workspace.
//!
//! Saving, recalculating a forecast, importing a file: work that outlives a
//! frame and that the user wants to see progress on from any pane. The
//! [`JobBoard`] is the pure state machine behind the indicator — the UI
//! drives it (`start`, `progress`, `complete`, `fail`, `cancel`, `retry`) and
//! reads it (`running`, `percent`). A job may carry the [`PaneDefinition`]
//! of the pane it belongs to, so the indicator can open or focus that pane
//! through the resolver.
//!
//! States: `Queued → Running → Completed | Failed | Cancelled`. A failed job
//! that is retryable can be retried, which makes a **new** queued job with
//! the same title, source and pane and `attempts + 1`; the failed one stays
//! on the board until pruned. Finished jobs are kept up to `keep_finished`
//! (newest first) so the indicator can show what just happened.

use crate::workspace::PaneDefinition;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// Default number of finished jobs kept on the board.
pub const DEFAULT_KEEP_FINISHED: usize = 20;

/// A job's id, unique on its board.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JobId(pub u64);

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "job_{}", self.0)
    }
}

/// Where a job is in its life.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum JobState {
    Queued,
    Running { done: u64, total: Option<u64>, message: Option<String> },
    Completed { summary: String },
    Failed { error: String, retryable: bool },
    Cancelled,
}

impl JobState {
    /// True for `Completed`, `Failed` and `Cancelled`.
    pub fn is_finished(&self) -> bool {
        matches!(self, JobState::Completed { .. } | JobState::Failed { .. } | JobState::Cancelled)
    }

    /// True for `Queued` and `Running`.
    pub fn is_active(&self) -> bool {
        !self.is_finished()
    }

    /// A short word for the indicator.
    pub fn label(&self) -> &'static str {
        match self {
            JobState::Queued => "queued",
            JobState::Running { .. } => "running",
            JobState::Completed { .. } => "done",
            JobState::Failed { .. } => "failed",
            JobState::Cancelled => "cancelled",
        }
    }
}

/// One background job.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: JobId,
    /// What the user sees: "Saving household", "Recalculating forecast".
    pub title: String,
    /// Which subsystem started it: `"save"`, `"recalculate"`, `"import"`, …
    pub source: String,
    pub state: JobState,
    /// The pane the job belongs to, for "show me" from the indicator.
    #[serde(default)]
    pub associated: Option<PaneDefinition>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// 1 for a first run, one more for every retry.
    pub attempts: u32,
    /// Whether the user may cancel it while it is active.
    pub cancellable: bool,
}

/// Why a transition refused.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum JobError {
    #[error("no job {0}")]
    UnknownJob(JobId),
    #[error("{0} cannot be cancelled")]
    NotCancellable(JobId),
    #[error("{0} has already finished")]
    AlreadyFinished(JobId),
    #[error("{0} did not fail, so there is nothing to retry")]
    NotFailed(JobId),
    #[error("{0} failed in a way that cannot be retried")]
    NotRetryable(JobId),
}

/// The jobs the indicator shows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobBoard {
    jobs: Vec<Job>,
    next_id: u64,
    /// How many finished jobs `prune` keeps.
    pub keep_finished: usize,
}

impl Default for JobBoard {
    fn default() -> Self {
        JobBoard::new(DEFAULT_KEEP_FINISHED)
    }
}

impl JobBoard {
    /// An empty board keeping `keep_finished` finished jobs.
    pub fn new(keep_finished: usize) -> Self {
        JobBoard {
            jobs: Vec::new(),
            next_id: 1,
            keep_finished,
        }
    }

    /// Adds a queued job and returns its id.
    pub fn start(&mut self, title: impl Into<String>, source: impl Into<String>, associated: Option<PaneDefinition>, cancellable: bool) -> JobId {
        let id = JobId(self.next_id);
        self.next_id += 1;
        let now = Utc::now();
        let job = Job {
            id,
            title: title.into(),
            source: source.into(),
            state: JobState::Queued,
            associated,
            created_at: now,
            updated_at: now,
            attempts: 1,
            cancellable,
        };
        log::info!("jobs: {id} queued: {} ({})", job.title, job.source);
        self.jobs.push(job);
        id
    }

    /// Reports progress; moves a queued job to running.
    pub fn progress(&mut self, id: JobId, done: u64, total: Option<u64>, message: Option<String>) -> Result<(), JobError> {
        let job = self.active_mut(id)?;
        job.state = JobState::Running { done, total, message };
        job.updated_at = Utc::now();
        log::debug!("jobs: {id} running: {done}/{}", total.map(|t| t.to_string()).unwrap_or_else(|| "?".to_owned()));
        Ok(())
    }

    /// Finishes a job successfully.
    pub fn complete(&mut self, id: JobId, summary: impl Into<String>) -> Result<(), JobError> {
        let job = self.active_mut(id)?;
        let summary = summary.into();
        log::info!("jobs: {id} completed: {} — {summary}", job.title);
        job.state = JobState::Completed { summary };
        job.updated_at = Utc::now();
        Ok(())
    }

    /// Finishes a job with an error.
    pub fn fail(&mut self, id: JobId, error: impl Into<String>, retryable: bool) -> Result<(), JobError> {
        let job = self.active_mut(id)?;
        let error = error.into();
        log::warn!("jobs: {id} failed (attempt {}, retryable: {retryable}): {} — {error}", job.attempts, job.title);
        job.state = JobState::Failed { error, retryable };
        job.updated_at = Utc::now();
        Ok(())
    }

    /// Cancels an active, cancellable job.
    pub fn cancel(&mut self, id: JobId) -> Result<(), JobError> {
        let job = self.get_mut(id)?;
        if job.state.is_finished() {
            return Err(JobError::AlreadyFinished(id));
        }
        if !job.cancellable {
            return Err(JobError::NotCancellable(id));
        }
        log::info!("jobs: {id} cancelled: {}", job.title);
        job.state = JobState::Cancelled;
        job.updated_at = Utc::now();
        Ok(())
    }

    /// Queues a fresh copy of a failed, retryable job with `attempts + 1`.
    pub fn retry(&mut self, id: JobId) -> Result<JobId, JobError> {
        let job = self.get(id).ok_or(JobError::UnknownJob(id))?.clone();
        match job.state {
            JobState::Failed { retryable: true, .. } => {}
            JobState::Failed { retryable: false, .. } => return Err(JobError::NotRetryable(id)),
            _ => return Err(JobError::NotFailed(id)),
        }
        let new_id = JobId(self.next_id);
        self.next_id += 1;
        let now = Utc::now();
        log::info!("jobs: {id} retried as {new_id} (attempt {}): {}", job.attempts + 1, job.title);
        self.jobs.push(Job {
            id: new_id,
            title: job.title,
            source: job.source,
            state: JobState::Queued,
            associated: job.associated,
            created_at: now,
            updated_at: now,
            attempts: job.attempts + 1,
            cancellable: job.cancellable,
        });
        Ok(new_id)
    }

    /// One job.
    pub fn get(&self, id: JobId) -> Option<&Job> {
        self.jobs.iter().find(|job| job.id == id)
    }

    /// Every job, oldest first.
    pub fn all(&self) -> &[Job] {
        &self.jobs
    }

    /// Queued and running jobs, oldest first.
    pub fn running(&self) -> Vec<&Job> {
        self.jobs.iter().filter(|job| job.state.is_active()).collect()
    }

    /// Number of queued and running jobs.
    pub fn running_count(&self) -> usize {
        self.jobs.iter().filter(|job| job.state.is_active()).count()
    }

    /// Finished jobs, newest first.
    pub fn finished(&self) -> Vec<&Job> {
        let mut out: Vec<&Job> = self.jobs.iter().filter(|job| job.state.is_finished()).collect();
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| b.id.cmp(&a.id)));
        out
    }

    /// Progress as a percentage: `0` queued, `done / total` running (when a
    /// total is known), `100` completed; `None` for a failed or cancelled job
    /// or a running one with no total.
    pub fn percent(&self, id: JobId) -> Option<u8> {
        match &self.get(id)?.state {
            JobState::Queued => Some(0),
            JobState::Running { done, total: Some(total), .. } if *total > 0 => Some(((done * 100) / total).min(100) as u8),
            JobState::Running { .. } => None,
            JobState::Completed { .. } => Some(100),
            JobState::Failed { .. } | JobState::Cancelled => None,
        }
    }

    /// Drops finished jobs beyond the newest `keep_finished`; returns how many went.
    pub fn prune(&mut self) -> usize {
        let doomed: Vec<JobId> = self.finished().into_iter().skip(self.keep_finished).map(|job| job.id).collect();
        let before = self.jobs.len();
        self.jobs.retain(|job| !doomed.contains(&job.id));
        let removed = before - self.jobs.len();
        if removed > 0 {
            log::info!("jobs: pruned {removed} finished job(s)");
        }
        removed
    }

    /// Total number of jobs on the board.
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    /// True when the board has no jobs at all.
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    fn get_mut(&mut self, id: JobId) -> Result<&mut Job, JobError> {
        self.jobs.iter_mut().find(|job| job.id == id).ok_or(JobError::UnknownJob(id))
    }

    fn active_mut(&mut self, id: JobId) -> Result<&mut Job, JobError> {
        let job = self.get_mut(id)?;
        if job.state.is_finished() {
            return Err(JobError::AlreadyFinished(id));
        }
        Ok(job)
    }
}
