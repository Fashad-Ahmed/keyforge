use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    Arc,
};

use cpal::{FromSample, SizedSample};
use crossbeam_queue::ArrayQueue;

use super::{AudioCommand, PcmSample, SampleId, COMMAND_QUEUE_CAPACITY};

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
        let slot = self
            .voices
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, voice)| {
                        voice
                            .as_ref()
                            .map(|voice| voice.started_at)
                            .unwrap_or(u128::MAX)
                    })
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

    pub(crate) fn render<T>(
        &mut self,
        output: &mut [T],
        output_rate: u32,
        output_channels: u16,
        commands: &ArrayQueue<AudioCommand>,
        active_stream_generation: &AtomicU64,
        master_volume: &AtomicU32,
    ) where
        T: SizedSample + FromSample<f32>,
    {
        let requested_volume = f32::from_bits(master_volume.load(Ordering::Acquire));
        if requested_volume != self.target_volume {
            let ramp_frames = (output_rate / 200).max(1);
            self.target_volume = requested_volume;
            self.ramp_frames_remaining = ramp_frames;
            self.volume_step = (self.target_volume - self.current_volume) / ramp_frames as f32;
        }

        for _ in 0..COMMAND_QUEUE_CAPACITY {
            let Some(command) = commands.pop() else {
                break;
            };
            let active_generation = active_stream_generation.load(Ordering::Acquire);
            if active_generation != 0 && command.stream_generation() == active_generation {
                self.start_voice(command);
            }
        }

        let output_channels = usize::from(output_channels);
        if output_channels == 0 {
            output.fill(T::EQUILIBRIUM);
            return;
        }

        for frame in output.chunks_exact_mut(output_channels) {
            if self.ramp_frames_remaining > 0 {
                self.current_volume += self.volume_step;
                self.ramp_frames_remaining -= 1;
                if self.ramp_frames_remaining == 0 {
                    self.current_volume = self.target_volume;
                }
            }

            let mut left = 0.0;
            let mut right = 0.0;

            for voice_slot in &mut self.voices {
                let Some(voice) = voice_slot.as_mut() else {
                    continue;
                };
                let (voice_left, voice_right) = interpolated_frame(voice);
                left += voice_left;
                right += voice_right;

                voice.source_position +=
                    f64::from(voice.sample.sample_rate()) / f64::from(output_rate);
                if voice.source_position >= voice.sample.frame_count() as f64 {
                    *voice_slot = None;
                }
            }

            if output_channels == 1 {
                frame[0] = T::from_sample(apply_volume((left + right) * 0.5, self.current_volume));
            } else {
                frame[0] = T::from_sample(apply_volume(left, self.current_volume));
                frame[1] = T::from_sample(apply_volume(right, self.current_volume));
                frame[2..].fill(T::EQUILIBRIUM);
            }
        }

        let trailing_start = output.len() - output.len() % output_channels;
        output[trailing_start..].fill(T::EQUILIBRIUM);
    }

    #[cfg(test)]
    fn active_sample_ids_for_test(&self) -> Vec<SampleId> {
        self.voices
            .iter()
            .filter_map(|voice| voice.as_ref().map(|voice| voice.sample_id))
            .collect()
    }

    #[cfg(test)]
    fn active_voice_count_for_test(&self) -> usize {
        self.voices.iter().flatten().count()
    }
}

fn sample_frame(sample: &PcmSample, frame_index: usize) -> (f32, f32) {
    let channels = sample.channels().as_usize();
    let samples = sample.samples();
    let offset = frame_index * channels;
    let left = samples[offset];
    let right = if channels == 2 {
        samples[offset + 1]
    } else {
        left
    };
    (left, right)
}

fn interpolated_frame(voice: &Voice) -> (f32, f32) {
    let current_index = voice.source_position.floor() as usize;
    let current = sample_frame(&voice.sample, current_index);
    let next = if current_index + 1 < voice.sample.frame_count() {
        sample_frame(&voice.sample, current_index + 1)
    } else {
        current
    };
    let fraction = (voice.source_position - current_index as f64) as f32;
    (
        current.0 + (next.0 - current.0) * fraction,
        current.1 + (next.1 - current.1) * fraction,
    )
}

fn apply_volume(value: f32, volume: f32) -> f32 {
    (value.clamp(-1.0, 1.0) * volume).clamp(-1.0, 1.0)
}

#[cfg(test)]
#[allow(unused_imports)]
mod tests {
    use super::*;
    use crate::audio::{AudioCommand, PcmSampleError, SampleId, COMMAND_QUEUE_CAPACITY};
    use crossbeam_queue::ArrayQueue;
    use std::sync::{
        atomic::{AtomicU32, AtomicU64},
        Arc,
    };

    const TEST_STREAM_GENERATION: u64 = 7;

    fn pcm(rate: u32, channels: u16, values: &[f32]) -> Arc<PcmSample> {
        Arc::new(PcmSample::new(rate, channels, values.to_vec()).unwrap())
    }

    fn command(id: u64, sample: Arc<PcmSample>) -> AudioCommand {
        AudioCommand::new_for_test(
            TEST_STREAM_GENERATION,
            SampleId::from_raw_for_test(id),
            sample,
        )
    }

    #[test]
    fn renders_mono_stereo_and_downmixes() {
        let queue = ArrayQueue::new(4);
        queue
            .push(command(1, pcm(48_000, 1, &[0.25, 0.5])))
            .unwrap();
        let mut mixer = MixerCore::new(1.0);
        let mut stereo = [0.0_f32; 4];
        mixer.render(
            &mut stereo,
            48_000,
            2,
            &queue,
            &AtomicU64::new(TEST_STREAM_GENERATION),
            &AtomicU32::new(1.0_f32.to_bits()),
        );
        assert_eq!(stereo, [0.25, 0.25, 0.5, 0.5]);

        queue.push(command(2, pcm(48_000, 2, &[0.2, 0.6]))).unwrap();
        let mut mono = [0.0_f32; 1];
        mixer.render(
            &mut mono,
            48_000,
            1,
            &queue,
            &AtomicU64::new(TEST_STREAM_GENERATION),
            &AtomicU32::new(1.0_f32.to_bits()),
        );
        assert!((mono[0] - 0.4).abs() < 0.000_01);
    }

    #[test]
    fn linearly_resamples_between_rates() {
        let queue = ArrayQueue::new(2);
        queue.push(command(1, pcm(24_000, 1, &[0.0, 1.0]))).unwrap();
        let mut output = [0.0_f32; 4];
        MixerCore::new(1.0).render(
            &mut output,
            48_000,
            1,
            &queue,
            &AtomicU64::new(TEST_STREAM_GENERATION),
            &AtomicU32::new(1.0_f32.to_bits()),
        );
        assert_eq!(output, [0.0, 0.5, 1.0, 1.0]);

        queue
            .push(command(2, pcm(48_000, 1, &[0.0, 0.25, 0.5, 0.75])))
            .unwrap();
        let mut downsampled = [0.0_f32; 2];
        MixerCore::new(1.0).render(
            &mut downsampled,
            24_000,
            1,
            &queue,
            &AtomicU64::new(TEST_STREAM_GENERATION),
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
            &AtomicU64::new(TEST_STREAM_GENERATION),
            &AtomicU32::new(1.0_f32.to_bits()),
        );
        assert_eq!(output, [0.25, 0.25, 0.0, 0.0, 0.0]);

        let mut silence = [1.0_f32; 2];
        MixerCore::new(1.0).render(
            &mut silence,
            48_000,
            2,
            &ArrayQueue::new(1),
            &AtomicU64::new(TEST_STREAM_GENERATION),
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
        mixer.render(
            &mut output,
            48_000,
            1,
            &queue,
            &AtomicU64::new(TEST_STREAM_GENERATION),
            &AtomicU32::new(1.0_f32.to_bits()),
        );
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
        mixer.render(
            &mut output,
            48_000,
            1,
            &queue,
            &AtomicU64::new(TEST_STREAM_GENERATION),
            &AtomicU32::new(1.0_f32.to_bits()),
        );
        assert_eq!(output, [1.0]);
        assert_eq!(mixer.active_voice_count_for_test(), 0);
    }

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
        MixerCore::new(1.0).render(
            &mut output,
            8_000,
            1,
            &queue,
            &AtomicU64::new(TEST_STREAM_GENERATION),
            &volume,
        );
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
            mixer.render(
                &mut output,
                48_000,
                1,
                &queue,
                &AtomicU64::new(TEST_STREAM_GENERATION),
                &volume,
            );
        });
        assert_eq!(allocations, 0);
        assert_eq!(Arc::strong_count(&retained), 1);
    }

    #[test]
    fn rendering_drains_at_most_one_command_queue_capacity() {
        let remaining = 44;
        let queue = ArrayQueue::new(COMMAND_QUEUE_CAPACITY + remaining);
        let retained = pcm(48_000, 1, &[0.01; 8]);
        for id in 1..=(COMMAND_QUEUE_CAPACITY + remaining) {
            queue
                .push(command(id as u64, Arc::clone(&retained)))
                .unwrap();
        }
        let mut output = [0.0_f32; 1];

        MixerCore::new(1.0).render(
            &mut output,
            48_000,
            1,
            &queue,
            &AtomicU64::new(TEST_STREAM_GENERATION),
            &AtomicU32::new(1.0_f32.to_bits()),
        );

        assert_eq!(queue.len(), remaining);
    }

    fn render_generation(command_generation: u64, active_generation: u64) -> f32 {
        let queue = ArrayQueue::new(1);
        queue
            .push(AudioCommand::new_for_test(
                command_generation,
                SampleId::from_raw_for_test(1),
                pcm(48_000, 1, &[0.25]),
            ))
            .unwrap();
        let mut output = [0.0_f32; 1];
        MixerCore::new(1.0).render(
            &mut output,
            48_000,
            1,
            &queue,
            &AtomicU64::new(active_generation),
            &AtomicU32::new(1.0_f32.to_bits()),
        );
        output[0]
    }

    #[test]
    fn discards_commands_when_no_stream_generation_is_active() {
        assert_eq!(render_generation(TEST_STREAM_GENERATION, 0), 0.0);
    }

    #[test]
    fn discards_commands_from_a_different_stream_generation() {
        assert_eq!(render_generation(TEST_STREAM_GENERATION, 8), 0.0);
    }

    #[test]
    fn renders_commands_for_the_active_stream_generation() {
        assert_eq!(
            render_generation(TEST_STREAM_GENERATION, TEST_STREAM_GENERATION),
            0.25
        );
    }
}
