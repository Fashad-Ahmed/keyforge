use std::{collections::HashMap, fmt, sync::Arc};

pub const MIN_SAMPLE_RATE: u32 = 8_000;
pub const MAX_SAMPLE_RATE: u32 = 192_000;
pub const MAX_SAMPLE_SECONDS: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelCount {
    Mono = 1,
    Stereo = 2,
}

impl ChannelCount {
    pub(crate) fn as_usize(self) -> usize {
        self as usize
    }
}

impl TryFrom<u16> for ChannelCount {
    type Error = PcmSampleError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Mono),
            2 => Ok(Self::Stereo),
            _ => Err(PcmSampleError::Channels),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmSampleError {
    SampleRate,
    Channels,
    Empty,
    IncompleteFrame,
    NonFinite,
    OutOfRange,
    TooLong,
}

impl fmt::Display for PcmSampleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid PCM sample: {self:?}")
    }
}

impl std::error::Error for PcmSampleError {}

#[derive(Debug, Clone, PartialEq)]
pub struct PcmSample {
    sample_rate: u32,
    channels: ChannelCount,
    samples: Arc<[f32]>,
}

impl PcmSample {
    pub fn new(sample_rate: u32, channels: u16, samples: Vec<f32>) -> Result<Self, PcmSampleError> {
        if !(MIN_SAMPLE_RATE..=MAX_SAMPLE_RATE).contains(&sample_rate) {
            return Err(PcmSampleError::SampleRate);
        }
        let channels = ChannelCount::try_from(channels)?;
        if samples.is_empty() {
            return Err(PcmSampleError::Empty);
        }
        if !samples.len().is_multiple_of(channels.as_usize()) {
            return Err(PcmSampleError::IncompleteFrame);
        }
        if samples.iter().any(|value| !value.is_finite()) {
            return Err(PcmSampleError::NonFinite);
        }
        if samples.iter().any(|value| !(-1.0..=1.0).contains(value)) {
            return Err(PcmSampleError::OutOfRange);
        }
        let frames = samples.len() / channels.as_usize();
        if frames > sample_rate as usize * MAX_SAMPLE_SECONDS {
            return Err(PcmSampleError::TooLong);
        }
        Ok(Self {
            sample_rate,
            channels,
            samples: samples.into(),
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> ChannelCount {
        self.channels
    }

    pub fn frame_count(&self) -> usize {
        self.samples.len() / self.channels.as_usize()
    }

    pub fn byte_len(&self) -> usize {
        std::mem::size_of_val(self.samples.as_ref())
    }

    #[allow(dead_code)]
    pub(crate) fn samples(&self) -> &[f32] {
        &self.samples
    }
}

pub const MAX_REGISTERED_SAMPLES: usize = 512;
pub const MAX_REGISTERED_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SampleId(u64);

impl SampleId {
    #[cfg(test)]
    pub(crate) fn from_raw_for_test(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RegistryLimits {
    pub max_samples: usize,
    pub max_bytes: usize,
}

impl Default for RegistryLimits {
    fn default() -> Self {
        Self {
            max_samples: MAX_REGISTERED_SAMPLES,
            max_bytes: MAX_REGISTERED_BYTES,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterSampleError {
    TooManySamples,
    MemoryLimitExceeded,
    IdentifierExhausted,
    RegistryUnavailable,
}

impl fmt::Display for RegisterSampleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "sample registration failed: {self:?}")
    }
}

impl std::error::Error for RegisterSampleError {}

pub(crate) struct SampleRegistry {
    samples: HashMap<SampleId, Arc<PcmSample>>,
    next_id: Option<u64>,
    registered_bytes: usize,
    limits: RegistryLimits,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RegistrySnapshot {
    sample_count: usize,
    registered_bytes: usize,
    next_id: Option<u64>,
}

impl Default for SampleRegistry {
    fn default() -> Self {
        Self::with_limits(RegistryLimits::default())
    }
}

impl SampleRegistry {
    pub(crate) fn with_limits(limits: RegistryLimits) -> Self {
        Self {
            samples: HashMap::new(),
            next_id: Some(1),
            registered_bytes: 0,
            limits,
        }
    }

    #[cfg(test)]
    fn with_next_id_for_test(next_id: u64) -> Self {
        Self {
            next_id: Some(next_id),
            ..Self::default()
        }
    }

    pub(crate) fn insert_batch(
        &mut self,
        samples: Vec<PcmSample>,
    ) -> Result<Vec<SampleId>, RegisterSampleError> {
        if samples.is_empty() {
            return Ok(Vec::new());
        }

        let final_count = self
            .samples
            .len()
            .checked_add(samples.len())
            .ok_or(RegisterSampleError::TooManySamples)?;
        if final_count > self.limits.max_samples {
            return Err(RegisterSampleError::TooManySamples);
        }

        let added_bytes = samples.iter().try_fold(0_usize, |total, sample| {
            total
                .checked_add(sample.byte_len())
                .ok_or(RegisterSampleError::MemoryLimitExceeded)
        })?;
        let final_bytes = self
            .registered_bytes
            .checked_add(added_bytes)
            .ok_or(RegisterSampleError::MemoryLimitExceeded)?;
        if final_bytes > self.limits.max_bytes {
            return Err(RegisterSampleError::MemoryLimitExceeded);
        }

        let first = self
            .next_id
            .ok_or(RegisterSampleError::IdentifierExhausted)?;
        let id_offset = u64::try_from(samples.len() - 1)
            .map_err(|_| RegisterSampleError::IdentifierExhausted)?;
        let last = first
            .checked_add(id_offset)
            .ok_or(RegisterSampleError::IdentifierExhausted)?;
        let next = last.checked_add(1);
        let ids: Vec<SampleId> = (first..=last).map(SampleId).collect();

        for (id, sample) in ids.iter().copied().zip(samples) {
            self.samples.insert(id, Arc::new(sample));
        }
        self.registered_bytes = final_bytes;
        self.next_id = next;
        Ok(ids)
    }

    pub(crate) fn insert(&mut self, sample: PcmSample) -> Result<SampleId, RegisterSampleError> {
        self.insert_batch(vec![sample])?
            .into_iter()
            .next()
            .ok_or(RegisterSampleError::IdentifierExhausted)
    }

    pub(crate) fn get(&self, id: SampleId) -> Option<Arc<PcmSample>> {
        self.samples.get(&id).cloned()
    }

    #[cfg(test)]
    pub(crate) fn sample_count_for_test(&self) -> usize {
        self.samples.len()
    }

    #[cfg(test)]
    pub(crate) fn registered_bytes_for_test(&self) -> usize {
        self.registered_bytes
    }

    #[cfg(test)]
    pub(crate) fn snapshot_for_test(&self) -> RegistrySnapshot {
        RegistrySnapshot {
            sample_count: self.sample_count_for_test(),
            registered_bytes: self.registered_bytes_for_test(),
            next_id: self.next_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mono(samples: Vec<f32>) -> Result<PcmSample, PcmSampleError> {
        PcmSample::new(48_000, 1, samples)
    }

    #[test]
    fn accepts_bounded_finite_pcm() {
        let sample = PcmSample::new(48_000, 2, vec![0.25, -0.25, 0.5, -0.5]).unwrap();
        assert_eq!(sample.sample_rate(), 48_000);
        assert_eq!(sample.channels(), ChannelCount::Stereo);
        assert_eq!(sample.frame_count(), 2);
        assert_eq!(sample.byte_len(), 16);
    }

    #[test]
    fn rejects_invalid_pcm_shapes_and_values() {
        assert_eq!(mono(vec![]), Err(PcmSampleError::Empty));
        assert_eq!(
            PcmSample::new(7_999, 1, vec![0.0]),
            Err(PcmSampleError::SampleRate)
        );
        assert_eq!(
            PcmSample::new(192_001, 1, vec![0.0]),
            Err(PcmSampleError::SampleRate)
        );
        assert_eq!(
            PcmSample::new(48_000, 3, vec![0.0, 0.0, 0.0]),
            Err(PcmSampleError::Channels)
        );
        assert_eq!(
            PcmSample::new(48_000, 2, vec![0.0]),
            Err(PcmSampleError::IncompleteFrame)
        );
        assert_eq!(mono(vec![f32::NAN]), Err(PcmSampleError::NonFinite));
        assert_eq!(mono(vec![1.01]), Err(PcmSampleError::OutOfRange));
    }

    #[test]
    fn rejects_pcm_longer_than_ten_seconds() {
        let too_long = vec![0.0; 80_001];
        assert_eq!(
            PcmSample::new(8_000, 1, too_long),
            Err(PcmSampleError::TooLong)
        );
    }

    #[test]
    fn registry_returns_unique_ids_and_samples() {
        let mut registry = SampleRegistry::default();
        let first = registry.insert(mono(vec![0.0]).unwrap()).unwrap();
        let second = registry.insert(mono(vec![0.5]).unwrap()).unwrap();
        assert_ne!(first, second);
        assert_eq!(registry.get(first).unwrap().samples(), &[0.0]);
    }

    #[test]
    fn registry_enforces_count_and_memory_limits() {
        let limits = RegistryLimits {
            max_samples: 1,
            max_bytes: 8,
        };
        let mut count_limited = SampleRegistry::with_limits(limits);
        count_limited.insert(mono(vec![0.0]).unwrap()).unwrap();
        assert_eq!(
            count_limited.insert(mono(vec![0.0]).unwrap()),
            Err(RegisterSampleError::TooManySamples)
        );

        let mut memory_limited = SampleRegistry::with_limits(limits);
        assert_eq!(
            memory_limited.insert(mono(vec![0.0, 0.0, 0.0]).unwrap()),
            Err(RegisterSampleError::MemoryLimitExceeded)
        );
    }

    #[test]
    fn registry_reports_identifier_exhaustion() {
        let mut registry = SampleRegistry::with_next_id_for_test(u64::MAX);
        registry.insert(mono(vec![0.0]).unwrap()).unwrap();
        assert_eq!(
            registry.insert(mono(vec![0.0]).unwrap()),
            Err(RegisterSampleError::IdentifierExhausted)
        );
    }

    #[test]
    fn batch_insert_assigns_ordered_ids_and_accounts_memory_once() {
        let mut registry = SampleRegistry::default();

        let ids = registry
            .insert_batch(vec![
                mono(vec![0.1]).unwrap(),
                mono(vec![0.2, 0.3]).unwrap(),
            ])
            .unwrap();

        assert_eq!(ids, vec![SampleId(1), SampleId(2)]);
        assert_eq!(registry.sample_count_for_test(), 2);
        assert_eq!(registry.registered_bytes_for_test(), 12);
    }

    #[test]
    fn failed_batch_leaves_registry_and_identifier_unchanged() {
        let limits = RegistryLimits {
            max_samples: 2,
            max_bytes: 8,
        };
        let mut registry = SampleRegistry::with_limits(limits);
        let before = registry.snapshot_for_test();

        assert_eq!(
            registry.insert_batch(vec![
                mono(vec![0.1]).unwrap(),
                mono(vec![0.2, 0.3]).unwrap()
            ]),
            Err(RegisterSampleError::MemoryLimitExceeded)
        );
        assert_eq!(registry.snapshot_for_test(), before);
        assert_eq!(
            registry.insert(mono(vec![0.4]).unwrap()).unwrap(),
            SampleId(1)
        );
    }

    #[test]
    fn count_limited_batch_leaves_registry_and_identifier_unchanged() {
        let mut registry = SampleRegistry::with_limits(RegistryLimits {
            max_samples: 1,
            max_bytes: 16,
        });
        let before = registry.snapshot_for_test();

        assert_eq!(
            registry.insert_batch(vec![mono(vec![0.1]).unwrap(), mono(vec![0.2]).unwrap()]),
            Err(RegisterSampleError::TooManySamples)
        );
        assert_eq!(registry.snapshot_for_test(), before);
    }

    #[test]
    fn batch_preflights_identifier_exhaustion_without_partial_insert() {
        let mut registry = SampleRegistry::with_next_id_for_test(u64::MAX);
        let before = registry.snapshot_for_test();

        assert_eq!(
            registry.insert_batch(vec![mono(vec![0.1]).unwrap(), mono(vec![0.2]).unwrap()]),
            Err(RegisterSampleError::IdentifierExhausted)
        );
        assert_eq!(registry.snapshot_for_test(), before);
    }

    #[test]
    fn batch_can_consume_the_final_identifier() {
        let mut registry = SampleRegistry::with_next_id_for_test(u64::MAX - 1);

        assert_eq!(
            registry
                .insert_batch(vec![mono(vec![0.1]).unwrap(), mono(vec![0.2]).unwrap()])
                .unwrap(),
            vec![SampleId(u64::MAX - 1), SampleId(u64::MAX)]
        );
        assert_eq!(
            registry.insert(mono(vec![0.3]).unwrap()),
            Err(RegisterSampleError::IdentifierExhausted)
        );
    }

    #[test]
    fn empty_batch_preserves_registry_state() {
        let mut registry = SampleRegistry::default();
        let before = registry.snapshot_for_test();

        assert_eq!(registry.insert_batch(Vec::new()), Ok(Vec::new()));
        assert_eq!(registry.snapshot_for_test(), before);
    }
}
