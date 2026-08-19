use std::{
    fmt::Debug,
    sync::{atomic::Ordering, Arc},
    thread::{self, Thread},
    time::{Duration, Instant},
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    BufferSize, DeviceId, FromSample, SampleFormat, SizedSample, StreamConfig,
    SupportedStreamConfig, SupportedStreamConfigRange,
};

use super::{mixer::MixerCore, recovery::RecoveryController, SharedState};

const DEVICE_CHECK_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BackendFailure;

pub(crate) struct CpalBackend {
    host: cpal::Host,
}

impl CpalBackend {
    pub(crate) fn new() -> Self {
        Self {
            host: cpal::default_host(),
        }
    }
}

fn select_supported_config(
    ranges: impl IntoIterator<Item = SupportedStreamConfigRange>,
) -> Option<SupportedStreamConfig> {
    let range = ranges
        .into_iter()
        .max_by(SupportedStreamConfigRange::cmp_default_heuristics)?;
    range
        .try_with_standard_sample_rate()
        .or_else(|| Some(range.with_max_sample_rate()))
}

fn buffer_preferences() -> [BufferSize; 3] {
    [
        BufferSize::Fixed(128),
        BufferSize::Fixed(256),
        BufferSize::Default,
    ]
}

fn build_typed_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    shared: Arc<SharedState>,
    supervisor: Thread,
) -> Result<cpal::Stream, BackendFailure>
where
    T: SizedSample + FromSample<f32>,
{
    let output_rate = config.sample_rate;
    let output_channels = config.channels;
    let initial_volume = f32::from_bits(shared.master_volume.load(Ordering::Acquire));
    let callback_shared = shared.clone();
    let error_shared = shared;
    let mut mixer = MixerCore::new(initial_volume);
    device
        .build_output_stream::<T, _, _>(
            config,
            move |output, _| {
                mixer.render(
                    output,
                    output_rate,
                    output_channels,
                    &callback_shared.commands,
                    &callback_shared.master_volume,
                );
            },
            move |_| {
                error_shared.stream_failed.store(true, Ordering::Release);
                supervisor.unpark();
            },
            Some(Duration::from_secs(2)),
        )
        .map_err(|_| BackendFailure)
}

fn build_stream_for_format(
    device: &cpal::Device,
    sample_format: SampleFormat,
    config: StreamConfig,
    shared: Arc<SharedState>,
    supervisor: Thread,
) -> Result<cpal::Stream, BackendFailure> {
    macro_rules! build {
        ($sample:ty) => {
            build_typed_stream::<$sample>(device, config, shared, supervisor)
        };
    }

    match sample_format {
        SampleFormat::I8 => build!(i8),
        SampleFormat::I16 => build!(i16),
        SampleFormat::I24 => build!(cpal::I24),
        SampleFormat::I32 => build!(i32),
        SampleFormat::I64 => build!(i64),
        SampleFormat::U8 => build!(u8),
        SampleFormat::U16 => build!(u16),
        SampleFormat::U24 => build!(cpal::U24),
        SampleFormat::U32 => build!(u32),
        SampleFormat::U64 => build!(u64),
        SampleFormat::F32 => build!(f32),
        SampleFormat::F64 => build!(f64),
        _ => Err(BackendFailure),
    }
}

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

impl OutputBackend for CpalBackend {
    type Stream = cpal::Stream;
    type DeviceToken = DeviceId;

    fn default_device_token(&mut self) -> Result<Option<Self::DeviceToken>, BackendFailure> {
        let Some(device) = self.host.default_output_device() else {
            return Ok(None);
        };
        device.id().map(Some).map_err(|_| BackendFailure)
    }

    fn open_default_stream(
        &mut self,
        shared: Arc<SharedState>,
        supervisor: Thread,
    ) -> Result<(Self::Stream, Self::DeviceToken), BackendFailure> {
        let device = self.host.default_output_device().ok_or(BackendFailure)?;
        let supported_configs = device
            .supported_output_configs()
            .map_err(|_| BackendFailure)?;
        let selected = select_supported_config(supported_configs).ok_or(BackendFailure)?;
        let device_token = device.id().map_err(|_| BackendFailure)?;
        let sample_format = selected.sample_format();
        let base_config = selected.config();

        for buffer_size in buffer_preferences() {
            let mut config = base_config;
            config.buffer_size = buffer_size;
            if let Ok(stream) = build_stream_for_format(
                &device,
                sample_format,
                config,
                shared.clone(),
                supervisor.clone(),
            ) {
                if stream.play().is_ok() {
                    return Ok((stream, device_token));
                }
            }
        }

        Err(BackendFailure)
    }
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

pub(crate) fn run_supervisor<B>(backend: B, shared: Arc<SharedState>)
where
    B: OutputBackend,
{
    let origin = Instant::now();
    let mut supervisor = OutputSupervisor::new(backend, shared, Duration::ZERO);

    loop {
        if supervisor.step(origin.elapsed(), thread::current()) == SupervisorStep::Stop {
            break;
        }
        thread::park_timeout(supervisor.wait_duration(origin.elapsed()));
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

    #[test]
    fn selects_stereo_f32_at_a_standard_rate() {
        use cpal::{SampleFormat, SupportedBufferSize, SupportedStreamConfigRange};

        let range = |channels, format, min, max| {
            SupportedStreamConfigRange::new(
                channels,
                min,
                max,
                SupportedBufferSize::Range {
                    min: 64,
                    max: 1_024,
                },
                format,
            )
        };
        let selected = select_supported_config(vec![
            range(1, SampleFormat::I16, 8_000, 96_000),
            range(2, SampleFormat::F32, 44_100, 96_000),
            range(6, SampleFormat::F32, 44_100, 192_000),
        ])
        .unwrap();

        assert_eq!(selected.channels(), 2);
        assert_eq!(selected.sample_format(), SampleFormat::F32);
        assert_eq!(selected.sample_rate(), 48_000);
    }

    #[test]
    fn buffer_preferences_are_low_latency_then_compatible() {
        assert_eq!(
            buffer_preferences(),
            [
                cpal::BufferSize::Fixed(128),
                cpal::BufferSize::Fixed(256),
                cpal::BufferSize::Default,
            ]
        );
    }

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
