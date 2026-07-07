use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

const CANCEL_MESSAGE: &str = "Login cancelled";
const TIMEOUT_MESSAGE: &str = "Device flow timed out";
const SLOW_DOWN_TIMEOUT_MESSAGE: &str = "Device flow timed out after one or more slow_down responses. This is often caused by clock drift in WSL or VM environments. Please sync or restart the VM clock and try again.";
const MINIMUM_INTERVAL_MS: u64 = 1000;
const DEFAULT_POLL_INTERVAL_SECONDS: u64 = 5;
const SLOW_DOWN_INTERVAL_INCREMENT_MS: u64 = 5000;

#[derive(Debug)]
pub enum DeviceCodePollResult<T> {
    Pending,
    SlowDown { interval_seconds: Option<u64> },
    Failed { message: String },
    Complete(T),
}

type DeviceCodePollFn<T> = Box<dyn Fn() -> Pin<Box<dyn Future<Output = DeviceCodePollResult<T>> + Send>> + Send>;

pub struct DeviceCodePollOptions<T> {
    pub interval_seconds: Option<u64>,
    pub expires_in_seconds: Option<u64>,
    pub wait_before_first_poll: bool,
    pub poll: DeviceCodePollFn<T>,
}

pub async fn poll_oauth_device_code_flow<T>(options: DeviceCodePollOptions<T>) -> anyhow::Result<T> {
    let deadline = options
        .expires_in_seconds
        .map(|s| tokio::time::Instant::now() + Duration::from_secs(s))
        .unwrap_or(tokio::time::Instant::now() + Duration::from_secs(3600 * 24));

    let mut interval_ms =
        (options.interval_seconds.unwrap_or(DEFAULT_POLL_INTERVAL_SECONDS) * 1000).max(MINIMUM_INTERVAL_MS);

    let mut slow_down_responses = 0u32;
    let poll = options.poll;

    if options.wait_before_first_poll {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if !remaining.is_zero() {
            tokio::time::sleep(remaining.min(Duration::from_millis(interval_ms))).await;
        }
    }

    while tokio::time::Instant::now() < deadline {
        match (poll)().await {
            DeviceCodePollResult::Complete(value) => return Ok(value),
            DeviceCodePollResult::Failed { message } => return Err(anyhow::anyhow!(message)),
            DeviceCodePollResult::SlowDown { interval_seconds } => {
                slow_down_responses += 1;
                interval_ms = interval_seconds
                    .filter(|s| *s > 0)
                    .map(|s| (s * 1000).max(MINIMUM_INTERVAL_MS))
                    .unwrap_or(interval_ms + SLOW_DOWN_INTERVAL_INCREMENT_MS);
            }
            DeviceCodePollResult::Pending => {}
        }

        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        tokio::time::sleep(remaining.min(Duration::from_millis(interval_ms))).await;
    }

    if slow_down_responses > 0 {
        Err(anyhow::anyhow!(SLOW_DOWN_TIMEOUT_MESSAGE))
    } else {
        Err(anyhow::anyhow!(TIMEOUT_MESSAGE))
    }
}

#[allow(dead_code)]
pub fn login_cancelled() -> anyhow::Error {
    anyhow::anyhow!(CANCEL_MESSAGE)
}
