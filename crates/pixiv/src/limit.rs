use std::{sync::Arc, time::Duration};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

#[derive(Clone)]
pub struct PixivRequestGate {
    semaphore: Arc<Semaphore>,
    rate: Option<Arc<RateGate>>,
}

impl PixivRequestGate {
    pub fn new(
        concurrency: usize,
        rate: Option<(u32, Duration)>,
    ) -> Result<Self, PixivRequestGateError> {
        if concurrency == 0 {
            return Err(PixivRequestGateError::InvalidConcurrency);
        }
        let rate = rate
            .map(|(requests, window)| RateGate::new(requests, window).map(Arc::new))
            .transpose()?;
        Ok(Self {
            semaphore: Arc::new(Semaphore::new(concurrency)),
            rate,
        })
    }

    pub async fn enter(&self) -> PixivRequestPermit {
        // Reserve concurrency first so requests waiting for a slot cannot accumulate
        // completed rate waits and start as a burst when a slot becomes available.
        let semaphore = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("Pixiv request gate is never closed");
        if let Some(rate) = &self.rate {
            rate.wait().await;
        }
        PixivRequestPermit {
            _semaphore: semaphore,
        }
    }
}

impl std::fmt::Debug for PixivRequestGate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PixivRequestGate")
            .field("available_permits", &self.semaphore.available_permits())
            .field("rate_limited", &self.rate.is_some())
            .finish()
    }
}

pub struct PixivRequestPermit {
    _semaphore: OwnedSemaphorePermit,
}

struct RateGate {
    interval: Duration,
    next_start: Mutex<tokio::time::Instant>,
}

impl RateGate {
    fn new(requests: u32, window: Duration) -> Result<Self, PixivRequestGateError> {
        if requests == 0 || window.is_zero() {
            return Err(PixivRequestGateError::InvalidRate);
        }
        let interval = window / requests;
        if interval.is_zero() {
            return Err(PixivRequestGateError::InvalidRate);
        }
        Ok(Self {
            interval,
            next_start: Mutex::new(tokio::time::Instant::now()),
        })
    }

    async fn wait(&self) {
        let scheduled = {
            let mut next_start = self.next_start.lock().await;
            let scheduled = (*next_start).max(tokio::time::Instant::now());
            *next_start = scheduled + self.interval;
            scheduled
        };
        tokio::time::sleep_until(scheduled).await;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PixivRequestGateError {
    #[error("Pixiv request concurrency must be positive")]
    InvalidConcurrency,
    #[error("Pixiv request rate must be positive and representable")]
    InvalidRate,
}
