use fundsp::prelude::{AttoHash, AudioUnit, BufferMut, BufferRef, SignalFrame};
use rustfft::{Fft, FftPlanner, num_complex::Complex};
use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;
use std::sync::Arc;
use std::{error::Error, fmt};

const DEFAULT_SAMPLE_RATE: f32 = 44_100.0;

pub const PATCH_SCRIPT_EXAMPLE: &str = r#"
# One command per line. Comments start with #.
patch gain=0.7 soft_clip=true
lfo name=wobble target=pitch wave=sine hz=5 depth=0.01
lfo name=shimmer target=formant wave=triangle hz=2 depth=0.08
voice wave=sine freq=220 gain=0.12 attack=0.002 sustain=0.03 decay=0.2 vibrato=0.02 vibrato_hz=5 formants=620:90:1,1040:150:0.8 formant_mix=0.45 mods=formant:sine:2:0.12,pitch:triangle:3:0.015
voice wave=triangle freq=440 gain=0.04 attack=0 sustain=0.02 decay=0.18 lpf=0.7 hpf=0.02 mods=gain:sine:8:0.18,lpf:hold:12:0.08
sfxr preset=laser mutate_seed=9 mutate=0.01
"#;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioAnalysisConfig {
    pub sample_rate: f32,
    pub gate_floor: f32,
    pub gate_ratio: f32,
    pub fft_size: usize,
    pub hop_size: usize,
    pub mel_band_count: usize,
    pub min_frequency_hz: f32,
    pub max_frequency_hz: f32,
}

impl Default for AudioAnalysisConfig {
    fn default() -> Self {
        Self {
            sample_rate: DEFAULT_SAMPLE_RATE,
            gate_floor: 0.0005,
            gate_ratio: 0.03,
            fft_size: 512,
            hop_size: 128,
            mel_band_count: 32,
            min_frequency_hz: 40.0,
            max_frequency_hz: 16_000.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioFeatures {
    pub attack_seconds: f32,
    pub duration_seconds: f32,
    pub peak: f32,
    pub rms: f32,
    pub zero_crossing_rate: f32,
    pub spectral_centroid_hz: f32,
    pub spectral_rolloff_hz: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Spectrogram {
    pub frames: usize,
    pub bands: usize,
    pub values: Vec<f32>,
}

impl Spectrogram {
    pub fn at(&self, frame: usize, band: usize) -> f32 {
        self.values[frame * self.bands + band]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioAnalysis {
    pub features: AudioFeatures,
    pub log_mel_spectrogram: Spectrogram,
    pub rms_envelope: Vec<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioComparison {
    pub reference: AudioAnalysis,
    pub candidate: AudioAnalysis,
    pub duration_ratio: f32,
    pub rms_ratio: f32,
    pub zero_crossing_ratio: f32,
    pub centroid_ratio: f32,
    pub envelope_distance: f32,
    pub log_mel_distance: f32,
    pub score: f32,
}

pub struct AudioAnalyzer {
    config: AudioAnalysisConfig,
    fft_size: usize,
    fft: Arc<dyn Fft<f32>>,
    fft_buffer: Vec<Complex<f32>>,
    spectrum_buffer: Vec<f32>,
    mel_edges: Vec<usize>,
}

impl AudioAnalyzer {
    pub fn new(config: AudioAnalysisConfig) -> Self {
        let fft_size = config.fft_size.max(32).next_power_of_two();
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(fft_size);
        let mel_edges = mel_band_edges(
            config.mel_band_count.max(1),
            fft_size,
            config.sample_rate,
            config.min_frequency_hz,
            config.max_frequency_hz,
        );
        Self {
            config,
            fft_size,
            fft,
            fft_buffer: vec![Complex::default(); fft_size],
            spectrum_buffer: vec![0.0; fft_size / 2 + 1],
            mel_edges,
        }
    }

    pub fn config(&self) -> &AudioAnalysisConfig {
        &self.config
    }

    pub fn analyze(&mut self, samples: &[f32]) -> AudioAnalysis {
        let features = self.extract_features(samples);
        let rms_envelope = rms_envelope(samples, self.fft_size, self.config.hop_size);
        let log_mel_spectrogram = self.log_mel_spectrogram(samples);
        AudioAnalysis {
            features,
            log_mel_spectrogram,
            rms_envelope,
        }
    }

    pub fn compare(
        &mut self,
        reference_samples: &[f32],
        candidate_samples: &[f32],
    ) -> AudioComparison {
        let reference = self.analyze(reference_samples);
        let candidate = self.analyze(candidate_samples);
        comparison_from_analysis(reference, candidate)
    }

    fn extract_features(&mut self, samples: &[f32]) -> AudioFeatures {
        let peak = samples
            .iter()
            .map(|sample| sample.abs())
            .fold(0.0, f32::max);
        let gate = (peak * self.config.gate_ratio).max(self.config.gate_floor);
        let first = samples
            .iter()
            .position(|sample| sample.abs() >= gate)
            .unwrap_or(0);
        let last = samples
            .iter()
            .rposition(|sample| sample.abs() >= gate)
            .unwrap_or(first);
        let active = if samples.is_empty() {
            &[][..]
        } else {
            &samples[first..=last]
        };
        let rms = mean_square(active).sqrt();
        let zero_crossings = active
            .windows(2)
            .filter(|pair| pair[0].signum() != pair[1].signum())
            .count();
        let duration_seconds = (last.saturating_sub(first) + usize::from(!samples.is_empty()))
            as f32
            / self.config.sample_rate.max(1.0);
        let spectrum = self.average_power_spectrum(active);
        let (spectral_centroid_hz, spectral_rolloff_hz) =
            spectral_shape(&spectrum, self.config.sample_rate, 0.85);
        AudioFeatures {
            attack_seconds: first as f32 / self.config.sample_rate.max(1.0),
            duration_seconds,
            peak,
            rms,
            zero_crossing_rate: zero_crossings as f32
                / duration_seconds.max(1.0 / self.config.sample_rate),
            spectral_centroid_hz,
            spectral_rolloff_hz,
        }
    }

    fn log_mel_spectrogram(&mut self, samples: &[f32]) -> Spectrogram {
        let bands = self.config.mel_band_count.max(1);
        if samples.is_empty() {
            return Spectrogram {
                frames: 1,
                bands,
                values: vec![0.0; bands],
            };
        }
        let mut values = Vec::new();
        let mut frames = 0;
        let mut start = 0;
        while start < samples.len() {
            self.write_frame_power_spectrum(samples, start);
            for band in 0..bands {
                let left = self.mel_edges[band];
                let center = self.mel_edges[band + 1].max(left + 1);
                let right = self.mel_edges[band + 2].max(center + 1);
                let mut energy = 0.0;
                let mut weight_sum = 0.0;
                for bin in left..right.min(self.spectrum_buffer.len()) {
                    let weight = if bin <= center {
                        (bin - left) as f32 / (center - left).max(1) as f32
                    } else {
                        (right - bin) as f32 / (right - center).max(1) as f32
                    }
                    .max(0.0);
                    energy += self.spectrum_buffer[bin] * weight;
                    weight_sum += weight;
                }
                values.push((energy / weight_sum.max(1.0) + 1.0e-9).ln());
            }
            frames += 1;
            start += self.config.hop_size.max(1);
        }
        normalize_in_place(&mut values);
        Spectrogram {
            frames,
            bands,
            values,
        }
    }

    fn average_power_spectrum(&mut self, samples: &[f32]) -> Vec<f32> {
        if samples.is_empty() {
            return vec![0.0; self.fft_size / 2 + 1];
        }
        let hop_size = (self.fft_size / 2).max(1);
        let mut sum = vec![0.0; self.fft_size / 2 + 1];
        let mut frames = 0.0;
        let mut start = 0;
        while start < samples.len() {
            self.write_frame_power_spectrum(samples, start);
            for (target, value) in sum.iter_mut().zip(&self.spectrum_buffer) {
                *target += *value;
            }
            frames += 1.0;
            start += hop_size;
        }
        for value in &mut sum {
            *value /= frames;
        }
        sum
    }

    fn write_frame_power_spectrum(&mut self, samples: &[f32], start: usize) {
        for offset in 0..self.fft_size {
            let sample =
                samples.get(start + offset).copied().unwrap_or(0.0) * hann(offset, self.fft_size);
            self.fft_buffer[offset] = Complex::new(sample, 0.0);
        }
        self.fft.process(&mut self.fft_buffer);
        for (bin, output) in self.spectrum_buffer.iter_mut().enumerate() {
            *output = self.fft_buffer[bin].norm_sqr();
        }
    }
}

pub fn analyze_audio(samples: &[f32], config: &AudioAnalysisConfig) -> AudioAnalysis {
    AudioAnalyzer::new(config.clone()).analyze(samples)
}

pub fn compare_audio(
    reference_samples: &[f32],
    candidate_samples: &[f32],
    config: &AudioAnalysisConfig,
) -> AudioComparison {
    AudioAnalyzer::new(config.clone()).compare(reference_samples, candidate_samples)
}

fn comparison_from_analysis(reference: AudioAnalysis, candidate: AudioAnalysis) -> AudioComparison {
    let duration_ratio = safe_ratio(
        candidate.features.duration_seconds,
        reference.features.duration_seconds,
    );
    let rms_ratio = safe_ratio(candidate.features.rms, reference.features.rms);
    let zero_crossing_ratio = safe_ratio(
        candidate.features.zero_crossing_rate,
        reference.features.zero_crossing_rate,
    );
    let centroid_ratio = safe_ratio(
        candidate.features.spectral_centroid_hz,
        reference.features.spectral_centroid_hz,
    );
    let envelope_distance = normalized_distance(&reference.rms_envelope, &candidate.rms_envelope);
    let log_mel_distance = normalized_distance(
        &reference.log_mel_spectrogram.values,
        &candidate.log_mel_spectrogram.values,
    );
    let ratio_penalty = ratio_distance(duration_ratio)
        + ratio_distance(rms_ratio)
        + ratio_distance(zero_crossing_ratio) * 0.5
        + ratio_distance(centroid_ratio) * 0.5;
    let score = 1.0 / (1.0 + envelope_distance * 0.7 + log_mel_distance + ratio_penalty);
    AudioComparison {
        reference,
        candidate,
        duration_ratio,
        rms_ratio,
        zero_crossing_ratio,
        centroid_ratio,
        envelope_distance,
        log_mel_distance,
        score,
    }
}

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

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Formant {
    pub frequency_hz: f32,
    pub bandwidth_hz: f32,
    pub gain: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct VoiceColor {
    pub noise_mix: f32,
    pub drive: f32,
    pub fold: f32,
    pub tremolo_depth: f32,
    pub tremolo_hz: f32,
    pub formant_mix: f32,
}

impl Default for VoiceColor {
    fn default() -> Self {
        Self {
            noise_mix: 0.0,
            drive: 0.0,
            fold: 0.0,
            tremolo_depth: 0.0,
            tremolo_hz: 0.0,
            formant_mix: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ModTarget {
    Gain,
    Pitch,
    Duty,
    LowPass,
    HighPass,
    Noise,
    Drive,
    Fold,
    FormantMix,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ModWaveform {
    Sine,
    Triangle,
    Square,
    SampleHold,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Modulator {
    pub target: ModTarget,
    pub waveform: ModWaveform,
    pub frequency_hz: f32,
    pub depth: f32,
    pub phase: f32,
    pub bias: f32,
}

impl Modulator {
    pub fn lfo(target: ModTarget, waveform: ModWaveform, frequency_hz: f32, depth: f32) -> Self {
        Self {
            target,
            waveform,
            frequency_hz,
            depth,
            phase: 0.0,
            bias: 0.0,
        }
    }

    pub fn with_phase(mut self, phase: f32) -> Self {
        self.phase = phase;
        self
    }

    pub fn with_bias(mut self, bias: f32) -> Self {
        self.bias = bias;
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ControlLane {
    pub name: String,
    pub modulator: Modulator,
}

impl ControlLane {
    pub fn new(name: impl Into<String>, modulator: Modulator) -> Self {
        Self {
            name: name.into(),
            modulator,
        }
    }
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
    pub color: VoiceColor,
    pub formants: Vec<Formant>,
    pub modulators: Vec<Modulator>,
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
            color: VoiceColor::default(),
            formants: Vec::new(),
            modulators: Vec::new(),
            gain,
        }
    }

    pub fn with_modulator(mut self, modulator: Modulator) -> Self {
        self.modulators.push(modulator);
        self
    }

    pub fn with_formant(mut self, formant: Formant) -> Self {
        self.formants.push(formant);
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SynthPatch {
    pub voices: Vec<Voice>,
    pub controls: Vec<ControlLane>,
    pub repeat: Option<Repeat>,
    pub gain: f32,
    pub soft_clip: bool,
}

impl SynthPatch {
    pub fn new(voices: Vec<Voice>) -> Self {
        Self {
            voices,
            controls: Vec::new(),
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

    pub fn from_script(script: &str) -> Result<Self, PatchScriptError> {
        parse_patch_script(script)
    }
}

#[derive(Clone, Debug)]
pub struct PatchBuilder {
    patch: SynthPatch,
}

impl Default for PatchBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PatchBuilder {
    pub fn new() -> Self {
        Self {
            patch: SynthPatch::new(Vec::new()),
        }
    }

    pub fn gain(mut self, gain: f32) -> Self {
        self.patch.gain = gain;
        self
    }

    pub fn soft_clip(mut self, soft_clip: bool) -> Self {
        self.patch.soft_clip = soft_clip;
        self
    }

    pub fn repeat(mut self, interval_seconds: f32) -> Self {
        self.patch.repeat = (interval_seconds > 0.0).then_some(Repeat { interval_seconds });
        self
    }

    pub fn control(mut self, lane: ControlLane) -> Self {
        self.patch.controls.push(lane);
        self
    }

    pub fn lfo(
        self,
        name: impl Into<String>,
        target: ModTarget,
        waveform: ModWaveform,
        frequency_hz: f32,
        depth: f32,
    ) -> Self {
        self.control(ControlLane::new(
            name,
            Modulator::lfo(target, waveform, frequency_hz, depth),
        ))
    }

    pub fn voice(mut self, voice: Voice) -> Self {
        self.patch.voices.push(voice);
        self
    }

    pub fn build(self) -> SynthPatch {
        self.patch
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct RenderOptions {
    pub sample_rate: f32,
    pub duration_seconds: f32,
    pub seed: u64,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            sample_rate: DEFAULT_SAMPLE_RATE,
            duration_seconds: 1.0,
            seed: 0x8a5c_51f7_d15c_a11d,
        }
    }
}

pub fn render_patch_mono(patch: SynthPatch, options: RenderOptions) -> Vec<f32> {
    let sample_count =
        (options.sample_rate.max(1.0) * options.duration_seconds.max(0.0)).ceil() as usize;
    let mut player = PatchPlayer::new(patch, options.sample_rate);
    player.set_seed(options.seed);
    let mut output = vec![0.0; sample_count];
    player.render_mono(&mut output);
    output
}

pub fn render_patch_interleaved_stereo(patch: SynthPatch, options: RenderOptions) -> Vec<f32> {
    let frame_count =
        (options.sample_rate.max(1.0) * options.duration_seconds.max(0.0)).ceil() as usize;
    let mut player = PatchPlayer::new(patch, options.sample_rate);
    player.set_seed(options.seed);
    let mut output = vec![0.0; frame_count * 2];
    player.render_interleaved_stereo(&mut output);
    output
}

pub fn render_script_mono(
    script: &str,
    options: RenderOptions,
) -> Result<Vec<f32>, PatchScriptError> {
    Ok(render_patch_mono(SynthPatch::from_script(script)?, options))
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
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "blip" => Some(Self::blip()),
            "pickup" | "coin" => Some(Self::pickup()),
            "laser" | "shoot" => Some(Self::laser()),
            "explosion" => Some(Self::explosion()),
            "powerup" => Some(Self::powerup()),
            "hit" | "hurt" => Some(Self::hit()),
            "jump" => Some(Self::jump()),
            _ => None,
        }
    }

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

    pub fn explosion() -> Self {
        Self {
            wave_type: Waveform::Noise,
            base_freq: 0.18,
            freq_ramp: -0.15,
            env_attack: 0.0,
            env_sustain: 0.29,
            env_decay: 0.36,
            env_punch: 0.52,
            pha_offset: -0.22,
            pha_ramp: -0.1,
            vib_strength: 0.22,
            vib_speed: 0.28,
            repeat_speed: 0.42,
            ..Default::default()
        }
    }

    pub fn powerup() -> Self {
        Self {
            wave_type: Waveform::Sine,
            base_freq: 0.36,
            freq_ramp: 0.28,
            env_attack: 0.0,
            env_sustain: 0.24,
            env_decay: 0.28,
            repeat_speed: 0.55,
            vib_strength: 0.18,
            vib_speed: 0.33,
            ..Default::default()
        }
    }

    pub fn hit() -> Self {
        Self {
            wave_type: Waveform::Noise,
            base_freq: 0.34,
            freq_ramp: -0.48,
            env_attack: 0.0,
            env_sustain: 0.05,
            env_decay: 0.2,
            hpf_freq: 0.12,
            ..Default::default()
        }
    }

    pub fn jump() -> Self {
        Self {
            wave_type: Waveform::Square,
            duty: 0.24,
            base_freq: 0.42,
            freq_ramp: 0.22,
            env_attack: 0.0,
            env_sustain: 0.22,
            env_decay: 0.18,
            hpf_freq: 0.05,
            lpf_freq: 0.72,
            ..Default::default()
        }
    }

    pub fn mutate(&mut self, seed: u64, amount: f32) {
        let mut rng = SeededRng::new(seed);
        let amount = amount.max(0.0);
        macro_rules! nudge {
            ($field:ident, $min:expr, $max:expr) => {
                self.$field =
                    (self.$field + rng.range(-amount, amount)).clamp($min as f32, $max as f32);
            };
        }
        nudge!(base_freq, 0.0, 1.0);
        nudge!(freq_ramp, -1.0, 1.0);
        nudge!(freq_dramp, -1.0, 1.0);
        nudge!(duty, 0.0, 1.0);
        nudge!(duty_ramp, -1.0, 1.0);
        nudge!(vib_strength, 0.0, 1.0);
        nudge!(vib_speed, 0.0, 1.0);
        nudge!(vib_delay, 0.0, 1.0);
        nudge!(env_attack, 0.0, 1.0);
        nudge!(env_sustain, 0.0, 1.0);
        nudge!(env_decay, 0.0, 1.0);
        nudge!(env_punch, -1.0, 1.0);
        nudge!(lpf_resonance, 0.0, 1.0);
        nudge!(lpf_freq, 0.0, 1.0);
        nudge!(lpf_ramp, -1.0, 1.0);
        nudge!(hpf_freq, 0.0, 1.0);
        nudge!(hpf_ramp, -1.0, 1.0);
        nudge!(pha_offset, -1.0, 1.0);
        nudge!(pha_ramp, -1.0, 1.0);
        nudge!(repeat_speed, 0.0, 1.0);
        nudge!(arp_speed, 0.0, 1.0);
        nudge!(arp_mod, -1.0, 1.0);
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
            color: VoiceColor {
                noise_mix: if self.wave_type == Waveform::Noise {
                    0.35
                } else {
                    0.0
                },
                drive: 0.12 + self.env_punch.max(0.0) * 0.18,
                fold: 0.0,
                tremolo_depth: self.vib_strength.clamp(0.0, 1.0) * 0.12,
                tremolo_hz: 8.0 + self.vib_speed.clamp(0.0, 1.0) * 18.0,
                formant_mix: 0.0,
            },
            formants: Vec::new(),
            modulators: Vec::new(),
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatchScriptError {
    pub line: usize,
    pub message: String,
}

impl PatchScriptError {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for PatchScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "patch script line {}: {}",
            self.line, self.message
        )
    }
}

impl Error for PatchScriptError {}

pub fn parse_patch_script(script: &str) -> Result<SynthPatch, PatchScriptError> {
    let mut patch = SynthPatch::new(Vec::new());
    for (index, raw_line) in script.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(command) = parts.next() else {
            continue;
        };
        let fields = parse_fields(parts, line_number)?;
        match command {
            "patch" => apply_patch_fields(&mut patch, &fields, line_number)?,
            "lfo" | "control" => patch
                .controls
                .push(control_lane_from_fields(&fields, line_number)?),
            "voice" => patch.voices.push(voice_from_fields(&fields, line_number)?),
            "sfxr" => {
                let mut params = if let Some(name) = field_value(&fields, "preset") {
                    SfxrParams::named(name).ok_or_else(|| {
                        PatchScriptError::new(line_number, format!("unknown sfxr preset `{name}`"))
                    })?
                } else {
                    SfxrParams::default()
                };
                apply_sfxr_fields(&mut params, &fields, line_number)?;
                let mapped = params.to_patch();
                patch.voices.extend(mapped.voices);
                patch.repeat = mapped.repeat;
                patch.gain *= mapped.gain;
            }
            unknown => {
                return Err(PatchScriptError::new(
                    line_number,
                    format!("unknown command `{unknown}`"),
                ));
            }
        }
    }
    if patch.voices.is_empty() {
        return Err(PatchScriptError::new(0, "script produced no voices"));
    }
    Ok(patch)
}

fn parse_fields<'a>(
    parts: impl Iterator<Item = &'a str>,
    line: usize,
) -> Result<Vec<(&'a str, &'a str)>, PatchScriptError> {
    let mut fields = Vec::new();
    for part in parts {
        let Some((key, value)) = part.split_once('=') else {
            return Err(PatchScriptError::new(
                line,
                format!("expected key=value field, got `{part}`"),
            ));
        };
        fields.push((key, value));
    }
    Ok(fields)
}

fn field_value<'a>(fields: &'a [(&str, &str)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(*value))
}

fn apply_patch_fields(
    patch: &mut SynthPatch,
    fields: &[(&str, &str)],
    line: usize,
) -> Result<(), PatchScriptError> {
    for (key, value) in fields {
        match *key {
            "gain" => patch.gain = parse_f32(value, line, key)?,
            "soft_clip" => patch.soft_clip = parse_bool(value, line, key)?,
            "repeat" => {
                let interval_seconds = parse_f32(value, line, key)?;
                patch.repeat = (interval_seconds > 0.0).then_some(Repeat { interval_seconds });
            }
            unknown => return Err(unknown_field(line, "patch", unknown)),
        }
    }
    Ok(())
}

fn control_lane_from_fields(
    fields: &[(&str, &str)],
    line: usize,
) -> Result<ControlLane, PatchScriptError> {
    let name = field_value(fields, "name")
        .ok_or_else(|| PatchScriptError::new(line, "control lane needs name"))?
        .to_owned();
    if name.is_empty() {
        return Err(PatchScriptError::new(line, "control lane name is empty"));
    }
    let target = parse_mod_target(
        field_value(fields, "target")
            .ok_or_else(|| PatchScriptError::new(line, "control lane needs target"))?,
        line,
    )?;
    let waveform = parse_mod_waveform(field_value(fields, "wave").unwrap_or("sine"), line)?;
    let frequency_hz = parse_optional_f32(fields, "hz", line)?.unwrap_or(1.0);
    let depth = parse_optional_f32(fields, "depth", line)?.unwrap_or(0.0);
    let phase = parse_optional_f32(fields, "phase", line)?.unwrap_or(0.0);
    let bias = parse_optional_f32(fields, "bias", line)?.unwrap_or(0.0);
    for (key, _) in fields {
        match *key {
            "name" | "target" | "wave" | "hz" | "depth" | "phase" | "bias" => {}
            unknown => return Err(unknown_field(line, "lfo", unknown)),
        }
    }
    Ok(ControlLane {
        name,
        modulator: Modulator {
            target,
            waveform,
            frequency_hz,
            depth,
            phase,
            bias,
        },
    })
}

fn voice_from_fields(fields: &[(&str, &str)], line: usize) -> Result<Voice, PatchScriptError> {
    let waveform = match field_value(fields, "wave").unwrap_or("sine") {
        "sine" => Waveform::Sine,
        "square" => Waveform::Square,
        "saw" | "sawtooth" => Waveform::Sawtooth,
        "tri" | "triangle" => Waveform::Triangle,
        "noise" => Waveform::Noise,
        other => {
            return Err(PatchScriptError::new(
                line,
                format!("unknown waveform `{other}`"),
            ));
        }
    };
    let frequency_hz = parse_optional_f32(fields, "freq", line)?.unwrap_or(440.0);
    let envelope = Envelope {
        attack_seconds: parse_optional_f32(fields, "attack", line)?.unwrap_or(0.0),
        sustain_seconds: parse_optional_f32(fields, "sustain", line)?.unwrap_or(0.1),
        decay_seconds: parse_optional_f32(fields, "decay", line)?.unwrap_or(0.2),
        punch: parse_optional_f32(fields, "punch", line)?.unwrap_or(0.0),
    };
    let arpeggio = match (
        parse_optional_f32(fields, "arp_delay", line)?,
        parse_optional_f32(fields, "arp_mult", line)?,
    ) {
        (Some(delay_seconds), Some(multiplier)) => Some(Arpeggio {
            delay_seconds,
            multiplier,
        }),
        (None, None) => None,
        _ => {
            return Err(PatchScriptError::new(
                line,
                "arpeggio needs both arp_delay and arp_mult",
            ));
        }
    };
    let color = VoiceColor {
        noise_mix: parse_optional_f32(fields, "noise", line)?.unwrap_or(0.0),
        drive: parse_optional_f32(fields, "drive", line)?.unwrap_or(0.0),
        fold: parse_optional_f32(fields, "fold", line)?.unwrap_or(0.0),
        tremolo_depth: parse_optional_f32(fields, "tremolo", line)?.unwrap_or(0.0),
        tremolo_hz: parse_optional_f32(fields, "tremolo_hz", line)?.unwrap_or(0.0),
        formant_mix: parse_optional_f32(fields, "formant_mix", line)?.unwrap_or(0.0),
    };
    let formants = match field_value(fields, "formants") {
        Some(value) => parse_formants(value, line)?,
        None => Vec::new(),
    };
    let modulators = match field_value(fields, "mods") {
        Some(value) => parse_modulators(value, line)?,
        None => Vec::new(),
    };
    let voice = Voice {
        oscillator: Oscillator {
            waveform,
            frequency_hz,
            duty: parse_optional_f32(fields, "duty", line)?.unwrap_or(0.5),
            phase: parse_optional_f32(fields, "phase", line)?.unwrap_or(0.0),
        },
        envelope,
        pitch: PitchMotion {
            min_frequency_hz: parse_optional_f32(fields, "min_freq", line)?.unwrap_or(20.0),
            ramp_per_second: parse_optional_f32(fields, "pitch_ramp", line)?.unwrap_or(0.0),
            delta_ramp_per_second: parse_optional_f32(fields, "pitch_dramp", line)?.unwrap_or(0.0),
            vibrato_depth: parse_optional_f32(fields, "vibrato", line)?.unwrap_or(0.0),
            vibrato_hz: parse_optional_f32(fields, "vibrato_hz", line)?.unwrap_or(0.0),
            vibrato_delay_seconds: parse_optional_f32(fields, "vibrato_delay", line)?
                .unwrap_or(0.0),
        },
        duty: DutyMotion {
            ramp_per_second: parse_optional_f32(fields, "duty_ramp", line)?.unwrap_or(0.0),
        },
        filter: Filter {
            low_pass: parse_optional_f32(fields, "lpf", line)?.unwrap_or(1.0),
            low_pass_ramp: parse_optional_f32(fields, "lpf_ramp", line)?.unwrap_or(0.0),
            low_pass_resonance: parse_optional_f32(fields, "resonance", line)?.unwrap_or(0.0),
            high_pass: parse_optional_f32(fields, "hpf", line)?.unwrap_or(0.0),
            high_pass_ramp: parse_optional_f32(fields, "hpf_ramp", line)?.unwrap_or(0.0),
        },
        phaser: Phaser {
            offset_seconds: parse_optional_f32(fields, "phaser", line)?.unwrap_or(0.0),
            ramp_seconds_per_second: parse_optional_f32(fields, "phaser_ramp", line)?
                .unwrap_or(0.0),
        },
        arpeggio,
        color,
        formants,
        modulators,
        gain: parse_optional_f32(fields, "gain", line)?.unwrap_or(0.2),
    };
    for (key, _) in fields {
        match *key {
            "wave" | "freq" | "duty" | "phase" | "attack" | "sustain" | "decay" | "punch"
            | "min_freq" | "pitch_ramp" | "pitch_dramp" | "vibrato" | "vibrato_hz"
            | "vibrato_delay" | "duty_ramp" | "lpf" | "lpf_ramp" | "resonance" | "hpf"
            | "hpf_ramp" | "phaser" | "phaser_ramp" | "arp_delay" | "arp_mult" | "noise"
            | "drive" | "fold" | "tremolo" | "tremolo_hz" | "formant_mix" | "formants" | "gain" => {
            }
            "mods" => {}
            unknown => return Err(unknown_field(line, "voice", unknown)),
        }
    }
    Ok(voice)
}

fn parse_formants(value: &str, line: usize) -> Result<Vec<Formant>, PatchScriptError> {
    if value.trim().is_empty() {
        return Ok(Vec::new());
    }
    value
        .split(',')
        .map(|spec| {
            let mut parts = spec.split(':');
            let frequency_hz = parts
                .next()
                .ok_or_else(|| PatchScriptError::new(line, "formant needs frequency"))?
                .parse::<f32>()
                .map_err(|_| PatchScriptError::new(line, "formant frequency must be a number"))?;
            let bandwidth_hz = parts
                .next()
                .ok_or_else(|| PatchScriptError::new(line, "formant needs bandwidth"))?
                .parse::<f32>()
                .map_err(|_| PatchScriptError::new(line, "formant bandwidth must be a number"))?;
            let gain = parts
                .next()
                .unwrap_or("1")
                .parse::<f32>()
                .map_err(|_| PatchScriptError::new(line, "formant gain must be a number"))?;
            if parts.next().is_some() {
                return Err(PatchScriptError::new(
                    line,
                    "formant format is frequency:bandwidth[:gain]",
                ));
            }
            Ok(Formant {
                frequency_hz,
                bandwidth_hz,
                gain,
            })
        })
        .collect()
}

fn parse_modulators(value: &str, line: usize) -> Result<Vec<Modulator>, PatchScriptError> {
    if value.trim().is_empty() {
        return Ok(Vec::new());
    }
    value
        .split(',')
        .map(|spec| {
            let mut parts = spec.split(':');
            let target = parse_mod_target(
                parts
                    .next()
                    .ok_or_else(|| PatchScriptError::new(line, "modulator needs target"))?,
                line,
            )?;
            let waveform = parse_mod_waveform(
                parts
                    .next()
                    .ok_or_else(|| PatchScriptError::new(line, "modulator needs waveform"))?,
                line,
            )?;
            let frequency_hz = parts
                .next()
                .ok_or_else(|| PatchScriptError::new(line, "modulator needs frequency"))?
                .parse::<f32>()
                .map_err(|_| PatchScriptError::new(line, "modulator frequency must be a number"))?;
            let depth = parts
                .next()
                .ok_or_else(|| PatchScriptError::new(line, "modulator needs depth"))?
                .parse::<f32>()
                .map_err(|_| PatchScriptError::new(line, "modulator depth must be a number"))?;
            let phase = parts
                .next()
                .unwrap_or("0")
                .parse::<f32>()
                .map_err(|_| PatchScriptError::new(line, "modulator phase must be a number"))?;
            let bias = parts
                .next()
                .unwrap_or("0")
                .parse::<f32>()
                .map_err(|_| PatchScriptError::new(line, "modulator bias must be a number"))?;
            if parts.next().is_some() {
                return Err(PatchScriptError::new(
                    line,
                    "modulator format is target:wave:hz:depth[:phase[:bias]]",
                ));
            }
            Ok(Modulator {
                target,
                waveform,
                frequency_hz,
                depth,
                phase,
                bias,
            })
        })
        .collect()
}

fn parse_mod_target(value: &str, line: usize) -> Result<ModTarget, PatchScriptError> {
    match value {
        "gain" => Ok(ModTarget::Gain),
        "pitch" => Ok(ModTarget::Pitch),
        "duty" => Ok(ModTarget::Duty),
        "lpf" => Ok(ModTarget::LowPass),
        "hpf" => Ok(ModTarget::HighPass),
        "noise" => Ok(ModTarget::Noise),
        "drive" => Ok(ModTarget::Drive),
        "fold" => Ok(ModTarget::Fold),
        "formant" | "formant_mix" => Ok(ModTarget::FormantMix),
        other => Err(PatchScriptError::new(
            line,
            format!("unknown modulator target `{other}`"),
        )),
    }
}

fn parse_mod_waveform(value: &str, line: usize) -> Result<ModWaveform, PatchScriptError> {
    match value {
        "sine" => Ok(ModWaveform::Sine),
        "tri" | "triangle" => Ok(ModWaveform::Triangle),
        "square" => Ok(ModWaveform::Square),
        "hold" | "sample_hold" => Ok(ModWaveform::SampleHold),
        other => Err(PatchScriptError::new(
            line,
            format!("unknown modulator waveform `{other}`"),
        )),
    }
}

fn apply_sfxr_fields(
    params: &mut SfxrParams,
    fields: &[(&str, &str)],
    line: usize,
) -> Result<(), PatchScriptError> {
    if let Some(seed) = field_value(fields, "mutate_seed") {
        let seed = seed
            .parse::<u64>()
            .map_err(|_| PatchScriptError::new(line, "mutate_seed must be an integer"))?;
        let amount = parse_optional_f32(fields, "mutate", line)?.unwrap_or(0.05);
        params.mutate(seed, amount);
    }
    for (key, value) in fields {
        match *key {
            "preset" | "mutate_seed" | "mutate" => {}
            "wave" => {
                params.wave_type = match *value {
                    "sine" => Waveform::Sine,
                    "square" => Waveform::Square,
                    "saw" | "sawtooth" => Waveform::Sawtooth,
                    "tri" | "triangle" => Waveform::Triangle,
                    "noise" => Waveform::Noise,
                    other => {
                        return Err(PatchScriptError::new(
                            line,
                            format!("unknown waveform `{other}`"),
                        ));
                    }
                };
            }
            "base" => params.base_freq = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "limit" => params.freq_limit = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "ramp" => params.freq_ramp = parse_f32(value, line, key)?.clamp(-1.0, 1.0),
            "dramp" => params.freq_dramp = parse_f32(value, line, key)?.clamp(-1.0, 1.0),
            "duty" => params.duty = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "duty_ramp" => params.duty_ramp = parse_f32(value, line, key)?.clamp(-1.0, 1.0),
            "vib" => params.vib_strength = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "vib_speed" => params.vib_speed = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "vib_delay" => params.vib_delay = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "attack" => params.env_attack = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "sustain" => params.env_sustain = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "decay" => params.env_decay = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "punch" => params.env_punch = parse_f32(value, line, key)?.clamp(-1.0, 1.0),
            "resonance" => params.lpf_resonance = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "lpf" => params.lpf_freq = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "lpf_ramp" => params.lpf_ramp = parse_f32(value, line, key)?.clamp(-1.0, 1.0),
            "hpf" => params.hpf_freq = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "hpf_ramp" => params.hpf_ramp = parse_f32(value, line, key)?.clamp(-1.0, 1.0),
            "phaser" => params.pha_offset = parse_f32(value, line, key)?.clamp(-1.0, 1.0),
            "phaser_ramp" => params.pha_ramp = parse_f32(value, line, key)?.clamp(-1.0, 1.0),
            "repeat" => params.repeat_speed = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "arp" => params.arp_speed = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "arp_mod" => params.arp_mod = parse_f32(value, line, key)?.clamp(-1.0, 1.0),
            unknown => return Err(unknown_field(line, "sfxr", unknown)),
        }
    }
    Ok(())
}

fn parse_optional_f32(
    fields: &[(&str, &str)],
    key: &str,
    line: usize,
) -> Result<Option<f32>, PatchScriptError> {
    field_value(fields, key)
        .map(|value| parse_f32(value, line, key))
        .transpose()
}

fn parse_f32(value: &str, line: usize, key: &str) -> Result<f32, PatchScriptError> {
    value
        .parse::<f32>()
        .map_err(|_| PatchScriptError::new(line, format!("`{key}` must be a number")))
}

fn parse_bool(value: &str, line: usize, key: &str) -> Result<bool, PatchScriptError> {
    match value {
        "true" | "yes" | "1" => Ok(true),
        "false" | "no" | "0" => Ok(false),
        _ => Err(PatchScriptError::new(
            line,
            format!("`{key}` must be true or false"),
        )),
    }
}

fn unknown_field(line: usize, command: &str, field: &str) -> PatchScriptError {
    PatchScriptError::new(line, format!("unknown {command} field `{field}`"))
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

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    pub fn set_seed(&mut self, seed: u64) {
        self.seed = seed;
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
            value += state.next_sample(
                voice,
                &self.patch.controls,
                repeat_age,
                sample_rate,
                self.seed,
            ) * voice.gain;
        }
        value *= self.patch.gain;
        self.sample_index = self.sample_index.saturating_add(1);
        if self.patch.soft_clip {
            (value * 1.35).tanh()
        } else {
            value.clamp(-1.0, 1.0)
        }
    }

    pub fn render_mono(&mut self, output: &mut [f32]) {
        for sample in output {
            *sample = self.next_sample();
        }
    }

    pub fn render_interleaved_stereo(&mut self, output: &mut [f32]) {
        for frame in output.chunks_mut(2) {
            let sample = self.next_sample();
            frame[0] = sample;
            if let Some(right) = frame.get_mut(1) {
                *right = sample;
            }
        }
    }
}

#[derive(Clone, Debug)]
struct VoiceState {
    phase: f32,
    sample_counter: u64,
    noise_epoch: u32,
    low_pass_position: f32,
    low_pass_delta: f32,
    high_pass_position: f32,
    phaser_cursor: usize,
    phaser_buffer: Vec<f32>,
    formants: Vec<FormantState>,
}

impl VoiceState {
    fn new(voice: &Voice) -> Self {
        Self {
            phase: 0.0,
            sample_counter: 0,
            noise_epoch: 0,
            low_pass_position: 0.0,
            low_pass_delta: 0.0,
            high_pass_position: 0.0,
            phaser_cursor: 0,
            phaser_buffer: vec![0.0; 2048],
            formants: voice
                .formants
                .iter()
                .map(|formant| FormantState::new(*formant))
                .collect(),
        }
    }

    fn next_sample(
        &mut self,
        voice: &Voice,
        controls: &[ControlLane],
        age: f32,
        sample_rate: f32,
        seed: u64,
    ) -> f32 {
        self.sample_counter = self.sample_counter.wrapping_add(1);
        let envelope = voice.envelope.amplitude(age);
        if envelope <= 0.0 {
            return 0.0;
        }

        let pitch_mod = mod_amount(&voice.modulators, controls, ModTarget::Pitch, age, seed);
        let mut frequency = frequency_at(voice, age) * 2.0_f32.powf(pitch_mod);
        if let Some(arpeggio) = voice.arpeggio {
            if age >= arpeggio.delay_seconds {
                frequency *= arpeggio.multiplier;
            }
        }
        let duty = (voice.oscillator.duty
            + voice.duty.ramp_per_second * age
            + mod_amount(&voice.modulators, controls, ModTarget::Duty, age, seed))
        .clamp(0.02, 0.98);
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
        sample = self.color(voice, controls, sample, age, seed);
        sample = self.filter(modulated_filter(voice, controls, age, seed), sample, age);
        sample = self.formants(voice, controls, sample, sample_rate, age, seed);
        sample = self.phaser(voice.phaser, sample, age, sample_rate);
        let gain_mod =
            (1.0 + mod_amount(&voice.modulators, controls, ModTarget::Gain, age, seed)).max(0.0);
        sample * envelope * gain_mod
    }

    fn color(
        &self,
        voice: &Voice,
        controls: &[ControlLane],
        sample: f32,
        age: f32,
        seed: u64,
    ) -> f32 {
        let mut value = sample;
        let noise_mix = (voice.color.noise_mix
            + mod_amount(&voice.modulators, controls, ModTarget::Noise, age, seed))
        .clamp(0.0, 1.0);
        if noise_mix > 0.0 {
            let noise = hash_noise(seed ^ self.sample_counter, self.noise_epoch);
            value = value * (1.0 - noise_mix) + noise * noise_mix;
        }
        let drive_amount = (voice.color.drive
            + mod_amount(&voice.modulators, controls, ModTarget::Drive, age, seed))
        .clamp(0.0, 1.0);
        if drive_amount > 0.0 {
            let drive = 1.0 + drive_amount * 12.0;
            value = (value * drive).tanh() / drive.tanh();
        }
        let fold_amount = (voice.color.fold
            + mod_amount(&voice.modulators, controls, ModTarget::Fold, age, seed))
        .clamp(0.0, 1.0);
        if fold_amount > 0.0 {
            value = wavefold(value * (1.0 + fold_amount * 3.5));
        }
        if voice.color.tremolo_depth > 0.0 && voice.color.tremolo_hz > 0.0 {
            let lfo = 0.5 + 0.5 * (age * voice.color.tremolo_hz * TAU).sin();
            value *= 1.0 - voice.color.tremolo_depth.clamp(0.0, 1.0) * lfo;
        }
        value
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

    fn formants(
        &mut self,
        voice: &Voice,
        controls: &[ControlLane],
        sample: f32,
        sample_rate: f32,
        age: f32,
        seed: u64,
    ) -> f32 {
        let mix = (voice.color.formant_mix
            + mod_amount(
                &voice.modulators,
                controls,
                ModTarget::FormantMix,
                age,
                seed,
            ))
        .clamp(0.0, 1.0);
        if mix <= 0.0 || self.formants.is_empty() {
            return sample;
        }
        let mut resonant = 0.0;
        let mut gain_sum = 0.0;
        for (state, formant) in self.formants.iter_mut().zip(&voice.formants) {
            resonant += state.process(sample, *formant, sample_rate) * formant.gain;
            gain_sum += formant.gain.abs();
        }
        let resonant = resonant / gain_sum.max(0.001);
        sample * (1.0 - mix) + resonant * mix
    }
}

#[derive(Clone, Copy, Debug)]
struct FormantState {
    source: Formant,
    sample_rate: f32,
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl FormantState {
    fn new(source: Formant) -> Self {
        Self {
            source,
            sample_rate: 0.0,
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn process(&mut self, input: f32, formant: Formant, sample_rate: f32) -> f32 {
        if self.source.frequency_hz != formant.frequency_hz
            || self.source.bandwidth_hz != formant.bandwidth_hz
            || self.source.gain != formant.gain
            || self.sample_rate != sample_rate
        {
            self.source = formant;
            self.sample_rate = sample_rate;
            self.update_coefficients(formant, sample_rate);
        }
        let output = self.b0 * input + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;
        output
    }

    fn update_coefficients(&mut self, formant: Formant, sample_rate: f32) {
        let frequency = formant.frequency_hz.clamp(20.0, sample_rate * 0.45);
        let bandwidth = formant.bandwidth_hz.max(10.0);
        let q = (frequency / bandwidth).clamp(0.2, 40.0);
        let omega = TAU * frequency / sample_rate.max(1.0);
        let alpha = omega.sin() / (2.0 * q);
        let cos = omega.cos();
        let a0 = 1.0 + alpha;
        self.b0 = alpha / a0;
        self.b1 = 0.0;
        self.b2 = -alpha / a0;
        self.a1 = -2.0 * cos / a0;
        self.a2 = (1.0 - alpha) / a0;
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
            controls: Vec::new(),
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
            controls: Vec::new(),
            repeat: None,
            gain: 0.9,
            soft_clip: true,
        }
    }

    pub fn aquarium_voice() -> SynthPatch {
        let envelope = Envelope {
            attack_seconds: 0.018,
            sustain_seconds: 0.34,
            decay_seconds: 0.28,
            punch: 0.08,
        };
        let mut voice = Voice::simple(
            Oscillator {
                waveform: Waveform::Triangle,
                frequency_hz: 220.0,
                duty: 0.5,
                phase: 0.0,
            },
            envelope,
            0.18,
        );
        voice.pitch.vibrato_depth = 0.018;
        voice.pitch.vibrato_hz = 5.6;
        voice.color = VoiceColor {
            noise_mix: 0.035,
            drive: 0.18,
            fold: 0.04,
            tremolo_depth: 0.12,
            tremolo_hz: 4.2,
            formant_mix: 0.68,
        };
        voice.formants = vec![
            Formant {
                frequency_hz: 520.0,
                bandwidth_hz: 85.0,
                gain: 0.9,
            },
            Formant {
                frequency_hz: 1380.0,
                bandwidth_hz: 180.0,
                gain: 1.0,
            },
            Formant {
                frequency_hz: 2550.0,
                bandwidth_hz: 300.0,
                gain: 0.42,
            },
        ];
        SynthPatch {
            voices: vec![voice],
            controls: Vec::new(),
            repeat: None,
            gain: 0.95,
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

fn modulated_filter(voice: &Voice, controls: &[ControlLane], age: f32, seed: u64) -> Filter {
    let mut filter = voice.filter;
    filter.low_pass = (filter.low_pass
        + mod_amount(&voice.modulators, controls, ModTarget::LowPass, age, seed))
    .clamp(0.0, 1.0);
    filter.high_pass = (filter.high_pass
        + mod_amount(&voice.modulators, controls, ModTarget::HighPass, age, seed))
    .clamp(0.0, 1.0);
    filter
}

fn mod_amount(
    modulators: &[Modulator],
    controls: &[ControlLane],
    target: ModTarget,
    age: f32,
    seed: u64,
) -> f32 {
    let local: f32 = modulators
        .iter()
        .filter(|modulator| modulator.target == target)
        .map(|modulator| modulator.bias + modulator.depth * modulator_value(*modulator, age, seed))
        .sum();
    let patch: f32 = controls
        .iter()
        .filter(|control| control.modulator.target == target)
        .map(|control| {
            let salt = stable_name_hash(&control.name);
            control.modulator.bias
                + control.modulator.depth * modulator_value(control.modulator, age, seed ^ salt)
        })
        .sum();
    local + patch
}

fn modulator_value(modulator: Modulator, age: f32, seed: u64) -> f32 {
    let phase = (age * modulator.frequency_hz + modulator.phase).fract();
    match modulator.waveform {
        ModWaveform::Sine => (phase * TAU).sin(),
        ModWaveform::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
        ModWaveform::Square => {
            if phase < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        ModWaveform::SampleHold => {
            let slot = (age * modulator.frequency_hz.max(0.001)).floor() as u32;
            hash_noise(seed ^ 0x4d6f_6475_6c61_7465, slot)
        }
    }
}

fn stable_name_hash(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ byte as u64).wrapping_mul(0x0000_0100_0000_01b3)
    })
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

fn wavefold(value: f32) -> f32 {
    let folded = (value + 1.0).rem_euclid(4.0);
    if folded <= 2.0 {
        folded - 1.0
    } else {
        3.0 - folded
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

fn rms_envelope(samples: &[f32], window_size: usize, hop_size: usize) -> Vec<f32> {
    let window_size = window_size.max(16);
    let hop_size = hop_size.max(1);
    if samples.is_empty() {
        return vec![0.0];
    }
    let mut envelope = Vec::new();
    let mut start = 0;
    while start < samples.len() {
        let end = (start + window_size).min(samples.len());
        envelope.push(mean_square(&samples[start..end]).sqrt());
        start += hop_size;
    }
    envelope
}

fn mel_band_edges(
    band_count: usize,
    fft_size: usize,
    sample_rate: f32,
    min_frequency_hz: f32,
    max_frequency_hz: f32,
) -> Vec<usize> {
    let min_mel = hz_to_mel(min_frequency_hz.max(0.0));
    let max_mel = hz_to_mel(
        max_frequency_hz
            .min(sample_rate * 0.5)
            .max(min_frequency_hz + 1.0),
    );
    (0..band_count + 2)
        .map(|index| {
            let t = index as f32 / (band_count + 1) as f32;
            let hz = mel_to_hz(min_mel + (max_mel - min_mel) * t);
            ((hz / sample_rate.max(1.0)) * fft_size as f32).round() as usize
        })
        .collect()
}

fn spectral_shape(spectrum: &[f32], sample_rate: f32, rolloff_portion: f32) -> (f32, f32) {
    let total: f32 = spectrum.iter().sum();
    if total <= f32::EPSILON {
        return (0.0, 0.0);
    }
    let mut weighted = 0.0;
    let mut cumulative = 0.0;
    let mut rolloff = 0.0;
    for (bin, energy) in spectrum.iter().enumerate() {
        let frequency =
            bin as f32 * sample_rate * 0.5 / (spectrum.len().saturating_sub(1)).max(1) as f32;
        weighted += frequency * energy;
        cumulative += energy;
        if rolloff == 0.0 && cumulative >= total * rolloff_portion {
            rolloff = frequency;
        }
    }
    (weighted / total, rolloff)
}

fn normalized_distance(reference: &[f32], candidate: &[f32]) -> f32 {
    let length = reference.len().max(candidate.len()).max(1);
    let mut error = 0.0;
    let mut scale = 0.0;
    for index in 0..length {
        let a = resampled_at(reference, index, length);
        let b = resampled_at(candidate, index, length);
        let delta = a - b;
        error += delta * delta;
        scale += a * a + b * b;
    }
    (error / scale.max(f32::EPSILON)).sqrt()
}

fn resampled_at(values: &[f32], index: usize, target_len: usize) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    if values.len() == 1 || target_len <= 1 {
        return values[0];
    }
    let position = index as f32 * (values.len() - 1) as f32 / (target_len - 1) as f32;
    let left = position.floor() as usize;
    let right = (left + 1).min(values.len() - 1);
    let t = position - left as f32;
    values[left] * (1.0 - t) + values[right] * t
}

fn normalize_in_place(values: &mut [f32]) {
    if values.is_empty() {
        return;
    }
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    let variance = values
        .iter()
        .map(|value| {
            let delta = value - mean;
            delta * delta
        })
        .sum::<f32>()
        / values.len() as f32;
    let scale = variance.sqrt().max(1.0e-6);
    for value in values {
        *value = (*value - mean) / scale;
    }
}

fn mean_square(samples: &[f32]) -> f32 {
    samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len().max(1) as f32
}

fn hann(index: usize, size: usize) -> f32 {
    if size <= 1 {
        return 1.0;
    }
    0.5 - 0.5 * (TAU * index as f32 / (size - 1) as f32).cos()
}

fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10.0_f32.powf(mel / 2595.0) - 1.0)
}

fn safe_ratio(candidate: f32, reference: f32) -> f32 {
    candidate / reference.max(f32::EPSILON)
}

fn ratio_distance(ratio: f32) -> f32 {
    ratio.max(f32::EPSILON).ln().abs()
}

#[derive(Clone, Copy, Debug)]
struct SeededRng {
    state: u64,
}

impl SeededRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x517c_c1b7_2722_0a95,
        }
    }

    fn next_f32(&mut self) -> f32 {
        self.state = self
            .state
            .wrapping_add(0x9e37_79b9_7f4a_7c15)
            .rotate_left(17);
        let mut value = self.state;
        value ^= value >> 30;
        value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value ^= value >> 27;
        value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^= value >> 31;
        (value >> 40) as f32 / 16_777_216.0
    }

    fn range(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.next_f32()
    }
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

    #[test]
    fn all_classic_sfxr_presets_map_to_sound() {
        for name in [
            "pickup",
            "laser",
            "explosion",
            "powerup",
            "hit",
            "jump",
            "blip",
        ] {
            let patch = SfxrParams::named(name).unwrap().to_patch();
            let mut player = PatchPlayer::new(patch, 44_100.0);
            let peak = (0..8192)
                .map(|_| player.next_sample().abs())
                .fold(0.0, f32::max);
            assert!(peak > 0.001, "{name} was silent");
        }
    }

    #[test]
    fn patch_script_parses_modular_and_sfxr_voices() {
        let patch = SynthPatch::from_script(PATCH_SCRIPT_EXAMPLE).unwrap();
        assert_eq!(patch.voices.len(), 3);
        let mut player = PatchPlayer::new(patch, 44_100.0);
        let energy: f32 = (0..4096).map(|_| player.next_sample().abs()).sum();
        assert!(energy > 1.0);
    }

    #[test]
    fn patch_script_reports_line_numbers() {
        let err = SynthPatch::from_script("patch gain=1\nvoice wave=beige").unwrap_err();
        assert_eq!(err.line, 2);
    }

    #[test]
    fn patch_script_reports_bad_modulators() {
        let err = SynthPatch::from_script("voice wave=sine mods=paperwork:sine:1:1").unwrap_err();
        assert_eq!(err.line, 1);
        assert!(err.to_string().contains("unknown modulator target"));
    }

    #[test]
    fn patch_script_parses_patch_level_control_lanes() {
        let patch = SynthPatch::from_script(
            "lfo name=wobble target=pitch wave=sine hz=6 depth=0.04\nvoice wave=sine freq=330 attack=0.01 sustain=0.1 decay=0.1",
        )
        .unwrap();
        assert_eq!(patch.controls.len(), 1);
        assert_eq!(patch.controls[0].name, "wobble");
        assert_eq!(patch.controls[0].modulator.target, ModTarget::Pitch);
        assert_eq!(patch.controls[0].modulator.waveform, ModWaveform::Sine);
    }

    #[test]
    fn patch_level_control_lanes_modulate_all_voices() {
        let dry = SynthPatch::from_script(
            "voice wave=triangle freq=220 gain=0.16 attack=0.01 sustain=0.35 decay=0.2\nvoice wave=sine freq=440 gain=0.08 attack=0.01 sustain=0.35 decay=0.2",
        )
        .unwrap();
        let moving = SynthPatch::from_script(
            "lfo name=shared_target target=pitch wave=sine hz=7 depth=0.05\nvoice wave=triangle freq=220 gain=0.16 attack=0.01 sustain=0.35 decay=0.2\nvoice wave=sine freq=440 gain=0.08 attack=0.01 sustain=0.35 decay=0.2",
        )
        .unwrap();
        let options = RenderOptions {
            duration_seconds: 0.38,
            ..RenderOptions::default()
        };
        let dry_buffer = render_patch_mono(dry, options);
        let moving_buffer = render_patch_mono(moving, options);
        let comparison = compare_audio(
            &dry_buffer,
            &moving_buffer,
            &AudioAnalysisConfig {
                fft_size: 256,
                hop_size: 256,
                mel_band_count: 18,
                ..AudioAnalysisConfig::default()
            },
        );
        assert!(comparison.log_mel_distance > 0.04);
        assert!(comparison.score < 0.96);
    }

    #[test]
    fn patch_builder_creates_renderable_modular_patch() {
        let voice = Voice::simple(
            Oscillator::sine(330.0),
            Envelope {
                attack_seconds: 0.004,
                sustain_seconds: 0.08,
                decay_seconds: 0.12,
                punch: 0.2,
            },
            0.18,
        )
        .with_modulator(Modulator::lfo(
            ModTarget::Duty,
            ModWaveform::Triangle,
            9.0,
            0.08,
        ))
        .with_formant(Formant {
            frequency_hz: 740.0,
            bandwidth_hz: 120.0,
            gain: 0.7,
        });
        let patch = PatchBuilder::new()
            .gain(0.8)
            .lfo("breath", ModTarget::Gain, ModWaveform::Sine, 5.0, 0.12)
            .voice(voice)
            .build();
        assert_eq!(patch.controls.len(), 1);
        let buffer = render_patch_mono(
            patch,
            RenderOptions {
                duration_seconds: 0.25,
                ..RenderOptions::default()
            },
        );
        let peak = buffer.iter().map(|sample| sample.abs()).fold(0.0, f32::max);
        assert!(peak > 0.01);
    }

    #[test]
    fn render_mono_matches_next_sample_sequence() {
        let patch = presets::aquarium_voice();
        let mut sampled = PatchPlayer::new(patch.clone(), 44_100.0);
        let mut chunked = PatchPlayer::new(patch, 44_100.0);
        sampled.set_seed(19);
        chunked.set_seed(19);
        let expected: Vec<f32> = (0..2048).map(|_| sampled.next_sample()).collect();
        let mut output = vec![0.0; expected.len()];
        chunked.render_mono(&mut output);
        assert_eq!(output, expected);
    }

    #[test]
    fn render_interleaved_stereo_duplicates_mono_frames() {
        let mut player = PatchPlayer::new(presets::aquarium_pluck(), 48_000.0);
        let mut output = vec![0.0; 1025];
        player.render_interleaved_stereo(&mut output);
        for frame in output.chunks(2).take(output.len() / 2) {
            assert_eq!(frame[0], frame[1]);
        }
    }

    #[test]
    fn render_script_mono_uses_requested_duration_and_seed() {
        let options = RenderOptions {
            sample_rate: 22_050.0,
            duration_seconds: 0.125,
            seed: 42,
        };
        let first = render_script_mono(
            "lfo name=shake target=gain wave=hold hz=30 depth=0.2\nvoice wave=saw freq=330 gain=0.15 attack=0.005 sustain=0.08 decay=0.08",
            options,
        )
        .unwrap();
        let second = render_script_mono(
            "lfo name=shake target=gain wave=hold hz=30 depth=0.2\nvoice wave=saw freq=330 gain=0.15 attack=0.005 sustain=0.08 decay=0.08",
            options,
        )
        .unwrap();
        assert_eq!(first.len(), 2757);
        assert_eq!(first, second);
        assert!(first.iter().any(|sample| sample.abs() > 0.001));
    }

    #[test]
    fn render_patch_interleaved_stereo_returns_frame_pairs() {
        let options = RenderOptions {
            sample_rate: 8_000.0,
            duration_seconds: 0.25,
            seed: 7,
        };
        let output = render_patch_interleaved_stereo(presets::aquarium_pluck(), options);
        assert_eq!(output.len(), 4000);
        for frame in output.chunks(2) {
            assert_eq!(frame[0], frame[1]);
        }
    }

    #[test]
    fn script_modulators_change_audio_motion() {
        let dry = SynthPatch::from_script(
            "voice wave=triangle freq=220 gain=0.2 attack=0.01 sustain=0.35 decay=0.2 lpf=0.55 formant_mix=0.45 formants=520:85:1,1380:180:0.8",
        )
        .unwrap();
        let moving = SynthPatch::from_script(
            "voice wave=triangle freq=220 gain=0.2 attack=0.01 sustain=0.35 decay=0.2 lpf=0.55 formant_mix=0.45 formants=520:85:1,1380:180:0.8 mods=pitch:sine:6:0.04,gain:triangle:4:0.3,lpf:sine:3:-0.2,formant:sine:2:0.2",
        )
        .unwrap();
        let mut dry_player = PatchPlayer::new(dry, 44_100.0);
        let mut moving_player = PatchPlayer::new(moving, 44_100.0);
        let mut dry_buffer = vec![0.0; 16_384];
        let mut moving_buffer = vec![0.0; 16_384];
        dry_player.render_mono(&mut dry_buffer);
        moving_player.render_mono(&mut moving_buffer);
        let comparison = compare_audio(
            &dry_buffer,
            &moving_buffer,
            &AudioAnalysisConfig {
                fft_size: 256,
                hop_size: 256,
                mel_band_count: 18,
                ..AudioAnalysisConfig::default()
            },
        );
        assert!(comparison.envelope_distance > 0.01);
        assert!(comparison.log_mel_distance > 0.03);
        assert!(comparison.score < 0.95);
    }

    #[test]
    fn audio_comparison_scores_identical_buffers_high() {
        let mut player = PatchPlayer::new(presets::aquarium_pluck(), 44_100.0);
        let buffer: Vec<f32> = (0..4096).map(|_| player.next_sample()).collect();
        let comparison = compare_audio(
            &buffer,
            &buffer,
            &AudioAnalysisConfig {
                fft_size: 128,
                hop_size: 256,
                mel_band_count: 12,
                ..AudioAnalysisConfig::default()
            },
        );
        assert!(comparison.score > 0.95);
        assert!(comparison.log_mel_distance < 0.001);
        assert!(comparison.envelope_distance < 0.001);
    }

    #[test]
    fn voice_color_and_formants_change_spectrum() {
        let dry = SynthPatch::from_script(
            "voice wave=triangle freq=220 gain=0.2 attack=0.01 sustain=0.25 decay=0.2",
        )
        .unwrap();
        let singing = SynthPatch::from_script(
            "voice wave=triangle freq=220 gain=0.2 attack=0.01 sustain=0.25 decay=0.2 drive=0.22 fold=0.08 noise=0.04 tremolo=0.12 tremolo_hz=4.4 formant_mix=0.7 formants=520:85:0.9,1380:180:1,2550:300:0.42",
        )
        .unwrap();
        let mut dry_player = PatchPlayer::new(dry, 44_100.0);
        let mut singing_player = PatchPlayer::new(singing, 44_100.0);
        let dry_buffer: Vec<f32> = (0..16_384).map(|_| dry_player.next_sample()).collect();
        let singing_buffer: Vec<f32> = (0..16_384).map(|_| singing_player.next_sample()).collect();
        let comparison = compare_audio(
            &dry_buffer,
            &singing_buffer,
            &AudioAnalysisConfig {
                fft_size: 256,
                hop_size: 256,
                mel_band_count: 18,
                ..AudioAnalysisConfig::default()
            },
        );
        assert!(comparison.log_mel_distance > 0.08);
        assert!(comparison.score < 0.9);
    }

    #[test]
    fn aquarium_voice_preset_generates_audible_formant_sound() {
        let mut player = PatchPlayer::new(presets::aquarium_voice(), 44_100.0);
        let buffer: Vec<f32> = (0..22_050).map(|_| player.next_sample()).collect();
        let analysis = analyze_audio(&buffer, &AudioAnalysisConfig::default());
        assert!(analysis.features.peak > 0.01);
        assert!(analysis.features.rms > 0.001);
        assert!(!analysis.log_mel_spectrogram.values.is_empty());
    }
}
