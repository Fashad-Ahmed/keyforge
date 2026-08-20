use std::{
    fmt::Debug,
    num::NonZeroU64,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StreamGeneration(NonZeroU64);

impl StreamGeneration {
    fn get(self) -> u64 {
        self.0.get()
    }
}

struct StreamGenerationCounter {
    next: Option<NonZeroU64>,
}

impl StreamGenerationCounter {
    fn new() -> Self {
        Self {
            next: NonZeroU64::new(1),
        }
    }

    fn next(&mut self) -> Result<StreamGeneration, BackendFailure> {
        let generation = self.next.ok_or(BackendFailure)?;
        self.next = generation.get().checked_add(1).and_then(NonZeroU64::new);
        Ok(StreamGeneration(generation))
    }
}

impl Default for StreamGenerationCounter {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) struct CpalBackend {
    host: cpal::Host,
    generations: StreamGenerationCounter,
}

impl CpalBackend {
    pub(crate) fn new() -> Self {
        Self {
            host: cpal::default_host(),
            generations: StreamGenerationCounter::new(),
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

fn report_stream_failure(shared: &SharedState, generation: StreamGeneration) {
    shared
        .stream_failure_generation
        .fetch_max(generation.get(), Ordering::Release);
}

fn build_typed_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    shared: Arc<SharedState>,
    supervisor: Thread,
    generation: StreamGeneration,
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
                    &callback_shared.active_stream_generation,
                    &callback_shared.master_volume,
                );
            },
            move |_| {
                report_stream_failure(&error_shared, generation);
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
    generation: StreamGeneration,
) -> Result<cpal::Stream, BackendFailure> {
    macro_rules! build {
        ($sample:ty) => {
            build_typed_stream::<$sample>(device, config, shared, supervisor, generation)
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

fn open_with_buffer_preferences<S>(
    shared: &SharedState,
    generations: &mut StreamGenerationCounter,
    mut build: impl FnMut(BufferSize, StreamGeneration) -> Result<S, BackendFailure>,
    mut play: impl FnMut(&S) -> Result<(), BackendFailure>,
) -> Result<(S, StreamGeneration), BackendFailure> {
    for buffer_size in buffer_preferences() {
        let generation = generations.next()?;
        if shared.shutdown.load(Ordering::Acquire) {
            return Err(BackendFailure);
        }
        let stream = match build(buffer_size, generation) {
            Ok(stream) => stream,
            Err(BackendFailure) => continue,
        };
        if shared.shutdown.load(Ordering::Acquire) {
            return Err(BackendFailure);
        }
        if play(&stream).is_err() {
            continue;
        }
        if shared.shutdown.load(Ordering::Acquire) {
            return Err(BackendFailure);
        }
        return Ok((stream, generation));
    }

    Err(BackendFailure)
}

pub(crate) trait OutputBackend {
    type Stream;
    type DeviceToken: Clone + Debug + Eq;

    fn default_device_token(&mut self) -> Result<Option<Self::DeviceToken>, BackendFailure>;
    fn open_default_stream(
        &mut self,
        shared: Arc<SharedState>,
        supervisor: Thread,
    ) -> Result<(Self::Stream, Self::DeviceToken, StreamGeneration), BackendFailure>;
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
    ) -> Result<(Self::Stream, Self::DeviceToken, StreamGeneration), BackendFailure> {
        let device = self.host.default_output_device().ok_or(BackendFailure)?;
        let supported_configs = device
            .supported_output_configs()
            .map_err(|_| BackendFailure)?;
        let selected = select_supported_config(supported_configs).ok_or(BackendFailure)?;
        let device_token = device.id().map_err(|_| BackendFailure)?;
        let sample_format = selected.sample_format();
        let base_config = selected.config();
        let callback_shared = shared.clone();
        let (stream, generation) = open_with_buffer_preferences(
            &shared,
            &mut self.generations,
            |buffer_size, generation| {
                let mut config = base_config;
                config.buffer_size = buffer_size;
                build_stream_for_format(
                    &device,
                    sample_format,
                    config,
                    callback_shared.clone(),
                    supervisor.clone(),
                    generation,
                )
            },
            |stream| stream.play().map_err(|_| BackendFailure),
        )?;

        Ok((stream, device_token, generation))
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
    stream_generation: Option<StreamGeneration>,
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
            stream_generation: None,
            device_token: None,
            next_device_check: now,
        }
    }

    pub(crate) fn step(&mut self, now: Duration, current_thread: Thread) -> SupervisorStep {
        if self.shared.shutdown.load(Ordering::Acquire) {
            self.shared.deactivate_stream_generation();
            self.stream = None;
            self.stream_generation = None;
            self.device_token = None;
            self.shared.clear_commands();
            self.recovery.stop();
            self.shared.set_status(self.recovery.status());
            return SupervisorStep::Stop;
        }

        let failed_generation = NonZeroU64::new(
            self.shared
                .stream_failure_generation
                .swap(0, Ordering::AcqRel),
        )
        .map(StreamGeneration);
        if self.stream.is_some() && failed_generation == self.stream_generation {
            self.shared.deactivate_stream_generation();
            self.recovery.stream_lost(now);
            self.shared.set_status(self.recovery.status());
            self.stream = None;
            self.stream_generation = None;
            self.device_token = None;
            self.shared.clear_commands();
        }

        if self.stream.is_some() && now >= self.next_device_check {
            let device_changed = match self.backend.default_device_token() {
                Ok(Some(token)) => self.device_token.as_ref() != Some(&token),
                Ok(None) => true,
                Err(BackendFailure) => false,
            };
            if device_changed {
                self.shared.deactivate_stream_generation();
                self.recovery.stream_lost(now);
                self.shared.set_status(self.recovery.status());
                self.stream = None;
                self.stream_generation = None;
                self.device_token = None;
                self.shared.clear_commands();
            }
            self.next_device_check = now.saturating_add(DEVICE_CHECK_INTERVAL);
        }

        if self.stream.is_none() && self.recovery.attempt_due(now) {
            self.shared.set_status(self.recovery.status());
            self.shared.clear_commands();
            match self
                .backend
                .open_default_stream(Arc::clone(&self.shared), current_thread)
            {
                Ok((stream, token, generation)) => {
                    self.shared.clear_commands();
                    self.stream = Some(stream);
                    self.stream_generation = Some(generation);
                    self.device_token = Some(token);
                    self.shared.activate_stream_generation(generation.get());
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
        sync::{
            atomic::{AtomicU32, Ordering},
            mpsc::{sync_channel, Receiver, SyncSender},
            Arc,
        },
        time::Duration,
    };

    use super::*;
    use crate::audio::{
        mixer::MixerCore, AudioEngineHandle, AudioEngineStatus, PcmSample, SampleId, SharedState,
    };

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

    #[test]
    fn shutdown_during_first_failed_build_prevents_later_attempts() {
        let shared = SharedState::new();
        let mut generations = StreamGenerationCounter::new();
        let mut attempts = Vec::new();
        let mut plays = 0;

        let result = open_with_buffer_preferences(
            &shared,
            &mut generations,
            |buffer_size, _| {
                attempts.push(buffer_size);
                shared.shutdown.store(true, Ordering::Release);
                Err::<(), _>(BackendFailure)
            },
            |_| {
                plays += 1;
                Ok(())
            },
        );

        assert_eq!(result, Err(BackendFailure));
        assert_eq!(attempts, [BufferSize::Fixed(128)]);
        assert_eq!(plays, 0);
    }

    #[test]
    fn shutdown_during_successful_build_prevents_play() {
        let shared = SharedState::new();
        let mut generations = StreamGenerationCounter::new();
        let mut attempts = Vec::new();
        let mut plays = 0;

        let result = open_with_buffer_preferences(
            &shared,
            &mut generations,
            |buffer_size, _| {
                attempts.push(buffer_size);
                shared.shutdown.store(true, Ordering::Release);
                Ok(())
            },
            |_| {
                plays += 1;
                Ok(())
            },
        );

        assert_eq!(result, Err(BackendFailure));
        assert_eq!(attempts, [BufferSize::Fixed(128)]);
        assert_eq!(plays, 0);
    }

    #[test]
    fn shutdown_during_successful_play_prevents_candidate_publication() {
        let shared = SharedState::new();
        let mut generations = StreamGenerationCounter::new();
        let mut builds = 0;
        let mut plays = 0;

        let result = open_with_buffer_preferences(
            &shared,
            &mut generations,
            |buffer_size, _| {
                builds += 1;
                assert_eq!(buffer_size, BufferSize::Fixed(128));
                Ok("candidate")
            },
            |candidate| {
                plays += 1;
                assert_eq!(*candidate, "candidate");
                shared.shutdown.store(true, Ordering::Release);
                Ok(())
            },
        );

        assert_eq!(result, Err(BackendFailure));
        assert_eq!(builds, 1);
        assert_eq!(plays, 1);
    }

    #[test]
    fn generation_exhaustion_never_wraps_to_the_no_signal_sentinel() {
        let mut generations = StreamGenerationCounter {
            next: std::num::NonZeroU64::new(u64::MAX),
        };

        assert_eq!(generations.next().unwrap().get(), u64::MAX);
        assert_eq!(generations.next(), Err(BackendFailure));
    }

    #[derive(Default)]
    struct FakeBackend {
        opens: VecDeque<Result<(FakeStream, u64), BackendFailure>>,
        default_outcomes: VecDeque<Result<Option<u64>, BackendFailure>>,
        default_id: Option<u64>,
        inject_sample_on_open: Option<SampleId>,
        open_count: usize,
        generations: StreamGenerationCounter,
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
            shared: Arc<SharedState>,
            _supervisor: std::thread::Thread,
        ) -> Result<(Self::Stream, Self::DeviceToken, StreamGeneration), BackendFailure> {
            self.open_count += 1;
            let generation = self.generations.next()?;
            if let Some(sample_id) = self.inject_sample_on_open.take() {
                AudioEngineHandle { shared }.play(sample_id).unwrap();
            }
            self.opens
                .pop_front()
                .unwrap_or(Err(BackendFailure))
                .map(|(stream, token)| (stream, token, generation))
        }
    }

    fn generation(value: u64) -> StreamGeneration {
        StreamGeneration(NonZeroU64::new(value).unwrap())
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
        let active_generation = supervisor.stream_generation.unwrap();
        report_stream_failure(&shared, active_generation);
        supervisor.step(Duration::from_secs(1), std::thread::current());
        assert!(shared.commands.is_empty());
        assert_eq!(shared.status(), AudioEngineStatus::Ready);
    }

    #[test]
    fn stale_stream_failure_does_not_replace_the_current_stream() {
        let shared = Arc::new(SharedState::new());
        let mut backend = FakeBackend::default();
        backend.opens.push_back(Ok((FakeStream, 1)));
        backend.opens.push_back(Ok((FakeStream, 1)));
        backend.opens.push_back(Ok((FakeStream, 1)));
        backend.default_id = Some(1);
        let mut supervisor = OutputSupervisor::new(backend, shared.clone(), Duration::ZERO);

        supervisor.step(Duration::ZERO, std::thread::current());
        let old_generation = supervisor.stream_generation.unwrap();
        report_stream_failure(&shared, old_generation);
        supervisor.step(Duration::from_millis(1), std::thread::current());
        assert_eq!(supervisor.backend.open_count, 2);
        let active_generation = supervisor.stream_generation.unwrap();

        report_stream_failure(&shared, old_generation);
        supervisor.step(Duration::from_millis(2), std::thread::current());

        assert_eq!(supervisor.backend.open_count, 2);
        assert_eq!(shared.status(), AudioEngineStatus::Ready);

        report_stream_failure(&shared, active_generation);
        report_stream_failure(&shared, old_generation);
        supervisor.step(Duration::from_millis(3), std::thread::current());

        assert_eq!(supervisor.backend.open_count, 3);
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
        report_stream_failure(&shared, generation(1));
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
        let active_generation = supervisor.stream_generation.unwrap();
        report_stream_failure(&shared, active_generation);
        supervisor.step(Duration::from_millis(1), std::thread::current());
        handle.play(id).unwrap();
        assert_eq!(shared.commands.len(), 1);
        supervisor.step(Duration::from_millis(101), std::thread::current());

        assert!(shared.commands.is_empty());
        assert_eq!(shared.status(), AudioEngineStatus::Ready);
    }

    #[test]
    fn command_injected_during_reopen_is_not_rendered_after_ready() {
        let shared = Arc::new(SharedState::new());
        let handle = AudioEngineHandle {
            shared: shared.clone(),
        };
        let id = handle
            .register_sample(PcmSample::new(48_000, 1, vec![0.5]).unwrap())
            .unwrap();
        let mut backend = FakeBackend::default();
        backend.opens.push_back(Ok((FakeStream, 1)));
        backend.opens.push_back(Ok((FakeStream, 1)));
        backend.default_id = Some(1);
        let mut supervisor = OutputSupervisor::new(backend, shared.clone(), Duration::ZERO);

        supervisor.step(Duration::ZERO, std::thread::current());
        supervisor.backend.inject_sample_on_open = Some(id);
        report_stream_failure(&shared, supervisor.stream_generation.unwrap());
        supervisor.step(Duration::from_millis(1), std::thread::current());

        assert_eq!(shared.status(), AudioEngineStatus::Ready);
        assert!(shared.commands.is_empty());
        let mut output = [0.0_f32; 1];
        MixerCore::new(1.0).render(
            &mut output,
            48_000,
            1,
            &shared.commands,
            &shared.active_stream_generation,
            &AtomicU32::new(1.0_f32.to_bits()),
        );
        assert_eq!(output, [0.0]);
    }

    struct BlockingReopenBackend {
        open_count: usize,
        entered: SyncSender<()>,
        release: Receiver<()>,
        generations: StreamGenerationCounter,
    }

    impl OutputBackend for BlockingReopenBackend {
        type Stream = FakeStream;
        type DeviceToken = u64;

        fn default_device_token(&mut self) -> Result<Option<Self::DeviceToken>, BackendFailure> {
            Ok(Some(1))
        }

        fn open_default_stream(
            &mut self,
            _shared: Arc<SharedState>,
            _supervisor: std::thread::Thread,
        ) -> Result<(Self::Stream, Self::DeviceToken, StreamGeneration), BackendFailure> {
            self.open_count += 1;
            let generation = self.generations.next()?;
            if self.open_count == 2 {
                self.entered.send(()).unwrap();
                self.release.recv().unwrap();
            }
            Ok((FakeStream, 1, generation))
        }
    }

    #[test]
    fn publishes_recovering_while_reopen_is_in_progress() {
        let shared = Arc::new(SharedState::new());
        let (entered_sender, entered_receiver) = sync_channel(0);
        let (release_sender, release_receiver) = sync_channel(0);
        let backend = BlockingReopenBackend {
            open_count: 0,
            entered: entered_sender,
            release: release_receiver,
            generations: StreamGenerationCounter::new(),
        };
        let mut supervisor = OutputSupervisor::new(backend, shared.clone(), Duration::ZERO);
        supervisor.step(Duration::ZERO, std::thread::current());
        report_stream_failure(&shared, supervisor.stream_generation.unwrap());

        let join = std::thread::spawn(move || {
            supervisor.step(Duration::from_millis(1), std::thread::current())
        });
        entered_receiver.recv().unwrap();
        let status_during_reopen = shared.status();
        let generation_during_reopen = shared.active_stream_generation.load(Ordering::Acquire);
        release_sender.send(()).unwrap();
        assert_eq!(join.join().unwrap(), SupervisorStep::Continue);

        assert_eq!(status_during_reopen, AudioEngineStatus::Recovering);
        assert_eq!(generation_during_reopen, 0);
        assert_ne!(shared.active_stream_generation.load(Ordering::Acquire), 0);
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
    fn changed_default_device_deactivates_generation_when_reopen_fails() {
        let shared = Arc::new(SharedState::new());
        let mut backend = FakeBackend::default();
        backend.opens.push_back(Ok((FakeStream, 1)));
        backend.opens.push_back(Err(BackendFailure));
        backend.default_id = Some(1);
        let mut supervisor = OutputSupervisor::new(backend, shared.clone(), Duration::ZERO);
        supervisor.step(Duration::ZERO, std::thread::current());
        assert_ne!(shared.active_stream_generation.load(Ordering::Acquire), 0);

        supervisor.backend.default_id = Some(2);
        supervisor.step(Duration::from_secs(2), std::thread::current());

        assert!(supervisor.stream.is_none());
        assert_eq!(shared.status(), AudioEngineStatus::Recovering);
        assert_eq!(shared.active_stream_generation.load(Ordering::Acquire), 0);
    }

    #[test]
    fn shutdown_deactivates_generation_and_is_terminal() {
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
        assert_eq!(shared.active_stream_generation.load(Ordering::Acquire), 0);
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
