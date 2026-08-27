//! Print the envelope around every mode switch in the stream `tests/mode_transition.rs`
//! builds, so a marginal result there can be told from a real dropout.
#[path = "common.rs"]
mod common;
use common::harness::{Codec, music_like, speech_like};
use opus_pure::Application;

const RATE: i32 = 48_000;
const FRAME: usize = 960;

fn rms(v: &[f32]) -> f32 {
    (v.iter().map(|s| s * s).sum::<f32>() / v.len() as f32).sqrt()
}

fn main() {
    let mut pcm = speech_like(RATE, RATE as usize);
    pcm.extend(music_like(RATE, RATE as usize));
    pcm.extend(speech_like(RATE, RATE as usize));

    let r = Codec::new(RATE, 1, Application::Audio)
        .frame_samples(FRAME)
        .bitrate(24_000)
        .roundtrip(&pcm);
    let modes = r.modes();
    let out = &r.decoded;
    let switches: Vec<usize> = (1..modes.len())
        .filter(|&i| modes[i] != modes[i - 1])
        .collect();
    println!("switches: {switches:?}");
    const HEAD: usize = 16;
    const ENV: usize = 10 * (RATE as usize / 400);
    for &i in &switches {
        println!("  packet {i}: {} -> {}", modes[i - 1], modes[i]);
        // Scan the neighbourhood: the true seam in the output sits one codec
        // delay after the packet boundary, and the test looks at the boundary.
        for off in [0i64, 120, 192, 300, 312, 330] {
            let start = (i * FRAME).saturating_add_signed(off as isize);
            if start < ENV || start + ENV > out.len() {
                continue;
            }
            let head = rms(&out[start..start + HEAD]);
            let env = rms(&out[start - ENV..start + ENV]);
            println!("      +{off:<5} head/env = {:>6.2}%", 100.0 * head / env);
        }
    }
}
