//! The background-job state machine.

use atlas_workspace::{Job, JobBoard, JobError, JobId, JobState, PaneDefinition};

#[test]
fn a_job_runs_from_queued_through_progress_to_completed() {
    let mut board = JobBoard::default();
    let pane = PaneDefinition::new("forecast");
    let id = board.start("Recalculating forecast", "recalculate", Some(pane.clone()), true);
    assert_eq!(id, JobId(1));
    assert_eq!(board.running_count(), 1);
    assert_eq!(board.percent(id), Some(0));
    assert_eq!(board.get(id).unwrap().state, JobState::Queued);
    assert_eq!(board.get(id).unwrap().attempts, 1);
    assert_eq!(board.get(id).unwrap().associated, Some(pane));

    board.progress(id, 25, Some(100), Some("months 1–3".to_owned())).unwrap();
    assert_eq!(board.percent(id), Some(25));
    assert!(matches!(board.get(id).unwrap().state, JobState::Running { done: 25, total: Some(100), .. }));
    board.progress(id, 7, None, None).unwrap();
    assert_eq!(board.percent(id), None, "no total, no percentage");
    assert_eq!(board.running().len(), 1);

    board.complete(id, "36 months recalculated").unwrap();
    assert_eq!(board.percent(id), Some(100));
    assert_eq!(board.running_count(), 0);
    assert_eq!(
        board.get(id).unwrap().state,
        JobState::Completed {
            summary: "36 months recalculated".to_owned()
        }
    );
    assert_eq!(board.progress(id, 1, None, None), Err(JobError::AlreadyFinished(id)));
    assert_eq!(board.complete(id, "again"), Err(JobError::AlreadyFinished(id)));
    assert_eq!(board.fail(id, "late", true), Err(JobError::AlreadyFinished(id)));
    assert_eq!(board.progress(JobId(99), 1, None, None), Err(JobError::UnknownJob(JobId(99))));
}

#[test]
fn a_failed_retryable_job_is_retried_as_a_fresh_queued_job() {
    let mut board = JobBoard::default();
    let pane = PaneDefinition::new("household");
    let id = board.start("Saving household", "save", Some(pane.clone()), false);
    board.progress(id, 1, Some(2), None).unwrap();
    board.fail(id, "disk full", true).unwrap();
    assert_eq!(board.percent(id), None);
    assert_eq!(board.running_count(), 0);

    let retry = board.retry(id).unwrap();
    assert_eq!(retry, JobId(2));
    let job: &Job = board.get(retry).unwrap();
    assert_eq!(job.state, JobState::Queued);
    assert_eq!(job.attempts, 2);
    assert_eq!(job.title, "Saving household");
    assert_eq!(job.source, "save");
    assert_eq!(job.associated, Some(pane));
    assert!(!job.cancellable);
    assert_eq!(board.running_count(), 1);
    assert_eq!(board.len(), 2, "the failed job stays on the board");

    assert_eq!(board.retry(retry), Err(JobError::NotFailed(retry)));
    board.fail(retry, "permission denied", false).unwrap();
    assert_eq!(board.retry(retry), Err(JobError::NotRetryable(retry)));
    assert_eq!(board.retry(JobId(42)), Err(JobError::UnknownJob(JobId(42))));
}

#[test]
fn cancel_refuses_finished_and_non_cancellable_jobs() {
    let mut board = JobBoard::default();
    let fixed = board.start("Import", "import", None, false);
    assert_eq!(board.cancel(fixed), Err(JobError::NotCancellable(fixed)));
    let stoppable = board.start("Export", "export", None, true);
    board.progress(stoppable, 1, Some(10), None).unwrap();
    board.cancel(stoppable).unwrap();
    assert_eq!(board.get(stoppable).unwrap().state, JobState::Cancelled);
    assert_eq!(board.cancel(stoppable), Err(JobError::AlreadyFinished(stoppable)));
    board.complete(fixed, "done").unwrap();
    assert_eq!(board.cancel(fixed), Err(JobError::AlreadyFinished(fixed)));
    assert_eq!(board.running_count(), 0);
    assert_eq!(board.cancel(JobId(7)), Err(JobError::UnknownJob(JobId(7))));
}

#[test]
fn prune_keeps_the_newest_finished_jobs_and_every_active_one() {
    let mut board = JobBoard::new(2);
    let mut finished = Vec::new();
    for n in 0..5 {
        let id = board.start(format!("Job {n}"), "test", None, true);
        board.complete(id, "ok").unwrap();
        finished.push(id);
    }
    let active = board.start("Still going", "test", None, true);
    board.progress(active, 1, None, None).unwrap();
    assert_eq!(board.len(), 6);

    let removed = board.prune();
    assert_eq!(removed, 3);
    assert_eq!(board.len(), 3);
    assert!(board.get(active).is_some());
    assert!(board.get(finished[4]).is_some(), "the newest finished job stays");
    assert!(board.get(finished[3]).is_some());
    assert!(board.get(finished[0]).is_none(), "the oldest goes");
    assert_eq!(board.finished().iter().map(|job| job.id).collect::<Vec<_>>(), vec![finished[4], finished[3]]);
    assert_eq!(board.prune(), 0);
    assert_eq!(board.running_count(), 1);
}

#[test]
fn job_state_labels_and_serialization() {
    let mut board = JobBoard::default();
    let id = board.start("Thing", "test", None, true);
    board.progress(id, 3, Some(4), Some("nearly".to_owned())).unwrap();
    let value = serde_json::to_value(board.get(id).unwrap()).unwrap();
    assert_eq!(value["state"]["state"], "running");
    assert_eq!(value["state"]["done"], 3);
    assert_eq!(value["id"], 1);
    assert_eq!(board.get(id).unwrap().state.label(), "running");
    assert!(board.get(id).unwrap().state.is_active());
    let back: Job = serde_json::from_value(value).unwrap();
    assert_eq!(&back, board.get(id).unwrap());
    assert_eq!(id.to_string(), "job_1");
}
