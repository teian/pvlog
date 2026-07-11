//! Bounded ingestion admission with retry timing and saturation metrics.

use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};
use thiserror::Error;

struct State {
    active: AtomicUsize,
    admitted: AtomicU64,
    rejected_concurrency: AtomicU64,
    rejected_lag: AtomicU64,
}
pub struct IngestionAdmission {
    state: Arc<State>,
    maximum_concurrent: usize,
    maximum_queue_lag: u64,
    retry_after_seconds: u32,
}
pub struct IngestionPermit {
    state: Arc<State>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IngestionAdmissionMetrics {
    pub active: usize,
    pub admitted: u64,
    pub rejected_concurrency: u64,
    pub rejected_queue_lag: u64,
}

impl IngestionAdmission {
    #[must_use]
    pub fn new(
        maximum_concurrent: usize,
        maximum_queue_lag: u64,
        retry_after_seconds: u32,
    ) -> Self {
        Self {
            state: Arc::new(State {
                active: AtomicUsize::new(0),
                admitted: AtomicU64::new(0),
                rejected_concurrency: AtomicU64::new(0),
                rejected_lag: AtomicU64::new(0),
            }),
            maximum_concurrent,
            maximum_queue_lag,
            retry_after_seconds,
        }
    }
    /// Attempts to admit durable ingestion work without queueing unbounded requests.
    /// # Errors
    /// Returns overload metadata when queue lag or concurrent work reaches its threshold.
    pub fn try_admit(&self, queue_lag: u64) -> Result<IngestionPermit, IngestionAdmissionError> {
        if queue_lag > self.maximum_queue_lag {
            self.state.rejected_lag.fetch_add(1, Ordering::Relaxed);
            return Err(IngestionAdmissionError::Overloaded {
                retry_after_seconds: self.retry_after_seconds,
                reason: "queue_lag",
            });
        }
        let admitted =
            self.state
                .active
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                    (active < self.maximum_concurrent).then_some(active + 1)
                });
        if admitted.is_err() {
            self.state
                .rejected_concurrency
                .fetch_add(1, Ordering::Relaxed);
            return Err(IngestionAdmissionError::Overloaded {
                retry_after_seconds: self.retry_after_seconds,
                reason: "concurrency",
            });
        }
        self.state.admitted.fetch_add(1, Ordering::Relaxed);
        Ok(IngestionPermit {
            state: self.state.clone(),
        })
    }
    #[must_use]
    pub fn metrics(&self) -> IngestionAdmissionMetrics {
        IngestionAdmissionMetrics {
            active: self.state.active.load(Ordering::Acquire),
            admitted: self.state.admitted.load(Ordering::Relaxed),
            rejected_concurrency: self.state.rejected_concurrency.load(Ordering::Relaxed),
            rejected_queue_lag: self.state.rejected_lag.load(Ordering::Relaxed),
        }
    }
}
impl Drop for IngestionPermit {
    fn drop(&mut self) {
        self.state.active.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum IngestionAdmissionError {
    #[error("ingestion capacity is saturated")]
    Overloaded {
        retry_after_seconds: u32,
        reason: &'static str,
    },
}
