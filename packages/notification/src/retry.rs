use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff: Duration,
}

impl RetryPolicy {
    pub fn new(max_attempts: u32, backoff: Duration) -> Self {
        Self {
            max_attempts: max_attempts.max(1),
            backoff,
        }
    }

    pub fn no_retry() -> Self {
        Self::new(1, Duration::from_secs(0))
    }

    pub fn should_retry_after(&self, attempts_used: u32) -> bool {
        attempts_used < self.max_attempts
    }

    pub fn next_delay(&self, attempts_used: u32) -> Duration {
        let multiplier = attempts_used.saturating_sub(1).min(10);
        self.backoff
            .saturating_mul(2_u32.saturating_pow(multiplier))
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::new(3, Duration::from_secs(30))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_policy_stops_after_max_attempts() {
        let policy = RetryPolicy::new(3, Duration::from_millis(10));

        assert!(policy.should_retry_after(1));
        assert!(policy.should_retry_after(2));
        assert!(!policy.should_retry_after(3));
    }

    #[test]
    fn retry_policy_uses_bounded_exponential_delay() {
        let policy = RetryPolicy::new(3, Duration::from_millis(10));

        assert_eq!(policy.next_delay(1), Duration::from_millis(10));
        assert_eq!(policy.next_delay(2), Duration::from_millis(20));
        assert_eq!(policy.next_delay(3), Duration::from_millis(40));
    }
}
