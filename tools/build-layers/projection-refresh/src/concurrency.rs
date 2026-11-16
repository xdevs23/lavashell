// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::sync::{Mutex, PoisonError};
use std::thread;

fn next_job<Job>(queue: &Mutex<VecDeque<(usize, Job)>>) -> Option<(usize, Job)> {
    queue
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .pop_front()
}

fn cancel_waiting<Job>(queue: &Mutex<VecDeque<(usize, Job)>>) {
    queue.lock().unwrap_or_else(PoisonError::into_inner).clear();
}

pub fn run_concurrently<Output, Job>(jobs: Vec<Job>) -> Result<Vec<Output>, Error>
where
    Output: Send,
    Job: FnOnce() -> Result<Output, Error> + Send,
{
    let count = jobs.len();
    let workers = thread::available_parallelism()
        .map_or(1, NonZeroUsize::get)
        .min(count);
    let queue = Mutex::new(jobs.into_iter().enumerate().collect::<VecDeque<_>>());
    let finished: Vec<Vec<(usize, Result<Output, Error>)>> = thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut results = Vec::new();
                    while let Some((index, job)) = next_job(&queue) {
                        let result = job();
                        if result.is_err() {
                            cancel_waiting(&queue);
                        }
                        results.push((index, result));
                    }
                    results
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().map_err(|_panic| Error::RefreshThread))
            .collect::<Result<_, _>>()
    })?;
    let mut ordered: Vec<(usize, Result<Output, Error>)> = finished.into_iter().flatten().collect();
    ordered.sort_by_key(|(index, _)| *index);
    let outputs: Vec<Output> = ordered
        .into_iter()
        .map(|(_, result)| result)
        .collect::<Result<_, _>>()?;
    if outputs.len() == count {
        Ok(outputs)
    } else {
        Err(Error::RefreshThread)
    }
}
