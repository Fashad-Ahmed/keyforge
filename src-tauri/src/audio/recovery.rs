use std::time::Duration;

use super::AudioEngineStatus;

const RETRY_DELAYS: [Duration; 6] = [
    Duration::from_millis(100),
    Duration::from_millis(250),
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
];

pub(crate) struct RecoveryController {
    status: AudioEngineStatus,
    consecutive_failures: usize,
    next_attempt_at: Duration,
    was_ready: bool,
    stopped: bool,
}

impl RecoveryController {
    pub(crate) fn new(now: Duration) -> Self {
        Self {
            status: AudioEngineStatus::Starting,
            consecutive_failures: 0,
            next_attempt_at: now,
            was_ready: false,
            stopped: false,
        }
    }

    pub(crate) fn attempt_due(&self, now: Duration) -> bool {
        !self.stopped && self.status != AudioEngineStatus::Ready && now >= self.next_attempt_at
    }

    pub(crate) fn opened(&mut self) {
        if self.stopped {
            return;
        }
        self.status = AudioEngineStatus::Ready;
        self.consecutive_failures = 0;
        self.was_ready = true;
    }

    pub(crate) fn open_failed(&mut self, now: Duration) {
        if self.stopped {
            return;
        }
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.status = if self.consecutive_failures >= 5 {
            AudioEngineStatus::Unavailable
        } else if self.was_ready {
            AudioEngineStatus::Recovering
        } else {
            AudioEngineStatus::Starting
        };
        let index = self
            .consecutive_failures
            .saturating_sub(1)
            .min(RETRY_DELAYS.len() - 1);
        self.next_attempt_at = now.saturating_add(RETRY_DELAYS[index]);
    }

    pub(crate) fn stream_lost(&mut self, now: Duration) {
        if self.stopped {
            return;
        }
        self.status = AudioEngineStatus::Recovering;
        self.consecutive_failures = 0;
        self.next_attempt_at = now;
        self.was_ready = true;
    }

    pub(crate) fn stop(&mut self) {
        self.stopped = true;
        self.status = AudioEngineStatus::Stopped;
    }

    pub(crate) fn status(&self) -> AudioEngineStatus {
        self.status
    }

    pub(crate) fn next_attempt_at(&self) -> Duration {
        self.next_attempt_at
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn follows_exact_bounded_retry_schedule() {
        let mut recovery = RecoveryController::new(Duration::ZERO);
        let expected = [
            Duration::from_millis(100),
            Duration::from_millis(250),
            Duration::from_millis(500),
            Duration::from_secs(1),
            Duration::from_secs(2),
            Duration::from_secs(5),
            Duration::from_secs(5),
        ];
        let mut now = Duration::ZERO;
        for delay in expected {
            assert!(recovery.attempt_due(now));
            recovery.open_failed(now);
            assert_eq!(recovery.next_attempt_at(), now + delay);
            now += delay;
        }
        assert_eq!(recovery.status(), AudioEngineStatus::Unavailable);
    }

    #[test]
    fn successful_open_resets_failures_and_stream_loss_recovers_immediately() {
        let mut recovery = RecoveryController::new(Duration::ZERO);
        recovery.open_failed(Duration::ZERO);
        recovery.opened();
        assert_eq!(recovery.status(), AudioEngineStatus::Ready);
        recovery.stream_lost(Duration::from_secs(10));
        assert_eq!(recovery.status(), AudioEngineStatus::Recovering);
        assert!(recovery.attempt_due(Duration::from_secs(10)));
        recovery.open_failed(Duration::from_secs(10));
        assert_eq!(
            recovery.next_attempt_at(),
            Duration::from_millis(100) + Duration::from_secs(10)
        );
    }

    #[test]
    fn stop_is_terminal() {
        let mut recovery = RecoveryController::new(Duration::ZERO);
        recovery.stop();
        assert_eq!(recovery.status(), AudioEngineStatus::Stopped);
        assert!(!recovery.attempt_due(Duration::from_secs(100)));
    }

    #[test]
    fn stop_ignores_all_subsequent_events() {
        let mut recovery = RecoveryController::new(Duration::ZERO);
        recovery.stop();
        recovery.opened();
        assert_eq!(recovery.status(), AudioEngineStatus::Stopped);
        assert!(!recovery.attempt_due(Duration::from_secs(100)));

        let mut recovery = RecoveryController::new(Duration::ZERO);
        recovery.stop();
        recovery.open_failed(Duration::from_secs(10));
        assert_eq!(recovery.status(), AudioEngineStatus::Stopped);
        assert!(!recovery.attempt_due(Duration::from_secs(100)));

        let mut recovery = RecoveryController::new(Duration::ZERO);
        recovery.stop();
        recovery.stream_lost(Duration::from_secs(10));
        assert_eq!(recovery.status(), AudioEngineStatus::Stopped);
        assert!(!recovery.attempt_due(Duration::from_secs(100)));
    }
}
