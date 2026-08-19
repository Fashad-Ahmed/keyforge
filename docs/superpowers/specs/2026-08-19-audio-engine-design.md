# KeyForge Milestone 2 — Audio Engine Design

**Date:** 2026-08-19  
**Status:** Approved design  
**Scope:** Native, low-latency PCM playback engine only

## Goal

Build a production-quality Rust audio subsystem that can preload validated PCM samples and play them through the operating system's default output device with low latency, bounded concurrency, in-memory master volume, deterministic tests, and automatic device recovery.

Milestone 2 establishes the audio boundary before sound-pack parsing or keyboard input exists. It does not change the frontend or the production Tauri IPC surface.

## Decisions

- Use CPAL directly for macOS, Windows, and Linux output.
- Keep mixing, voice management, resampling, volume, and recovery policy in KeyForge-owned Rust code.
- Accept validated, decoded PCM only.
- Support 32 simultaneous voices and replace the oldest voice when full.
- Recover automatically when the default output device disappears, is invalidated, or changes.
- Add no production Tauri command, permission, event, or UI.
- Provide a developer-only Rust smoke example that generates its own tone in memory.
- Use deterministic fake components for tests; automated tests must not require an audio device.

## Non-Goals

Milestone 2 does not include:

- keyboard hooks or input events;
- the `SoundEvent` domain model;
- sound-pack manifests, file access, importing, decoding, or format validation;
- bundled production sounds;
- output-device enumeration or selection;
- persistent settings;
- frontend volume or playback controls;
- Tauri IPC commands or events for audio;
- telemetry, analytics, network access, an application logging framework, or audio diagnostic logging;
- spatial audio, effects, streaming audio, clocks, or music playback.

These remain assigned to later milestones. In particular, M3 owns sound-pack files and decoding, M4 owns sanitized input events, and M6 owns product UI and persistent volume settings.

## Dependency Choice

### Selected: CPAL with a KeyForge mixer

CPAL supplies the platform output stream and structured stream/device errors. KeyForge supplies a small fixed-capacity mixer and recovery supervisor. This keeps control over latency and callback behavior while avoiding decoder and game-audio dependency trees.

The implementation will add only:

- `cpal` 0.18, with only the features required for desktop playback;
- `crossbeam-queue` 0.3, using its fixed-capacity `ArrayQueue`.

Exact patch versions will be committed in `Cargo.lock` during implementation. No decoding, recording, noise-generation, logging, telemetry, or serialization feature will be enabled through the audio dependency.

### Rejected: Rodio

Rodio reduces mixer code, but its general-purpose source model does not naturally enforce KeyForge's exact voice-stealing policy. Its current safe default output buffer is also substantially larger than the latency target unless manually overridden. Disabling all decoder and recording defaults would still leave recovery and bounded voice ownership as KeyForge responsibilities.

### Rejected: Kira

Kira provides static sounds, resource capacities, and a mock backend, but also introduces game-audio concepts KeyForge does not need: tracks, clocks, effects, spatial audio, modulators, and associated APIs. Direct CPAL plus a focused mixer is a smaller and clearer trust boundary.

## Module Architecture

The audio subsystem lives under `src-tauri/src/audio/` and has five focused units:

```text
audio/
├── mod.rs          Public Rust API and shared types
├── sample.rs       PCM validation, registry, IDs, and memory accounting
├── mixer.rs        Pure fixed-voice renderer
├── recovery.rs     Deterministic recovery state machine and backoff
└── cpal_output.rs  CPAL device selection, stream callbacks, and supervisor
```

The developer smoke example lives at:

```text
src-tauri/examples/audio_smoke.rs
```

### `AudioEngine`

`AudioEngine` is the owning RAII object. Starting it creates the control state and supervisor thread. It does not require an audio device to exist at construction time; absence of a device is a recoverable runtime state.

Its responsibilities are:

- own the supervisor thread and shutdown signal;
- own the sample registry lifetime;
- expose a clonable `AudioEngineHandle`;
- stop the stream and join the supervisor during explicit shutdown or drop.

The Tauri `run()` function will not create an `AudioEngine` in M2. There is no production consumer yet, so opening an audio device during ordinary application startup would add behavior without user value. A later integration milestone will own engine startup.

### `AudioEngineHandle`

`AudioEngineHandle` is `Clone + Send + Sync` and is the future integration point for sanitized input and settings code. It exposes only Rust methods:

```rust
pub fn register_sample(&self, sample: PcmSample) -> Result<SampleId, RegisterSampleError>;
pub fn play(&self, sample_id: SampleId) -> Result<(), PlayError>;
pub fn set_master_volume(&self, volume: f32) -> Result<(), VolumeError>;
pub fn status(&self) -> AudioEngineStatus;
```

It does not expose device names, native handles, raw callbacks, paths, encoded bytes, or decoder objects.

### `MixerCore`

`MixerCore` is platform-independent and deterministic. It owns a fixed array of 32 voice slots and renders directly into caller-provided output buffers.

It is responsible for:

- draining pending `Play` commands at the start of each callback;
- assigning a monotonically increasing start sequence to each voice;
- choosing a free voice or replacing the smallest start sequence;
- reading immutable interleaved PCM frames;
- linear sample-rate conversion to the active device rate;
- mono duplication and stereo channel handling;
- master-volume smoothing;
- summing, clamping, and native sample conversion.

The mixer has no knowledge of Tauri, files, packs, keyboards, or CPAL devices.

### `RecoveryController`

`RecoveryController` is a pure state machine driven by explicit events and timestamps. It produces actions for the supervisor but performs no sleeping or device access itself. This makes retry behavior deterministic in tests.

### `CpalOutput`

`CpalOutput` is the only platform-facing module. A dedicated supervisor thread owns the current CPAL device and stream. The CPAL data callback owns `MixerCore`; the CPAL error callback only signals the supervisor.

The supervisor:

- queries only the OS default output device;
- opens and starts a supported output stream;
- periodically checks whether the default output device changed;
- drops and recreates the stream after device loss or invalidation;
- clears stale play commands before reopening;
- updates a coarse atomic engine status;
- never publishes device identity outside the native module.

## PCM Contract

`PcmSample` represents immutable, interleaved `f32` frames with:

- a non-zero source sample rate;
- one or two channels;
- shared immutable sample storage.

Registration rejects data that violates any invariant:

- sample rate below 8,000 Hz or above 192,000 Hz;
- channel count other than mono or stereo;
- empty data;
- data length not divisible by channel count;
- any non-finite value;
- any value outside `-1.0..=1.0`;
- duration greater than 10 seconds;
- more than 512 registered samples;
- more than 128 MiB of total registered PCM.

Memory accounting uses the actual number of stored `f32` values. IDs are opaque monotonically increasing `u64` values. Identifier exhaustion returns an error rather than wrapping.

M2 intentionally has no unregister operation. The registry retains one strong reference for the engine lifetime, so ending or replacing a voice cannot deallocate sample storage on the real-time callback. M3 may add deferred reclamation when sound-pack switching is designed.

## Command Queue

Playback requests use a preallocated `crossbeam_queue::ArrayQueue` with capacity 256. It is a bounded multi-producer queue whose backing buffer is allocated at construction. A `Play` command contains the immutable sample reference required by the callback. Queue submission never blocks.

If the queue is full, `play` returns `PlayError::QueueFull`. The engine does not silently accumulate unbounded work. During device recovery, pending play commands are discarded so sounds requested while output was unavailable do not burst later.

Master volume does not consume queue capacity. It is stored as validated atomic `f32` bits and sampled once per output callback. A newly created mixer reads the latest value immediately.

## Device Configuration and Rendering

The backend prefers a supported configuration with:

1. the default output device;
2. mono or stereo output, preferring stereo;
3. the device's preferred sample rate;
4. a fixed buffer of 128 frames, then 256 frames;
5. the device's default buffer size if fixed sizes are rejected.

The selected configuration may use any CPAL-supported native sample representation. Mixing remains in `f32`, followed by clamping and conversion into the provided native output buffer.

Channel behavior is deterministic:

- mono source to mono output: copy;
- stereo source to mono output: average left and right;
- mono source to two or more outputs: duplicate into the first two channels and silence remaining channels;
- stereo source to two or more outputs: write left and right to the first two channels and silence remaining channels.

The backend prefers mono/stereo configurations to avoid ambiguous surround layouts. More-than-stereo output is a compatibility fallback only.

Linear interpolation is used for per-voice sample-rate conversion. Keyboard sounds are short, and this method is predictable, allocation-free, and sufficient for the first audio milestone. Higher-quality resampling is not added without measured need.

## Voice and Volume Policy

The mixer owns exactly 32 voice slots. Each successful play request starts from frame zero. When all slots are active, the voice with the oldest start sequence is replaced. Ties cannot occur because sequence assignment happens inside the callback.

Master volume is a linear gain constrained to `0.0..=1.0`. Invalid or non-finite values return `VolumeError`. Gain changes ramp from the current value to the new target over five milliseconds at the active output sample rate, preventing discontinuity clicks without introducing a timer or allocation.

There is no per-sample or per-play gain in M2.

## Real-Time Safety

After stream construction, the CPAL data callback must not:

- allocate or grow a collection;
- lock a mutex or wait on a condition variable;
- access files or decode audio;
- sleep, retry, or perform device discovery;
- call Tauri APIs;
- log or format diagnostic strings;
- send raw or derived audio information to the frontend.

All voice slots, queue storage, and mixer state are preallocated. Registry lookup and `Arc` cloning happen on the caller's thread. The registry keeps samples alive until engine shutdown, so dropping a callback voice only decrements a reference count and cannot free the backing PCM.

## Status and Errors

The coarse Rust-only status is:

```rust
pub enum AudioEngineStatus {
    Starting,
    Ready,
    Recovering,
    Unavailable,
    Stopped,
}
```

Status is stored atomically and contains no device identifiers or native error strings.

Typed public errors distinguish caller-actionable conditions:

- invalid PCM and capacity exhaustion during registration;
- unknown sample ID, queue full, or shutdown during playback;
- invalid master volume;
- supervisor thread creation or shutdown failure.

Backend errors remain private to `cpal_output`. They affect recovery state but are not logged, serialized, or exposed over IPC in M2.

## Recovery Policy

The error callback signals stream invalidation without doing recovery work. The supervisor drops the failed stream, clears queued play commands, and attempts to open the current default output device.

Retry delays are:

```text
100 ms, 250 ms, 500 ms, 1 s, 2 s, then 5 s repeatedly
```

The status is `Recovering` after a previously ready stream fails. After five consecutive open failures it becomes `Unavailable`, while probes continue every five seconds. A successful open resets the failure count and status to `Ready`.

While ready, the supervisor checks the default-device identity every two seconds. If it changes, the stream is rebuilt against the new default device. Device identity is used only for internal comparison and is never exposed.

Shutdown interrupts retry waiting, stops further probes, drops the stream, marks the status `Stopped`, and joins the supervisor thread.

## Testing Strategy

### PCM and registry tests

- accept valid mono and stereo PCM;
- reject every invalid rate, channel, frame, amplitude, and duration case;
- enforce sample-count and total-memory limits;
- return unique opaque IDs and detect exhaustion.

### Mixer tests

- render exact mono and stereo frames;
- downmix and expand channels according to policy;
- exercise equal-rate, upsampled, and downsampled playback;
- start multiple sounds in the same callback;
- allow 32 overlapping voices;
- steal the oldest voice on the thirty-third play;
- finish voices and reuse slots;
- clamp mixed peaks;
- apply a deterministic five-millisecond volume ramp;
- render silence with no voices;
- prove callback rendering performs no allocation after construction.

### Queue and handle tests

- reject unknown IDs;
- return `QueueFull` without blocking;
- accept calls from multiple producer threads;
- reject calls after shutdown;
- update volume independently of queue saturation.

### Recovery tests

A fake backend and fake clock drive the supervisor policy without real sleeps:

- startup with no device;
- exact retry sequence and five-second ceiling;
- transition to unavailable after five failures;
- successful recovery and retry reset;
- stream invalidation after readiness;
- default-device identity change;
- stale-command clearing;
- shutdown during retry.

### Real backend verification

Automated tests never require an audio device. The real CPAL backend is compiled on GitHub-hosted macOS, Windows, and Linux runners. Linux CI installs the required ALSA development package in addition to existing Tauri prerequisites.

The manual smoke example generates a short, low-amplitude tone, registers it, plays it through the default device, waits only long enough for completion, and exits. It reads no files and is not called by the production application.

## CI and Verification

The Rust workflow expands to a macOS, Windows, and Linux matrix. Each platform runs formatting or compilation as appropriate, Clippy with warnings denied, and all deterministic tests. The example is compiled but never automatically executed.

Milestone verification includes:

```bash
pnpm install --frozen-lockfile
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
cargo run --locked --manifest-path src-tauri/Cargo.toml --example audio_smoke
```

The smoke example is a manual acceptance check because CI runners may not provide an output device.

## Security Properties

- The audio engine accepts no file path, encoded data, script, binary, or pack metadata.
- It performs no networking and adds no network-capable dependency.
- It adds no telemetry, analytics, application logging framework, or audio diagnostic logging.
- It creates no new Tauri command, event, capability, or permission.
- Raw keyboard events and typed content remain absent from the subsystem.
- Device identifiers and backend errors remain native and private.
- Bounded samples, memory, commands, voices, and retry timing prevent unbounded resource growth.
- The real-time callback has no file, decoder, IPC, logging, blocking, or allocation behavior.

## Acceptance Criteria

Milestone 2 is complete only when:

- valid predecoded PCM can be registered and played through the default output device;
- 32 sounds can overlap and the thirty-third deterministically replaces the oldest;
- master volume is bounded, in-memory, and click-smoothed;
- the engine recovers from missing, invalidated, and changed default devices;
- the callback path is allocation-free and lock-free after initialization;
- all deterministic tests pass without an audio device;
- the backend compiles and tests on macOS, Windows, and Linux CI;
- the developer smoke example produces audible output on a supported desktop;
- the production Tauri IPC and capability files are unchanged;
- no sound-pack, keyboard-hook, UI, persistence, networking, telemetry, or updater functionality is introduced.

## Sources Consulted

- [CPAL documentation](https://docs.rs/cpal/latest/cpal/)
- [CPAL stream traits](https://docs.rs/cpal/latest/cpal/traits/trait.StreamTrait.html)
- [Rodio stream documentation](https://docs.rs/rodio/latest/rodio/stream/)
- [Kira documentation](https://docs.rs/kira/latest/kira/)
- [Crossbeam `ArrayQueue` documentation](https://docs.rs/crossbeam-queue/latest/crossbeam_queue/struct.ArrayQueue.html)
