//! The worker pool shared by the provider update checks: one index goes to one worker and every answer keeps the place
//! of the request it answers.

use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;

use super::{UPDATE_WORKERS, take_index};

/// The error detail of a worker that panicked. It becomes the error of the requests whose worker stopped.
pub const WORKER_PANIC: &str = "an update worker panicked";

/// What one worker run brought.
pub struct WorkerRun<T> {
    /// `(index, value)` for every index the work answered with `Some`, in request order.
    pub values: Vec<(usize, T)>,
    /// Whether a worker panicked. The values above are the ones that finished before it.
    pub panicked: bool,
}

/// Runs `work` for every index in `0..len` on the update workers. A `None` answer skips that index; the answers keep
/// the request order even when the workers finish in another one. A panicking worker stops only its own thread: the
/// answers the other workers gave are kept, and `panicked` tells the caller what happened to the missing ones.
pub fn in_workers<T: Send>(len: usize, work: impl Fn(usize) -> Option<T> + Sync) -> WorkerRun<T> {
    let next = AtomicUsize::new(0);
    let answers: Mutex<Vec<(usize, T)>> = Mutex::new(Vec::with_capacity(len));
    let mut panicked = false;
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..UPDATE_WORKERS.min(len))
            .map(|_| {
                scope.spawn(|| {
                    while let Some(index) = take_index(&next, len) {
                        if let Some(value) = work(index) {
                            answers
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .push((index, value));
                        }
                    }
                })
            })
            .collect();
        for worker in workers {
            // Every handle is joined here: `thread::scope` resumes the panic of a worker that was not joined.
            if worker.join().is_err() {
                panicked = true;
            }
        }
    });
    let mut values = answers
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    values.sort_unstable_by_key(|(index, _)| *index);
    WorkerRun { values, panicked }
}

/// Runs `work` for every request on the update workers and gives one answer per request, in the request order. A
/// request whose worker stopped gets `lost` instead, so the answers of the requests that finished are kept.
pub fn per_request<T: Sync, U: Send>(
    requests: &[T],
    work: impl Fn(&T) -> U + Sync,
    lost: impl Fn(&T) -> U,
) -> Vec<U> {
    let run = in_workers(requests.len(), |index| requests.get(index).map(&work));
    if run.panicked {
        crate::infra::log::error(WORKER_PANIC);
    }
    let mut answers: Vec<Option<U>> = requests.iter().map(|_| None).collect();
    for (index, value) in run.values {
        if let Some(slot) = answers.get_mut(index) {
            *slot = Some(value);
        }
    }
    answers
        .into_iter()
        .zip(requests)
        .map(|(answer, request)| answer.unwrap_or_else(|| lost(request)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;

    #[test]
    fn a_mixed_short_list_runs_every_index_and_keeps_the_order() {
        let run = in_workers(3, |index| (index != 1).then_some(index * 10));
        assert!(!run.panicked);
        assert_eq!(run.values, [(0, 0), (2, 20)]);
    }

    #[test]
    fn an_empty_list_starts_no_worker() {
        let run = in_workers(0, |_| Some(1));
        assert!(!run.panicked);
        assert!(run.values.is_empty());
    }

    /// The first request answers last, so the workers finish in another order; the run still reports the request
    /// order.
    #[test]
    fn the_answers_keep_the_request_order() {
        let barrier = Barrier::new(UPDATE_WORKERS);
        let finished = Mutex::new(Vec::new());
        let run = in_workers(UPDATE_WORKERS, |index| {
            if index == 0 {
                barrier.wait();
            }
            finished
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(index);
            if index != 0 {
                barrier.wait();
            }
            Some(index)
        });
        let finished = finished
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(finished.last(), Some(&0), "the first request finished last");
        assert!(!run.panicked);
        assert_eq!(
            run.values,
            [(0, 0), (1, 1), (2, 2), (3, 3)],
            "the answers keep the request order"
        );
    }

    #[test]
    fn per_request_keeps_every_answer_in_the_request_order() {
        let answers = per_request(&[1, 2, 3], |value| value * 10, |_| 0);
        assert_eq!(answers, [10, 20, 30]);
    }

    #[test]
    fn a_lost_request_gets_its_own_answer_and_the_others_keep_theirs() {
        let requests: Vec<usize> = (0..9).collect();
        let answers = per_request(
            &requests,
            |&index| {
                assert_ne!(index, 4, "the worker for this request stopped");
                index
            },
            |&index| 100 + index,
        );
        assert_eq!(answers, [0, 1, 2, 3, 104, 5, 6, 7, 8]);
    }

    #[test]
    fn a_worker_panic_keeps_the_other_answers() {
        let run = in_workers(9, |index| {
            assert_ne!(index, 4, "the worker for this request stopped");
            Some(index)
        });
        assert!(run.panicked);
        assert_eq!(
            run.values
                .iter()
                .map(|(index, _)| *index)
                .collect::<Vec<_>>(),
            [0, 1, 2, 3, 5, 6, 7, 8],
            "only the panicked request is missing"
        );
    }
}
