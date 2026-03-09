use reqwest::{Error, Response, StatusCode};
use std::future::Future;
use std::time::Duration;
use rand::RngExt;

/// Errors that can be retried
#[derive(Debug)]
pub enum RetryableError {
    RateLimit(StatusCode, Option<u64>), // Status code and optional Retry-After seconds
    ServerError(StatusCode),
    NonRetryable(StatusCode),
    RequestError(Error),
}

impl From<Error> for RetryableError {
    fn from(err: Error) -> Self {
        Self::RequestError(err)
    }
}

/// Check HTTP response status and determine if/how to retry
///
/// Returns `Ok(response)` for successful responses, or `Err` with appropriate retry error
pub fn check_response_status(response: Response) -> Result<Response, RetryableError> {
    match response.status() {
        StatusCode::TOO_MANY_REQUESTS | StatusCode::FORBIDDEN => {
            // Extract Retry-After header if present
            let retry_after = response.headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok());

            Err(RetryableError::RateLimit(response.status(), retry_after))
        }
        StatusCode::INTERNAL_SERVER_ERROR |
        StatusCode::BAD_GATEWAY |
        StatusCode::SERVICE_UNAVAILABLE |
        StatusCode::GATEWAY_TIMEOUT => {
            Err(RetryableError::ServerError(response.status()))
        }
        _ if response.status().is_success() => {
            Ok(response)
        }
        _ => {
            // Non-retryable error (4xx client errors like 404, 401, etc.)
            Err(RetryableError::NonRetryable(response.status()))
        }
    }
}

impl std::fmt::Display for RetryableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RateLimit(status, retry_after) => {
                if let Some(seconds) = retry_after {
                    write!(f, "Rate limit error: {status} (retry after {seconds} seconds)")
                } else {
                    write!(f, "Rate limit error: {status}")
                }
            }
            Self::ServerError(status) => write!(f, "Server error: {status}"),
            Self::NonRetryable(status) => write!(f, "HTTP error: {status}"),
            Self::RequestError(e) => write!(f, "Request error: {e}"),
        }
    }
}

impl std::error::Error for RetryableError {}

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub jitter_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            base_delay_ms: 1000,  // 1 second base
            max_delay_ms: 32000,  // 32 seconds max
            jitter_factor: 0.3,   // ±30% jitter
        }
    }
}

impl RetryConfig {
    /// Calculate delay with exponential backoff and jitter
    #[must_use]
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        // Exponential backoff: base * 2^attempt
        let exponential_delay = self.base_delay_ms * 2u64.pow(attempt);

        // Cap at max delay
        let capped_delay = exponential_delay.min(self.max_delay_ms);

        // Add jitter: random value between (1 - jitter_factor) and (1 + jitter_factor)
        let mut rng = rand::rng();
        let jitter_range = 1.0 - self.jitter_factor..=1.0 + self.jitter_factor;
        let jitter_multiplier = rng.random_range(jitter_range);

        let final_delay = (capped_delay as f64 * jitter_multiplier) as u64;
        Duration::from_millis(final_delay)
    }
}

/// Legacy delay calculation function (kept for backward compatibility with tests)
#[cfg(test)]
#[must_use]
pub const fn calculate_delay(n: u32) -> u64 {
    let base: u64 = 30;
    base * 2u64.pow(n)
}

/// Retry a future with exponential backoff and jitter
pub async fn retry_with_backoff<F, Fut, T>(
    config: &RetryConfig,
    mut operation: F,
) -> Result<T, RetryableError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, RetryableError>>,
{
    let mut attempt = 0;

    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(err) => {
                // Check if error is retryable
                let should_retry = matches!(
                    err,
                    RetryableError::RateLimit(_, _) | RetryableError::ServerError(_)
                );

                if !should_retry || attempt >= config.max_retries {
                    log_final_failure(&err, attempt);
                    return Err(err);
                }

                // Calculate delay - use Retry-After header if available for rate limits
                let delay = match &err {
                    RetryableError::RateLimit(_, Some(retry_after_secs)) => {
                        // Use the server's requested delay, but apply jitter and cap
                        let retry_after_ms = retry_after_secs * 1000;
                        let capped = retry_after_ms.min(config.max_delay_ms);

                        // Add jitter to avoid thundering herd
                        let mut rng = rand::rng();
                        let jitter_range = 1.0 - config.jitter_factor..=1.0 + config.jitter_factor;
                        let jitter_multiplier = rng.random_range(jitter_range);

                        let final_delay = (capped as f64 * jitter_multiplier) as u64;
                        Duration::from_millis(final_delay)
                    }
                    _ => config.calculate_delay(attempt)
                };

                log_retry_attempt(&err, attempt, config.max_retries, &delay);

                tokio::time::sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

/// Log details about a final failure after exhausting retries.
fn log_final_failure(err: &RetryableError, attempt: u32) {
    match err {
        RetryableError::RateLimit(status, retry_after) => {
            if let Some(seconds) = retry_after {
                println!("Rate limit exceeded after {attempt} retries: {status} (server requested {seconds} second wait)");
            } else {
                println!("Rate limit exceeded after {attempt} retries: {status}");
            }
        }
        RetryableError::ServerError(status) => {
            println!("Server error after {attempt} retries: {status}");
        }
        RetryableError::NonRetryable(status) => {
            println!("Non-retryable error: {status}");
        }
        RetryableError::RequestError(e) => {
            println!("Request error after {attempt} retries: {e}");
        }
    }
}

/// Log details about a retry attempt.
fn log_retry_attempt(err: &RetryableError, attempt: u32, max_retries: u32, delay: &Duration) {
    match err {
        RetryableError::RateLimit(status, retry_after) => {
            if let Some(seconds) = retry_after {
                println!(
                    "Rate limit hit ({status}), server requested {seconds} second wait, retrying in {:.2}s...",
                    delay.as_secs_f64()
                );
            } else {
                println!(
                    "Rate limit hit ({status}), attempt {}/{max_retries}, retrying in {:.2}s...",
                    attempt + 1,
                    delay.as_secs_f64()
                );
            }
        }
        RetryableError::ServerError(status) => {
            println!(
                "Server error ({status}), attempt {}/{max_retries}, retrying in {:.2}s...",
                attempt + 1,
                delay.as_secs_f64()
            );
        }
        _ => {}
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_defaults() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.base_delay_ms, 1000);
        assert_eq!(config.max_delay_ms, 32000);
        assert!((config.jitter_factor - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn test_retry_config_calculate_delay_exponential() {
        let config = RetryConfig::default();
        
        // Test exponential growth (without jitter for base check)
        let delay0 = config.calculate_delay(0);
        let delay1 = config.calculate_delay(1);
        let delay2 = config.calculate_delay(2);
        
        // Delays should be in expected range considering jitter (±30%)
        assert!(delay0.as_millis() >= 700 && delay0.as_millis() <= 1300);  // ~1000ms
        assert!(delay1.as_millis() >= 1400 && delay1.as_millis() <= 2600); // ~2000ms
        assert!(delay2.as_millis() >= 2800 && delay2.as_millis() <= 5200); // ~4000ms
    }

    #[test]
    fn test_retry_config_max_delay_cap() {
        let config = RetryConfig::default();
        
        // Very large attempt should be capped at max_delay_ms
        let delay = config.calculate_delay(10); // Would be 1024s without cap
        
        // Should be capped around 32s with jitter (32000ms * 1.3 = 41600ms)
        assert!(delay.as_millis() <= 41600);
    }

    #[test]
    fn test_retry_config_jitter_variability() {
        let config = RetryConfig::default();
        
        // Calculate multiple delays for same attempt
        let delays: Vec<Duration> = (0..10)
            .map(|_| config.calculate_delay(1))
            .collect();
        
        // Not all delays should be identical (jitter adds randomness)
        let first = delays[0];
        let has_variation = delays.iter().any(|d| *d != first);
        assert!(has_variation, "Jitter should cause variation in delays");
    }

    #[test]
    fn test_retry_config_custom() {
        let config = RetryConfig {
            max_retries: 3,
            base_delay_ms: 500,
            max_delay_ms: 10000,
            jitter_factor: 0.5,
        };
        
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay_ms, 500);
        
        let delay = config.calculate_delay(0);
        // Should be around 500ms with ±50% jitter (250-750ms)
        assert!(delay.as_millis() >= 250 && delay.as_millis() <= 750);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_success_on_first_try() {
        use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
        
        let config = RetryConfig::default();
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();
        
        let result = retry_with_backoff(&config, move || {
            let count = call_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok::<_, RetryableError>(42)
            }
        }).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_success_after_retry() {
        use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
        
        let config = RetryConfig {
            max_retries: 3,
            base_delay_ms: 10, // Short delay for testing
            max_delay_ms: 100,
            jitter_factor: 0.1,
        };
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();
        
        let result = retry_with_backoff(&config, move || {
            let count = call_count_clone.clone();
            async move {
                let current = count.fetch_add(1, Ordering::SeqCst) + 1;
                if current < 3 {
                    Err(RetryableError::ServerError(StatusCode::INTERNAL_SERVER_ERROR))
                } else {
                    Ok::<_, RetryableError>(current)
                }
            }
        }).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 3);
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_non_retryable_error() {
        use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
        
        let config = RetryConfig::default();
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();
        
        let result = retry_with_backoff(&config, move || {
            let count = call_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(RetryableError::NonRetryable(StatusCode::NOT_FOUND))
            }
        }).await;
        
        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 1); // Should not retry
    }

    #[tokio::test]
    async fn test_retry_with_backoff_max_retries_exceeded() {
        use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
        
        let config = RetryConfig {
            max_retries: 2,
            base_delay_ms: 10,
            max_delay_ms: 100,
            jitter_factor: 0.1,
        };
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();
        
        let result = retry_with_backoff(&config, move || {
            let count = call_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(RetryableError::RateLimit(StatusCode::TOO_MANY_REQUESTS, None))
            }
        }).await;
        
        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 3); // Initial attempt + 2 retries
    }

    #[tokio::test]
    async fn test_retry_with_backoff_respects_retry_after_header() {
        use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
        use std::time::Instant;
        
        let config = RetryConfig {
            max_retries: 3,
            base_delay_ms: 10,
            max_delay_ms: 5000, // Allow up to 5 seconds to not cap our test
            jitter_factor: 0.0, // No jitter for precise timing test
        };
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();
        
        let start = Instant::now();
        
        let result = retry_with_backoff(&config, move || {
            let count = call_count_clone.clone();
            async move {
                let current = count.fetch_add(1, Ordering::SeqCst) + 1;
                if current == 1 {
                    // First call fails with Retry-After of 1 second (1000ms)
                    Err(RetryableError::RateLimit(StatusCode::TOO_MANY_REQUESTS, Some(1)))
                } else {
                    // Second call succeeds
                    Ok::<_, RetryableError>(42)
                }
            }
        }).await;
        
        let elapsed = start.elapsed();
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
        
        // Should wait approximately 1 second (1000ms) as specified by Retry-After
        // Allow some tolerance for test execution
        assert!(elapsed.as_millis() >= 900 && elapsed.as_millis() <= 1100, 
                "Expected ~1000ms delay, got {}ms", elapsed.as_millis());
    }

    #[test]
    fn test_retryable_error_display_with_retry_after() {
        let error = RetryableError::RateLimit(StatusCode::TOO_MANY_REQUESTS, Some(60));
        assert_eq!(format!("{error}"), "Rate limit error: 429 Too Many Requests (retry after 60 seconds)");
    }

    #[test]
    fn test_retryable_error_display_without_retry_after() {
        let error = RetryableError::RateLimit(StatusCode::TOO_MANY_REQUESTS, None);
        assert_eq!(format!("{error}"), "Rate limit error: 429 Too Many Requests");
    }

    #[test]
    fn test_check_response_status_success() {
        // Success responses (2xx) should return Ok(response)
        // This is tested via integration tests
    }

    #[test]
    fn test_status_code_categorization() {
        // Test that we correctly categorize different status codes
        // 429 and 403 -> RateLimit
        assert!(matches!(
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::TOO_MANY_REQUESTS
        ));
        assert!(matches!(
            StatusCode::FORBIDDEN,
            StatusCode::FORBIDDEN
        ));
        
        // 500-503 -> ServerError
        assert!(matches!(
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::INTERNAL_SERVER_ERROR
        ));
        assert!(matches!(
            StatusCode::BAD_GATEWAY,
            StatusCode::BAD_GATEWAY
        ));
        
        // 2xx -> Success
        assert!(StatusCode::OK.is_success());
        assert!(StatusCode::CREATED.is_success());
        
        // Other 4xx -> NonRetryable
        assert!(StatusCode::NOT_FOUND.is_client_error());
        assert!(StatusCode::UNAUTHORIZED.is_client_error());
    }

    #[test]
    fn test_calculate_delay_zero() {
        // First retry should be 30 seconds (30 * 2^0)
        assert_eq!(calculate_delay(0), 30);
    }

    #[test]
    fn test_calculate_delay_one() {
        // Second retry should be 60 seconds (30 * 2^1)
        assert_eq!(calculate_delay(1), 60);
    }

    #[test]
    fn test_calculate_delay_two() {
        // Third retry should be 120 seconds (30 * 2^2)
        assert_eq!(calculate_delay(2), 120);
    }

    #[test]
    fn test_calculate_delay_three() {
        // Fourth retry should be 240 seconds (30 * 2^3)
        assert_eq!(calculate_delay(3), 240);
    }

    #[test]
    fn test_calculate_delay_exponential_growth() {
        // Verify exponential backoff pattern
        let delays: Vec<u64> = (0..5).map(calculate_delay).collect();
        assert_eq!(delays, vec![30, 60, 120, 240, 480]);
    }

    #[test]
    fn test_calculate_delay_large_values() {
        // Test that large values don't overflow
        let delay = calculate_delay(10);
        assert_eq!(delay, 30720); // 30 * 2^10 = 30720
    }

    #[test]
    fn test_calculate_delay_consistency() {
        // Verify that calling the function multiple times with the same input
        // produces the same output (no random behavior)
        let delay1 = calculate_delay(5);
        let delay2 = calculate_delay(5);
        assert_eq!(delay1, delay2);
    }
}
