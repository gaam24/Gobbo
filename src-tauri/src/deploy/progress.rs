//! One progress rule for an apply: count the files, send one message per percent, and always report
//! the last file. Link and overlay differ in what they do around a file, not in how they report it.

use super::apply::Job;
use super::{DeployError, DeployProgress, DeployStage};
#[cfg(test)]
use crate::error::FileError;
#[cfg(test)]
use std::cell::Cell;
#[cfg(test)]
use std::io;

// A test-only countdown for a failure between the journal entries of an apply. Production code has no fault hook.
#[cfg(test)]
thread_local! {
    static FAIL_AFTER: Cell<Option<(usize, io::ErrorKind)>> = const { Cell::new(None) };
}

/// Fails before the next `files` writes, with an error of `kind`. `fail_after(0, _)` clears an earlier fault.
#[cfg(test)]
pub fn fail_after(files: usize, kind: io::ErrorKind) {
    FAIL_AFTER.with(|cell| cell.set((files > 0).then_some((files, kind))));
}

/// Consumes one step of the fault, if any, and returns the error kind to inject.
#[cfg(test)]
fn injected_failure() -> Option<io::ErrorKind> {
    FAIL_AFTER.with(|cell| {
        let (left, kind) = cell.get()?;

        if left == 1 {
            cell.set(None);
        } else {
            cell.set(Some((left - 1, kind)));
        }

        Some(kind)
    })
}

/// Reports the writing phase of a deployment. The channel hears a step only when the percentage moves, so a list of
/// thousands of files does not flood it; the last file is always sent.
pub(super) struct ProgressMeter<'a> {
    total: usize,
    last_percent: u32,
    progress: &'a mut dyn FnMut(DeployProgress),
}

impl<'a> ProgressMeter<'a> {
    pub(super) fn new(total: usize, progress: &'a mut dyn FnMut(DeployProgress)) -> Self {
        Self {
            total,
            last_percent: 0,
            progress,
        }
    }

    /// Counts one finished file, where `done` runs from 1 to `total`.
    pub(super) fn step(&mut self, done: usize) {
        let step = DeployProgress::at(DeployStage::Writing, done, self.total);

        if step.percent != self.last_percent || done == self.total {
            (self.progress)(step);
            self.last_percent = step.percent;
        }
    }
}

/// Writes every job with the shared progress rule. `before` runs first for each file (a folder, a journal entry) and
/// `write` places the file itself; both receive the same job. A failure stops the loop before the next file.
pub(super) fn write_jobs(
    jobs: &[Job],
    progress: &mut dyn FnMut(DeployProgress),
    mut before: impl FnMut(&Job) -> Result<(), DeployError>,
    mut write: impl FnMut(&Job) -> Result<(), DeployError>,
) -> Result<(), DeployError> {
    let mut meter = ProgressMeter::new(jobs.len(), progress);

    for (index, job) in jobs.iter().enumerate() {
        before(job)?;
        #[cfg(test)]
        if let Some(kind) = injected_failure() {
            return Err(FileError::new(&job.dst, io::Error::from(kind)).into());
        }

        write(job)?;
        meter.step(index + 1);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ProgressMeter;
    use super::write_jobs;
    use crate::deploy::DeployError;
    use crate::deploy::DeployProgress;
    use crate::deploy::apply::Job;
    use std::path::PathBuf;

    #[test]
    fn one_step_per_percent_and_the_last_file() {
        let mut seen = Vec::new();
        {
            let mut send = |progress: DeployProgress| seen.push(progress.percent);
            let mut meter = ProgressMeter::new(3, &mut send);
            meter.step(1);
            meter.step(2);
            meter.step(3);
        }

        assert_eq!(seen, [33, 66, 100]);
    }

    #[test]
    fn files_that_do_not_move_the_percent_are_not_sent() {
        let mut seen = Vec::new();
        {
            let mut send = |progress: DeployProgress| seen.push(progress.percent);
            let mut meter = ProgressMeter::new(200, &mut send);
            meter.step(1);
            meter.step(2);
            meter.step(3);
            meter.step(200);
        }

        assert_eq!(seen, [1, 100]);
    }

    #[test]
    fn write_jobs_runs_before_and_write_for_every_job_and_reports_it() {
        let jobs = [
            Job {
                src: PathBuf::from("a"),
                dst: PathBuf::from("a"),
                dir: PathBuf::from("."),
            },
            Job {
                src: PathBuf::from("b"),
                dst: PathBuf::from("b"),
                dir: PathBuf::from("."),
            },
        ];
        let mut seen = Vec::new();
        let mut prepared = Vec::new();
        let mut written = Vec::new();
        write_jobs(
            &jobs,
            &mut |progress| seen.push(progress.percent),
            |job| {
                prepared.push(job.src.clone());
                Ok(())
            },
            |job| {
                written.push(job.src.clone());
                Ok::<_, DeployError>(())
            },
        )
        .unwrap();
        assert_eq!(seen, [50, 100]);
        let names = [PathBuf::from("a"), PathBuf::from("b")];
        assert_eq!(prepared, names);
        assert_eq!(written, names);
    }
}
