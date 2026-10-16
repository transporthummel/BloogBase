//! Long-running work in the background, shown in the jobs panel (PLAN.md §3):
//! downloads and installs. Commands start a job and return right away; the
//! UI follows along through `JobUpdated` events.

use std::future::Future;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::error::{CommandError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum JobKind {
    /// Downloading DOSBox Staging.
    Runtime,
    /// Downloading a free episode; the subject is the game id.
    Shareware,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum JobPhase {
    Downloading,
    Installing,
    Done,
    Failed,
}

impl JobPhase {
    pub fn finished(self) -> bool {
        matches!(self, JobPhase::Done | JobPhase::Failed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: u32,
    pub kind: JobKind,
    pub subject: Option<String>,
    pub phase: JobPhase,
    /// Bytes downloaded so far.
    pub received: u32,
    /// Expected download size in bytes; 0 while unknown.
    pub total: u32,
    pub error: Option<CommandError>,
}

/// Sent whenever a job starts, makes progress or finishes.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct JobUpdated(pub Job);

static NEXT_ID: AtomicU32 = AtomicU32::new(1);

#[derive(Default)]
pub struct Jobs(Mutex<Vec<Job>>);

impl Jobs {
    pub fn list(&self) -> Vec<Job> {
        self.lock().clone()
    }

    /// Registers a job, or returns the unfinished one doing the same work
    /// (`false`), so a double click never starts a download twice.
    fn begin(&self, kind: JobKind, subject: Option<String>) -> (Job, bool) {
        let mut jobs = self.lock();
        if let Some(job) = jobs
            .iter()
            .find(|j| j.kind == kind && j.subject == subject && !j.phase.finished())
        {
            return (job.clone(), false);
        }
        // A retry replaces the failed attempt.
        jobs.retain(|j| !(j.kind == kind && j.subject == subject));
        let job = Job {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            kind,
            subject,
            phase: JobPhase::Downloading,
            received: 0,
            total: 0,
            error: None,
        };
        jobs.push(job.clone());
        (job, true)
    }

    fn update(&self, id: u32, f: impl FnOnce(&mut Job)) -> Option<Job> {
        let mut jobs = self.lock();
        let job = jobs.iter_mut().find(|j| j.id == id)?;
        f(job);
        Some(job.clone())
    }

    /// Forgets finished jobs.
    pub fn clear_finished(&self) {
        self.lock().retain(|j| !j.phase.finished());
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Job>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Handed to the work of a job to report progress.
#[derive(Clone)]
pub struct Reporter {
    app: AppHandle,
    id: u32,
}

impl Reporter {
    pub fn progress(&self, phase: JobPhase, received: u64, total: u64) {
        self.set(|j| {
            j.phase = phase;
            j.received = clamp(received);
            j.total = clamp(total);
        });
    }

    fn finish(&self, result: Result<()>) {
        self.set(|j| match result {
            Ok(()) => j.phase = JobPhase::Done,
            Err(err) => {
                j.phase = JobPhase::Failed;
                j.error = Some(err.into());
            }
        });
    }

    fn set(&self, f: impl FnOnce(&mut Job)) {
        if let Some(job) = self.app.state::<Jobs>().update(self.id, f) {
            let _ = JobUpdated(job).emit(&self.app);
        }
    }
}

fn clamp(n: u64) -> u32 {
    n.min(u32::MAX as u64) as u32
}

/// Runs `work` as a background job, unless the same job is already running.
pub fn start<Fut>(
    app: &AppHandle,
    kind: JobKind,
    subject: Option<String>,
    work: impl FnOnce(Reporter) -> Fut,
) -> Job
where
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    let (job, fresh) = app.state::<Jobs>().begin(kind, subject);
    if fresh {
        let _ = JobUpdated(job.clone()).emit(app);
        let reporter = Reporter {
            app: app.clone(),
            id: job.id,
        };
        let work = work(reporter.clone());
        tauri::async_runtime::spawn(async move { reporter.finish(work.await) });
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_work_is_not_started_twice() {
        let jobs = Jobs::default();
        let (a, fresh) = jobs.begin(JobKind::Shareware, Some("keen1".into()));
        assert!(fresh);
        let (b, fresh) = jobs.begin(JobKind::Shareware, Some("keen1".into()));
        assert!(!fresh);
        assert_eq!(a.id, b.id);
        let (c, fresh) = jobs.begin(JobKind::Shareware, Some("keen4".into()));
        assert!(fresh);
        assert_ne!(a.id, c.id);
        assert_eq!(jobs.list().len(), 2);
    }

    #[test]
    fn retry_replaces_the_failed_job_and_clearing_keeps_running_ones() {
        let jobs = Jobs::default();
        let (a, _) = jobs.begin(JobKind::Runtime, None);
        jobs.update(a.id, |j| j.phase = JobPhase::Failed).unwrap();
        let (b, fresh) = jobs.begin(JobKind::Runtime, None);
        assert!(fresh);
        assert_eq!(jobs.list().len(), 1);

        let (c, _) = jobs.begin(JobKind::Shareware, Some("keen4".into()));
        jobs.update(c.id, |j| j.phase = JobPhase::Done).unwrap();
        jobs.clear_finished();
        let left: Vec<_> = jobs.list().iter().map(|j| j.id).collect();
        assert_eq!(left, [b.id]);
    }
}
