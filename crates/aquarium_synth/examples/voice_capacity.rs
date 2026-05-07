use aquarium_synth::{
    PatchPlayer, RenderOptions, SynthPatch, Voice, presets, render_script_mono,
};
use std::time::{Duration, Instant};

const SAMPLE_RATE: f32 = 44_100.0;
const DURATION_SECONDS: f32 = 2.0;
const MIN_BENCH_SECONDS: f32 = 0.9;

struct Case {
    name: &'static str,
    patch: SynthPatch,
}

fn main() {
    let cases = [
        Case {
            name: "simple_pluck",
            patch: stack_patch(presets::aquarium_pluck(), 1),
        },
        Case {
            name: "colored_voice",
            patch: stack_patch(presets::aquarium_voice(), 1),
        },
        Case {
            name: "mod_bus_wobble",
            patch: SynthPatch::from_script(
                "d w=saw f=55 g=.18 s=.8 d=.25 l=.34 h=.02 drv=.3 fl=.08 fm=2 fmi=.8 fmd=.7 fs=520:90:.7,1250:170:1,2600:320:.45 fmix=.35;mod n=wob hz=4 w=tri g=.42 l=.48 fmix=.38 fmi=1.6 drv=.2 fl=.14;v;v f=110 g=.08 du=.42",
            )
            .expect("wobble benchmark patch parses"),
        },
    ];

    println!("sample_rate_hz,{SAMPLE_RATE}");
    println!("duration_seconds,{DURATION_SECONDS}");
    println!("case,voices,best_ms,median_ms,best_speedup_x,median_speedup_x,estimated_realtime_voices");
    for case in cases {
        for voices in [1, 2, 4, 8, 16, 32, 64, 128, 256] {
            let patch = stack_patch(case.patch.clone(), voices);
            let result = bench_patch(&patch);
            let best_speedup = DURATION_SECONDS as f64 / result.best.as_secs_f64();
            let median_speedup = DURATION_SECONDS as f64 / result.median.as_secs_f64();
            let estimated = median_speedup * voices as f64;
            println!(
                "{},{},{:.3},{:.3},{:.2},{:.2},{:.1}",
                case.name,
                voices,
                result.best.as_secs_f64() * 1000.0,
                result.median.as_secs_f64() * 1000.0,
                best_speedup,
                median_speedup,
                estimated
            );
            if median_speedup < 1.25 {
                break;
            }
        }
    }

    let script = "bus n=sweep w=tri hz=3 to=g:.2,l:-.18,p:.01,fmix:.2,fmi:1.1;v w=saw f=110 g=.2 s=.5 d=.2 fm=2 fmi=.7 fmd=.4 fs=500:90:1,1400:160:.8 fmix=.3";
    let start = Instant::now();
    let _ = render_script_mono(
        script,
        RenderOptions {
            sample_rate: SAMPLE_RATE,
            duration_seconds: DURATION_SECONDS,
            ..RenderOptions::default()
        },
    )
    .expect("script benchmark renders");
    eprintln!("parse_plus_render_ms,{:.3}", start.elapsed().as_secs_f64() * 1000.0);
}

struct BenchResult {
    best: Duration,
    median: Duration,
}

fn bench_patch(patch: &SynthPatch) -> BenchResult {
    let mut best = Duration::MAX;
    let mut elapsed = Duration::ZERO;
    let frame_count = (SAMPLE_RATE * DURATION_SECONDS).ceil() as usize;
    let mut output = vec![0.0; frame_count];
    let mut player = PatchPlayer::new(patch.clone(), SAMPLE_RATE);
    let mut samples = Vec::new();
    while elapsed.as_secs_f32() < MIN_BENCH_SECONDS {
        player.reset();
        let start = Instant::now();
        player.render_mono(&mut output);
        let sample_elapsed = start.elapsed();
        best = best.min(sample_elapsed);
        samples.push(sample_elapsed);
        elapsed += sample_elapsed;
    }
    samples.sort_unstable();
    let median = samples[samples.len() / 2];
    BenchResult { best, median }
}

fn stack_patch(source: SynthPatch, count: usize) -> SynthPatch {
    let voices = source
        .voices
        .iter()
        .cycle()
        .take(count)
        .cloned()
        .enumerate()
        .map(|(index, mut voice)| {
            spread_voice(&mut voice, index);
            voice
        })
        .collect();
    SynthPatch {
        voices,
        controls: source.controls,
        repeat: source.repeat,
        gain: source.gain / (count as f32).sqrt().max(1.0),
        soft_clip: source.soft_clip,
    }
}

fn spread_voice(voice: &mut Voice, index: usize) {
    let octave = (index % 4) as f32 * 0.25;
    let detune = ((index / 4) as f32 % 7.0 - 3.0) * 0.004;
    voice.oscillator.frequency_hz *= 2.0_f32.powf(octave + detune);
    voice.oscillator.phase = (voice.oscillator.phase + index as f32 * 0.113).fract();
}
