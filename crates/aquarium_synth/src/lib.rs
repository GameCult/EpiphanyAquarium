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

pub const CLASSIC_SFXR_NAMES: [&str; 7] = [
    "pickup",
    "laser",
    "explosion",
    "powerup",
    "hit",
    "jump",
    "blip",
];

pub const CLASSIC_SFXR_GOLF_SCRIPT: &str = "pickup;laser;explosion;powerup;hit;jump;blip";

pub const CLASSIC_SFXR_PRIMITIVE_GOLF_SCRIPTS: [(&str, &str); 7] = [
    (
        "pickup",
        "v w=sq f=148.7934 g=.22 s=.01451247166 d=.1306122449 pu=.45 drv=.201 ad=.081844 am=1.116121255",
    ),
    (
        "laser",
        "v w=saw f=229.0554 g=.22 s=.08185941043 d=.07346938776 pr=.703836 du=.31 dur=.056 h=.04 ph=.0001458 phr=-.000504 drv=.12",
    ),
    (
        "explosion",
        "p r=.174744;v w=n f=20 g=.22 s=.1907029478 d=.293877551 pu=.52 pr=.0320625 ph=-.0008712 phr=-.00035 vi=.11 vh=3.4112 nz=.35 drv=.2136 tr=.0264 th=13.04",
    ),
    (
        "powerup",
        "p r=.11315;v w=sin f=57.5946 g=.22 s=.1306122449 d=.1777777778 pr=-.208544 vi=.09 vh=3.9602 drv=.12 tr=.0216 th=13.94",
    ),
    (
        "hit",
        "v w=n f=51.4206 g=.22 s=.00566893424 d=.09070294785 pr=1.050624 h=.12 nz=.35 drv=.12",
    ),
    (
        "jump",
        "v w=sq f=78.2334 g=.22 s=.1097505669 d=.07346938776 pr=-.101156 du=.38 l=.72 h=.05 drv=.12",
    ),
    (
        "blip",
        "v w=sin f=78.2334 g=.22 s=.03832199546 d=.01451247166 h=.1 drv=.12",
    ),
];

pub const CLASSIC_SFXR_ABSTRACT_GOLF_SCRIPT: &str = concat!(
    "d g=.22 drv=.12;",
    "def name=N w=n nz=.35;",
    "v w=sq f=148.7934 s=.01451247166 d=.1306122449 pu=.45 drv=.201 ad=.081844 am=1.116121255;",
    "v w=saw f=229.0554 s=.08185941043 d=.07346938776 pr=.703836 du=.31 dur=.056 h=.04 ph=.0001458 phr=-.000504;",
    "p r=.174744;v u=N f=20 s=.1907029478 d=.293877551 pu=.52 pr=.0320625 ph=-.0008712 phr=-.00035 vi=.11 vh=3.4112 drv=.2136 tr=.0264 th=13.04;",
    "p r=.11315;v w=sin f=57.5946 s=.1306122449 d=.1777777778 pr=-.208544 vi=.09 vh=3.9602 tr=.0216 th=13.94;",
    "v u=N f=51.4206 s=.00566893424 d=.09070294785 pr=1.050624 h=.12;",
    "v w=sq f=78.2334 s=.1097505669 d=.07346938776 pr=-.101156 du=.38 l=.72 h=.05;",
    "v w=sin f=78.2334 s=.03832199546 d=.01451247166 h=.1"
);

pub const CLASSIC_808_NAMES: [&str; 6] = ["kick", "snare", "clap", "hat", "tom", "cowbell"];

pub const CLASSIC_808_PRIMITIVE_GOLF_SCRIPTS: [(&str, &str); 6] = [
    (
        "kick",
        "d w=sin g=.8 drv=.18;v f=58 s=.045 d=.42 pu=.65 pr=-3.8 min=32 l=.85",
    ),
    (
        "snare",
        "d drv=.12;def n=N w=n nz=.85 h=.45 l=.55;v w=sin f=180 g=.08 s=.02 d=.12 pr=-1.2;v u=N f=140 g=.55 s=.035 d=.2",
    ),
    (
        "clap",
        "d w=n nz=.95 h=.55 l=.42 g=.22 d=.11 drv=.1;v f=1800 s=.018 ph=.004;v f=2200 s=.022 ph=.008;v f=2600 s=.028 ph=.012",
    ),
    (
        "hat",
        "d h=.9 l=.24 drv=.05;v w=n f=9000 g=.16 s=.006 d=.055 nz=1;v w=sq f=6800 g=.045 s=.005 d=.04",
    ),
    (
        "tom",
        "d w=sin g=.55 drv=.12;v f=115 s=.055 d=.34 pu=.28 pr=-1.45 min=62 l=.78",
    ),
    (
        "cowbell",
        "d w=sq h=.18 l=.82 drv=.16;v f=540 g=.16 s=.05 d=.18 du=.43;v f=800 g=.12 s=.045 d=.16 du=.47",
    ),
];

pub const FM_BELL_NAMES: [&str; 4] = ["bell", "chime", "coin", "gong"];

pub const FM_BELL_PRIMITIVE_GOLF_SCRIPTS: [(&str, &str); 4] = [
    (
        "bell",
        "d w=sin g=.24 a=.002 s=.04 d=1.2 l=.9;def n=O fm=4.1 fmd=.55;v u=O f=440 fmi=5.8;v u=O f=880 g=.08 fmi=2.4",
    ),
    (
        "chime",
        "d w=sin g=.18 a=.001 s=.025 d=.9 h=.02;def n=O fm=3 fmd=.38;v u=O f=660 fmi=4.2;v u=O f=990 g=.07 fmi=2.1",
    ),
    (
        "coin",
        "d w=sin a=0 s=.02 d=.45 h=.04 drv=.08;v f=1200 g=.18 fm=5 fmi=3.4 fmd=.18;v f=1800 g=.09 fm=7 fmi=1.8 fmd=.12",
    ),
    (
        "gong",
        "d w=sin a=.003 s=.08 d=1.6 l=.82 drv=.1;v f=196 g=.24 fm=2.414 fmi=7.2 fmd=.9;v f=311 g=.12 fm=3.73 fmi=4.1 fmd=.7",
    ),
];

pub const WOBBLE_BASS_NAMES: [&str; 4] = ["talker", "growl", "yoy", "neuro"];

pub const WOBBLE_BASS_PRIMITIVE_GOLF_SCRIPTS: [(&str, &str); 4] = [
    (
        "talker",
        "d w=saw f=55 g=.18 s=.8 d=.25 l=.34 h=.02 drv=.3 fl=.08 fm=2 fmi=.8 fmd=.7 fs=520:90:.7,1250:170:1,2600:320:.45 fmix=.35;wob hz=4 w=tri g=.42 l=.48 fmix=.38 fmi=1.6 drv=.2 fl=.14;v;v f=110 g=.08 du=.42",
    ),
    (
        "growl",
        "d w=saw f=44 g=.2 s=.7 d=.3 l=.28 h=.01 drv=.42 fl=.18 fm=1.5 fmi=1.2 fmd=.45 nz=.05;wob hz=6 w=sin g=.32 l=.55 p=.035 drv=.28 fl=.22 nz=.08 fmi=2.2;v;v w=sq f=88 g=.09 du=.36",
    ),
    (
        "yoy",
        "d w=sq f=62 g=.16 s=.65 d=.22 l=.3 h=.03 drv=.25 fm=3 fmi=.7 fmd=.5 fs=400:80:.6,900:120:1,2100:260:.4 fmix=.28;wob hz=5 w=sq g=.5 l=.5 fmix=.45 p=.025 du=.08;v;v w=saw f=124 g=.07",
    ),
    (
        "neuro",
        "d w=saw f=49 g=.16 s=.75 d=.28 l=.25 h=.04 drv=.38 fl=.24 fm=2.7 fmi=1.5 fmd=.35 nz=.04;wob hz=7 w=hold g=.28 l=.52 p=.04 drv=.25 fl=.3 nz=.1 fmi=2.8;v;v w=tri f=147 g=.06",
    ),
];

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
pub struct FrequencyModulation {
    pub ratio: f32,
    pub index: f32,
    pub index_decay_seconds: f32,
}

impl Default for FrequencyModulation {
    fn default() -> Self {
        Self {
            ratio: 1.0,
            index: 0.0,
            index_decay_seconds: 0.0,
        }
    }
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
    FmIndex,
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
    pub fm: FrequencyModulation,
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
            fm: FrequencyModulation::default(),
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
pub struct PatchScriptMetrics {
    pub byte_count: usize,
    pub line_count: usize,
    pub statement_count: usize,
    pub field_count: usize,
    pub average_fields_per_statement: f32,
    pub alias_field_ratio: f32,
    pub terse_score: f32,
    pub readability_score: f32,
    pub balanced_score: f32,
}

pub fn patch_script_metrics(script: &str) -> PatchScriptMetrics {
    let statements = script_statements(script);
    let statement_count = statements.len();
    let line_count = script
        .lines()
        .filter(|line| !line.split('#').next().unwrap_or("").trim().is_empty())
        .count()
        .max(1);
    let mut field_count = 0usize;
    let mut alias_fields = 0usize;
    let mut named_commands = 0usize;
    let mut numeric_chars = 0usize;
    let mut numeric_values = 0usize;
    for statement in &statements {
        let mut parts = statement.split_whitespace();
        if let Some(command) = parts.next() {
            if command.len() > 1 {
                named_commands += 1;
            }
        }
        for part in parts {
            let Some((key, value)) = part.split_once('=') else {
                continue;
            };
            field_count += 1;
            if key.len() <= 2 {
                alias_fields += 1;
            }
            if value.parse::<f32>().is_ok() {
                numeric_values += 1;
                numeric_chars += value
                    .chars()
                    .filter(|character| character.is_ascii_digit())
                    .count();
            }
        }
    }
    let byte_count = script.trim().len();
    let average_fields_per_statement = field_count as f32 / statement_count.max(1) as f32;
    let alias_field_ratio = alias_fields as f32 / field_count.max(1) as f32;
    let named_command_ratio = named_commands as f32 / statement_count.max(1) as f32;
    let line_room = (line_count as f32 / statement_count.max(1) as f32).clamp(0.0, 1.0);
    let field_load = (1.0 - (average_fields_per_statement / 16.0).clamp(0.0, 1.0)).max(0.0);
    let numeric_breath = if numeric_values == 0 {
        1.0
    } else {
        (1.0 - ((numeric_chars as f32 / numeric_values as f32) - 4.0).max(0.0) / 8.0)
            .clamp(0.0, 1.0)
    };
    let readability_score = (0.30 * (1.0 - alias_field_ratio)
        + 0.20 * named_command_ratio
        + 0.20 * line_room
        + 0.15 * field_load
        + 0.15 * numeric_breath)
        .clamp(0.0, 1.0);
    let terse_score = (1.0 / (1.0 + byte_count as f32 / 160.0)).clamp(0.0, 1.0);
    let balanced_score = if readability_score + terse_score <= f32::EPSILON {
        0.0
    } else {
        2.0 * readability_score * terse_score / (readability_score + terse_score)
    };
    PatchScriptMetrics {
        byte_count,
        line_count,
        statement_count,
        field_count,
        average_fields_per_statement,
        alias_field_ratio,
        terse_score,
        readability_score,
        balanced_score,
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
            fm: FrequencyModulation::default(),
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
    let mut compiler = PatchScriptCompiler::default();
    for (line_number, statement) in script_statements_with_lines(script) {
        compiler.parse_statement(statement, line_number)?;
    }
    if compiler.patch.voices.is_empty() {
        return Err(PatchScriptError::new(0, "script produced no voices"));
    }
    Ok(compiler.patch)
}

fn script_statements(script: &str) -> Vec<&str> {
    script_statements_with_lines(script)
        .into_iter()
        .map(|(_, statement)| statement)
        .collect()
}

fn script_statements_with_lines(script: &str) -> Vec<(usize, &str)> {
    script
        .lines()
        .enumerate()
        .flat_map(|(index, raw_line)| {
            let line_number = index + 1;
            raw_line
                .split('#')
                .next()
                .unwrap_or("")
                .split(';')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(move |statement| (line_number, statement))
        })
        .collect()
}

struct PatchScriptCompiler {
    patch: SynthPatch,
    voice_defaults: Vec<(String, String)>,
    voice_templates: Vec<(String, Vec<(String, String)>)>,
}

impl Default for PatchScriptCompiler {
    fn default() -> Self {
        Self {
            patch: SynthPatch::new(Vec::new()),
            voice_defaults: Vec::new(),
            voice_templates: Vec::new(),
        }
    }
}

impl PatchScriptCompiler {
    fn parse_statement(
        &mut self,
        statement: &str,
        line_number: usize,
    ) -> Result<(), PatchScriptError> {
        let mut parts = statement.split_whitespace();
        let Some(command) = parts.next() else {
            return Ok(());
        };
        let fields = parse_fields(parts, line_number)?;
        match command {
            name if SfxrParams::named(name).is_some() => {
                let mut params = SfxrParams::named(name).expect("checked by match guard");
                apply_sfxr_fields(&mut params, &fields, line_number)?;
                append_sfxr_patch(&mut self.patch, params);
            }
            "patch" | "p" => apply_patch_fields(&mut self.patch, &fields, line_number)?,
            "defaults" | "default" | "d" => self.apply_voice_defaults(&fields),
            "def" | "template" | "t" => self.define_voice_template(&fields, line_number)?,
            "wobble" | "wob" | "wb" => self.apply_wobble_bus(&fields, line_number)?,
            "lfo" | "control" | "l" => self
                .patch
                .controls
                .push(control_lane_from_fields(&fields, line_number)?),
            "voice" | "v" => {
                let merged = self.voice_fields(&fields, line_number)?;
                let borrowed = borrowed_fields(&merged);
                self.patch
                    .voices
                    .push(voice_from_fields(&borrowed, line_number)?);
            }
            "sfxr" | "s" => {
                let mut params = if let Some(name) =
                    field_value(&fields, "preset").or_else(|| field_value(&fields, "p"))
                {
                    SfxrParams::named(name).ok_or_else(|| {
                        PatchScriptError::new(line_number, format!("unknown sfxr preset `{name}`"))
                    })?
                } else {
                    SfxrParams::default()
                };
                apply_sfxr_fields(&mut params, &fields, line_number)?;
                append_sfxr_patch(&mut self.patch, params);
            }
            unknown => {
                return Err(PatchScriptError::new(
                    line_number,
                    format!("unknown command `{unknown}`"),
                ));
            }
        }
        Ok(())
    }

    fn apply_wobble_bus(
        &mut self,
        fields: &[(&str, &str)],
        line: usize,
    ) -> Result<(), PatchScriptError> {
        let frequency_hz = parse_optional_f32_any(fields, &["hz", "rate"], line)?.unwrap_or(4.0);
        let waveform = parse_mod_waveform(
            field_value_any(fields, &["wave", "w"]).unwrap_or("sine"),
            line,
        )?;
        let phase = parse_optional_f32(fields, "phase", line)?.unwrap_or(0.0);
        let bus = field_value_any(fields, &["name", "n"]).unwrap_or("wob");
        for (key, value) in fields {
            if matches!(*key, "hz" | "rate" | "wave" | "w" | "phase" | "name" | "n") {
                continue;
            }
            let Some(target) = wobble_target(key) else {
                return Err(unknown_field(line, "wobble", key));
            };
            let depth = parse_f32(value, line, key)?;
            self.patch.controls.push(ControlLane::new(
                format!("{bus}_{key}"),
                Modulator::lfo(target, waveform, frequency_hz, depth).with_phase(phase),
            ));
        }
        Ok(())
    }

    fn apply_voice_defaults(&mut self, fields: &[(&str, &str)]) {
        merge_owned_fields(&mut self.voice_defaults, fields);
    }

    fn define_voice_template(
        &mut self,
        fields: &[(&str, &str)],
        line: usize,
    ) -> Result<(), PatchScriptError> {
        let name = field_value(fields, "name")
            .or_else(|| field_value(fields, "n"))
            .ok_or_else(|| PatchScriptError::new(line, "template needs name"))?;
        let mut template = self
            .voice_templates
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, fields)| fields.clone())
            .unwrap_or_default();
        let fields_without_name: Vec<(&str, &str)> = fields
            .iter()
            .copied()
            .filter(|(key, _)| !matches!(*key, "name" | "n"))
            .collect();
        merge_owned_fields(&mut template, &fields_without_name);
        upsert_owned_template(&mut self.voice_templates, name, template);
        Ok(())
    }

    fn voice_fields(
        &self,
        fields: &[(&str, &str)],
        line: usize,
    ) -> Result<Vec<(String, String)>, PatchScriptError> {
        let mut merged = self.voice_defaults.clone();
        if let Some(names) = field_value_any(fields, &["use", "u"]) {
            for name in names
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
            {
                let (_, template) = self
                    .voice_templates
                    .iter()
                    .find(|(candidate, _)| candidate == name)
                    .ok_or_else(|| {
                        PatchScriptError::new(line, format!("unknown voice template `{name}`"))
                    })?;
                merge_owned_pairs(&mut merged, template);
            }
        }
        let explicit: Vec<(&str, &str)> = fields
            .iter()
            .copied()
            .filter(|(key, _)| !matches!(*key, "use" | "u"))
            .collect();
        merge_owned_fields(&mut merged, &explicit);
        Ok(merged)
    }
}

fn upsert_owned_template(
    templates: &mut Vec<(String, Vec<(String, String)>)>,
    name: &str,
    fields: Vec<(String, String)>,
) {
    if let Some((_, existing)) = templates
        .iter_mut()
        .find(|(candidate, _)| candidate == name)
    {
        *existing = fields;
    } else {
        templates.push((name.to_owned(), fields));
    }
}

fn merge_owned_pairs(target: &mut Vec<(String, String)>, fields: &[(String, String)]) {
    for (key, value) in fields {
        upsert_owned_field(target, key, value);
    }
}

fn merge_owned_fields(target: &mut Vec<(String, String)>, fields: &[(&str, &str)]) {
    for (key, value) in fields {
        upsert_owned_field(target, key, value);
    }
}

fn upsert_owned_field(target: &mut Vec<(String, String)>, key: &str, value: &str) {
    if let Some((_, existing)) = target.iter_mut().find(|(candidate, _)| candidate == key) {
        *existing = value.to_owned();
    } else {
        target.push((key.to_owned(), value.to_owned()));
    }
}

fn borrowed_fields(fields: &[(String, String)]) -> Vec<(&str, &str)> {
    fields
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect()
}

fn append_sfxr_patch(patch: &mut SynthPatch, params: SfxrParams) {
    let mapped = params.to_patch();
    patch.voices.extend(mapped.voices);
    patch.repeat = mapped.repeat;
    patch.gain *= mapped.gain;
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

fn field_value_any<'a>(fields: &'a [(&str, &str)], keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| field_value(fields, key))
}

fn apply_patch_fields(
    patch: &mut SynthPatch,
    fields: &[(&str, &str)],
    line: usize,
) -> Result<(), PatchScriptError> {
    for (key, value) in fields {
        match *key {
            "gain" | "g" => patch.gain = parse_f32(value, line, key)?,
            "soft_clip" | "sc" => patch.soft_clip = parse_bool(value, line, key)?,
            "repeat" | "r" | "rp" => {
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
    let waveform = match field_value_any(fields, &["wave", "w"]).unwrap_or("sine") {
        "sine" | "sin" => Waveform::Sine,
        "square" | "sq" => Waveform::Square,
        "saw" | "sawtooth" => Waveform::Sawtooth,
        "tri" | "triangle" => Waveform::Triangle,
        "noise" | "n" => Waveform::Noise,
        other => {
            return Err(PatchScriptError::new(
                line,
                format!("unknown waveform `{other}`"),
            ));
        }
    };
    let frequency_hz = parse_optional_f32_any(fields, &["freq", "f"], line)?.unwrap_or(440.0);
    let envelope = Envelope {
        attack_seconds: parse_optional_f32_any(fields, &["attack", "a"], line)?.unwrap_or(0.0),
        sustain_seconds: parse_optional_f32_any(fields, &["sustain", "s"], line)?.unwrap_or(0.1),
        decay_seconds: parse_optional_f32_any(fields, &["decay", "d"], line)?.unwrap_or(0.2),
        punch: parse_optional_f32_any(fields, &["punch", "pu"], line)?.unwrap_or(0.0),
    };
    let arpeggio = match (
        parse_optional_f32_any(fields, &["arp_delay", "ad"], line)?,
        parse_optional_f32_any(fields, &["arp_mult", "am"], line)?,
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
        noise_mix: parse_optional_f32_any(fields, &["noise", "nz"], line)?.unwrap_or(0.0),
        drive: parse_optional_f32_any(fields, &["drive", "drv"], line)?.unwrap_or(0.0),
        fold: parse_optional_f32_any(fields, &["fold", "fl"], line)?.unwrap_or(0.0),
        tremolo_depth: parse_optional_f32_any(fields, &["tremolo", "tr"], line)?.unwrap_or(0.0),
        tremolo_hz: parse_optional_f32_any(fields, &["tremolo_hz", "th"], line)?.unwrap_or(0.0),
        formant_mix: parse_optional_f32_any(fields, &["formant_mix", "fmix"], line)?.unwrap_or(0.0),
    };
    let formants = match field_value_any(fields, &["formants", "fs"]) {
        Some(value) => parse_formants(value, line)?,
        None => Vec::new(),
    };
    let modulators = match field_value_any(fields, &["mods", "m"]) {
        Some(value) => parse_modulators(value, line)?,
        None => Vec::new(),
    };
    let voice = Voice {
        oscillator: Oscillator {
            waveform,
            frequency_hz,
            duty: parse_optional_f32_any(fields, &["duty", "du"], line)?.unwrap_or(0.5),
            phase: parse_optional_f32_any(fields, &["phase", "pa"], line)?.unwrap_or(0.0),
        },
        envelope,
        pitch: PitchMotion {
            min_frequency_hz: parse_optional_f32_any(fields, &["min_freq", "min"], line)?
                .unwrap_or(20.0),
            ramp_per_second: parse_optional_f32_any(fields, &["pitch_ramp", "pr"], line)?
                .unwrap_or(0.0),
            delta_ramp_per_second: parse_optional_f32_any(fields, &["pitch_dramp", "pdr"], line)?
                .unwrap_or(0.0),
            vibrato_depth: parse_optional_f32_any(fields, &["vibrato", "vi"], line)?.unwrap_or(0.0),
            vibrato_hz: parse_optional_f32_any(fields, &["vibrato_hz", "vh"], line)?.unwrap_or(0.0),
            vibrato_delay_seconds: parse_optional_f32_any(fields, &["vibrato_delay", "vd"], line)?
                .unwrap_or(0.0),
        },
        duty: DutyMotion {
            ramp_per_second: parse_optional_f32_any(fields, &["duty_ramp", "dur"], line)?
                .unwrap_or(0.0),
        },
        filter: Filter {
            low_pass: parse_optional_f32_any(fields, &["lpf", "l"], line)?.unwrap_or(1.0),
            low_pass_ramp: parse_optional_f32_any(fields, &["lpf_ramp", "lr"], line)?
                .unwrap_or(0.0),
            low_pass_resonance: parse_optional_f32_any(fields, &["resonance", "res"], line)?
                .unwrap_or(0.0),
            high_pass: parse_optional_f32_any(fields, &["hpf", "h"], line)?.unwrap_or(0.0),
            high_pass_ramp: parse_optional_f32_any(fields, &["hpf_ramp", "hr"], line)?
                .unwrap_or(0.0),
        },
        phaser: Phaser {
            offset_seconds: parse_optional_f32_any(fields, &["phaser", "ph"], line)?.unwrap_or(0.0),
            ramp_seconds_per_second: parse_optional_f32_any(fields, &["phaser_ramp", "phr"], line)?
                .unwrap_or(0.0),
        },
        arpeggio,
        fm: FrequencyModulation {
            ratio: parse_optional_f32_any(fields, &["fm_ratio", "fm"], line)?.unwrap_or(1.0),
            index: parse_optional_f32_any(fields, &["fm_index", "fmi"], line)?.unwrap_or(0.0),
            index_decay_seconds: parse_optional_f32_any(fields, &["fm_decay", "fmd"], line)?
                .unwrap_or(0.0),
        },
        color,
        formants,
        modulators,
        gain: parse_optional_f32_any(fields, &["gain", "g"], line)?.unwrap_or(0.2),
    };
    for (key, _) in fields {
        match *key {
            "wave" | "freq" | "duty" | "phase" | "attack" | "sustain" | "decay" | "punch"
            | "min_freq" | "pitch_ramp" | "pitch_dramp" | "vibrato" | "vibrato_hz"
            | "vibrato_delay" | "duty_ramp" | "lpf" | "lpf_ramp" | "resonance" | "hpf"
            | "hpf_ramp" | "phaser" | "phaser_ramp" | "arp_delay" | "arp_mult" | "noise"
            | "drive" | "fold" | "tremolo" | "tremolo_hz" | "formant_mix" | "formants"
            | "fm_ratio" | "fm_index" | "fm_decay" | "gain" => {}
            "w" | "f" | "g" | "a" | "s" | "d" | "pu" | "min" | "pr" | "pdr" | "vi" | "vh"
            | "vd" | "du" | "dur" | "l" | "lr" | "res" | "h" | "hr" | "ph" | "phr" | "ad"
            | "am" | "nz" | "drv" | "fl" | "tr" | "th" | "fmix" | "fm" | "fmi" | "fmd" | "fs"
            | "m" | "pa" => {}
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
        "fm" | "fm_index" => Ok(ModTarget::FmIndex),
        other => Err(PatchScriptError::new(
            line,
            format!("unknown modulator target `{other}`"),
        )),
    }
}

fn wobble_target(value: &str) -> Option<ModTarget> {
    Some(match value {
        "gain" | "g" => ModTarget::Gain,
        "pitch" | "p" => ModTarget::Pitch,
        "duty" | "du" => ModTarget::Duty,
        "lpf" | "l" => ModTarget::LowPass,
        "hpf" | "h" => ModTarget::HighPass,
        "noise" | "nz" => ModTarget::Noise,
        "drive" | "drv" => ModTarget::Drive,
        "fold" | "fl" => ModTarget::Fold,
        "formant" | "formant_mix" | "fmix" => ModTarget::FormantMix,
        "fm" | "fmi" | "fm_index" => ModTarget::FmIndex,
        _ => return None,
    })
}

fn parse_mod_waveform(value: &str, line: usize) -> Result<ModWaveform, PatchScriptError> {
    match value {
        "sine" | "sin" => Ok(ModWaveform::Sine),
        "tri" | "triangle" => Ok(ModWaveform::Triangle),
        "square" | "sq" => Ok(ModWaveform::Square),
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
    if let Some(seed) = field_value(fields, "mutate_seed").or_else(|| field_value(fields, "ms")) {
        let seed = seed
            .parse::<u64>()
            .map_err(|_| PatchScriptError::new(line, "mutate_seed must be an integer"))?;
        let amount = parse_optional_f32(fields, "mutate", line)?
            .or(parse_optional_f32(fields, "m", line)?)
            .unwrap_or(0.05);
        params.mutate(seed, amount);
    }
    for (key, value) in fields {
        match *key {
            "preset" | "p" | "mutate_seed" | "ms" | "mutate" | "m" => {}
            "wave" | "w" => {
                params.wave_type = match *value {
                    "sine" => Waveform::Sine,
                    "sin" => Waveform::Sine,
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
            "base" | "b" => params.base_freq = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "limit" | "lim" => params.freq_limit = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "ramp" | "r" => params.freq_ramp = parse_f32(value, line, key)?.clamp(-1.0, 1.0),
            "dramp" | "dr" => params.freq_dramp = parse_f32(value, line, key)?.clamp(-1.0, 1.0),
            "duty" | "du" => params.duty = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "duty_ramp" | "dur" => params.duty_ramp = parse_f32(value, line, key)?.clamp(-1.0, 1.0),
            "vib" | "vi" => params.vib_strength = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "vib_speed" | "vs" => params.vib_speed = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "vib_delay" | "vd" => params.vib_delay = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "attack" | "a" => params.env_attack = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "sustain" | "s" => params.env_sustain = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "decay" | "d" => params.env_decay = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "punch" | "pu" => params.env_punch = parse_f32(value, line, key)?.clamp(-1.0, 1.0),
            "resonance" | "res" => {
                params.lpf_resonance = parse_f32(value, line, key)?.clamp(0.0, 1.0)
            }
            "lpf" => params.lpf_freq = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "lpf_ramp" | "lpfr" => params.lpf_ramp = parse_f32(value, line, key)?.clamp(-1.0, 1.0),
            "hpf" => params.hpf_freq = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "hpf_ramp" | "hpfr" => params.hpf_ramp = parse_f32(value, line, key)?.clamp(-1.0, 1.0),
            "phaser" | "ph" => params.pha_offset = parse_f32(value, line, key)?.clamp(-1.0, 1.0),
            "phaser_ramp" | "phr" => {
                params.pha_ramp = parse_f32(value, line, key)?.clamp(-1.0, 1.0)
            }
            "repeat" | "rep" => params.repeat_speed = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "arp" => params.arp_speed = parse_f32(value, line, key)?.clamp(0.0, 1.0),
            "arp_mod" | "am" => params.arp_mod = parse_f32(value, line, key)?.clamp(-1.0, 1.0),
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

fn parse_optional_f32_any(
    fields: &[(&str, &str)],
    keys: &[&str],
    line: usize,
) -> Result<Option<f32>, PatchScriptError> {
    match keys
        .iter()
        .find_map(|key| field_value(fields, key).map(|value| (*key, value)))
    {
        Some((key, value)) => parse_f32(value, line, key).map(Some),
        None => Ok(None),
    }
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
    fm_phase: f32,
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
            fm_phase: 0.0,
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
        self.fm_phase = (self.fm_phase + frequency * voice.fm.ratio.max(0.0) / sample_rate).fract();
        if self.phase < previous_phase {
            self.noise_epoch = self.noise_epoch.wrapping_add(1);
        }
        let fm_index_mod =
            mod_amount(&voice.modulators, controls, ModTarget::FmIndex, age, seed).max(0.0);
        let mut fm = voice.fm;
        fm.index += fm_index_mod;
        let phase = self.phase + voice.oscillator.phase + fm_phase_offset(fm, self.fm_phase, age);
        let mut sample = oscillator_sample(
            voice.oscillator.waveform,
            phase,
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

fn fm_phase_offset(fm: FrequencyModulation, phase: f32, age: f32) -> f32 {
    if fm.index <= 0.0 || fm.ratio <= 0.0 {
        return 0.0;
    }
    let decay = if fm.index_decay_seconds > 0.0 {
        (-age / fm.index_decay_seconds.max(0.0001)).exp()
    } else {
        1.0
    };
    (phase * TAU).sin() * fm.index * decay / TAU
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
        for name in CLASSIC_SFXR_NAMES {
            let patch = SfxrParams::named(name).unwrap().to_patch();
            let mut player = PatchPlayer::new(patch, 44_100.0);
            let peak = (0..8192)
                .map(|_| player.next_sample().abs())
                .fold(0.0, f32::max);
            assert!(peak > 0.001, "{name} was silent");
        }
    }

    #[test]
    fn classic_sfxr_golf_script_expands_all_presets() {
        let patch = SynthPatch::from_script(CLASSIC_SFXR_GOLF_SCRIPT).unwrap();
        assert_eq!(patch.voices.len(), CLASSIC_SFXR_NAMES.len());
        let mut player = PatchPlayer::new(patch, 44_100.0);
        let peak = (0..16_384)
            .map(|_| player.next_sample().abs())
            .fold(0.0, f32::max);
        assert!(peak > 0.01);
        assert!(CLASSIC_SFXR_GOLF_SCRIPT.lines().count() == 1);
    }

    #[test]
    fn bare_sfxr_atoms_match_named_preset_outputs() {
        for name in CLASSIC_SFXR_NAMES {
            let script_patch = SynthPatch::from_script(name).unwrap();
            let named_patch = SfxrParams::named(name).unwrap().to_patch();
            let mut script_player = PatchPlayer::new(script_patch, 44_100.0);
            let mut named_player = PatchPlayer::new(named_patch, 44_100.0);
            let script_buffer: Vec<f32> = (0..8192).map(|_| script_player.next_sample()).collect();
            let named_buffer: Vec<f32> = (0..8192).map(|_| named_player.next_sample()).collect();
            assert_eq!(script_buffer, named_buffer, "{name} did not round trip");
        }
    }

    #[test]
    fn golfed_sfxr_atoms_accept_short_input_overrides() {
        let golfed = SynthPatch::from_script("laser ms=9 m=0.01").unwrap();
        let verbose =
            SynthPatch::from_script("sfxr preset=laser mutate_seed=9 mutate=0.01").unwrap();
        let mut golfed_player = PatchPlayer::new(golfed, 44_100.0);
        let mut verbose_player = PatchPlayer::new(verbose, 44_100.0);
        let golfed_buffer: Vec<f32> = (0..4096).map(|_| golfed_player.next_sample()).collect();
        let verbose_buffer: Vec<f32> = (0..4096).map(|_| verbose_player.next_sample()).collect();
        assert_eq!(golfed_buffer, verbose_buffer);
    }

    #[test]
    fn primitive_golf_scripts_use_only_graph_primitives() {
        for (name, script) in CLASSIC_SFXR_PRIMITIVE_GOLF_SCRIPTS {
            assert!(
                !script.contains("sfxr"),
                "{name} primitive script used sfxr command"
            );
            for statement in script
                .split(';')
                .map(str::trim)
                .filter(|part| !part.is_empty())
            {
                let command = statement.split_whitespace().next().unwrap();
                assert!(
                    matches!(
                        command,
                        "p" | "patch" | "v" | "voice" | "l" | "lfo" | "control"
                    ),
                    "{name} primitive script used non-primitive command `{command}`"
                );
                assert!(
                    !CLASSIC_SFXR_NAMES.contains(&command),
                    "{name} primitive script used preset atom `{command}`"
                );
            }
        }
    }

    #[test]
    fn primitive_golf_scripts_match_classic_outputs() {
        for (name, script) in CLASSIC_SFXR_PRIMITIVE_GOLF_SCRIPTS {
            let primitive = SynthPatch::from_script(script).unwrap();
            let reference = SfxrParams::named(name).unwrap().to_patch();
            let mut primitive_player = PatchPlayer::new(primitive, 44_100.0);
            let mut reference_player = PatchPlayer::new(reference, 44_100.0);
            let primitive_buffer: Vec<f32> = (0..16_384)
                .map(|_| primitive_player.next_sample())
                .collect();
            let reference_buffer: Vec<f32> = (0..16_384)
                .map(|_| reference_player.next_sample())
                .collect();
            let comparison = compare_audio(
                &reference_buffer,
                &primitive_buffer,
                &AudioAnalysisConfig {
                    fft_size: 256,
                    hop_size: 256,
                    mel_band_count: 18,
                    ..AudioAnalysisConfig::default()
                },
            );
            assert!(
                comparison.score > 0.995,
                "{name} score was {}",
                comparison.score
            );
            assert!(
                comparison.log_mel_distance < 0.01,
                "{name} log-mel distance was {}",
                comparison.log_mel_distance
            );
        }
    }

    #[test]
    fn primitive_golf_scripts_expose_readability_metrics() {
        for (name, script) in CLASSIC_SFXR_PRIMITIVE_GOLF_SCRIPTS {
            let metrics = patch_script_metrics(script);
            assert_eq!(
                metrics.statement_count,
                script
                    .split(';')
                    .filter(|part| !part.trim().is_empty())
                    .count(),
                "{name} statement count drifted"
            );
            assert!(metrics.terse_score > 0.35, "{name} was not terse enough");
            assert!(
                metrics.readability_score > 0.15,
                "{name} readability score was {}",
                metrics.readability_score
            );
            assert!(
                metrics.balanced_score > 0.22,
                "{name} balanced score was {}",
                metrics.balanced_score
            );
        }
    }

    #[test]
    fn voice_defaults_and_templates_abstract_common_structure() {
        let abstracted = SynthPatch::from_script(
            "d g=.22 drv=.12;def name=N w=n nz=.35;v u=N f=51.4206 s=.00566893424 d=.09070294785 pr=1.050624 h=.12",
        )
        .unwrap();
        let explicit = SynthPatch::from_script(
            "v w=n f=51.4206 g=.22 s=.00566893424 d=.09070294785 pr=1.050624 h=.12 nz=.35 drv=.12",
        )
        .unwrap();
        let mut abstracted_player = PatchPlayer::new(abstracted, 44_100.0);
        let mut explicit_player = PatchPlayer::new(explicit, 44_100.0);
        let abstracted_buffer: Vec<f32> =
            (0..8192).map(|_| abstracted_player.next_sample()).collect();
        let explicit_buffer: Vec<f32> = (0..8192).map(|_| explicit_player.next_sample()).collect();
        assert_eq!(abstracted_buffer, explicit_buffer);
    }

    #[test]
    fn abstract_golf_script_borrows_compiler_structure() {
        let patch = SynthPatch::from_script(CLASSIC_SFXR_ABSTRACT_GOLF_SCRIPT).unwrap();
        assert_eq!(patch.voices.len(), CLASSIC_SFXR_NAMES.len());
        let flat_script = CLASSIC_SFXR_PRIMITIVE_GOLF_SCRIPTS
            .iter()
            .map(|(_, script)| *script)
            .collect::<Vec<_>>()
            .join(";");
        let abstracted = patch_script_metrics(CLASSIC_SFXR_ABSTRACT_GOLF_SCRIPT);
        let flat = patch_script_metrics(&flat_script);
        assert!(abstracted.field_count < flat.field_count);
        assert!(abstracted.byte_count < flat.byte_count);
        assert!(CLASSIC_SFXR_ABSTRACT_GOLF_SCRIPT.contains("def "));
        assert!(CLASSIC_SFXR_ABSTRACT_GOLF_SCRIPT.contains("u=N"));
    }

    #[test]
    fn classic_808_scripts_use_only_graph_primitives() {
        for (name, script) in CLASSIC_808_PRIMITIVE_GOLF_SCRIPTS {
            let patch = SynthPatch::from_script(script).unwrap();
            assert!(!patch.voices.is_empty(), "{name} produced no voices");
            for statement in script
                .split(';')
                .map(str::trim)
                .filter(|part| !part.is_empty())
            {
                let command = statement.split_whitespace().next().unwrap();
                assert!(
                    matches!(
                        command,
                        "p" | "patch"
                            | "d"
                            | "defaults"
                            | "def"
                            | "template"
                            | "t"
                            | "v"
                            | "voice"
                            | "l"
                            | "lfo"
                            | "control"
                    ),
                    "{name} used non-primitive command `{command}`"
                );
            }
        }
    }

    #[test]
    fn classic_808_scripts_have_recognizable_audio_shapes() {
        for (name, script) in CLASSIC_808_PRIMITIVE_GOLF_SCRIPTS {
            let buffer = render_script_mono(
                script,
                RenderOptions {
                    duration_seconds: 0.6,
                    ..RenderOptions::default()
                },
            )
            .unwrap();
            let analysis = analyze_audio(
                &buffer,
                &AudioAnalysisConfig {
                    fft_size: 256,
                    hop_size: 256,
                    mel_band_count: 18,
                    ..AudioAnalysisConfig::default()
                },
            );
            assert!(analysis.features.peak > 0.01, "{name} was too quiet");
            assert!(
                analysis.features.duration_seconds < 0.58,
                "{name} rang too long"
            );
            match name {
                "kick" => {
                    assert!(analysis.features.duration_seconds > 0.25);
                    assert!(analysis.features.spectral_centroid_hz < 450.0);
                }
                "snare" | "clap" => {
                    assert!(
                        analysis.features.spectral_centroid_hz > 500.0,
                        "{name} centroid was {}",
                        analysis.features.spectral_centroid_hz
                    );
                    assert!(
                        analysis.features.zero_crossing_rate > 800.0,
                        "{name} zcr was {}",
                        analysis.features.zero_crossing_rate
                    );
                }
                "hat" => {
                    assert!(analysis.features.duration_seconds < 0.12);
                    assert!(
                        analysis.features.zero_crossing_rate > 1200.0,
                        "{name} zcr was {}",
                        analysis.features.zero_crossing_rate
                    );
                }
                "tom" => {
                    assert!(analysis.features.duration_seconds > 0.18);
                    assert!(analysis.features.spectral_centroid_hz < 650.0);
                }
                "cowbell" => {
                    assert!(analysis.features.zero_crossing_rate > 500.0);
                    assert!(analysis.features.spectral_centroid_hz > 350.0);
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn classic_808_scripts_score_as_golfable_but_readable() {
        for (name, script) in CLASSIC_808_PRIMITIVE_GOLF_SCRIPTS {
            let metrics = patch_script_metrics(script);
            assert!(metrics.terse_score > 0.35, "{name} was not terse enough");
            assert!(
                metrics.readability_score > 0.14,
                "{name} readability was {}",
                metrics.readability_score
            );
            assert!(
                metrics.balanced_score > 0.22,
                "{name} balanced score was {}",
                metrics.balanced_score
            );
        }
    }

    #[test]
    fn fm_fields_create_bright_inharmonic_spectra() {
        let plain = render_script_mono(
            "v w=sin f=440 g=.2 s=.04 d=.8",
            RenderOptions {
                duration_seconds: 0.9,
                ..RenderOptions::default()
            },
        )
        .unwrap();
        let fm = render_script_mono(
            "v w=sin f=440 g=.2 s=.04 d=.8 fm=4.1 fmi=5.8 fmd=.45",
            RenderOptions {
                duration_seconds: 0.9,
                ..RenderOptions::default()
            },
        )
        .unwrap();
        let comparison = compare_audio(
            &plain,
            &fm,
            &AudioAnalysisConfig {
                fft_size: 256,
                hop_size: 256,
                mel_band_count: 18,
                ..AudioAnalysisConfig::default()
            },
        );
        assert!(comparison.log_mel_distance > 0.08);
        assert!(
            comparison.candidate.features.spectral_centroid_hz
                > comparison.reference.features.spectral_centroid_hz * 1.5
        );
    }

    #[test]
    fn fm_bell_scripts_use_only_graph_primitives() {
        for (name, script) in FM_BELL_PRIMITIVE_GOLF_SCRIPTS {
            let patch = SynthPatch::from_script(script).unwrap();
            assert!(!patch.voices.is_empty(), "{name} produced no voices");
            for statement in script
                .split(';')
                .map(str::trim)
                .filter(|part| !part.is_empty())
            {
                let command = statement.split_whitespace().next().unwrap();
                assert!(
                    matches!(
                        command,
                        "p" | "patch"
                            | "d"
                            | "defaults"
                            | "def"
                            | "template"
                            | "t"
                            | "v"
                            | "voice"
                            | "l"
                            | "lfo"
                            | "control"
                    ),
                    "{name} used non-primitive command `{command}`"
                );
            }
        }
    }

    #[test]
    fn fm_bell_scripts_have_bell_like_audio_shapes() {
        for (name, script) in FM_BELL_PRIMITIVE_GOLF_SCRIPTS {
            let buffer = render_script_mono(
                script,
                RenderOptions {
                    duration_seconds: 1.8,
                    ..RenderOptions::default()
                },
            )
            .unwrap();
            let analysis = analyze_audio(
                &buffer,
                &AudioAnalysisConfig {
                    fft_size: 256,
                    hop_size: 256,
                    mel_band_count: 18,
                    ..AudioAnalysisConfig::default()
                },
            );
            assert!(analysis.features.peak > 0.01, "{name} was too quiet");
            assert!(
                analysis.features.duration_seconds > 0.35,
                "{name} decayed too quickly"
            );
            assert!(
                analysis.features.spectral_centroid_hz > 450.0,
                "{name} centroid was {}",
                analysis.features.spectral_centroid_hz
            );
            assert!(
                analysis.features.zero_crossing_rate > 500.0,
                "{name} zcr was {}",
                analysis.features.zero_crossing_rate
            );
        }
    }

    #[test]
    fn fm_bell_scripts_score_as_golfable_but_readable() {
        for (name, script) in FM_BELL_PRIMITIVE_GOLF_SCRIPTS {
            let metrics = patch_script_metrics(script);
            assert!(metrics.terse_score > 0.35, "{name} was not terse enough");
            assert!(
                metrics.readability_score > 0.14,
                "{name} readability was {}",
                metrics.readability_score
            );
            assert!(
                metrics.balanced_score > 0.22,
                "{name} balanced score was {}",
                metrics.balanced_score
            );
        }
    }

    #[test]
    fn wobble_bus_expands_to_many_control_lanes() {
        let patch = SynthPatch::from_script(
            "wob hz=4 w=tri g=.4 l=.5 p=.03 drv=.2 fmi=1.4;v w=saw f=55 s=.5 d=.2",
        )
        .unwrap();
        assert_eq!(patch.controls.len(), 5);
        assert!(
            patch
                .controls
                .iter()
                .any(|lane| lane.modulator.target == ModTarget::Gain)
        );
        assert!(
            patch
                .controls
                .iter()
                .any(|lane| lane.modulator.target == ModTarget::LowPass)
        );
        assert!(
            patch
                .controls
                .iter()
                .any(|lane| lane.modulator.target == ModTarget::Pitch)
        );
        assert!(
            patch
                .controls
                .iter()
                .any(|lane| lane.modulator.target == ModTarget::Drive)
        );
        assert!(
            patch
                .controls
                .iter()
                .any(|lane| lane.modulator.target == ModTarget::FmIndex)
        );
    }

    #[test]
    fn wobble_bass_scripts_use_only_graph_primitives() {
        for (name, script) in WOBBLE_BASS_PRIMITIVE_GOLF_SCRIPTS {
            let patch = SynthPatch::from_script(script).unwrap();
            assert!(!patch.voices.is_empty(), "{name} produced no voices");
            assert!(
                !patch.controls.is_empty(),
                "{name} produced no wobble controls"
            );
            for statement in script
                .split(';')
                .map(str::trim)
                .filter(|part| !part.is_empty())
            {
                let command = statement.split_whitespace().next().unwrap();
                assert!(
                    matches!(
                        command,
                        "p" | "patch"
                            | "d"
                            | "defaults"
                            | "def"
                            | "template"
                            | "t"
                            | "v"
                            | "voice"
                            | "l"
                            | "lfo"
                            | "control"
                            | "wob"
                            | "wobble"
                            | "wb"
                    ),
                    "{name} used non-primitive command `{command}`"
                );
            }
        }
    }

    #[test]
    fn wobble_bass_scripts_have_moving_bass_shapes() {
        let static_buffer = render_script_mono(
            "d w=saw f=55 g=.2 s=.8 d=.25 l=.34 h=.02 drv=.3;v;v f=110 g=.08",
            RenderOptions {
                duration_seconds: 1.0,
                ..RenderOptions::default()
            },
        )
        .unwrap();
        for (name, script) in WOBBLE_BASS_PRIMITIVE_GOLF_SCRIPTS {
            let buffer = render_script_mono(
                script,
                RenderOptions {
                    duration_seconds: 1.0,
                    ..RenderOptions::default()
                },
            )
            .unwrap();
            let comparison = compare_audio(
                &static_buffer,
                &buffer,
                &AudioAnalysisConfig {
                    fft_size: 256,
                    hop_size: 128,
                    mel_band_count: 18,
                    ..AudioAnalysisConfig::default()
                },
            );
            assert!(
                comparison.candidate.features.peak > 0.01,
                "{name} was too quiet"
            );
            assert!(
                comparison.candidate.features.duration_seconds > 0.6,
                "{name} was not sustained"
            );
            assert!(
                comparison.envelope_distance > 0.08,
                "{name} envelope did not wobble enough: {}",
                comparison.envelope_distance
            );
            assert!(
                comparison.log_mel_distance > 0.08,
                "{name} spectrum did not move enough: {}",
                comparison.log_mel_distance
            );
        }
    }

    #[test]
    fn wobble_bass_scripts_score_as_golfable_but_readable() {
        for (name, script) in WOBBLE_BASS_PRIMITIVE_GOLF_SCRIPTS {
            let metrics = patch_script_metrics(script);
            assert!(metrics.terse_score > 0.28, "{name} was not terse enough");
            assert!(
                metrics.readability_score > 0.12,
                "{name} readability was {}",
                metrics.readability_score
            );
            assert!(
                metrics.balanced_score > 0.18,
                "{name} balanced score was {}",
                metrics.balanced_score
            );
        }
    }

    #[test]
    fn readability_metric_rewards_descriptive_spacing() {
        let golfed = patch_script_metrics(CLASSIC_SFXR_PRIMITIVE_GOLF_SCRIPTS[1].1);
        let readable = patch_script_metrics(
            "patch repeat=.11315\nvoice wave=sine freq=57.5946 gain=.22 sustain=.1306122449 decay=.1777777778 pitch_ramp=-.208544 vibrato=.09 vibrato_hz=3.9602 drive=.12",
        );
        assert!(readable.readability_score > golfed.readability_score);
        assert!(golfed.terse_score > readable.terse_score);
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
