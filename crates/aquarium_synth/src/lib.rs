use fundsp::prelude::{AttoHash, AudioUnit, BufferMut, BufferRef, SignalFrame};
use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;

const DEFAULT_SAMPLE_RATE: f32 = 44_100.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Waveform {
    Sine,
    Square,
    Sawtooth,
    Triangle,
    Noise,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Envelope {
    pub attack_seconds: f32,
    pub sustain_seconds: f32,
    pub decay_seconds: f32,
    pub punch: f32,
}

impl Envelope {
    pub fn percussive(sustain_seconds: f32, decay_seconds: f32) -> Self {
        Self {
            attack_seconds: 0.0,
            sustain_seconds,
            decay_seconds,
            punch: 0.0,
        }
    }

    pub fn duration_seconds(self) -> f32 {
        self.attack_seconds + self.sustain_seconds + self.decay_seconds
    }

    fn amplitude(self, age: f32) -> f32 {
        if age < 0.0 {
            return 0.0;
        }
        if self.attack_seconds > 0.0 && age < self.attack_seconds {
            return (age / self.attack_seconds).clamp(0.0, 1.0);
        }
        let sustain_start = self.attack_seconds;
        if self.sustain_seconds > 0.0 && age < sustain_start + self.sustain_seconds {
            let remaining = 1.0 - (age - sustain_start) / self.sustain_seconds;
            return 1.0 + remaining.clamp(0.0, 1.0) * 2.0 * self.punch;
        }
        let decay_start = sustain_start + self.sustain_seconds;
        if self.decay_seconds > 0.0 && age < decay_start + self.decay_seconds {
            return (1.0 - (age - decay_start) / self.decay_seconds).clamp(0.0, 1.0);
        }
        0.0
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Oscillator {
    pub waveform: Waveform,
    pub frequency_hz: f32,
    pub duty: f32,
    pub phase: f32,
}

impl Oscillator {
    pub fn sine(frequency_hz: f32) -> Self {
        Self {
            waveform: Waveform::Sine,
            frequency_hz,
            duty: 0.5,
            phase: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PitchMotion {
    pub min_frequency_hz: f32,
    pub ramp_per_second: f32,
    pub delta_ramp_per_second: f32,
    pub vibrato_depth: f32,
    pub vibrato_hz: f32,
    pub vibrato_delay_seconds: f32,
}

impl Default for PitchMotion {
    fn default() -> Self {
        Self {
            min_frequency_hz: 20.0,
            ramp_per_second: 0.0,
            delta_ramp_per_second: 0.0,
            vibrato_depth: 0.0,
            vibrato_hz: 0.0,
            vibrato_delay_seconds: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct DutyMotion {
    pub ramp_per_second: f32,
}

impl Default for DutyMotion {
    fn default() -> Self {
        Self {
            ramp_per_second: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Filter {
    pub low_pass: f32,
    pub low_pass_ramp: f32,
    pub low_pass_resonance: f32,
    pub high_pass: f32,
    pub high_pass_ramp: f32,
}

impl Default for Filter {
    fn default() -> Self {
        Self {
            low_pass: 1.0,
            low_pass_ramp: 0.0,
            low_pass_resonance: 0.0,
            high_pass: 0.0,
            high_pass_ramp: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Phaser {
    pub offset_seconds: f32,
    pub ramp_seconds_per_second: f32,
}

impl Default for Phaser {
    fn default() -> Self {
        Self {
            offset_seconds: 0.0,
            ramp_seconds_per_second: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Repeat {
    pub interval_seconds: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Arpeggio {
    pub delay_seconds: f32,
    pub multiplier: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Voice {
    pub oscillator: Oscillator,
    pub envelope: Envelope,
    pub pitch: PitchMotion,
    pub duty: DutyMotion,
    pub filter: Filter,
    pub phaser: Phaser,
    pub arpeggio: Option<Arpeggio>,
    pub gain: f32,
}

impl Voice {
    pub fn simple(oscillator: Oscillator, envelope: Envelope, gain: f32) -> Self {
        Self {
            oscillator,
            envelope,
            pitch: PitchMotion::default(),
            duty: DutyMotion::default(),
            filter: Filter::default(),
            phaser: Phaser::default(),
            arpeggio: None,
            gain,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SynthPatch {
    pub voices: Vec<Voice>,
    pub repeat: Option<Repeat>,
    pub gain: f32,
    pub soft_clip: bool,
}

impl SynthPatch {
    pub fn new(voices: Vec<Voice>) -> Self {
        Self {
            voices,
            repeat: None,
            gain: 1.0,
            soft_clip: true,
        }
    }

    pub fn duration_seconds(&self) -> f32 {
        self.voices
            .iter()
            .map(|voice| voice.envelope.duration_seconds())
            .fold(0.0, f32::max)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SfxrParams {
    pub wave_type: Waveform,
    pub base_freq: f32,
    pub freq_limit: f32,
    pub freq_ramp: f32,
    pub freq_dramp: f32,
    pub duty: f32,
    pub duty_ramp: f32,
    pub vib_strength: f32,
    pub vib_speed: f32,
    pub vib_delay: f32,
    pub env_attack: f32,
    pub env_sustain: f32,
    pub env_decay: f32,
    pub env_punch: f32,
    pub lpf_resonance: f32,
    pub lpf_freq: f32,
    pub lpf_ramp: f32,
    pub hpf_freq: f32,
    pub hpf_ramp: f32,
    pub pha_offset: f32,
    pub pha_ramp: f32,
    pub repeat_speed: f32,
    pub arp_speed: f32,
    pub arp_mod: f32,
}

impl Default for SfxrParams {
    fn default() -> Self {
        Self {
            wave_type: Waveform::Square,
            base_freq: 0.3,
            freq_limit: 0.0,
            freq_ramp: 0.0,
            freq_dramp: 0.0,
            duty: 0.0,
            duty_ramp: 0.0,
            vib_strength: 0.0,
            vib_speed: 0.0,
            vib_delay: 0.0,
            env_attack: 0.4,
            env_sustain: 0.1,
            env_decay: 0.5,
            env_punch: 0.0,
            lpf_resonance: 0.0,
            lpf_freq: 1.0,
            lpf_ramp: 0.0,
            hpf_freq: 0.0,
            hpf_ramp: 0.0,
            pha_offset: 0.0,
            pha_ramp: 0.0,
            repeat_speed: 0.0,
            arp_speed: 0.0,
            arp_mod: 0.0,
        }
    }
}

impl SfxrParams {
    pub fn blip() -> Self {
        Self {
            wave_type: Waveform::Sine,
            base_freq: 0.42,
            env_attack: 0.0,
            env_sustain: 0.13,
            env_decay: 0.08,
            hpf_freq: 0.1,
            ..Default::default()
        }
    }

    pub fn pickup() -> Self {
        Self {
            base_freq: 0.58,
            env_attack: 0.0,
            env_sustain: 0.08,
            env_decay: 0.24,
            env_punch: 0.45,
            arp_speed: 0.58,
            arp_mod: 0.34,
            ..Default::default()
        }
    }

    pub fn laser() -> Self {
        Self {
            wave_type: Waveform::Sawtooth,
            base_freq: 0.72,
            freq_limit: 0.18,
            freq_ramp: -0.42,
            duty: 0.38,
            duty_ramp: -0.16,
            env_attack: 0.0,
            env_sustain: 0.19,
            env_decay: 0.18,
            pha_offset: 0.09,
            pha_ramp: -0.12,
            hpf_freq: 0.04,
            ..Default::default()
        }
    }

    pub fn to_patch(self) -> SynthPatch {
        let base_frequency = sfxr_frequency_hz(self.base_freq);
        let envelope = Envelope {
            attack_seconds: normalized_env_seconds(self.env_attack),
            sustain_seconds: normalized_env_seconds(self.env_sustain),
            decay_seconds: normalized_env_seconds(self.env_decay),
            punch: self.env_punch.clamp(-1.0, 1.0),
        };
        let voice = Voice {
            oscillator: Oscillator {
                waveform: self.wave_type,
                frequency_hz: base_frequency,
                duty: 0.5 - self.duty.clamp(0.0, 1.0) * 0.5,
                phase: 0.0,
            },
            envelope,
            pitch: PitchMotion {
                min_frequency_hz: sfxr_frequency_hz(self.freq_limit).min(base_frequency),
                ramp_per_second: -self.freq_ramp.powi(3) * 9.5,
                delta_ramp_per_second: -self.freq_dramp.powi(3) * 0.65,
                vibrato_depth: self.vib_strength.clamp(0.0, 1.0) * 0.5,
                vibrato_hz: 2.0 + self.vib_speed.clamp(0.0, 1.0).powi(2) * 18.0,
                vibrato_delay_seconds: self.vib_delay.clamp(0.0, 1.0).powi(2) * 0.8,
            },
            duty: DutyMotion {
                ramp_per_second: -self.duty_ramp * 0.35,
            },
            filter: Filter {
                low_pass: self.lpf_freq.clamp(0.0, 1.0),
                low_pass_ramp: self.lpf_ramp.clamp(-1.0, 1.0),
                low_pass_resonance: self.lpf_resonance.clamp(0.0, 1.0),
                high_pass: self.hpf_freq.clamp(0.0, 1.0),
                high_pass_ramp: self.hpf_ramp.clamp(-1.0, 1.0),
            },
            phaser: Phaser {
                offset_seconds: self.pha_offset.signum() * self.pha_offset.powi(2).abs() * 0.018,
                ramp_seconds_per_second: self.pha_ramp.signum()
                    * self.pha_ramp.powi(2).abs()
                    * 0.035,
            },
            arpeggio: if self.arp_speed > 0.0 {
                Some(Arpeggio {
                    delay_seconds: (1.0 - self.arp_speed).powi(2) * 0.46 + 0.0007,
                    multiplier: if self.arp_mod >= 0.0 {
                        1.0 / (1.0 - self.arp_mod.powi(2) * 0.9).max(0.1)
                    } else {
                        (1.0 - self.arp_mod.powi(2) * 0.75).max(0.1)
                    },
                })
            } else {
                None
            },
            gain: 0.22,
        };
        let mut patch = SynthPatch::new(vec![voice]);
        patch.repeat = if self.repeat_speed > 0.0 {
            Some(Repeat {
                interval_seconds: (1.0 - self.repeat_speed).powi(2) * 0.46 + 0.02,
            })
        } else {
            None
        };
        patch
    }
}

#[derive(Clone, Debug)]
pub struct PatchUnit {
    player: PatchPlayer,
}

impl PatchUnit {
    pub fn new(patch: SynthPatch) -> Self {
        Self {
            player: PatchPlayer::new(patch, DEFAULT_SAMPLE_RATE),
        }
    }
}

impl AudioUnit for PatchUnit {
    fn reset(&mut self) {
        self.player.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.player.set_sample_rate(sample_rate as f32);
    }

    fn tick(&mut self, _input: &[f32], output: &mut [f32]) {
        let value = self.player.next_sample();
        if let Some(left) = output.get_mut(0) {
            *left = value;
        }
        if let Some(right) = output.get_mut(1) {
            *right = value;
        }
    }

    fn process(&mut self, size: usize, _input: &BufferRef, output: &mut BufferMut) {
        for index in 0..size {
            let value = self.player.next_sample();
            for channel in 0..output.channels() {
                output.set_f32(channel, index, value);
            }
        }
    }

    fn inputs(&self) -> usize {
        0
    }

    fn outputs(&self) -> usize {
        1
    }

    fn route(&mut self, _input: &SignalFrame, _frequency: f64) -> SignalFrame {
        SignalFrame::new(1)
    }

    fn get_id(&self) -> u64 {
        0xa901_05f7_5caa_0001
    }

    fn ping(&mut self, probe: bool, hash: AttoHash) -> AttoHash {
        if !probe {
            self.player.seed = hash.state();
        }
        hash.hash(self.get_id())
    }

    fn footprint(&self) -> usize {
        std::mem::size_of::<Self>()
    }
}

#[derive(Clone, Debug)]
pub struct PatchPlayer {
    patch: SynthPatch,
    voices: Vec<VoiceState>,
    sample_rate: f32,
    sample_index: u64,
    seed: u64,
}

impl PatchPlayer {
    pub fn new(patch: SynthPatch, sample_rate: f32) -> Self {
        let mut player = Self {
            voices: Vec::new(),
            patch,
            sample_rate,
            sample_index: 0,
            seed: 0x8a5c_51f7_d15c_a11d,
        };
        player.reset();
        player
    }

    pub fn reset(&mut self) {
        self.sample_index = 0;
        self.voices = self.patch.voices.iter().map(VoiceState::new).collect();
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(8_000.0);
        self.reset();
    }

    pub fn next_sample(&mut self) -> f32 {
        let age = self.sample_index as f32 / self.sample_rate;
        let repeat_age = match self.patch.repeat {
            Some(repeat) => age % repeat.interval_seconds.max(1.0 / self.sample_rate),
            None => age,
        };
        let mut value = 0.0;
        let sample_rate = self.sample_rate;
        for (voice, state) in self.patch.voices.iter().zip(self.voices.iter_mut()) {
            value += state.next_sample(voice, repeat_age, sample_rate, self.seed) * voice.gain;
        }
        value *= self.patch.gain;
        self.sample_index = self.sample_index.saturating_add(1);
        if self.patch.soft_clip {
            (value * 1.35).tanh()
        } else {
            value.clamp(-1.0, 1.0)
        }
    }
}

#[derive(Clone, Debug)]
struct VoiceState {
    phase: f32,
    noise_epoch: u32,
    low_pass_position: f32,
    low_pass_delta: f32,
    high_pass_position: f32,
    phaser_cursor: usize,
    phaser_buffer: Vec<f32>,
}

impl VoiceState {
    fn new(_voice: &Voice) -> Self {
        Self {
            phase: 0.0,
            noise_epoch: 0,
            low_pass_position: 0.0,
            low_pass_delta: 0.0,
            high_pass_position: 0.0,
            phaser_cursor: 0,
            phaser_buffer: vec![0.0; 2048],
        }
    }

    fn next_sample(&mut self, voice: &Voice, age: f32, sample_rate: f32, seed: u64) -> f32 {
        let envelope = voice.envelope.amplitude(age);
        if envelope <= 0.0 {
            return 0.0;
        }

        let mut frequency = frequency_at(voice, age);
        if let Some(arpeggio) = voice.arpeggio {
            if age >= arpeggio.delay_seconds {
                frequency *= arpeggio.multiplier;
            }
        }
        let duty = (voice.oscillator.duty + voice.duty.ramp_per_second * age).clamp(0.02, 0.98);
        let previous_phase = self.phase;
        self.phase = (self.phase + frequency / sample_rate).fract();
        if self.phase < previous_phase {
            self.noise_epoch = self.noise_epoch.wrapping_add(1);
        }
        let mut sample = oscillator_sample(
            voice.oscillator.waveform,
            self.phase + voice.oscillator.phase,
            duty,
            seed ^ self.noise_epoch as u64,
        );
        sample = self.filter(voice.filter, sample, age);
        sample = self.phaser(voice.phaser, sample, age, sample_rate);
        sample * envelope
    }

    fn filter(&mut self, filter: Filter, sample: f32, age: f32) -> f32 {
        let low_pass = (filter.low_pass * (1.0 + filter.low_pass_ramp * age * 1.8)).clamp(0.0, 1.0);
        let high_pass =
            (filter.high_pass * (1.0 + filter.high_pass_ramp * age * 2.0)).clamp(0.0, 1.0);
        let low_pass_width = low_pass.powi(3) * 0.1;
        let damping = (5.0 / (1.0 + filter.low_pass_resonance.powi(2) * 20.0)
            * (0.01 + low_pass_width))
            .min(0.8);
        let previous = self.low_pass_position;
        if low_pass_width > 0.0 {
            self.low_pass_delta += (sample - self.low_pass_position) * low_pass_width;
            self.low_pass_delta -= self.low_pass_delta * damping;
        } else {
            self.low_pass_position = sample;
            self.low_pass_delta = 0.0;
        }
        self.low_pass_position += self.low_pass_delta;
        let high_pass_width = (high_pass.powi(2) * 0.1).clamp(0.00001, 0.1);
        self.high_pass_position += self.low_pass_position - previous;
        self.high_pass_position -= self.high_pass_position * high_pass_width;
        self.high_pass_position
    }

    fn phaser(&mut self, phaser: Phaser, sample: f32, age: f32, sample_rate: f32) -> f32 {
        let offset = (phaser.offset_seconds + phaser.ramp_seconds_per_second * age).abs();
        if offset <= 0.00001 {
            return sample;
        }
        let delay_samples = (offset * sample_rate) as usize;
        let delay_samples = delay_samples.min(self.phaser_buffer.len() - 1);
        let read = (self.phaser_cursor + self.phaser_buffer.len() - delay_samples)
            % self.phaser_buffer.len();
        let delayed = self.phaser_buffer[read];
        self.phaser_buffer[self.phaser_cursor] = sample;
        self.phaser_cursor = (self.phaser_cursor + 1) % self.phaser_buffer.len();
        (sample + delayed) * 0.5
    }
}

pub mod presets {
    use super::*;

    pub fn aquarium_pluck() -> SynthPatch {
        let envelope = Envelope {
            attack_seconds: 0.002,
            sustain_seconds: 0.09,
            decay_seconds: 0.43,
            punch: 0.38,
        };
        SynthPatch {
            voices: vec![
                Voice::simple(Oscillator::sine(440.0), envelope, 0.18),
                Voice::simple(
                    Oscillator {
                        waveform: Waveform::Triangle,
                        frequency_hz: 880.0,
                        duty: 0.5,
                        phase: 0.0,
                    },
                    envelope,
                    0.05,
                ),
                Voice::simple(Oscillator::sine(1760.0), envelope, 0.018),
            ],
            repeat: None,
            gain: 0.95,
            soft_clip: true,
        }
    }

    pub fn aquarium_heartbeat() -> SynthPatch {
        let envelope = Envelope {
            attack_seconds: 0.004,
            sustain_seconds: 0.08,
            decay_seconds: 0.2,
            punch: 0.62,
        };
        SynthPatch {
            voices: vec![
                Voice::simple(Oscillator::sine(72.0), envelope, 0.22),
                Voice::simple(Oscillator::sine(116.0), envelope, 0.09),
            ],
            repeat: None,
            gain: 0.9,
            soft_clip: true,
        }
    }
}

fn frequency_at(voice: &Voice, age: f32) -> f32 {
    let slide =
        voice.pitch.ramp_per_second * age + 0.5 * voice.pitch.delta_ramp_per_second * age * age;
    let mut frequency = voice.oscillator.frequency_hz * 2.0_f32.powf(slide);
    if age >= voice.pitch.vibrato_delay_seconds && voice.pitch.vibrato_depth > 0.0 {
        let vibrato_age = age - voice.pitch.vibrato_delay_seconds;
        frequency *=
            1.0 + (vibrato_age * voice.pitch.vibrato_hz * TAU).sin() * voice.pitch.vibrato_depth;
    }
    frequency
        .max(voice.pitch.min_frequency_hz)
        .clamp(10.0, 22_000.0)
}

fn oscillator_sample(waveform: Waveform, phase: f32, duty: f32, seed: u64) -> f32 {
    let phase = phase.fract();
    match waveform {
        Waveform::Sine => (phase * TAU).sin(),
        Waveform::Square => {
            if phase < duty {
                0.5
            } else {
                -0.5
            }
        }
        Waveform::Sawtooth => {
            if phase < duty {
                -1.0 + 2.0 * phase / duty.max(0.001)
            } else {
                1.0 - 2.0 * (phase - duty) / (1.0 - duty).max(0.001)
            }
        }
        Waveform::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
        Waveform::Noise => hash_noise(seed, (phase * 32.0) as u32),
    }
}

fn hash_noise(seed: u64, slot: u32) -> f32 {
    let mut value = seed ^ (slot as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    value ^= value >> 33;
    value = value.wrapping_mul(0xff51_afd7_ed55_8ccd);
    value ^= value >> 33;
    value = value.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    value ^= value >> 33;
    ((value >> 40) as f32 / 8_388_608.0) * 2.0 - 1.0
}

fn sfxr_frequency_hz(value: f32) -> f32 {
    let period = 100.0 / (value.clamp(0.0, 1.0).powi(2) + 0.001);
    (DEFAULT_SAMPLE_RATE / period).clamp(20.0, 20_000.0)
}

fn normalized_env_seconds(value: f32) -> f32 {
    value.clamp(0.0, 1.0).powi(2) * 100_000.0 / DEFAULT_SAMPLE_RATE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sfxr_mapping_preserves_basic_surface() {
        let patch = SfxrParams::laser().to_patch();
        assert_eq!(patch.voices.len(), 1);
        assert_eq!(patch.voices[0].oscillator.waveform, Waveform::Sawtooth);
        assert!(patch.voices[0].pitch.ramp_per_second > 0.0);
        assert!(patch.duration_seconds() > 0.1);
    }

    #[test]
    fn patch_player_generates_non_silent_audio() {
        let mut player = PatchPlayer::new(presets::aquarium_pluck(), 44_100.0);
        let peak = (0..4096)
            .map(|_| player.next_sample().abs())
            .fold(0.0, f32::max);
        assert!(peak > 0.01);
    }

    #[test]
    fn envelope_reaches_silence() {
        let envelope = Envelope::percussive(0.01, 0.01);
        assert_eq!(envelope.amplitude(1.0), 0.0);
    }
}
