use std::{
    fmt,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering},
        Arc, Mutex,
    },
};

use crossbeam_queue::ArrayQueue;

#[allow(dead_code)]
mod sample;

use sample::SampleRegistry;

pub use sample::{ChannelCount, PcmSample, PcmSampleError, RegisterSampleError, SampleId};

pub const COMMAND_QUEUE_CAPACITY: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AudioEngineStatus {
    Starting = 0,
    Ready = 1,
    Recovering = 2,
    Unavailable = 3,
    Stopped = 4,
}

impl AudioEngineStatus {
    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Starting,
            1 => Self::Ready,
            2 => Self::Recovering,
            3 => Self::Unavailable,
            _ => Self::Stopped,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayError {
    UnknownSample,
    QueueFull,
    RegistryUnavailable,
    Stopped,
}

impl fmt::Display for PlayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "sample playback failed: {self:?}")
    }
}

impl std::error::Error for PlayError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeError {
    Invalid,
    Stopped,
}

impl fmt::Display for VolumeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "master volume update failed: {self:?}")
    }
}

impl std::error::Error for VolumeError {}

#[allow(dead_code)]
pub(crate) struct AudioCommand {
    sample_id: SampleId,
    sample: Arc<PcmSample>,
}

#[allow(dead_code)]
impl AudioCommand {
    pub(crate) fn sample_id(&self) -> SampleId {
        self.sample_id
    }

    pub(crate) fn into_parts(self) -> (SampleId, Arc<PcmSample>) {
        (self.sample_id, self.sample)
    }
}

#[allow(dead_code)]
pub(crate) struct SharedState {
    registry: Mutex<SampleRegistry>,
    commands: ArrayQueue<AudioCommand>,
    master_volume: AtomicU32,
    status: AtomicU8,
    shutdown: AtomicBool,
    stream_failed: AtomicBool,
}

#[allow(dead_code)]
impl SharedState {
    fn new() -> Self {
        Self {
            registry: Mutex::new(SampleRegistry::default()),
            commands: ArrayQueue::new(COMMAND_QUEUE_CAPACITY),
            master_volume: AtomicU32::new(1.0_f32.to_bits()),
            status: AtomicU8::new(AudioEngineStatus::Starting as u8),
            shutdown: AtomicBool::new(false),
            stream_failed: AtomicBool::new(false),
        }
    }

    pub(crate) fn set_status(&self, status: AudioEngineStatus) {
        self.status.store(status as u8, Ordering::Release);
    }

    pub(crate) fn clear_commands(&self) {
        while self.commands.pop().is_some() {}
    }
}

#[derive(Clone)]
pub struct AudioEngineHandle {
    shared: Arc<SharedState>,
}

impl AudioEngineHandle {
    #[cfg(test)]
    fn new_for_test() -> Self {
        Self {
            shared: Arc::new(SharedState::new()),
        }
    }

    pub fn register_sample(&self, sample: PcmSample) -> Result<SampleId, RegisterSampleError> {
        if self.shared.shutdown.load(Ordering::Acquire) {
            return Err(RegisterSampleError::RegistryUnavailable);
        }
        self.shared
            .registry
            .lock()
            .map_err(|_| RegisterSampleError::RegistryUnavailable)?
            .insert(sample)
    }

    pub fn play(&self, sample_id: SampleId) -> Result<(), PlayError> {
        if self.shared.shutdown.load(Ordering::Acquire) {
            return Err(PlayError::Stopped);
        }
        let sample = self
            .shared
            .registry
            .lock()
            .map_err(|_| PlayError::RegistryUnavailable)?
            .get(sample_id)
            .ok_or(PlayError::UnknownSample)?;
        self.shared
            .commands
            .push(AudioCommand { sample_id, sample })
            .map_err(|_| PlayError::QueueFull)
    }

    pub fn set_master_volume(&self, volume: f32) -> Result<(), VolumeError> {
        if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
            return Err(VolumeError::Invalid);
        }
        if self.shared.shutdown.load(Ordering::Acquire) {
            return Err(VolumeError::Stopped);
        }
        self.shared
            .master_volume
            .store(volume.to_bits(), Ordering::Release);
        Ok(())
    }

    pub fn status(&self) -> AudioEngineStatus {
        if self.shared.shutdown.load(Ordering::Acquire) {
            return AudioEngineStatus::Stopped;
        }
        AudioEngineStatus::from_u8(self.shared.status.load(Ordering::Acquire))
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;

    fn handle() -> AudioEngineHandle {
        AudioEngineHandle::new_for_test()
    }

    fn sample(value: f32) -> PcmSample {
        PcmSample::new(48_000, 1, vec![value]).unwrap()
    }

    #[test]
    fn registers_and_queues_known_samples() {
        let handle = handle();
        let id = handle.register_sample(sample(0.25)).unwrap();
        handle.play(id).unwrap();
        let command = handle.shared.commands.pop().unwrap();
        assert_eq!(command.sample_id(), id);
    }

    #[test]
    fn rejects_unknown_ids_and_full_queue() {
        let handle = handle();
        assert_eq!(
            handle.play(SampleId::from_raw_for_test(99)),
            Err(PlayError::UnknownSample)
        );
        let id = handle.register_sample(sample(0.0)).unwrap();
        for _ in 0..COMMAND_QUEUE_CAPACITY {
            handle.play(id).unwrap();
        }
        assert_eq!(handle.play(id), Err(PlayError::QueueFull));
    }

    #[test]
    fn multiple_producers_share_the_bounded_queue() {
        let handle = handle();
        let id = handle.register_sample(sample(0.0)).unwrap();
        let joins: Vec<_> = (0..4)
            .map(|_| {
                let handle = handle.clone();
                thread::spawn(move || {
                    for _ in 0..32 {
                        handle.play(id).unwrap();
                    }
                })
            })
            .collect();
        for join in joins {
            join.join().unwrap();
        }
        assert_eq!(handle.shared.commands.len(), 128);
    }

    #[test]
    fn volume_is_validated_even_when_the_queue_is_full() {
        let handle = handle();
        let id = handle.register_sample(sample(0.0)).unwrap();
        for _ in 0..COMMAND_QUEUE_CAPACITY {
            handle.play(id).unwrap();
        }
        handle.set_master_volume(0.4).unwrap();
        assert_eq!(
            f32::from_bits(handle.shared.master_volume.load(Ordering::Acquire)),
            0.4
        );
        assert_eq!(
            handle.set_master_volume(f32::NAN),
            Err(VolumeError::Invalid)
        );
        assert_eq!(handle.set_master_volume(1.1), Err(VolumeError::Invalid));
    }

    #[test]
    fn stopped_handle_rejects_new_work() {
        let handle = handle();
        let id = handle.register_sample(sample(0.0)).unwrap();
        handle.shared.shutdown.store(true, Ordering::Release);
        assert_eq!(handle.play(id), Err(PlayError::Stopped));
        assert_eq!(handle.status(), AudioEngineStatus::Stopped);
    }
}
