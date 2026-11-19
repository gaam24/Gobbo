//! The pacing clocks and API budgets that keep Gobbo inside a provider's request limits.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use crate::domain::rate::DownloadPolicy;

/// The request budget a provider reported in its response headers. Only the values needed to decide whether to wait
/// are kept; the raw headers never leave this layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimit {
    pub remaining: u64,
    pub reset: SystemTime,
}

/// The clock behind one provider's download policy. `Unlimited` returns at once; a window budget sleeps only when
/// the next start would exceed the ceiling, then forgets the starts that fell out of the window.
#[derive(Default)]
pub struct Pacer {
    starts: Mutex<VecDeque<Instant>>,
}

impl Pacer {
    /// Reserves a start, waiting until the window has room. No wait at all for an unlimited policy.
    pub fn wait(&self, policy: DownloadPolicy) {
        let DownloadPolicy::WindowBudget { max, window } = policy else {
            return;
        };
        loop {
            let mut starts = self.starts.lock().unwrap_or_else(PoisonError::into_inner);
            let now = Instant::now();
            while starts
                .front()
                .is_some_and(|start| now.duration_since(*start) >= window)
            {
                starts.pop_front();
            }
            if starts.len() < max as usize {
                starts.push_back(now);
                return;
            }
            let Some(oldest) = starts.front().copied() else {
                return;
            };
            let delay = window.saturating_sub(now.duration_since(oldest));
            drop(starts);
            if delay.is_zero() {
                continue;
            }
            std::thread::sleep(delay);
        }
    }
}

/// One clock per provider and account tier. Request pacing is not game state; it guards how fast Gobbo calls a
/// remote service, like the agent and the API client's limiter do. Thunderstore and paid accounts get their own
/// entries and never wait on a Nexus free account's ceiling.
static PACERS: OnceLock<Mutex<BTreeMap<String, Arc<Pacer>>>> = OnceLock::new();

/// The last budget a provider API reported. It is updated from response headers and consulted before the next
/// request: an exhausted budget waits until its reset, while a healthy one adds no delay at all.
#[derive(Default)]
pub struct ApiBudget {
    remaining: Mutex<Option<u64>>,
    reset: Mutex<Option<SystemTime>>,
}

impl ApiBudget {
    /// Records what the server said about the current window.
    pub fn update(&self, info: Option<RateLimit>) {
        if let Some(info) = info {
            *self
                .remaining
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(info.remaining);
            *self.reset.lock().unwrap_or_else(PoisonError::into_inner) = Some(info.reset);
        }
    }

    /// Waits only when the budget is actually exhausted. No headers yet, or requests left, means no wait.
    pub fn wait(&self) {
        let remaining = *self
            .remaining
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let reset = *self.reset.lock().unwrap_or_else(PoisonError::into_inner);
        if let (Some(0), Some(reset)) = (remaining, reset)
            && let Ok(delay) = reset.duration_since(SystemTime::now())
            && !delay.is_zero()
        {
            std::thread::sleep(delay + Duration::from_millis(200));
        }
    }
}

static API_BUDGETS: OnceLock<Mutex<BTreeMap<String, Arc<ApiBudget>>>> = OnceLock::new();

/// The budget for one provider API, keyed so one service never waits on another's quota.
pub fn api_budget(key: &str) -> Arc<ApiBudget> {
    API_BUDGETS
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry(key.to_string())
        .or_insert_with(|| Arc::new(ApiBudget::default()))
        .clone()
}

/// The clock for one pacing key. Callers build the key from the provider id and the account tier.
pub fn pacer(key: &str) -> Arc<Pacer> {
    PACERS
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry(key.to_string())
        .or_insert_with(|| Arc::new(Pacer::default()))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exhausted_api_budget_waits_but_a_healthy_one_does_not() {
        let budget = ApiBudget::default();
        let started = Instant::now();
        budget.wait();
        assert!(started.elapsed() < Duration::from_secs(1));

        budget.update(Some(RateLimit {
            remaining: 0,
            reset: SystemTime::now() + Duration::from_millis(60),
        }));
        let started = Instant::now();
        budget.wait();
        assert!(
            started.elapsed() >= Duration::from_millis(60),
            "an exhausted budget must wait for the reset the server announced"
        );

        budget.update(Some(RateLimit {
            remaining: 5,
            reset: SystemTime::now() + Duration::from_secs(60),
        }));
        let started = Instant::now();
        budget.wait();
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_window_budget_waits_only_after_its_ceiling_is_reached() {
        let policy = DownloadPolicy::WindowBudget {
            max: 2,
            window: Duration::from_millis(60),
        };
        let clock = Pacer::default();
        let started = Instant::now();
        clock.wait(policy);
        clock.wait(policy);
        assert!(started.elapsed() < Duration::from_millis(60));
        clock.wait(policy);
        assert!(
            started.elapsed() >= Duration::from_millis(60),
            "the third start must wait for the window to free a slot"
        );
    }

    #[test]
    fn an_unlimited_policy_never_waits() {
        let clock = Pacer::default();
        let started = Instant::now();
        for _ in 0..20 {
            clock.wait(DownloadPolicy::Unlimited);
        }
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn two_pacing_keys_do_not_share_a_clock() {
        let policy = DownloadPolicy::WindowBudget {
            max: 1,
            window: Duration::from_millis(60),
        };
        let first = pacer("example-one");
        let second = pacer("example-two");
        first.wait(policy);
        let started = Instant::now();
        second.wait(policy);
        assert!(
            started.elapsed() < Duration::from_millis(60),
            "a different key must not inherit another provider's used slots"
        );
    }
}
