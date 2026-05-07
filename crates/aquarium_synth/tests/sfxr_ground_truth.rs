use aquarium_synth::{AudioAnalysisConfig, PatchPlayer, SfxrParams, Waveform, compare_audio};
use sfxr::{Generator, Sample, WaveType};

const SAMPLE_RATE: f32 = 44_100.0;
const FRAMES: usize = 44_100;

#[test]
fn classic_sfxr_presets_stay_near_reference_shape() {
    let config = AudioAnalysisConfig {
        fft_size: 128,
        hop_size: 512,
        mel_band_count: 16,
        ..AudioAnalysisConfig::default()
    };
    for case in reference_cases() {
        let reference_sample = (case.reference)();
        let patch_sample = (case.reference)();
        let comparison = compare_audio(
            &render_reference(reference_sample),
            &render_patch(sfxr_params_from_reference(patch_sample)),
            &config,
        );
        let reference = &comparison.reference.features;
        let actual = &comparison.candidate.features;

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
        assert!(
            comparison.score > 0.02,
            "{} had a suspiciously low comparison score: {:.4}",
            case.name,
            comparison.score
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
