use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

pub(super) type HandlerMetrics = BTreeMap<String, Arc<Metrics>>;
const WINDOW_SECONDS: u64 = 300;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Snapshot {
    pub window_seconds: u64,
    pub calls: u64,
    pub latency_min_ms: Option<f64>,
    pub latency_max_ms: Option<f64>,
    pub latency_mean_ms: Option<f64>,
    pub response_codes: BTreeMap<String, u64>,
}

#[derive(Default)]
pub(super) struct Metrics {
    window: Mutex<Window>,
}

impl Metrics {
    pub(super) fn record(&self, latency: Duration, code: u16) {
        let mut window = self.window.lock().expect("handler metrics");
        window.record(Instant::now(), latency, code);
    }

    pub(super) fn snapshot(&self) -> Snapshot {
        let mut window = self.window.lock().expect("handler metrics");
        window.snapshot(Instant::now())
    }
}

struct Window {
    origin: Instant,
    buckets: VecDeque<Bucket>,
}

struct Bucket {
    second: u64,
    calls: u64,
    total: Duration,
    min: Duration,
    max: Duration,
    codes: BTreeMap<u16, u64>,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
            buckets: VecDeque::new(),
        }
    }
}

impl Window {
    fn record(&mut self, now: Instant, latency: Duration, code: u16) {
        let second = self.expire(now);
        if self.buckets.back().is_none_or(|b| b.second != second) {
            self.buckets.push_back(Bucket {
                second,
                calls: 0,
                total: Duration::ZERO,
                min: latency,
                max: latency,
                codes: BTreeMap::new(),
            });
        }
        let bucket = self.buckets.back_mut().expect("current metrics bucket");
        bucket.calls += 1;
        bucket.total += latency;
        bucket.min = bucket.min.min(latency);
        bucket.max = bucket.max.max(latency);
        *bucket.codes.entry(code).or_default() += 1;
    }

    fn snapshot(&mut self, now: Instant) -> Snapshot {
        self.expire(now);
        let calls = self.buckets.iter().map(|b| b.calls).sum();
        let total: Duration = self.buckets.iter().map(|b| b.total).sum();
        let milliseconds = |duration: Duration| duration.as_secs_f64() * 1000.0;
        let mut response_codes = BTreeMap::new();
        for bucket in &self.buckets {
            for (&code, &count) in &bucket.codes {
                *response_codes.entry(code.to_string()).or_default() += count;
            }
        }
        Snapshot {
            window_seconds: WINDOW_SECONDS,
            calls,
            latency_min_ms: self.buckets.iter().map(|b| b.min).min().map(milliseconds),
            latency_max_ms: self.buckets.iter().map(|b| b.max).max().map(milliseconds),
            latency_mean_ms: (calls > 0).then(|| milliseconds(total) / calls as f64),
            response_codes,
        }
    }

    fn expire(&mut self, now: Instant) -> u64 {
        let second = now.duration_since(self.origin).as_secs();
        while self
            .buckets
            .front()
            .is_some_and(|b| second.saturating_sub(b.second) >= WINDOW_SECONDS)
        {
            self.buckets.pop_front();
        }
        second
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_aggregates_calls_and_expires_idle_buckets() {
        let mut window = Window::default();
        let now = Instant::now();
        let empty = window.snapshot(now);
        assert_eq!(empty.calls, 0);
        assert_eq!(empty.latency_min_ms, None);
        assert_eq!(empty.latency_max_ms, None);
        assert_eq!(empty.latency_mean_ms, None);
        assert!(empty.response_codes.is_empty());

        window.record(now, Duration::from_millis(2), 200);
        window.record(now, Duration::from_millis(10), 500);
        window.record(now + Duration::from_secs(1), Duration::from_millis(3), 200);
        let snapshot = window.snapshot(now + Duration::from_secs(299));
        assert_eq!(snapshot.calls, 3);
        assert_eq!(snapshot.latency_min_ms, Some(2.0));
        assert_eq!(snapshot.latency_max_ms, Some(10.0));
        assert_eq!(snapshot.latency_mean_ms, Some(5.0));
        assert_eq!(
            snapshot.response_codes,
            BTreeMap::from([("200".into(), 2), ("500".into(), 1)])
        );

        let snapshot = window.snapshot(now + Duration::from_secs(300));
        assert_eq!(snapshot.calls, 1);
        assert_eq!(snapshot.latency_min_ms, Some(3.0));
        assert_eq!(snapshot.latency_max_ms, Some(3.0));
        assert_eq!(snapshot.response_codes, BTreeMap::from([("200".into(), 1)]));
        let snapshot = window.snapshot(now + Duration::from_secs(301));
        assert_eq!(snapshot.calls, 0);
        assert_eq!(snapshot.latency_mean_ms, None);
        assert!(snapshot.response_codes.is_empty());

        window.record(now + Duration::from_secs(1000), Duration::ZERO, 204);
        let snapshot = window.snapshot(now + Duration::from_secs(1000));
        assert_eq!(snapshot.calls, 1);
        assert_eq!(snapshot.latency_min_ms, Some(0.0));
    }
}
