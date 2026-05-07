use fundsp::prelude::{AttoHash, AudioUnit, BufferMut, BufferRef, SignalFrame};
use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;
use std::{error::Error, fmt};

const DEFAULT_SAMPLE_RATE: f32 = 44_100.0;

pub const PATCH_SCRIPT_EXAMPLE: &str = r#"
# One command per line. Comments start with #.
patch gain=0.7 soft_clip=true
voice wave=sine freq=220 gain=0.12 attack=0.002 sustain=0.03 decay=0.2 vibrato=0.02 vibrato_hz=5
voice wave=triangle freq=440 gain=0.04 attack=0 sustain=0.02 decay=0.18 lpf=0.7 hpf=0.02
sfxr preset=laser mutate_seed=9 mutate=0.01
"#;

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

    pub fn from_script(script: &str) -> Result<Self, PatchScriptError> {
        parse_patch_script(script)
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
        gain: parse_optional_f32(fields, "gain", line)?.unwrap_or(0.2),
    };
    for (key, _) in fields {
        match *key {
            "wave" | "freq" | "duty" | "phase" | "attack" | "sustain" | "decay" | "punch"
            | "min_freq" | "pitch_ramp" | "pitch_dramp" | "vibrato" | "vibrato_hz"
            | "vibrato_delay" | "duty_ramp" | "lpf" | "lpf_ramp" | "resonance" | "hpf"
            | "hpf_ramp" | "phaser" | "phaser_ramp" | "arp_delay" | "arp_mult" | "gain" => {}
            unknown => return Err(unknown_field(line, "voice", unknown)),
        }
    }
    Ok(voice)
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
}
