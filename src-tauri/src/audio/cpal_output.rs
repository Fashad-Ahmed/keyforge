use std::{
    fmt::Debug,
    sync::{atomic::Ordering, Arc},
    thread::Thread,
    time::Duration,
};

use super::{recovery::RecoveryController, SharedState};

const DEVICE_CHECK_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BackendFailure;

pub(crate) trait OutputBackend {
    type Stream;
    type DeviceToken: Clone + Debug + Eq;

    fn default_device_token(&mut self) -> Result<Option<Self::DeviceToken>, BackendFailure>;
    fn open_default_stream(
        &mut self,
        shared: Arc<SharedState>,
        supervisor: Thread,
    ) -> Result<(Self::Stream, Self::DeviceToken), BackendFailure>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SupervisorStep {
    Continue,
    Stop,
}

pub(crate) struct OutputSupervisor<B: OutputBackend> {
    backend: B,
    shared: Arc<SharedState>,
    recovery: RecoveryController,
    stream: Option<B::Stream>,
    device_token: Option<B::DeviceToken>,
    next_device_check: Duration,
}

impl<B: OutputBackend> OutputSupervisor<B> {
    pub(crate) fn new(backend: B, shared: Arc<SharedState>, now: Duration) -> Self {
        Self {
            backend,
            shared,
            recovery: RecoveryController::new(now),
            stream: None,
            device_token: None,
            next_device_check: now,
        }
    }

    pub(crate) fn step(&mut self, now: Duration, current_thread: Thread) -> SupervisorStep {
        if self.shared.shutdown.load(Ordering::Acquire) {
            self.stream = None;
            self.device_token = None;
            self.shared.clear_commands();
            self.recovery.stop();
            self.shared.set_status(self.recovery.status());
            return SupervisorStep::Stop;
        }

        if self.shared.stream_failed.swap(false, Ordering::AcqRel) && self.stream.is_some() {
            self.stream = None;
            self.device_token = None;
            self.shared.clear_commands();
            self.recovery.stream_lost(now);
        }

        if self.stream.is_some() && now >= self.next_device_check {
            let device_changed = match self.backend.default_device_token() {
                Ok(Some(token)) => self.device_token.as_ref() != Some(&token),
                Ok(None) => true,
                Err(BackendFailure) => false,
            };
            if device_changed {
                self.stream = None;
                self.device_token = None;
                self.shared.clear_commands();
                self.recovery.stream_lost(now);
            }
            self.next_device_check = now.saturating_add(DEVICE_CHECK_INTERVAL);
        }

        if self.stream.is_none() && self.recovery.attempt_due(now) {
            self.shared.clear_commands();
            match self
                .backend
                .open_default_stream(Arc::clone(&self.shared), current_thread)
            {
                Ok((stream, token)) => {
                    self.stream = Some(stream);
                    self.device_token = Some(token);
                    self.recovery.opened();
                    self.next_device_check = now.saturating_add(DEVICE_CHECK_INTERVAL);
                }
                Err(BackendFailure) => self.recovery.open_failed(now),
            }
        }

        self.shared.set_status(self.recovery.status());
        SupervisorStep::Continue
    }

    pub(crate) fn wait_duration(&self, now: Duration) -> Duration {
        let deadline = if self.stream.is_some() {
            self.next_device_check
        } else {
            self.recovery.next_attempt_at()
        };
        deadline.saturating_sub(now).min(DEVICE_CHECK_INTERVAL)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{atomic::Ordering, Arc},
        time::Duration,
    };

    use super::*;
    use crate::audio::{AudioEngineHandle, AudioEngineStatus, PcmSample, SharedState};

    #[derive(Default)]
    struct FakeBackend {
        opens: VecDeque<Result<(FakeStream, u64), BackendFailure>>,
        default_outcomes: VecDeque<Result<Option<u64>, BackendFailure>>,
        default_id: Option<u64>,
        open_count: usize,
    }

    struct FakeStream;

    impl OutputBackend for FakeBackend {
        type Stream = FakeStream;
        type DeviceToken = u64;

        fn default_device_token(&mut self) -> Result<Option<Self::DeviceToken>, BackendFailure> {
            self.default_outcomes
                .pop_front()
                .unwrap_or(Ok(self.default_id))
        }

        fn open_default_stream(
            &mut self,
            _shared: Arc<SharedState>,
            _supervisor: std::thread::Thread,
        ) -> Result<(Self::Stream, Self::DeviceToken), BackendFailure> {
            self.open_count += 1;
            self.opens.pop_front().unwrap_or(Err(BackendFailure))
        }
    }

    #[test]
    fn retries_without_sleeping_in_tests_and_recovers() {
        let shared = Arc::new(SharedState::new());
        let mut backend = FakeBackend::default();
        backend.opens.push_back(Err(BackendFailure));
        backend.opens.push_back(Ok((FakeStream, 7)));
        backend.default_id = Some(7);
        let mut supervisor = OutputSupervisor::new(backend, shared.clone(), Duration::ZERO);

        supervisor.step(Duration::ZERO, std::thread::current());
        assert_eq!(shared.status(), AudioEngineStatus::Starting);
        supervisor.step(Duration::from_millis(99), std::thread::current());
        assert_eq!(supervisor.backend.open_count, 1);
        supervisor.step(Duration::from_millis(100), std::thread::current());
        assert_eq!(shared.status(), AudioEngineStatus::Ready);
        assert_eq!(supervisor.backend.open_count, 2);
    }

    #[test]
    fn stream_failure_clears_stale_commands_before_reopen() {
        let shared = Arc::new(SharedState::new());
        let handle = AudioEngineHandle {
            shared: shared.clone(),
        };
        let id = handle
            .register_sample(PcmSample::new(48_000, 1, vec![0.0]).unwrap())
            .unwrap();
        handle.play(id).unwrap();
        let mut backend = FakeBackend::default();
        backend.opens.push_back(Ok((FakeStream, 1)));
        backend.opens.push_back(Ok((FakeStream, 1)));
        backend.default_id = Some(1);
        let mut supervisor = OutputSupervisor::new(backend, shared.clone(), Duration::ZERO);
        supervisor.step(Duration::ZERO, std::thread::current());
        shared.stream_failed.store(true, Ordering::Release);
        supervisor.step(Duration::from_secs(1), std::thread::current());
        assert!(shared.commands.is_empty());
        assert_eq!(shared.status(), AudioEngineStatus::Ready);
    }

    #[test]
    fn late_stream_failure_during_backoff_preserves_retry_deadline() {
        let shared = Arc::new(SharedState::new());
        let mut backend = FakeBackend::default();
        backend.opens.push_back(Err(BackendFailure));
        backend.opens.push_back(Err(BackendFailure));
        let mut supervisor = OutputSupervisor::new(backend, shared.clone(), Duration::ZERO);

        supervisor.step(Duration::ZERO, std::thread::current());
        shared.stream_failed.store(true, Ordering::Release);
        supervisor.step(Duration::from_millis(50), std::thread::current());

        assert_eq!(supervisor.backend.open_count, 1);
        assert_eq!(
            supervisor.wait_duration(Duration::from_millis(50)),
            Duration::from_millis(50)
        );
        assert_eq!(shared.status(), AudioEngineStatus::Starting);
    }

    #[test]
    fn commands_queued_during_recovery_are_cleared_before_successful_reopen() {
        let shared = Arc::new(SharedState::new());
        let handle = AudioEngineHandle {
            shared: shared.clone(),
        };
        let id = handle
            .register_sample(PcmSample::new(48_000, 1, vec![0.0]).unwrap())
            .unwrap();
        let mut backend = FakeBackend::default();
        backend.opens.push_back(Ok((FakeStream, 1)));
        backend.opens.push_back(Err(BackendFailure));
        backend.opens.push_back(Ok((FakeStream, 1)));
        backend.default_id = Some(1);
        let mut supervisor = OutputSupervisor::new(backend, shared.clone(), Duration::ZERO);

        supervisor.step(Duration::ZERO, std::thread::current());
        shared.stream_failed.store(true, Ordering::Release);
        supervisor.step(Duration::from_millis(1), std::thread::current());
        handle.play(id).unwrap();
        assert_eq!(shared.commands.len(), 1);
        supervisor.step(Duration::from_millis(101), std::thread::current());

        assert!(shared.commands.is_empty());
        assert_eq!(shared.status(), AudioEngineStatus::Ready);
    }

    #[test]
    fn changed_default_device_rebuilds_after_two_second_poll() {
        let shared = Arc::new(SharedState::new());
        let mut backend = FakeBackend::default();
        backend.opens.push_back(Ok((FakeStream, 1)));
        backend.opens.push_back(Ok((FakeStream, 2)));
        backend.default_id = Some(1);
        let mut supervisor = OutputSupervisor::new(backend, shared, Duration::ZERO);
        supervisor.step(Duration::ZERO, std::thread::current());
        supervisor.backend.default_id = Some(2);
        supervisor.step(Duration::from_millis(1_999), std::thread::current());
        assert_eq!(supervisor.backend.open_count, 1);
        supervisor.step(Duration::from_secs(2), std::thread::current());
        assert_eq!(supervisor.backend.open_count, 2);
    }

    #[test]
    fn shutdown_drops_stream_and_is_terminal() {
        let shared = Arc::new(SharedState::new());
        let mut backend = FakeBackend::default();
        backend.opens.push_back(Ok((FakeStream, 1)));
        backend.default_id = Some(1);
        let mut supervisor = OutputSupervisor::new(backend, shared.clone(), Duration::ZERO);
        supervisor.step(Duration::ZERO, std::thread::current());
        shared.shutdown.store(true, Ordering::Release);
        assert_eq!(
            supervisor.step(Duration::from_secs(1), std::thread::current()),
            SupervisorStep::Stop
        );
        assert_eq!(shared.status(), AudioEngineStatus::Stopped);
        assert!(supervisor.stream.is_none());
    }

    #[test]
    fn wait_duration_uses_retry_deadline() {
        let shared = Arc::new(SharedState::new());
        let mut backend = FakeBackend::default();
        backend.opens.push_back(Err(BackendFailure));
        let mut supervisor = OutputSupervisor::new(backend, shared, Duration::ZERO);

        supervisor.step(Duration::ZERO, std::thread::current());

        assert_eq!(
            supervisor.wait_duration(Duration::ZERO),
            Duration::from_millis(100)
        );
    }

    #[test]
    fn wait_duration_returns_zero_when_retry_is_already_due() {
        let supervisor = OutputSupervisor::new(
            FakeBackend::default(),
            Arc::new(SharedState::new()),
            Duration::ZERO,
        );

        assert_eq!(supervisor.wait_duration(Duration::ZERO), Duration::ZERO);
    }

    #[test]
    fn wait_duration_uses_ready_device_deadline_and_cap() {
        let shared = Arc::new(SharedState::new());
        let mut backend = FakeBackend::default();
        backend.opens.push_back(Ok((FakeStream, 1)));
        let now = Duration::from_secs(10);
        let mut supervisor = OutputSupervisor::new(backend, shared, now);

        supervisor.step(now, std::thread::current());

        assert_eq!(supervisor.wait_duration(now), Duration::from_secs(2));
        assert_eq!(
            supervisor.wait_duration(now + Duration::from_secs(1)),
            Duration::from_secs(1)
        );
    }

    #[test]
    fn default_token_query_error_retains_live_stream_and_ready_status() {
        let shared = Arc::new(SharedState::new());
        let mut backend = FakeBackend::default();
        backend.opens.push_back(Ok((FakeStream, 1)));
        backend.default_outcomes.push_back(Err(BackendFailure));
        let mut supervisor = OutputSupervisor::new(backend, shared.clone(), Duration::ZERO);

        supervisor.step(Duration::ZERO, std::thread::current());
        supervisor.step(Duration::from_secs(2), std::thread::current());

        assert!(supervisor.stream.is_some());
        assert_eq!(supervisor.backend.open_count, 1);
        assert_eq!(shared.status(), AudioEngineStatus::Ready);
    }
}
