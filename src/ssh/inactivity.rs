//! Session inactivity monitor — detects long-running commands and notifies.
//!
//! Monitors terminal output for shell prompts and sends desktop notifications
//! when a command finishes after a threshold of inactivity.

use std::time::{Duration, Instant};

/// Detector for command completion in terminal output.
pub struct InactivityMonitor {
    /// How long to wait without output before considering a command "finished"
    pub threshold: Duration,
    /// Last time output was received
    last_output: Instant,
    /// Whether we're waiting for a command to finish
    waiting: bool,
    /// The last line of output (to detect prompt patterns)
    last_line: String,
}

impl InactivityMonitor {
    pub fn new(threshold_secs: u64) -> Self {
        Self {
            threshold: Duration::from_secs(threshold_secs),
            last_output: Instant::now(),
            waiting: false,
            last_line: String::new(),
        }
    }

    /// Feed a chunk of terminal output.
    /// Returns Some(message) if a command appears to have finished.
    pub fn feed(&mut self, data: &[u8]) -> Option<String> {
        self.last_output = Instant::now();
        self.waiting = true;

        // Track last line for prompt detection
        if let Ok(text) = std::str::from_utf8(data) {
            for line in text.lines() {
                if !line.trim().is_empty() {
                    self.last_line = line.to_string();
                }
            }
        }

        None
    }

    /// Check if the session has been inactive long enough to consider a command "done".
    pub fn check(&self) -> Option<String> {
        if !self.waiting {
            return None;
        }

        let elapsed = self.last_output.elapsed();
        if elapsed >= self.threshold {
            // Reset for next command
            // self.waiting = false;  (caller should recreate or reset)

            // Detect prompt patterns
            let summary = if self.last_line.contains('$')
                || self.last_line.contains('#')
                || self.last_line.contains('>')
            {
                format!("Command finished: {}", self.last_line)
            } else {
                "Command finished — terminal inactive".to_string()
            };

            return Some(summary);
        }

        None
    }

    /// Reset the monitor for a new command.
    pub fn reset(&mut self) {
        self.last_output = Instant::now();
        self.waiting = false;
        self.last_line.clear();
    }
}

/// Send a desktop notification (cross-platform).
#[cfg(not(test))]
#[cfg(feature = "gui")]
pub fn send_notification(title: &str, body: &str) {
    let _ = notify_rust::Notification::new()
        .summary(title)
        .body(body)
        .appname("ShellMounter")
        .timeout(notify_rust::Timeout::Milliseconds(5000))
        .show();
}

#[cfg(not(feature = "gui"))]
#[cfg(feature = "gui")]
pub fn send_notification(_title: &str, _body: &str) {
    // No-op in tests
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(not(feature = "gui"))]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_feed_updates_timestamp() {
        let mut monitor = InactivityMonitor::new(1);
        assert!(monitor.check().is_none());

        monitor.feed(b"ls -la\n");
        assert!(monitor.check().is_none(), "immediately after feed");
    }

    #[test]
    fn test_timeout_detection() {
        let mut monitor = InactivityMonitor::new(0); // zero threshold for testing
        monitor.feed(b"echo done\n$ ");
        sleep(Duration::from_millis(10));
        assert!(monitor.check().is_some(), "should detect inactivity");
    }

    #[test]
    fn test_prompt_detection() {
        let mut monitor = InactivityMonitor::new(0);
        monitor.feed(b"total 42\nuser@host:~$ ");
        sleep(Duration::from_millis(10));

        let result = monitor.check();
        assert!(result.is_some());
        assert!(result.unwrap().contains("user@host:~$"));
    }

    #[test]
    fn test_reset() {
        let mut monitor = InactivityMonitor::new(0);
        monitor.feed(b"data\n");
        sleep(Duration::from_millis(10));
        assert!(monitor.check().is_some());

        monitor.reset();
        assert!(monitor.check().is_none(), "after reset");
    }
}
