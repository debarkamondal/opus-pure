//! Per-channel round-trip SNR for stereo SILK, aligned for codec delay.
use opus_pure::{Application, OpusDecoder, OpusEncoder};

/// A pitched buzz shaped by a slow envelope.
///
/// Deliberately not `common::speech_like`: this probe wants a strongly voiced,
/// highly periodic source, because that is where SILK's parametric stereo has
/// the most to throw away. Named for what it is so it cannot be mistaken for a
/// stale copy of the suite's generator.
fn voiced_buzz(rate: i32, n: usize) -> Vec<f32> {
    let mut v = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / rate as f32;
        let f0 = 120.0 + 20.0 * (2.0 * std::f32::consts::PI * 2.5 * t).sin();
        let mut s = 0.0;
        for h in 1..12 {
            s += (2.0 * std::f32::consts::PI * f0 * h as f32 * t).sin() / h as f32;
        }
        let env = 0.5 + 0.5 * (2.0 * std::f32::consts::PI * 3.0 * t).sin();
        v.push(0.3 * s * env);
    }
    v
}

/// `src` is the whole source; `start` is where `dec[0]` nominally lines up.
/// The search runs both directions because the codec's delay shifts the decoded
/// signal earlier than the sample index suggests.
/// How much of the source's stereo image survives: the correlation between the
/// decoded side signal and the source's, at the best alignment. SILK is
/// parametric, so its waveform SNR says little about quality — but whether the
/// side channel is reproduced at all is exactly what mid-only coding loses.
fn image_match(dec: &[f32], src: &[f32], start: usize, max_lag: isize) -> f32 {
    let mut best = 0.0f32;
    for lag in -max_lag..=max_lag {
        let s0 = start as isize + lag;
        if s0 < 0 || s0 as usize + 1000 >= src.len() {
            continue;
        }
        let off = s0 as usize;
        let n = dec.len().min(src.len() - off);
        let (mut sa, mut sb, mut sab) = (0.0f64, 0.0f64, 0.0f64);
        for i in 0..n {
            let (a, b) = (dec[i] as f64, src[off + i] as f64);
            sa += a * a;
            sb += b * b;
            sab += a * b;
        }
        if sa > 0.0 && sb > 0.0 {
            let c = (sab / (sa.sqrt() * sb.sqrt())) as f32;
            if c.abs() > best.abs() {
                best = c;
            }
        }
    }
    best
}

fn main() {
    for &(rate, bitrate) in &[
        (16_000i32, 32_000i32),
        (16_000, 40_000),
        (16_000, 48_000),
        (12_000, 36_000),
    ] {
        let secs = 2.0f32;
        let n = (rate as f32 * secs) as usize;
        let l = voiced_buzz(rate, n);
        // Right: the same voice, delayed and level-shifted — a real stereo image.
        let r: Vec<f32> = (0..n)
            .map(|i| if i >= 11 { l[i - 11] * 0.6 } else { 0.0 })
            .collect();
        let mut src = vec![0.0f32; n * 2];
        for i in 0..n {
            src[2 * i] = l[i];
            src[2 * i + 1] = r[i];
        }

        let frame = (rate / 50) as usize;
        let mut enc = OpusEncoder::new(rate, 2, Application::Voip).unwrap();
        enc.bitrate_bps = bitrate;
        let mut dec = OpusDecoder::new(rate, 2).unwrap();
        let (mut out, mut bytes, mut silk) = (Vec::new(), 0usize, 0usize);
        for f in 0..(n / frame) {
            let mut pkt = vec![0u8; 4000];
            let k = enc
                .encode(&src[f * frame * 2..(f + 1) * frame * 2], frame, &mut pkt)
                .unwrap();
            bytes += k;
            if pkt[0] >> 3 < 12 {
                silk += 1;
            }
            let mut o = vec![0.0f32; frame * 2];
            dec.decode(&pkt[..k], frame, &mut o).unwrap();
            out.extend_from_slice(&o);
        }
        let skip = frame * 20;
        let dl: Vec<f32> = out.iter().skip(skip * 2).step_by(2).copied().collect();
        let dr: Vec<f32> = out.iter().skip(skip * 2 + 1).step_by(2).copied().collect();
        let dside: Vec<f32> = dl.iter().zip(&dr).map(|(a, b)| (a - b) * 0.5).collect();
        let sside: Vec<f32> = l.iter().zip(&r).map(|(a, b)| (a - b) * 0.5).collect();
        let e_d: f64 =
            dside.iter().map(|x| (*x as f64) * (*x as f64)).sum::<f64>() / dside.len() as f64;
        let e_s: f64 = sside[skip..]
            .iter()
            .map(|x| (*x as f64) * (*x as f64))
            .sum::<f64>()
            / (sside.len() - skip) as f64;
        println!(
            "{rate} Hz {:2} kb/s target -> {:4.1} actual, {silk}/{} SILK   image match {:+.3}   side energy kept {:.0}%",
            bitrate / 1000,
            bytes as f32 * 8.0 / secs / 1000.0,
            n / frame,
            image_match(&dside, &sside, skip, frame as isize),
            100.0 * e_d / e_s,
        );
    }
}
