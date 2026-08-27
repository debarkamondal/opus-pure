//! How much of a decorrelated stereo pair survives a round trip, per mode.
//!
//! Written for the SILK mid/side work; `tests/stereo.rs` is the gate that came
//! out of it. Kept because it prints the whole picture at once — mode mix,
//! achieved rate, per-channel and cross-channel correlation, side energy.
#[path = "common.rs"]
mod common;
use common::sine;

use opus_pure::{Application, Bandwidth, OpusDecoder, OpusEncoder};

fn corr(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    let (mut sa, mut sb, mut sab) = (0.0f64, 0.0f64, 0.0f64);
    for i in 0..n {
        sa += (a[i] * a[i]) as f64;
        sb += (b[i] * b[i]) as f64;
        sab += (a[i] * b[i]) as f64;
    }
    if sa <= 0.0 || sb <= 0.0 {
        return 0.0;
    }
    (sab / (sa.sqrt() * sb.sqrt())) as f32
}
fn energy(a: &[f32]) -> f64 {
    a.iter().map(|x| (*x as f64) * (*x as f64)).sum::<f64>() / a.len() as f64
}
/// Best correlation over the codec's algorithmic delay: a plain corr() against
/// an unaligned reference reads a delay as a phase inversion.
fn aligned(a: &[f32], b: &[f32], max_lag: usize) -> f32 {
    let mut best = 0.0f32;
    for lag in 0..max_lag {
        if lag + 64 >= b.len() {
            break;
        }
        let c = corr(a, &b[lag..]);
        if c.abs() > best.abs() {
            best = c;
        }
    }
    best
}

fn run(rate: i32, bitrate: i32, app: Application, bw: Option<Bandwidth>, label: &str) {
    let secs = 1.5f32;
    let n = (rate as f32 * secs) as usize;
    let l = sine(rate, n, 440.0, 0.4);
    let r = sine(rate, n, 836.0, 0.4);
    let mut src = vec![0.0f32; n * 2];
    for i in 0..n {
        src[2 * i] = l[i];
        src[2 * i + 1] = r[i];
    }

    let frame = (rate / 50) as usize;
    let mut enc = OpusEncoder::new(rate, 2, app).unwrap();
    enc.bitrate_bps = bitrate;
    enc.force_bandwidth = bw;
    let mut dec = OpusDecoder::new(rate, 2).unwrap();
    let mut out = Vec::new();
    let mut modes = std::collections::BTreeMap::new();
    let mut bytes = 0usize;
    for f in 0..(n / frame) {
        let mut pkt = vec![0u8; 4000];
        let k = enc
            .encode(&src[f * frame * 2..(f + 1) * frame * 2], frame, &mut pkt)
            .unwrap();
        bytes += k;
        let toc = pkt[0] >> 3;
        let m = if toc < 12 {
            "silk"
        } else if toc < 16 {
            "hybrid"
        } else {
            "celt"
        };
        *modes.entry(m).or_insert(0usize) += 1;
        let mut o = vec![0.0f32; frame * 2];
        dec.decode(&pkt[..k], frame, &mut o).unwrap();
        out.extend_from_slice(&o);
    }
    let skip = frame * 15;
    let dl: Vec<f32> = out.iter().skip(skip * 2).step_by(2).copied().collect();
    let dr: Vec<f32> = out.iter().skip(skip * 2 + 1).step_by(2).copied().collect();
    let side: Vec<f32> = dl.iter().zip(&dr).map(|(a, b)| (a - b) * 0.5).collect();
    let in_side: Vec<f32> = l[skip..]
        .iter()
        .zip(&r[skip..])
        .map(|(a, b)| (a - b) * 0.5)
        .collect();
    println!(
        "{label:34} modes={modes:?} kb/s={:.1}",
        bytes as f32 * 8.0 / secs / 1000.0
    );
    println!(
        "    corr(L,R) decoded = {:+.4}   input = {:+.4}",
        corr(&dl, &dr),
        corr(&l[skip..], &r[skip..])
    );
    let lag = frame;
    println!(
        "    aligned corr(L,inL) = {:+.4}   corr(R,inR) = {:+.4}",
        aligned(&dl, &l[skip..], lag),
        aligned(&dr, &r[skip..], lag)
    );
    println!(
        "    cross   corr(L,inR) = {:+.4}   corr(R,inL) = {:+.4}",
        aligned(&dl, &r[skip..], lag),
        aligned(&dr, &l[skip..], lag)
    );
    println!(
        "    side energy decoded = {:.3e}   input = {:.3e}   ratio = {:.3}",
        energy(&side),
        energy(&in_side),
        energy(&side) / energy(&in_side).max(1e-30)
    );
}

fn main() {
    run(
        16_000,
        24_000,
        Application::Voip,
        None,
        "16 kHz VoIP 24 kb/s",
    );
    run(8_000, 16_000, Application::Voip, None, "8 kHz VoIP 16 kb/s");
    run(
        16_000,
        40_000,
        Application::Voip,
        None,
        "16 kHz VoIP 40 kb/s",
    );
    run(
        48_000,
        32_000,
        Application::Audio,
        Some(Bandwidth::Wideband),
        "48 kHz audio 32 kb/s WB",
    );
    run(
        48_000,
        64_000,
        Application::Voip,
        Some(Bandwidth::Superwideband),
        "48 kHz VoIP 64 kb/s SWB (hybrid)",
    );
}
