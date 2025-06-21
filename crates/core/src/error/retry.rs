//! Retry logic and error recovery utilities

use super::YuhaError;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

/// Retry policy configuration
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial delay between retries
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Backoff multiplier
    pub backoff_multiplier: f64,
    /// Whether to add jitter to delays
    pub jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }
}

impl RetryPolicy {
    /// Create a new retry policy
    pub fn new(max_attempts: u32) -> Self {
        Self {
            max_attempts,
            ..Default::default()
        }
    }

    /// Set the initial delay
    pub fn with_initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    /// Set the maximum delay
    pub fn with_max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }

    /// Set the backoff multiplier
    pub fn with_backoff_multiplier(mut self, multiplier: f64) -> Self {
        self.backoff_multiplier = multiplier;
        self
    }

    /// Enable or disable jitter
    pub fn with_jitter(mut self, jitter: bool) -> Self {
        self.jitter = jitter;
        self
    }

    /// Calculate delay for a given attempt
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }

        let delay_ms = self.initial_delay.as_millis() as f64
            * self.backoff_multiplier.powi((attempt - 1) as i32);
        let delay = Duration::from_millis(delay_ms as u64).min(self.max_delay);

        if self.jitter {
            // Add up to 10% jitter
            let jitter_range = delay.as_millis() / 10;
            let jitter = fastrand::u64(0..=jitter_range as u64);
            delay + Duration::from_millis(jitter)
        } else {
            delay
        }
    }

    /// Check if an error should be retried
    pub fn should_retry(&self, error: &YuhaError, attempt: u32) -> bool {
        if attempt >= self.max_attempts {
            return false;
        }

        error.is_retriable()
    }
}

/// Retry a future operation according to the retry policy
pub async fn retry_async<F, Fut, T>(mut operation: F, policy: &RetryPolicy) -> Result<T, YuhaError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, YuhaError>>,
{
    let mut last_error = None;

    for attempt in 0..=policy.max_attempts {
        if attempt > 0 {
            let delay = policy.delay_for_attempt(attempt);
            debug!(
                "Retrying operation (attempt {}/{}) after {:?}",
                attempt, policy.max_attempts, delay
            );
            sleep(delay).await;
        }

        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!("Operation succeeded after {} retries", attempt);
                }
                return Ok(result);
            }
            Err(error) => {
                if !policy.should_retry(&error, attempt) {
                    warn!("Operation failed and will not be retried: {}", error);
                    return Err(error);
                }
                warn!(
                    "Operation failed (attempt {}/{}): {}",
                    attempt + 1,
                    policy.max_attempts,
                    error
                );
                last_error = Some(error);
            }
        }
    }

    // All retries exhausted
    Err(last_error.unwrap_or_else(|| YuhaError::internal("Retry operation failed without error")))
}

/// Retry with exponential backoff
pub async fn retry_with_backoff<F, Fut, T>(operation: F, max_attempts: u32) -> Result<T, YuhaError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, YuhaError>>,
{
    let policy = RetryPolicy::new(max_attempts);
    retry_async(operation, &policy).await
}

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

/// Circuit breaker for preventing cascading failures
#[derive(Debug)]
pub struct CircuitBreaker {
    state: CircuitState,
    failure_count: u32,
    failure_threshold: u32,
    success_threshold: u32,
    timeout: Duration,
    last_failure_time: Option<std::time::Instant>,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, timeout: Duration) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            failure_threshold,
            success_threshold: 1,
            timeout,
            last_failure_time: None,
        }
    }

    /// Check if the circuit breaker allows the operation
    pub fn can_execute(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                if let Some(last_failure) = self.last_failure_time {
                    if last_failure.elapsed() >= self.timeout {
                        self.state = CircuitState::HalfOpen;
                        true
                    } else {
                        false
                    }
                } else {
                    true
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Record a successful operation
    pub fn record_success(&mut self) {
        match self.state {
            CircuitState::HalfOpen => {
                self.state = CircuitState::Closed;
                self.failure_count = 0;
            }
            CircuitState::Closed => {
                self.failure_count = 0;
            }
            CircuitState::Open => {}
        }
    }

    /// Record a failed operation
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure_time = Some(std::time::Instant::now());

        if self.failure_count >= self.failure_threshold {
            self.state = CircuitState::Open;
        }
    }

    /// Get the current state
    pub fn state(&self) -> CircuitState {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_policy_delay_calculation() {
        let policy = RetryPolicy::default().with_jitter(false);
        assert_eq!(policy.delay_for_attempt(0), Duration::ZERO);
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(100));
        assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(200));
        assert_eq!(policy.delay_for_attempt(3), Duration::from_millis(400));
    }

    #[test]
    fn test_circuit_breaker() {
        let mut breaker = CircuitBreaker::new(2, Duration::from_millis(100));

        assert_eq!(breaker.state(), CircuitState::Closed);
        assert!(breaker.can_execute());

        // Record failures
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Closed);

        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);
        assert!(!breaker.can_execute());
        // Record success to close circuit
        breaker.record_success();
        // Circuit should still be open since we recorded success in open state
        assert_eq!(breaker.state(), CircuitState::Open);
    }
}
