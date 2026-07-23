use std::collections::HashSet;
use std::hash::Hash;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, OnceLock};

#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;

/// One unique CPU preparation job with its stable collection order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct IndexedJob<K, I> {
    pub(super) order: usize,
    pub(super) key: K,
    pub(super) input: I,
}

impl<K, I> IndexedJob<K, I> {
    pub(super) fn new(order: usize, key: K, input: I) -> Self {
        Self { order, key, input }
    }
}

/// An owned CPU preparation result associated with its source job.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PreparedResult<K, O> {
    order: usize,
    key: K,
    output: O,
}

impl<K, O> PreparedResult<K, O> {
    pub(super) fn new(order: usize, key: K, output: O) -> Self {
        Self { order, key, output }
    }
}

/// Validation error returned before any prepared results are committed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct InvalidPreparedResults;

/// Failure returned when any CPU job fails before the batch can be committed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct BatchExecutionError;

/// Synchronous CPU batch executor with bounded native parallelism.
///
/// Native batches above the serial threshold run on one process-wide Rayon
/// pool. WASM, one-worker environments, small batches, and pool construction
/// failures use direct iteration through the same owned job/result contract.
pub(super) struct BatchExecutor {
    #[cfg(not(target_arch = "wasm32"))]
    workers: usize,
    #[cfg(not(target_arch = "wasm32"))]
    serial_threshold: usize,
    #[cfg(not(target_arch = "wasm32"))]
    pool: Option<Arc<rayon::ThreadPool>>,
}

impl BatchExecutor {
    const SERIAL_THRESHOLD: usize = 4;
    #[cfg(any(target_os = "android", target_os = "ios"))]
    const MAX_WORKERS: usize = 2;
    #[cfg(all(
        not(target_arch = "wasm32"),
        not(any(target_os = "android", target_os = "ios"))
    ))]
    const MAX_WORKERS: usize = 4;

    pub(super) fn new() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let workers = std::thread::available_parallelism()
            .map(|parallelism| {
                parallelism
                    .get()
                    .saturating_sub(1)
                    .max(1)
            })
            .unwrap_or(1)
            .min(Self::MAX_WORKERS);
        #[cfg(target_arch = "wasm32")]
        let workers = 1;
        #[cfg(not(target_arch = "wasm32"))]
        {
            static POOL: OnceLock<Option<Arc<rayon::ThreadPool>>> = OnceLock::new();
            let pool = POOL
                .get_or_init(|| Self::build_pool(workers))
                .clone();
            Self {
                workers,
                serial_threshold: Self::SERIAL_THRESHOLD,
                pool,
            }
        }
        #[cfg(target_arch = "wasm32")]
        Self::with_configuration(workers, Self::SERIAL_THRESHOLD)
    }

    #[cfg(test)]
    fn for_test(workers: usize, serial_threshold: usize) -> Self {
        Self::with_configuration(workers, serial_threshold)
    }

    #[cfg(all(not(target_arch = "wasm32"), test))]
    fn with_configuration(workers: usize, serial_threshold: usize) -> Self {
        let workers = workers.max(1);
        let pool = Self::build_pool(workers);

        Self {
            workers,
            serial_threshold,
            pool,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn build_pool(workers: usize) -> Option<Arc<rayon::ThreadPool>> {
        (workers > 1)
            .then(|| {
                rayon::ThreadPoolBuilder::new()
                    .num_threads(workers)
                    .thread_name(|index| format!("cupid-text-{index}"))
                    .build()
                    .ok()
                    .map(Arc::new)
            })
            .flatten()
    }

    #[cfg(target_arch = "wasm32")]
    fn with_configuration(_workers: usize, _serial_threshold: usize) -> Self {
        Self {}
    }

    pub(super) fn execute_with_context<K, I, O, C, MakeContext, Prepare>(
        &self,
        jobs: &[IndexedJob<K, I>],
        make_context: MakeContext,
        prepare: Prepare,
    ) -> Result<Vec<PreparedResult<K, O>>, BatchExecutionError>
    where
        K: Clone + Send + Sync,
        I: Sync,
        O: Send,
        C: Send,
        MakeContext: Fn() -> C + Send + Sync,
        Prepare: Fn(&mut C, &IndexedJob<K, I>) -> Option<O> + Send + Sync,
    {
        if jobs.is_empty() {
            return Ok(Vec::new());
        }

        #[cfg(not(target_arch = "wasm32"))]
        if jobs.len() >= self.serial_threshold
            && self.workers > 1
            && let Some(pool) = &self.pool
        {
            return pool.install(|| {
                jobs.par_iter()
                    .map_init(&make_context, |context, job| {
                        prepare(context, job)
                            .map(|output| PreparedResult::new(job.order, job.key.clone(), output))
                    })
                    .collect::<Option<Vec<_>>>()
                    .ok_or(BatchExecutionError)
            });
        }

        let mut context = make_context();
        jobs.iter()
            .map(|job| {
                prepare(&mut context, job)
                    .map(|output| PreparedResult::new(job.order, job.key.clone(), output))
                    .ok_or(BatchExecutionError)
            })
            .collect()
    }
}

/// Collects unique preparation jobs while retaining first-seen source order.
pub(super) struct PreparationBatch<K, I> {
    keys: HashSet<K>,
    jobs: Vec<IndexedJob<K, I>>,
}

impl<K, I> PreparationBatch<K, I>
where
    K: Clone + Eq + Hash,
{
    pub(super) fn new() -> Self {
        Self {
            keys: HashSet::new(),
            jobs: Vec::new(),
        }
    }

    pub(super) fn push(&mut self, key: K, input: I) {
        if !self.keys.insert(key.clone()) {
            return;
        }

        self.jobs
            .push(IndexedJob::new(self.jobs.len(), key, input));
    }

    pub(super) fn jobs(&self) -> &[IndexedJob<K, I>] {
        &self.jobs
    }

    /// Validates and orders a complete result set before exposing any output.
    pub(super) fn merge<O>(
        &self,
        results: Vec<PreparedResult<K, O>>,
    ) -> Result<Vec<(K, O)>, InvalidPreparedResults> {
        if results.len() != self.jobs.len() {
            return Err(InvalidPreparedResults);
        }

        let mut ordered = std::iter::repeat_with(|| None)
            .take(self.jobs.len())
            .collect::<Vec<_>>();
        for result in results {
            let Some(job) = self.jobs.get(result.order) else {
                return Err(InvalidPreparedResults);
            };
            if result.key != job.key || ordered[result.order].is_some() {
                return Err(InvalidPreparedResults);
            }
            let order = result.order;
            ordered[order] = Some(result);
        }

        let mut by_order = Vec::with_capacity(ordered.len());
        for job in &self.jobs {
            let Some(result) = ordered[job.order].take() else {
                return Err(InvalidPreparedResults);
            };
            by_order.push((result.key, result.output));
        }

        Ok(by_order)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::{BatchExecutor, IndexedJob, PreparationBatch, PreparedResult};

    #[test]
    fn collection_preserves_first_seen_request_and_span_order() {
        let mut batch = PreparationBatch::new();

        batch.push("request-0/span-0", 10);
        batch.push("request-0/span-1", 11);
        batch.push("request-1/span-0", 20);

        let jobs = batch.jobs();
        assert_eq!(jobs.len(), 3);
        assert_eq!(jobs[0], IndexedJob::new(0, "request-0/span-0", 10));
        assert_eq!(jobs[1], IndexedJob::new(1, "request-0/span-1", 11));
        assert_eq!(jobs[2], IndexedJob::new(2, "request-1/span-0", 20));
    }

    #[test]
    fn collection_eliminates_duplicate_cache_keys() {
        let mut batch = PreparationBatch::new();

        batch.push("shared", 10);
        batch.push("unique", 20);
        batch.push("shared", 30);

        assert_eq!(
            batch.jobs(),
            &[
                IndexedJob::new(0, "shared", 10),
                IndexedJob::new(1, "unique", 20),
            ]
        );
    }

    #[test]
    fn empty_batch_merges_to_an_empty_commit() {
        let batch = PreparationBatch::<&str, i32>::new();

        assert_eq!(
            batch.merge(Vec::<PreparedResult<&str, i32>>::new()),
            Ok(vec![])
        );
    }

    #[test]
    fn merge_is_ordered_and_all_or_nothing() {
        let mut batch = PreparationBatch::new();
        batch.push("first", 10);
        batch.push("second", 20);

        let reversed = vec![
            PreparedResult::new(1, "second", 200),
            PreparedResult::new(0, "first", 100),
        ];
        assert_eq!(
            batch.merge(reversed),
            Ok(vec![("first", 100), ("second", 200)])
        );

        let incomplete = vec![PreparedResult::new(0, "first", 100)];
        assert!(
            batch
                .merge(incomplete)
                .is_err()
        );

        let duplicate = vec![
            PreparedResult::new(0, "first", 100),
            PreparedResult::new(0, "first", 101),
        ];
        assert!(
            batch
                .merge(duplicate)
                .is_err()
        );
    }

    #[test]
    fn executor_merges_out_of_order_work_in_source_order() {
        let executor = BatchExecutor::for_test(2, 1);
        let mut batch = PreparationBatch::new();
        batch.push("slow", 30_u64);
        batch.push("fast", 0_u64);

        let results = executor
            .execute_with_context(
                batch.jobs(),
                || (),
                |_, job| {
                    std::thread::sleep(Duration::from_millis(job.input));
                    Some(job.input)
                },
            )
            .unwrap();

        assert_eq!(batch.merge(results), Ok(vec![("slow", 30), ("fast", 0)]));
    }

    #[test]
    fn executor_uses_serial_path_below_threshold() {
        let executor = BatchExecutor::for_test(4, 4);
        let mut batch = PreparationBatch::new();
        batch.push("one", 1);
        batch.push("two", 2);
        batch.push("three", 3);
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        executor
            .execute_with_context(batch.jobs(), || (), {
                let active = active.clone();
                let peak = peak.clone();
                move |_, _| {
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(current, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(2));
                    active.fetch_sub(1, Ordering::SeqCst);
                    Some(())
                }
            })
            .unwrap();

        assert_eq!(peak.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn executor_failure_exposes_no_partial_result_set() {
        let executor = BatchExecutor::for_test(2, 1);
        let mut batch = PreparationBatch::new();
        batch.push("good", 1);
        batch.push("bad", 2);

        let result = executor.execute_with_context(
            batch.jobs(),
            || (),
            |_, job| (job.key != "bad").then_some(job.input),
        );

        assert!(result.is_err());
    }

    #[test]
    fn serial_and_parallel_execution_produce_identical_ordered_results() {
        let serial = BatchExecutor::for_test(1, 1);
        let parallel = BatchExecutor::for_test(4, 1);
        let mut batch = PreparationBatch::new();
        for value in 0..16 {
            batch.push(value, value * 2);
        }

        let execute = |executor: &BatchExecutor| {
            let results = executor
                .execute_with_context(
                    batch.jobs(),
                    || 10,
                    |context, job| Some(*context + job.input),
                )
                .unwrap();
            batch.merge(results).unwrap()
        };

        assert_eq!(execute(&parallel), execute(&serial));
    }

    #[test]
    fn empty_batch_does_not_construct_a_worker_context() {
        let executor = BatchExecutor::for_test(4, 1);
        let contexts = AtomicUsize::new(0);
        let jobs = Vec::<IndexedJob<i32, i32>>::new();

        let results = executor
            .execute_with_context(
                &jobs,
                || {
                    contexts.fetch_add(1, Ordering::SeqCst);
                },
                |_, _| Some(()),
            )
            .unwrap();

        assert!(results.is_empty());
        assert_eq!(contexts.load(Ordering::SeqCst), 0);
    }
}
