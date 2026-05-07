use aquarium_synth::{PatchPlayer, SfxrParams, Waveform};
use sfxr::{Generator, Sample, WaveType};

const SAMPLE_RATE: f32 = 44_100.0;
const FRAMES: usize = 44_100;

#[derive(Clone, Copy, Debug)]
struct AudioFeatures {
    attack_seconds: f32,
    duration_seconds: f32,
    peak: f32,
    rms: f32,
    zero_crossing_rate: f32,
}

#[test]
fn classic_sfxr_presets_stay_near_reference_shape() {
    for case in reference_cases() {
        let reference_sample = (case.reference)();
        let patch_sample = (case.reference)();
        let reference = features(&render_reference(reference_sample));
        let actual = features(&render_patch(sfxr_params_from_reference(patch_sample)));

        assert!(
            actual.peak > 0.001,
            "{} generated silence in the modular synth path",
            case.name
        );
        assert_feature_ratio(
            case.name,
            "duration",
            actual.duration_seconds,
            reference.duration_seconds,
            0.18,
            4.5,
        );
        assert_feature_ratio(case.name, "rms", actual.rms, reference.rms, 0.05, 12.0);
        assert_feature_ratio(
            case.name,
            "zero crossing rate",
            actual.zero_crossing_rate,
            reference.zero_crossing_rate,
            0.025,
            18.0,
        );
        assert_feature_ratio(
            case.name,
            "attack",
            actual.attack_seconds + 0.002,
            reference.attack_seconds + 0.002,
            0.02,
            50.0,
        );
    }
}

struct ReferenceCase {
    name: &'static str,
    reference: fn() -> Sample,
}

fn reference_cases() -> [ReferenceCase; 7] {
    [
        ReferenceCase {
            name: "pickup",
            reference: || Sample::pickup(Some(10)),
        },
        ReferenceCase {
            name: "laser",
            reference: || Sample::laser(Some(20)),
        },
        ReferenceCase {
            name: "explosion",
            reference: || Sample::explosion(Some(30)),
        },
        ReferenceCase {
            name: "powerup",
            reference: || Sample::powerup(Some(40)),
        },
        ReferenceCase {
            name: "hit",
            reference: || Sample::hit(Some(50)),
        },
        ReferenceCase {
            name: "jump",
            reference: || Sample::jump(Some(60)),
        },
        ReferenceCase {
            name: "blip",
            reference: || Sample::blip(Some(70)),
        },
    ]
}

fn render_reference(sample: Sample) -> Vec<f32> {
    let mut generator = Generator::new(sample);
    let mut buffer = vec![0.0; FRAMES];
    generator.generate(&mut buffer);
    buffer
}

fn render_patch(params: SfxrParams) -> Vec<f32> {
    let mut player = PatchPlayer::new(params.to_patch(), SAMPLE_RATE);
    (0..FRAMES).map(|_| player.next_sample()).collect()
}

fn sfxr_params_from_reference(sample: Sample) -> SfxrParams {
    SfxrParams {
        wave_type: match sample.wave_type {
            WaveType::Square => Waveform::Square,
            WaveType::Sine => Waveform::Sine,
            WaveType::Noise => Waveform::Noise,
            WaveType::Triangle => Waveform::Triangle,
        },
        base_freq: sample.base_freq as f32,
        freq_limit: sample.freq_limit as f32,
        freq_ramp: sample.freq_ramp as f32,
        freq_dramp: sample.freq_dramp as f32,
        duty: sample.duty,
        duty_ramp: sample.duty_ramp,
        vib_strength: sample.vib_strength as f32,
        vib_speed: sample.vib_speed as f32,
        vib_delay: sample.vib_delay,
        env_attack: sample.env_attack,
        env_sustain: sample.env_sustain,
        env_decay: sample.env_decay,
        env_punch: sample.env_punch,
        lpf_resonance: sample.lpf_resonance,
        lpf_freq: sample.lpf_freq,
        lpf_ramp: sample.lpf_ramp,
        hpf_freq: sample.hpf_freq,
        hpf_ramp: sample.hpf_ramp,
        pha_offset: sample.pha_offset,
        pha_ramp: sample.pha_ramp,
        repeat_speed: sample.repeat_speed,
        arp_speed: sample.arp_speed,
        arp_mod: sample.arp_mod as f32,
    }
}

fn features(buffer: &[f32]) -> AudioFeatures {
    let peak = buffer.iter().map(|sample| sample.abs()).fold(0.0, f32::max);
    let gate = (peak * 0.03).max(0.0005);
    let first = buffer
        .iter()
        .position(|sample| sample.abs() >= gate)
        .unwrap_or(0);
    let last = buffer
        .iter()
        .rposition(|sample| sample.abs() >= gate)
        .unwrap_or(first);
    let active = &buffer[first..=last];
    let rms = (active.iter().map(|sample| sample * sample).sum::<f32>()
        / active.len().max(1) as f32)
        .sqrt();
    let zero_crossings = active
        .windows(2)
        .filter(|pair| pair[0].signum() != pair[1].signum())
        .count();
    let duration_seconds = (last.saturating_sub(first) + 1) as f32 / SAMPLE_RATE;
    AudioFeatures {
        attack_seconds: first as f32 / SAMPLE_RATE,
        duration_seconds,
        peak,
        rms,
        zero_crossing_rate: zero_crossings as f32 / duration_seconds.max(1.0 / SAMPLE_RATE),
    }
}

fn assert_feature_ratio(
    preset: &str,
    feature: &str,
    actual: f32,
    reference: f32,
    min_ratio: f32,
    max_ratio: f32,
) {
    let ratio = actual / reference.max(f32::EPSILON);
    assert!(
        ratio >= min_ratio && ratio <= max_ratio,
        "{preset} {feature} drifted too far from sfxr reference: actual={actual:.6}, reference={reference:.6}, ratio={ratio:.3}"
    );
}
