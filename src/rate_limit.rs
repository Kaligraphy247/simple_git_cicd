use std::{collections::HashMap, time::Instant};

pub struct RateLimiter {
    /// tracks timestamps per project
    requests: HashMap<String, Vec<Instant>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            requests: HashMap::new(),
        }
    }

    /// Checks if the given key has exceeded the rate limit within the specified time window.
    /// Returns `true` if the rate limit is exceeded, `false` otherwise.
    pub fn check_rate_limit(&mut self, key: &str, max: usize, window_secs: u64) -> bool {
        let now = Instant::now();
        let window_duration = std::time::Duration::from_secs(window_secs);

        let timestamps = self
            .requests
            .entry(key.to_string())
            .or_insert_with(Vec::new);

        // Remove timestamps older than window
        timestamps.retain(|&t| now.duration_since(t) < window_duration);

        if timestamps.len() < max {
            timestamps.push(now);
            false
        } else {
            true
        }
    }
}
