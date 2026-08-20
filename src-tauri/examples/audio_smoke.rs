use std::{
    error::Error,
    f32::consts::TAU,
    io, thread,
    time::{Duration, Instant},
};

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
