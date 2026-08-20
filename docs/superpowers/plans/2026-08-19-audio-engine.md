# KeyForge Milestone 2 — Audio Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a native, low-latency Rust audio engine that registers validated PCM, mixes 32 overlapping voices, controls in-memory volume, and recovers the default output device without changing production IPC.

**Architecture:** A platform-independent `MixerCore` consumes immutable PCM through a bounded `crossbeam_queue::ArrayQueue`, while a dedicated CPAL supervisor owns the default device and stream. Sample validation, recovery policy, and supervisor behavior remain independently deterministic and testable without an audio device.

**Tech Stack:** Rust 1.88.0, CPAL 0.18.1 with no optional features, crossbeam-queue 0.3.13, Cargo tests, GitHub Actions on Ubuntu 24.04, macOS 15, and Windows 2025.

**Spec:** `docs/superpowers/specs/2026-08-19-audio-engine-design.md`

## Global Constraints

- Read `AGENTS.md`, the design spec, `SECURITY.md`, `docs/architecture/trust-boundaries.md`, and `docs/security/threat-model.md` before implementation.
- Work task-by-task with one focused commit per completed task.
- Use TDD for every behavior change and observe each new test fail for the intended reason before implementing it.
- Do not add keyboard hooks, `SoundEvent`, sound-pack parsing, file access, decoding, bundled sounds, persistent settings, UI, tray behavior, autostart, updater, networking, telemetry, analytics, or community functionality.
- Do not add or change any production Tauri command, event, capability, or permission.
- Raw keyboard events, typed content, device identifiers, and backend error strings must not cross IPC.
- Accept only immutable interleaved `f32` PCM with one or two channels and rates from 8,000 through 192,000 Hz.
- Reject PCM longer than 10 seconds, more than 512 registered samples, or more than 128 MiB total registered PCM.
- Use exactly 32 voices with deterministic oldest-voice stealing and a 256-command bounded queue.
- Master volume is in-memory, finite, within `0.0..=1.0`, and smoothed over five milliseconds.
- The CPAL callback must not allocate, lock, access files, decode, sleep, retry, log, format strings, call Tauri, or communicate with the frontend.
- The real backend uses only the OS default output device and automatically recovers from missing, invalidated, or changed devices.
- Automated tests must never require an audio device.
- Add only `cpal = { version = "0.18.1", default-features = false }` and `crossbeam-queue = "0.3.13"` as runtime dependencies.
- Keep `src-tauri/Cargo.lock` committed and use `--locked` after dependency resolution.

---

## File Structure

```text
src-tauri/
├── Cargo.toml                         Add CPAL and crossbeam-queue only
├── Cargo.lock                         Lock the reviewed dependency graph
├── examples/
│   └── audio_smoke.rs                 Developer-only generated-tone smoke check
└── src/
    ├── lib.rs                         Export the Rust audio module only
    ├── test_alloc.rs                  Test-only thread-local allocation probe
    └── audio/
        ├── mod.rs                     Engine owner, handle, status, public errors
        ├── sample.rs                  PCM contract, registry, IDs, memory limits
        ├── mixer.rs                   Fixed-capacity voices and rendering
        ├── recovery.rs                Pure retry/status state machine
        └── cpal_output.rs             Backend trait, supervisor, CPAL adapter

.github/workflows/ci.yml               Cross-platform Rust matrix and ALSA package
README.md                              Manual smoke command and M2 architecture
docs/architecture/trust-boundaries.md  Decoded PCM and callback boundaries
docs/security/threat-model.md          Audio resource and device-failure mitigations
security/tauri-policy.test.ts          Regression: IPC/capabilities remain unchanged
```

### Task 1: Validated PCM and Bounded Sample Registry

**Files:**
- Create: `src-tauri/src/audio/sample.rs`
- Create: `src-tauri/src/audio/mod.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: no audio code
- Produces: `ChannelCount`, `PcmSample`, `PcmSampleError`, `SampleId`, `SampleRegistry`, `RegisterSampleError`, and `RegistryLimits`

- [ ] **Step 1: Expose the empty module and write the failing PCM tests**

Add `pub mod audio;` above `run()` in `src-tauri/src/lib.rs`. Create `src-tauri/src/audio/mod.rs` with `mod sample;` and public re-exports for the names used below. Create `sample.rs` with this test module before defining the types:

```rust
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
        assert_eq!(PcmSample::new(7_999, 1, vec![0.0]), Err(PcmSampleError::SampleRate));
        assert_eq!(PcmSample::new(192_001, 1, vec![0.0]), Err(PcmSampleError::SampleRate));
        assert_eq!(PcmSample::new(48_000, 3, vec![0.0, 0.0, 0.0]), Err(PcmSampleError::Channels));
        assert_eq!(PcmSample::new(48_000, 2, vec![0.0]), Err(PcmSampleError::IncompleteFrame));
        assert_eq!(mono(vec![f32::NAN]), Err(PcmSampleError::NonFinite));
        assert_eq!(mono(vec![1.01]), Err(PcmSampleError::OutOfRange));
    }

    #[test]
    fn rejects_pcm_longer_than_ten_seconds() {
        let too_long = vec![0.0; 80_001];
        assert_eq!(PcmSample::new(8_000, 1, too_long), Err(PcmSampleError::TooLong));
    }
}
```

- [ ] **Step 2: Run the PCM tests and verify the red state**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml audio::sample::tests -- --nocapture
```

Expected: compilation fails because `PcmSample`, `PcmSampleError`, and `ChannelCount` do not exist.

- [ ] **Step 3: Implement the minimal PCM contract**

Implement these exact constants and types in `sample.rs`:

```rust
use std::{fmt, sync::Arc};

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
        if samples.len() % channels.as_usize() != 0 {
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
        Ok(Self { sample_rate, channels, samples: samples.into() })
    }

    pub fn sample_rate(&self) -> u32 { self.sample_rate }
    pub fn channels(&self) -> ChannelCount { self.channels }
    pub fn frame_count(&self) -> usize { self.samples.len() / self.channels.as_usize() }
    pub fn byte_len(&self) -> usize { std::mem::size_of_val(self.samples.as_ref()) }
    pub(crate) fn samples(&self) -> &[f32] { &self.samples }
}
```

Re-export the public PCM names from `audio/mod.rs`.

- [ ] **Step 4: Run the PCM tests and verify green**

Run the command from Step 2. Expected: all three tests pass.

- [ ] **Step 5: Write the failing registry limit tests**

Append tests that use deliberately small test limits so no large allocation is required:

```rust
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
    let limits = RegistryLimits { max_samples: 1, max_bytes: 8 };
    let mut count_limited = SampleRegistry::with_limits(limits);
    count_limited.insert(mono(vec![0.0]).unwrap()).unwrap();
    assert_eq!(count_limited.insert(mono(vec![0.0]).unwrap()), Err(RegisterSampleError::TooManySamples));

    let mut memory_limited = SampleRegistry::with_limits(limits);
    assert_eq!(memory_limited.insert(mono(vec![0.0, 0.0, 0.0]).unwrap()), Err(RegisterSampleError::MemoryLimitExceeded));
}

#[test]
fn registry_reports_identifier_exhaustion() {
    let mut registry = SampleRegistry::with_next_id_for_test(u64::MAX);
    registry.insert(mono(vec![0.0]).unwrap()).unwrap();
    assert_eq!(registry.insert(mono(vec![0.0]).unwrap()), Err(RegisterSampleError::IdentifierExhausted));
}
```

- [ ] **Step 6: Run the registry tests and verify the red state**

Run the command from Step 2. Expected: compilation fails because the registry types do not exist.

- [ ] **Step 7: Implement the bounded registry**

Use these exact production limits and interfaces:

```rust
use std::collections::HashMap;

pub const MAX_REGISTERED_SAMPLES: usize = 512;
pub const MAX_REGISTERED_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SampleId(u64);

#[derive(Debug, Clone, Copy)]
pub(crate) struct RegistryLimits {
    pub max_samples: usize,
    pub max_bytes: usize,
}

impl Default for RegistryLimits {
    fn default() -> Self {
        Self { max_samples: MAX_REGISTERED_SAMPLES, max_bytes: MAX_REGISTERED_BYTES }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterSampleError {
    TooManySamples,
    MemoryLimitExceeded,
    IdentifierExhausted,
    RegistryUnavailable,
}

pub(crate) struct SampleRegistry {
    samples: HashMap<SampleId, Arc<PcmSample>>,
    next_id: Option<u64>,
    registered_bytes: usize,
    limits: RegistryLimits,
}

impl Default for SampleRegistry {
    fn default() -> Self {
        Self::with_limits(RegistryLimits::default())
    }
}

impl SampleRegistry {
    pub(crate) fn with_limits(limits: RegistryLimits) -> Self {
        Self { samples: HashMap::new(), next_id: Some(1), registered_bytes: 0, limits }
    }

    #[cfg(test)]
    fn with_next_id_for_test(next_id: u64) -> Self {
        let mut registry = Self::default();
        registry.next_id = Some(next_id);
        registry
    }

    pub(crate) fn insert(&mut self, sample: PcmSample) -> Result<SampleId, RegisterSampleError> {
        if self.samples.len() >= self.limits.max_samples {
            return Err(RegisterSampleError::TooManySamples);
        }
        let bytes = sample.byte_len();
        let new_total = self.registered_bytes.checked_add(bytes)
            .ok_or(RegisterSampleError::MemoryLimitExceeded)?;
        if new_total > self.limits.max_bytes {
            return Err(RegisterSampleError::MemoryLimitExceeded);
        }
        let raw_id = self.next_id.ok_or(RegisterSampleError::IdentifierExhausted)?;
        self.next_id = raw_id.checked_add(1);
        let id = SampleId(raw_id);
        self.samples.insert(id, Arc::new(sample));
        self.registered_bytes = new_total;
        Ok(id)
    }

    pub(crate) fn get(&self, id: SampleId) -> Option<Arc<PcmSample>> {
        self.samples.get(&id).cloned()
    }
}
```

Implement `Display` and `Error` for `RegisterSampleError` using fixed variant names only; do not include sample data. Re-export `PcmSample`, `PcmSampleError`, `SampleId`, and `RegisterSampleError` from `audio/mod.rs`.

- [ ] **Step 8: Verify Task 1**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml audio::sample::tests
```

Expected: all commands exit 0.

- [ ] **Step 9: Commit Task 1**

```bash
git add src-tauri/src/lib.rs src-tauri/src/audio/mod.rs src-tauri/src/audio/sample.rs
git commit -m "feat: define bounded PCM sample registry"
```

### Task 2: Thread-Safe Control Handle and Bounded Playback Queue

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/src/audio/mod.rs`

**Interfaces:**
- Consumes: `PcmSample`, `SampleId`, `SampleRegistry`, `RegisterSampleError`
- Produces: `AudioEngineHandle::{register_sample, play, set_master_volume, status}`, `AudioEngineStatus`, `PlayError`, `VolumeError`, `SharedState`, and `AudioCommand`

- [ ] **Step 1: Add only the queue dependency and inspect the lockfile delta**

Add:

```toml
crossbeam-queue = "0.3.13"
```

under `[dependencies]`, then run:

```bash
cargo check --manifest-path src-tauri/Cargo.toml
git diff -- src-tauri/Cargo.toml src-tauri/Cargo.lock
```

Expected: Cargo resolves `crossbeam-queue` and its required Crossbeam utility dependency; no decoder, network, telemetry, or logging framework appears.

- [ ] **Step 2: Write failing handle tests in `audio/mod.rs`**

Add:

```rust
#[cfg(test)]
mod tests {
    use std::{sync::Arc, thread};
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
        assert_eq!(handle.play(SampleId::from_raw_for_test(99)), Err(PlayError::UnknownSample));
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
        let joins: Vec<_> = (0..4).map(|_| {
            let handle = handle.clone();
            thread::spawn(move || {
                for _ in 0..32 { handle.play(id).unwrap(); }
            })
        }).collect();
        for join in joins { join.join().unwrap(); }
        assert_eq!(handle.shared.commands.len(), 128);
    }

    #[test]
    fn volume_is_validated_even_when_the_queue_is_full() {
        let handle = handle();
        let id = handle.register_sample(sample(0.0)).unwrap();
        for _ in 0..COMMAND_QUEUE_CAPACITY { handle.play(id).unwrap(); }
        handle.set_master_volume(0.4).unwrap();
        assert_eq!(f32::from_bits(handle.shared.master_volume.load(Ordering::Acquire)), 0.4);
        assert_eq!(handle.set_master_volume(f32::NAN), Err(VolumeError::Invalid));
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
```

- [ ] **Step 3: Run the handle tests and verify the red state**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml audio::tests -- --nocapture
```

Expected: compilation fails because the engine handle, shared state, errors, and test constructors do not exist.

- [ ] **Step 4: Implement the control plane in `audio/mod.rs`**

Use these constants and representations:

```rust
use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering},
    Arc, Mutex,
};
use crossbeam_queue::ArrayQueue;

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

pub(crate) struct AudioCommand {
    sample_id: SampleId,
    sample: Arc<PcmSample>,
}

impl AudioCommand {
    pub(crate) fn sample_id(&self) -> SampleId { self.sample_id }
    pub(crate) fn into_parts(self) -> (SampleId, Arc<PcmSample>) { (self.sample_id, self.sample) }
}

pub(crate) struct SharedState {
    registry: Mutex<SampleRegistry>,
    commands: ArrayQueue<AudioCommand>,
    master_volume: AtomicU32,
    status: AtomicU8,
    shutdown: AtomicBool,
    stream_failed: AtomicBool,
}

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
```

Implement methods with these exact rules:

```rust
impl AudioEngineHandle {
    #[cfg(test)]
    fn new_for_test() -> Self { Self { shared: Arc::new(SharedState::new()) } }

    pub fn register_sample(&self, sample: PcmSample) -> Result<SampleId, RegisterSampleError> {
        if self.shared.shutdown.load(Ordering::Acquire) {
            return Err(RegisterSampleError::RegistryUnavailable);
        }
        self.shared.registry.lock()
            .map_err(|_| RegisterSampleError::RegistryUnavailable)?
            .insert(sample)
    }

    pub fn play(&self, sample_id: SampleId) -> Result<(), PlayError> {
        if self.shared.shutdown.load(Ordering::Acquire) { return Err(PlayError::Stopped); }
        let sample = self.shared.registry.lock()
            .map_err(|_| PlayError::RegistryUnavailable)?
            .get(sample_id)
            .ok_or(PlayError::UnknownSample)?;
        self.shared.commands.push(AudioCommand { sample_id, sample })
            .map_err(|_| PlayError::QueueFull)
    }

    pub fn set_master_volume(&self, volume: f32) -> Result<(), VolumeError> {
        if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
            return Err(VolumeError::Invalid);
        }
        if self.shared.shutdown.load(Ordering::Acquire) { return Err(VolumeError::Stopped); }
        self.shared.master_volume.store(volume.to_bits(), Ordering::Release);
        Ok(())
    }

    pub fn status(&self) -> AudioEngineStatus {
        if self.shared.shutdown.load(Ordering::Acquire) { return AudioEngineStatus::Stopped; }
        AudioEngineStatus::from_u8(self.shared.status.load(Ordering::Acquire))
    }
}
```

Define `PlayError::{UnknownSample, QueueFull, RegistryUnavailable, Stopped}` and `VolumeError::{Invalid, Stopped}` with fixed `Display` implementations and `std::error::Error`. Add `SampleId::from_raw_for_test` behind `#[cfg(test)]` in `sample.rs`.

- [ ] **Step 5: Run the handle tests and verify green**

Run the command from Step 3. Expected: all five tests pass.

- [ ] **Step 6: Verify Task 2 and dependency policy**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo tree --manifest-path src-tauri/Cargo.toml --invert crossbeam-queue
```

Expected: all checks pass; the tree shows KeyForge directly depends on `crossbeam-queue` and contains no new decoder or network client.

- [ ] **Step 7: Commit Task 2**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/audio/mod.rs src-tauri/src/audio/sample.rs
git commit -m "feat: add bounded audio control queue"
```

### Task 3: Deterministic Fixed-Voice Mixer

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Create: `src-tauri/src/audio/mixer.rs`
- Modify: `src-tauri/src/audio/mod.rs`

**Interfaces:**
- Consumes: `AudioCommand`, `SharedState`, `PcmSample`, `SampleId`
- Produces: `MixerCore::new`, `MixerCore::render<T>`, 32 voice slots, channel mapping, resampling, and oldest stealing

- [ ] **Step 1: Add CPAL with no optional features and inspect dependencies**

Add:

```toml
cpal = { version = "0.18.1", default-features = false }
```

Run `cargo check --manifest-path src-tauri/Cargo.toml`, then inspect `git diff -- src-tauri/Cargo.toml src-tauri/Cargo.lock` and `cargo tree --manifest-path src-tauri/Cargo.toml -e features -i cpal`.

Expected: CPAL resolves its native default backends only. No `asio`, `jack`, `pipewire`, `pulseaudio`, `realtime`, `recording`, decoder, networking, or telemetry feature is enabled.

- [ ] **Step 2: Write failing mixer tests**

Create `mixer.rs`, declare `mod mixer;` in `audio/mod.rs`, and add:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AudioCommand, PcmSampleError, SampleId};
    use crossbeam_queue::ArrayQueue;
    use std::sync::{atomic::AtomicU32, Arc};

    fn pcm(rate: u32, channels: u16, values: &[f32]) -> Arc<PcmSample> {
        Arc::new(PcmSample::new(rate, channels, values.to_vec()).unwrap())
    }

    fn command(id: u64, sample: Arc<PcmSample>) -> AudioCommand {
        AudioCommand::new_for_test(SampleId::from_raw_for_test(id), sample)
    }

    #[test]
    fn renders_mono_stereo_and_downmixes() {
        let queue = ArrayQueue::new(4);
        queue.push(command(1, pcm(48_000, 1, &[0.25, 0.5]))).unwrap();
        let mut mixer = MixerCore::new(1.0);
        let mut stereo = [0.0_f32; 4];
        mixer.render(&mut stereo, 48_000, 2, &queue, &AtomicU32::new(1.0_f32.to_bits()));
        assert_eq!(stereo, [0.25, 0.25, 0.5, 0.5]);

        queue.push(command(2, pcm(48_000, 2, &[0.2, 0.6]))).unwrap();
        let mut mono = [0.0_f32; 1];
        mixer.render(&mut mono, 48_000, 1, &queue, &AtomicU32::new(1.0_f32.to_bits()));
        assert!((mono[0] - 0.4).abs() < 0.000_01);
    }

    #[test]
    fn linearly_resamples_between_rates() {
        let queue = ArrayQueue::new(2);
        queue.push(command(1, pcm(24_000, 1, &[0.0, 1.0]))).unwrap();
        let mut output = [0.0_f32; 4];
        MixerCore::new(1.0).render(&mut output, 48_000, 1, &queue, &AtomicU32::new(1.0_f32.to_bits()));
        assert_eq!(output, [0.0, 0.5, 1.0, 1.0]);

        queue.push(command(2, pcm(48_000, 1, &[0.0, 0.25, 0.5, 0.75]))).unwrap();
        let mut downsampled = [0.0_f32; 2];
        MixerCore::new(1.0).render(
            &mut downsampled,
            24_000,
            1,
            &queue,
            &AtomicU32::new(1.0_f32.to_bits()),
        );
        assert_eq!(downsampled, [0.0, 0.5]);
    }

    #[test]
    fn silences_unused_channels_and_incomplete_output_frames() {
        let queue = ArrayQueue::new(1);
        queue.push(command(1, pcm(48_000, 1, &[0.25]))).unwrap();
        let mut output = [1.0_f32; 5];
        MixerCore::new(1.0).render(
            &mut output,
            48_000,
            4,
            &queue,
            &AtomicU32::new(1.0_f32.to_bits()),
        );
        assert_eq!(output, [0.25, 0.25, 0.0, 0.0, 0.0]);

        let mut silence = [1.0_f32; 2];
        MixerCore::new(1.0).render(
            &mut silence,
            48_000,
            2,
            &ArrayQueue::new(1),
            &AtomicU32::new(1.0_f32.to_bits()),
        );
        assert_eq!(silence, [0.0, 0.0]);
    }

    #[test]
    fn thirty_third_voice_replaces_the_oldest() {
        let queue = ArrayQueue::new(33);
        for id in 1..=33 {
            queue.push(command(id, pcm(48_000, 1, &[0.01; 8]))).unwrap();
        }
        let mut mixer = MixerCore::new(1.0);
        let mut output = [0.0_f32; 1];
        mixer.render(&mut output, 48_000, 1, &queue, &AtomicU32::new(1.0_f32.to_bits()));
        let ids = mixer.active_sample_ids_for_test();
        assert_eq!(ids.len(), 32);
        assert!(!ids.contains(&SampleId::from_raw_for_test(1)));
        assert!(ids.contains(&SampleId::from_raw_for_test(33)));
        assert!((output[0] - 0.32).abs() < 0.000_01);
    }

    #[test]
    fn clamps_overlapping_peaks_and_reuses_finished_slots() {
        let queue = ArrayQueue::new(4);
        queue.push(command(1, pcm(48_000, 1, &[0.8]))).unwrap();
        queue.push(command(2, pcm(48_000, 1, &[0.8]))).unwrap();
        let mut mixer = MixerCore::new(1.0);
        let mut output = [0.0_f32; 1];
        mixer.render(&mut output, 48_000, 1, &queue, &AtomicU32::new(1.0_f32.to_bits()));
        assert_eq!(output, [1.0]);
        assert_eq!(mixer.active_voice_count_for_test(), 0);
    }
}
```

- [ ] **Step 3: Run mixer tests and verify the red state**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml audio::mixer::tests -- --nocapture
```

Expected: compilation fails because `MixerCore` and the test command constructor do not exist.

- [ ] **Step 4: Implement voice startup, resampling, and channel rendering**

Use these exact structural types:

```rust
use std::sync::{atomic::{AtomicU32, Ordering}, Arc};
use cpal::{FromSample, SizedSample};
use crossbeam_queue::ArrayQueue;
use super::{AudioCommand, PcmSample, SampleId};

pub const MAX_VOICES: usize = 32;

struct Voice {
    sample_id: SampleId,
    sample: Arc<PcmSample>,
    source_position: f64,
    started_at: u128,
}

pub(crate) struct MixerCore {
    voices: [Option<Voice>; MAX_VOICES],
    next_sequence: u128,
    current_volume: f32,
    target_volume: f32,
    volume_step: f32,
    ramp_frames_remaining: u32,
}

impl MixerCore {
    pub(crate) fn new(initial_volume: f32) -> Self {
        Self {
            voices: std::array::from_fn(|_| None),
            next_sequence: 0,
            current_volume: initial_volume,
            target_volume: initial_volume,
            volume_step: 0.0,
            ramp_frames_remaining: 0,
        }
    }

    fn start_voice(&mut self, command: AudioCommand) {
        let slot = self.voices.iter().position(Option::is_none).unwrap_or_else(|| {
            self.voices.iter().enumerate()
                .min_by_key(|(_, voice)| voice.as_ref().map(|voice| voice.started_at).unwrap_or(u128::MAX))
                .map(|(index, _)| index)
                .expect("voice array is non-empty")
        });
        let (sample_id, sample) = command.into_parts();
        self.voices[slot] = Some(Voice {
            sample_id,
            sample,
            source_position: 0.0,
            started_at: self.next_sequence,
        });
        self.next_sequence = self.next_sequence.saturating_add(1);
    }
}
```

Implement `render<T>` with `T: SizedSample + FromSample<f32>` and these rules:

1. Drain `commands.pop()` until empty and call `start_voice` for each command.
2. Iterate `output.chunks_exact_mut(output_channels)`; fill incomplete trailing samples with equilibrium before returning.
3. For each voice, linearly interpolate the current and next source frame at `source_position`.
4. Advance by `sample.sample_rate() as f64 / output_rate as f64` once per output frame.
5. Accumulate left/right separately; downmix stereo with `(left + right) * 0.5` for mono output.
6. For output channels greater than two, write front left/right and write zero to every remaining channel.
7. Clamp each mixed value to `-1.0..=1.0`, multiply by the current master volume, clamp again, and convert with `T::from_sample`.
8. Set a voice slot to `None` after its source position reaches `frame_count()`.

Add private `sample_frame(sample, frame_index)` and `interpolated_frame(voice)` helpers that return `(left, right)` without allocation. At the final source frame, reuse that frame as the interpolation endpoint. Add `AudioCommand::new_for_test` and the two `#[cfg(test)]` mixer inspection methods used by the tests.

- [ ] **Step 5: Run mixer tests and verify green**

Run the command from Step 3. Expected: all five tests pass.

- [ ] **Step 6: Verify Task 3**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo tree --manifest-path src-tauri/Cargo.toml -e features -i cpal
```

Expected: all checks pass and CPAL has no optional features.

- [ ] **Step 7: Commit Task 3**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/audio/mod.rs src-tauri/src/audio/mixer.rs
git commit -m "feat: add deterministic fixed-voice mixer"
```

### Task 4: Click-Free Volume and Allocation-Free Rendering

**Files:**
- Create: `src-tauri/src/test_alloc.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/audio/mixer.rs`

**Interfaces:**
- Consumes: `MixerCore::render<T>` and atomic master-volume bits
- Produces: five-millisecond volume ramps and `test_alloc::allocations_during`

- [ ] **Step 1: Write failing volume and allocation tests**

Append to `mixer.rs` tests:

```rust
#[test]
fn ramps_volume_over_exactly_five_milliseconds() {
    assert_eq!(
        PcmSample::new(1_000, 1, vec![1.0; 5]),
        Err(PcmSampleError::SampleRate),
    );
    let queue = ArrayQueue::new(1);
    queue.push(command(1, pcm(8_000, 1, &[1.0; 40]))).unwrap();
    let volume = AtomicU32::new(0.0_f32.to_bits());
    let mut output = [0.0_f32; 40];
    MixerCore::new(1.0).render(&mut output, 8_000, 1, &queue, &volume);
    assert!((output[0] - 0.975).abs() < 0.000_01);
    assert_eq!(output[39], 0.0);
}

#[test]
fn rendering_allocates_nothing_after_construction() {
    let queue = ArrayQueue::new(1);
    let retained = pcm(48_000, 1, &[0.1; 64]);
    queue.push(command(1, retained.clone())).unwrap();
    let volume = AtomicU32::new(1.0_f32.to_bits());
    let mut mixer = MixerCore::new(1.0);
    let mut output = [0.0_f32; 64];
    let allocations = crate::test_alloc::allocations_during(|| {
        mixer.render(&mut output, 48_000, 1, &queue, &volume);
    });
    assert_eq!(allocations, 0);
    assert_eq!(Arc::strong_count(&retained), 1);
}
```

- [ ] **Step 2: Run the new tests and verify the red state**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml audio::mixer::tests -- --nocapture
```

Expected: the ramp assertion fails because volume changes are immediate, and compilation fails because `test_alloc` does not exist.

- [ ] **Step 3: Add the test-only allocation probe**

Add `#[cfg(test)] mod test_alloc;` to `src-tauri/src/lib.rs`. Create `test_alloc.rs`:

```rust
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

struct CountingAllocator;

thread_local! {
    static TRACKING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        TRACKING.with(|tracking| {
            if tracking.get() {
                ALLOCATIONS.with(|count| count.set(count.get() + 1));
            }
        });
        // SAFETY: delegation preserves the caller-provided layout contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: pointer and layout came from the delegated system allocator.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        TRACKING.with(|tracking| {
            if tracking.get() {
                ALLOCATIONS.with(|count| count.set(count.get() + 1));
            }
        });
        // SAFETY: delegation preserves the original allocation contract.
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

pub(crate) fn allocations_during(action: impl FnOnce()) -> usize {
    // Initialize both thread locals before tracking so first access is not counted.
    TRACKING.with(|_| {});
    ALLOCATIONS.with(|count| count.set(0));
    TRACKING.with(|tracking| tracking.set(true));
    action();
    TRACKING.with(|tracking| tracking.set(false));
    ALLOCATIONS.with(Cell::get)
}
```

- [ ] **Step 4: Implement the exact ramp policy in `MixerCore`**

At the start of `render`, load the atomic target. If it differs from `target_volume`, calculate:

```rust
let ramp_frames = (output_rate / 200).max(1); // five milliseconds
self.target_volume = requested_volume;
self.ramp_frames_remaining = ramp_frames;
self.volume_step = (self.target_volume - self.current_volume) / ramp_frames as f32;
```

Before mixing each output frame, advance the ramp:

```rust
if self.ramp_frames_remaining > 0 {
    self.current_volume += self.volume_step;
    self.ramp_frames_remaining -= 1;
    if self.ramp_frames_remaining == 0 {
        self.current_volume = self.target_volume;
    }
}
```

Apply `current_volume` after summing voices and before final clamping/conversion. Do not create a temporary `Vec` or boxed iterator anywhere in `render`.

- [ ] **Step 5: Run the tests and verify green**

Run the command from Step 2. Expected: all mixer tests pass and the measured allocation count is zero.

- [ ] **Step 6: Verify and commit Task 4**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
git add src-tauri/src/lib.rs src-tauri/src/test_alloc.rs src-tauri/src/audio/mixer.rs
git commit -m "feat: enforce real-time mixer safety"
```

### Task 5: Deterministic Recovery State Machine

**Files:**
- Create: `src-tauri/src/audio/recovery.rs`
- Modify: `src-tauri/src/audio/mod.rs`

**Interfaces:**
- Consumes: `AudioEngineStatus`
- Produces: `RecoveryController::{new, attempt_due, opened, open_failed, stream_lost, stop, status, next_attempt_at}`

- [ ] **Step 1: Write failing recovery tests**

Create `recovery.rs`, add `mod recovery;` to `audio/mod.rs`, and start with:

```rust
#[cfg(test)]
mod tests {
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
        assert_eq!(recovery.next_attempt_at(), Duration::from_millis(100) + Duration::from_secs(10));
    }

    #[test]
    fn stop_is_terminal() {
        let mut recovery = RecoveryController::new(Duration::ZERO);
        recovery.stop();
        assert_eq!(recovery.status(), AudioEngineStatus::Stopped);
        assert!(!recovery.attempt_due(Duration::from_secs(100)));
    }
}
```

- [ ] **Step 2: Run recovery tests and verify the red state**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml audio::recovery::tests -- --nocapture
```

Expected: compilation fails because `RecoveryController` does not exist.

- [ ] **Step 3: Implement the recovery controller**

Use:

```rust
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
        self.status = AudioEngineStatus::Ready;
        self.consecutive_failures = 0;
        self.was_ready = true;
    }

    pub(crate) fn open_failed(&mut self, now: Duration) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.status = if self.consecutive_failures >= 5 {
            AudioEngineStatus::Unavailable
        } else if self.was_ready {
            AudioEngineStatus::Recovering
        } else {
            AudioEngineStatus::Starting
        };
        let index = self.consecutive_failures.saturating_sub(1).min(RETRY_DELAYS.len() - 1);
        self.next_attempt_at = now.saturating_add(RETRY_DELAYS[index]);
    }

    pub(crate) fn stream_lost(&mut self, now: Duration) {
        self.status = AudioEngineStatus::Recovering;
        self.consecutive_failures = 0;
        self.next_attempt_at = now;
        self.was_ready = true;
    }

    pub(crate) fn stop(&mut self) { self.stopped = true; self.status = AudioEngineStatus::Stopped; }
    pub(crate) fn status(&self) -> AudioEngineStatus { self.status }
    pub(crate) fn next_attempt_at(&self) -> Duration { self.next_attempt_at }
}
```

- [ ] **Step 4: Run tests and verify green**

Run the command from Step 2. Expected: all three tests pass.

- [ ] **Step 5: Verify and commit Task 5**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
git add src-tauri/src/audio/mod.rs src-tauri/src/audio/recovery.rs
git commit -m "feat: define deterministic audio recovery policy"
```

### Task 6: Device-Agnostic Output Supervisor

**Files:**
- Create: `src-tauri/src/audio/cpal_output.rs`
- Modify: `src-tauri/src/audio/mod.rs`

**Interfaces:**
- Consumes: `SharedState`, `RecoveryController`, `MixerCore`
- Produces: internal `OutputBackend`, `OutputSupervisor<B>`, `SupervisorStep`, stale-command clearing, device-change handling

- [ ] **Step 1: Write failing supervisor tests with a fake backend and fake clock values**

Create `cpal_output.rs`, declare `mod cpal_output;`, and define tests around a backend whose outcomes are queued:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, sync::Arc};

    #[derive(Default)]
    struct FakeBackend {
        opens: VecDeque<Result<(FakeStream, u64), BackendFailure>>,
        default_id: Option<u64>,
        open_count: usize,
    }
    struct FakeStream;

    impl OutputBackend for FakeBackend {
        type Stream = FakeStream;
        type DeviceToken = u64;

        fn default_device_token(&mut self) -> Result<Option<Self::DeviceToken>, BackendFailure> {
            Ok(self.default_id)
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
        let handle = AudioEngineHandle { shared: shared.clone() };
        let id = handle.register_sample(PcmSample::new(48_000, 1, vec![0.0]).unwrap()).unwrap();
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
        assert_eq!(supervisor.step(Duration::from_secs(1), std::thread::current()), SupervisorStep::Stop);
        assert_eq!(shared.status(), AudioEngineStatus::Stopped);
        assert!(supervisor.stream.is_none());
    }
}
```

Add a `SharedState::status()` helper that decodes the atomic status for native internal use.

- [ ] **Step 2: Run supervisor tests and verify the red state**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml audio::cpal_output::tests -- --nocapture
```

Expected: compilation fails because the backend trait and supervisor do not exist.

- [ ] **Step 3: Implement the generic supervisor**

Define:

```rust
use std::{fmt::Debug, sync::{atomic::Ordering, Arc}, thread::Thread, time::Duration};

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
pub(crate) enum SupervisorStep { Continue, Stop }

pub(crate) struct OutputSupervisor<B: OutputBackend> {
    backend: B,
    shared: Arc<SharedState>,
    recovery: RecoveryController,
    stream: Option<B::Stream>,
    device_token: Option<B::DeviceToken>,
    next_device_check: Duration,
}
```

Implement `new` and `step(now, current_thread)` with this order:

1. If `shutdown` is true, drop `stream`, clear commands, stop recovery, publish `Stopped`, and return `Stop`.
2. Atomically swap `stream_failed` to false. If it was true, drop the stream/token, clear commands, and call `recovery.stream_lost(now)`.
3. If a stream exists and `now >= next_device_check`, call `default_device_token`. A different token or no token drops the stream, clears commands, and calls `stream_lost(now)`. Set the next check to `now + 2 seconds`.
4. If no stream exists and `recovery.attempt_due(now)`, call `open_default_stream`. On success, store stream/token, call `opened`, and set the next device check. On failure, call `open_failed(now)`.
5. Publish `recovery.status()` into `SharedState` and return `Continue`.

Do not print or preserve `BackendFailure` details.

Add the exact deadline helper used by the production loop:

```rust
pub(crate) fn wait_duration(&self, now: Duration) -> Duration {
    let deadline = if self.stream.is_some() {
        self.next_device_check
    } else {
        self.recovery.next_attempt_at()
    };
    deadline
        .saturating_sub(now)
        .min(DEVICE_CHECK_INTERVAL)
}
```

- [ ] **Step 4: Run supervisor tests and verify green**

Run the command from Step 2. Expected: all four tests pass.

- [ ] **Step 5: Verify and commit Task 6**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
git add src-tauri/src/audio/mod.rs src-tauri/src/audio/cpal_output.rs
git commit -m "feat: add recoverable audio output supervisor"
```

### Task 7: CPAL Backend, Engine Lifecycle, and Manual Smoke Example

**Files:**
- Modify: `src-tauri/src/audio/cpal_output.rs`
- Modify: `src-tauri/src/audio/mod.rs`
- Create: `src-tauri/examples/audio_smoke.rs`

**Interfaces:**
- Consumes: `OutputBackend`, `OutputSupervisor`, `MixerCore`, `SharedState`
- Produces: `CpalBackend`, `AudioEngine::{start, handle, shutdown}`, real default-device output, and `audio_smoke`

- [ ] **Step 1: Write failing CPAL selection and engine lifecycle tests**

Add to `cpal_output.rs` tests:

```rust
#[test]
fn selects_stereo_f32_at_a_standard_rate() {
    use cpal::{SampleFormat, SupportedBufferSize, SupportedStreamConfigRange};
    let range = |channels, format, min, max| {
        SupportedStreamConfigRange::new(
            channels,
            min,
            max,
            SupportedBufferSize::Range { min: 64, max: 1_024 },
            format,
        )
    };
    let selected = select_supported_config(vec![
        range(1, SampleFormat::I16, 8_000, 96_000),
        range(2, SampleFormat::F32, 44_100, 96_000),
        range(6, SampleFormat::F32, 44_100, 192_000),
    ]).unwrap();
    assert_eq!(selected.channels(), 2);
    assert_eq!(selected.sample_format(), SampleFormat::F32);
    assert_eq!(selected.sample_rate(), 48_000);
}

#[test]
fn buffer_preferences_are_low_latency_then_compatible() {
    assert_eq!(
        buffer_preferences(),
        [cpal::BufferSize::Fixed(128), cpal::BufferSize::Fixed(256), cpal::BufferSize::Default]
    );
}
```

Add to `audio/mod.rs` tests a private no-device backend implementing `OutputBackend`, then:

```rust
#[test]
fn engine_shutdown_stops_all_cloned_handles() {
    let engine = AudioEngine::start_with_backend_for_test(NoDeviceBackend).unwrap();
    let handle = engine.handle();
    let id = handle.register_sample(sample(0.0)).unwrap();
    engine.shutdown().unwrap();
    assert_eq!(handle.status(), AudioEngineStatus::Stopped);
    assert_eq!(handle.play(id), Err(PlayError::Stopped));
}
```

Create `src-tauri/examples/audio_smoke.rs` immediately with the final code from Step 5 so `cargo check --example audio_smoke` also participates in the red state.

- [ ] **Step 2: Run tests and example check to verify the red state**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml audio:: -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml --example audio_smoke
```

Expected: compilation fails because CPAL selection helpers and `AudioEngine` do not exist.

- [ ] **Step 3: Implement deterministic CPAL configuration selection**

In `cpal_output.rs`, add:

```rust
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    BufferSize, DeviceId, FromSample, SampleFormat, SizedSample,
    StreamConfig, SupportedStreamConfig, SupportedStreamConfigRange,
};

fn select_supported_config(
    ranges: impl IntoIterator<Item = SupportedStreamConfigRange>,
) -> Option<SupportedStreamConfig> {
    let range = ranges.into_iter().max_by(SupportedStreamConfigRange::cmp_default_heuristics)?;
    range.clone().try_with_standard_sample_rate().or_else(|| Some(range.with_max_sample_rate()))
}

fn buffer_preferences() -> [BufferSize; 3] {
    [BufferSize::Fixed(128), BufferSize::Fixed(256), BufferSize::Default]
}
```

This uses CPAL's own documented preference order: stereo, then mono, then larger layouts; `f32` first; 48 kHz, then 44.1 kHz, then the highest supported rate.

- [ ] **Step 4: Implement `CpalBackend` and the typed callback**

Define `CpalBackend { host: cpal::Host }`, with `new()` calling `cpal::default_host()`. Its `DeviceToken` is `DeviceId` and its `Stream` is `cpal::Stream`.

`default_device_token` must call only `host.default_output_device()` followed by `device.id()`. It must never enumerate or format device data.

`open_default_stream` must:

1. get the default output device;
2. obtain `supported_output_configs()`;
3. call `select_supported_config`;
4. obtain the private `DeviceId`;
5. attempt 128 frames, 256 frames, and default buffering in order;
6. start the first successfully built stream with `StreamTrait::play`;
7. return only `(Stream, DeviceId)` internally;
8. reduce every CPAL error to `BackendFailure` without formatting or logging it.

Use this callback builder:

```rust
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
    let output_channels = config.channels as usize;
    let initial_volume = f32::from_bits(shared.master_volume.load(Ordering::Acquire));
    let callback_shared = shared.clone();
    let error_shared = shared;
    let mut mixer = MixerCore::new(initial_volume);
    device.build_output_stream::<T, _, _>(
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
    ).map_err(|_| BackendFailure)
}
```

Dispatch `SupportedStreamConfig::sample_format()` through this helper:

```rust
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
```

For each buffer preference, clone `shared` and `supervisor`, pass the candidate `StreamConfig` to this helper, and call `play()` on a successfully built stream. This is runtime sample-format dispatch, not decoder behavior.

- [ ] **Step 5: Implement `AudioEngine` ownership and the smoke example**

In `audio/mod.rs`, define:

```rust
pub struct AudioEngine {
    handle: AudioEngineHandle,
    supervisor: Option<std::thread::JoinHandle<()>>,
}

impl AudioEngine {
    pub fn start() -> Result<Self, AudioEngineStartError> {
        Self::start_with_backend(cpal_output::CpalBackend::new())
    }

    pub fn handle(&self) -> AudioEngineHandle { self.handle.clone() }

    pub fn shutdown(mut self) -> Result<(), AudioEngineShutdownError> {
        self.stop_and_join()
    }

    fn start_with_backend<B>(backend: B) -> Result<Self, AudioEngineStartError>
    where
        B: cpal_output::OutputBackend + Send + 'static,
        B::Stream: Send + 'static,
        B::DeviceToken: Send + 'static,
    {
        let shared = Arc::new(SharedState::new());
        let thread_shared = shared.clone();
        let supervisor = std::thread::Builder::new()
            .name("keyforge-audio".into())
            .spawn(move || cpal_output::run_supervisor(backend, thread_shared))
            .map_err(AudioEngineStartError::Thread)?;
        Ok(Self { handle: AudioEngineHandle { shared }, supervisor: Some(supervisor) })
    }

    #[cfg(test)]
    fn start_with_backend_for_test<B>(backend: B) -> Result<Self, AudioEngineStartError>
    where
        B: cpal_output::OutputBackend + Send + 'static,
        B::Stream: Send + 'static,
        B::DeviceToken: Send + 'static,
    {
        Self::start_with_backend(backend)
    }

    fn stop_and_join(&mut self) -> Result<(), AudioEngineShutdownError> {
        self.handle.shared.shutdown.store(true, Ordering::Release);
        if let Some(supervisor) = self.supervisor.take() {
            supervisor.thread().unpark();
            supervisor.join().map_err(|_| AudioEngineShutdownError::SupervisorPanicked)?;
        }
        Ok(())
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) { let _ = self.stop_and_join(); }
}
```

`run_supervisor` creates `OutputSupervisor`, uses an `Instant` origin, calls `step(origin.elapsed(), thread::current())`, and parks until the earlier of the next retry or two-second device check. The stream-error callback and shutdown path unpark it immediately. It exits only on `SupervisorStep::Stop`.

Define `AudioEngineStartError::Thread(std::io::Error)` and `AudioEngineShutdownError::SupervisorPanicked`, with `Display` and `Error`. Do not expose CPAL errors.

Use this final smoke example:

```rust
use std::{error::Error, f32::consts::TAU, io, thread, time::{Duration, Instant}};
use keyforge_lib::audio::{AudioEngine, AudioEngineStatus, PcmSample};

fn main() -> Result<(), Box<dyn Error>> {
    const RATE: u32 = 48_000;
    const DURATION_MS: u32 = 150;
    const GAIN: f32 = 0.1;
    let frames = RATE as usize * DURATION_MS as usize / 1_000;
    let samples = (0..frames)
        .map(|frame| ((frame as f32 * 880.0 * TAU) / RATE as f32).sin() * GAIN)
        .collect();

    let engine = AudioEngine::start()?;
    let handle = engine.handle();
    let deadline = Instant::now() + Duration::from_secs(10);
    while handle.status() != AudioEngineStatus::Ready && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
    }
    if handle.status() != AudioEngineStatus::Ready {
        return Err(io::Error::new(io::ErrorKind::NotConnected, "audio output unavailable").into());
    }
    let id = handle.register_sample(PcmSample::new(RATE, 1, samples)?)?;
    handle.play(id)?;
    thread::sleep(Duration::from_millis(300));
    engine.shutdown()?;
    Ok(())
}
```

- [ ] **Step 6: Run tests and compile the example to verify green**

Run the commands from Step 2. Expected: all tests pass and the example compiles without opening an audio device.

- [ ] **Step 7: Verify the real backend build and commit Task 7**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
git add src-tauri/src/audio/mod.rs src-tauri/src/audio/cpal_output.rs src-tauri/examples/audio_smoke.rs
git commit -m "feat: add recoverable CPAL audio backend"
```

### Task 8: Cross-Platform CI and Security Documentation

**Files:**
- Create: `security/audio-policy.test.ts`
- Modify: `.github/workflows/ci.yml`
- Modify: `README.md`
- Modify: `docs/architecture/trust-boundaries.md`
- Modify: `docs/security/threat-model.md`

**Interfaces:**
- Consumes: complete Rust audio engine
- Produces: cross-platform compile/test gates, dependency/IPC regression tests, and M2 operational documentation

- [ ] **Step 1: Write the failing audio policy test**

Create `security/audio-policy.test.ts`:

```ts
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(resolve(process.cwd(), path), "utf8");
}

it("keeps audio native and outside the Tauri IPC permission surface", () => {
  const rust = read("src-tauri/src/lib.rs");
  const capability = JSON.parse(read("src-tauri/capabilities/main.json")) as {
    permissions: string[];
  };
  expect(rust).toContain(
    "tauri::generate_handler![commands::app_info::get_app_info]",
  );
  expect(rust).not.toMatch(/generate_handler!\[[^\]]*audio/s);
  expect(capability.permissions).toEqual([]);
});

it("uses only the approved minimal audio dependencies", () => {
  const cargo = read("src-tauri/Cargo.toml");
  expect(cargo).toContain(
    'cpal = { version = "0.18.1", default-features = false }',
  );
  expect(cargo).toContain('crossbeam-queue = "0.3.13"');
  expect(cargo).not.toMatch(/rodio|kira|symphonia|reqwest|tracing|log\s*=/);
});

it("compiles native audio on all desktop targets", () => {
  const workflow = read(".github/workflows/ci.yml");
  expect(workflow).toContain(
    "os: [ubuntu-24.04, macos-15, windows-2025]",
  );
  expect(workflow).toContain("libasound2-dev");
  expect(workflow).toContain(
    "cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets",
  );
});
```

- [ ] **Step 2: Run the policy test and verify the red state**

Run:

```bash
pnpm vitest run security/audio-policy.test.ts
```

Expected: the cross-platform CI test fails because the Rust job does not yet contain the matrix or ALSA dependency.

- [ ] **Step 3: Convert the Rust CI job to an immutable desktop matrix**

Keep the existing pinned action SHAs and replace the Rust job runner with:

```yaml
strategy:
  fail-fast: false
  matrix:
    os: [ubuntu-24.04, macos-15, windows-2025]
runs-on: ${{ matrix.os }}
```

Guard the existing apt step with `if: runner.os == 'Linux'` and include `libasound2-dev` in the installed package list. Keep Rust 1.88.0 and the existing pinned toolchain action.

Use these gates:

```yaml
- run: cargo metadata --locked --manifest-path src-tauri/Cargo.toml --no-deps --format-version 1
- if: runner.os == 'Linux'
  run: cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
- run: cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
- run: cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
```

Do not add caches, third-party audio actions, secrets, artifact uploads, or write permissions.

- [ ] **Step 4: Document only implemented M2 behavior**

Update README with:

- the native decoded-PCM boundary;
- the 32-voice and in-memory volume behavior;
- the absence of production IPC/UI integration;
- `cargo run --locked --manifest-path src-tauri/Cargo.toml --example audio_smoke` as a manual developer command;
- a warning that the command emits a short tone through the default output device.

Add a new trust boundary section stating that validated decoded PCM passes from the future pack manager into the audio registry, while encoded bytes and paths never reach the engine. Add threat-model mitigations for bounded PCM memory, bounded voices/commands, allocation-free callback work, and private device recovery.

- [ ] **Step 5: Run frontend policy/tests/build and verify green**

```bash
pnpm test
pnpm build
```

Expected: all Vitest files pass, the policy test is green, and static export succeeds.

- [ ] **Step 6: Verify workflow syntax and native tests**

```bash
ruby -e 'require "yaml"; YAML.safe_load(File.read(".github/workflows/ci.yml"), permitted_classes: [], aliases: true); puts "CI YAML parses"'
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
```

Expected: YAML parses and all Rust gates exit 0.

- [ ] **Step 7: Commit Task 8**

```bash
git add .github/workflows/ci.yml README.md docs/architecture/trust-boundaries.md docs/security/threat-model.md security/audio-policy.test.ts
git commit -m "ci: verify audio engine across desktop targets"
```

### Task 9: Final Security Review and Acceptance Verification

**Files:**
- Review only; modify files only if a finding requires a new TDD fix and focused commit

**Interfaces:**
- Consumes: all Milestone 2 tasks
- Produces: evidence that the implementation and documentation satisfy the approved spec

- [ ] **Step 1: Review the full Milestone diff**

Run:

```bash
git diff --check origin/main...HEAD
git diff --stat origin/main...HEAD
git diff origin/main...HEAD -- src-tauri/src src-tauri/Cargo.toml src-tauri/capabilities/main.json src-tauri/tauri.conf.json .github/workflows/ci.yml security README.md docs
```

Review specifically for callback allocation/locking, unbounded resources, encoded/file input, raw/native IPC exposure, new capabilities, accidental network/logging dependencies, device identity exposure, non-deterministic tests, and macOS/Windows/Linux assumptions. Classify findings as Critical, Important, or Minor; fix every Critical and Important item with a failing test first.

- [ ] **Step 2: Run the exact complete automated suite**

```bash
pnpm install --frozen-lockfile
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
```

Expected: every command exits 0 and `out/index.html` exists.

- [ ] **Step 3: Run the manual audio smoke check**

Warn the user that a short quiet tone will play, then run:

```bash
cargo run --locked --manifest-path src-tauri/Cargo.toml --example audio_smoke
```

Expected: process exits 0 and the user confirms hearing one short tone. If no device is available, report the environment limitation and do not claim the manual acceptance criterion passed.

- [ ] **Step 4: Confirm the unchanged desktop shell still launches**

Run:

```bash
pnpm tauri dev
```

Expected: Next.js binds only to `127.0.0.1`, the Tauri binary launches, `/` returns HTTP 200, and no audio device is opened because M2 is not wired into production startup. Stop the development process after confirmation.

- [ ] **Step 5: Confirm git and security invariants**

```bash
git status --short
git log --oneline origin/main..HEAD
```

Expected: clean worktree; one focused commit per completed task; no production Tauri IPC or capability delta; no keyboard, sound-pack, UI, persistence, updater, telemetry, analytics, or networking functionality.

- [ ] **Step 6: Request final code review and use the branch-finishing workflow**

Invoke `superpowers:requesting-code-review`. Address Critical and Important review findings with TDD and re-run Step 2. When all automated and manual acceptance checks pass, invoke `superpowers:finishing-a-development-branch`; do not merge without explicit user authorization.
