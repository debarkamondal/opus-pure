//! Per-band quality of a decoded signal against the audio that was encoded.
//!
//! This exists for one question: a hybrid packet's CELT high band codes fewer
//! bits here than in libopus, and a broadband SNR cannot say whether that costs
//! anything. The band above 8 kHz carries a small share of the energy, so an
//! average over the whole spectrum is dominated by SILK and moves by hundredths
//! of a dB whatever CELT does. Splitting the measurement along CELT's own band
//! edges puts the answer where the bits are spent.
//!
//! Three numbers per band, because one of them stops meaning anything at low
//! rates and it is not obvious in advance which:
//!
//! * `snr` — waveform SNR. The strict measure, and the right one while the
//!   decoder is still coding a waveform. CELT fills a band it cannot afford
//!   with spectrally-shaped noise instead (RFC 6716 §4.3.4's folding), which
//!   has the right energy and no phase relationship to the source at all, so
//!   `snr` collapses towards 0 dB there no matter how good the encoder is. A
//!   band where both stacks sit near 0 dB is a band this column cannot judge.
//! * `bias` — mean of 10·log10(E_dec / E_src) over analysis blocks. How much
//!   louder or quieter the band is than the source on average. What CELT
//!   promises to preserve whether or not it can afford the shape, and a steady
//!   offset here is a deliberate choice by the encoder rather than an error.
//! * `env` — standard deviation of that same ratio *about its mean*. How well
//!   the band's moving envelope is tracked once a constant offset is taken out.
//!   The two columns are orthogonal on purpose: an encoder that attenuates a
//!   band it cannot code by a steady 3 dB should show up in `bias` alone, and
//!   reading that attenuation as tracking error is what an RMS about zero
//!   would do. This is what fine energy bits buy, so it is the column a
//!   bit-allocation difference should show up in first.
//!
//! Both decodes are compared against the same source, and both should be
//! produced by the same decoder — `split` will decode either stack's packets —
//! so what is left between the two columns is the encoder.
//!
//! The summary rows span the bands the codec was actually asked to code:
//! `--top` exists because a superwideband stream is silent above 12 kHz on
//! purpose, and folding those bands into an average turns "correctly absent"
//! into a 14 dB energy deficit.
//!
//! usage: band <src.f32> <rate> <ch> [--above <Hz>] [--top <Hz>] <label>=<dec.f32> ...
#[path = "common.rs"]
mod common;
use common::read_f32;
use opus_pure::probe::CELT_BAND_EDGES_200HZ;

/// Analysis window. 1024 points at 48 kHz is 21.3 ms and 46.9 Hz per bin, which
/// resolves the narrowest band CELT has up here (8.0-9.6 kHz) into 34 bins.
const N: usize = 1024;
/// 75% overlap. The window is not COLA at this hop, which would matter if
/// anything were resynthesised from it; nothing is, and the extra blocks buy a
/// steadier `env` statistic for free.
const HOP: usize = N / 4;

/// One unit of `CELT_BAND_EDGES_200HZ`, in Hz.
const EDGE_HZ: f64 = 200.0;

/// Widest codec delay to search for, in samples. The encoder's is a few hundred
/// at 48 kHz; this is generous enough to cover every mode and rate without the
/// search finding a spurious peak somewhere unrelated.
const MAX_LAG: usize = 1000;

/// A block counts towards `env` only if the source has real energy in that band
/// there. Speech pauses otherwise dominate the statistic with the ratio of one
/// near-silence to another, which says nothing about the codec.
const ACTIVE_FLOOR_DB: f64 = -40.0;

/// 4-term Blackman-Harris. Its −92 dB sidelobes are the point: the low band can
/// sit 40 dB above the high band, and a window that leaked that across would
/// have this tool measuring its own skirts rather than the codec.
fn window(n: usize) -> Vec<f64> {
    const A: [f64; 4] = [0.35875, 0.48829, 0.14128, 0.01168];
    (0..n)
        .map(|i| {
            let t = std::f64::consts::TAU * i as f64 / n as f64;
            A[0] - A[1] * t.cos() + A[2] * (2.0 * t).cos() - A[3] * (3.0 * t).cos()
        })
        .collect()
}

/// In-place iterative radix-2 FFT.
fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    assert!(n.is_power_of_two());
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2usize;
    while len <= n {
        let ang = -std::f64::consts::TAU / len as f64;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (s, c) = (ang * k as f64).sin_cos();
                let (i0, i1) = (start + k, start + k + len / 2);
                let (ur, ui) = (re[i0], im[i0]);
                let (vr, vi) = (re[i1] * c - im[i1] * s, re[i1] * s + im[i1] * c);
                re[i0] = ur + vr;
                im[i0] = ui + vi;
                re[i1] = ur - vr;
                im[i1] = ui - vi;
            }
        }
        len <<= 1;
    }
}

/// Best integer lag of `dec` behind `src`, by normalised cross-correlation.
///
/// Integer is enough and fractional would be wrong to apply: the encoder delay
/// is a whole number of samples and pinned as one by `tests/encoder_delay.rs`.
/// A residual sub-sample offset would show up as a phase error that grows with
/// frequency, which is exactly the thing being measured, so the correlation is
/// reported alongside for a misalignment to be visible rather than absorbed.
fn best_lag(dec: &[f32], src: &[f32], max_lag: usize) -> (usize, f64) {
    let n = dec.len().saturating_sub(max_lag);
    let mut best = (0usize, f64::MIN);
    for lag in 0..=max_lag {
        let (mut num, mut a, mut b) = (0.0f64, 0.0f64, 0.0f64);
        for i in 0..n {
            let (d, s) = (dec[i + lag] as f64, src[i] as f64);
            num += d * s;
            a += d * d;
            b += s * s;
        }
        let c = if a == 0.0 || b == 0.0 {
            0.0
        } else {
            num / (a * b).sqrt()
        };
        if c > best.1 {
            best = (lag, c);
        }
    }
    best
}

/// What one decode scores in one band.
#[derive(Clone, Default)]
struct Band {
    sig: f64,
    err: f64,
    /// Per-block 10·log10(E_dec / E_src), for blocks the source is active in.
    /// `bias` is the mean of these and `env` the deviation about it.
    ratio: Vec<f64>,
}

/// Mean and standard-deviation-about-the-mean of a band's block ratios.
fn bias_env(r: &[f64]) -> (f64, f64) {
    if r.is_empty() {
        return (f64::NAN, f64::NAN);
    }
    let mean = r.iter().sum::<f64>() / r.len() as f64;
    let var = r.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / r.len() as f64;
    (mean, var.sqrt())
}

fn db(x: f64) -> f64 {
    if x > 0.0 {
        10.0 * x.log10()
    } else {
        f64::NEG_INFINITY
    }
}

fn deinterleave(pcm: &[f32], ch: usize, c: usize) -> Vec<f32> {
    pcm[c..].iter().step_by(ch).copied().collect()
}

/// Bin range of each band, clipped to Nyquist. Bands entirely above it are
/// dropped rather than reported empty: at 24 kHz there is no 15.6-20 kHz band.
fn band_bins(rate: f64) -> Vec<(usize, usize, f64, f64)> {
    let per_unit = EDGE_HZ * N as f64 / rate;
    let nyq = N / 2;
    let mut v = Vec::new();
    for w in CELT_BAND_EDGES_200HZ.windows(2) {
        let (lo, hi) = (
            (w[0] as f64 * per_unit).round() as usize,
            (w[1] as f64 * per_unit).round() as usize,
        );
        if lo >= nyq {
            break;
        }
        v.push((
            lo,
            hi.min(nyq),
            w[0] as f64 * EDGE_HZ,
            w[1] as f64 * EDGE_HZ,
        ));
    }
    v
}

/// Per-band totals for one channel, plus each block's source energy and energy
/// ratio. The gate on `env` needs the band's loudest block, which is not known
/// until every block has been seen, so the ratios come back ungated.
struct Analysis {
    bands: Vec<Band>,
    /// `[band][block]` source energy, and the matching dB ratio.
    src_e: Vec<Vec<f64>>,
    ratio: Vec<Vec<f64>>,
}

fn analyse(src: &[f32], dec: &[f32], bins: &[(usize, usize, f64, f64)], win: &[f64]) -> Analysis {
    let mut bands = vec![Band::default(); bins.len()];
    let mut src_e = vec![Vec::new(); bins.len()];
    let mut ratio = vec![Vec::new(); bins.len()];
    let n = src.len().min(dec.len());
    let (mut sr, mut si) = (vec![0.0f64; N], vec![0.0f64; N]);
    let (mut dr, mut di) = (vec![0.0f64; N], vec![0.0f64; N]);
    let mut start = 0usize;
    while start + N <= n {
        for i in 0..N {
            sr[i] = src[start + i] as f64 * win[i];
            dr[i] = dec[start + i] as f64 * win[i];
        }
        si.fill(0.0);
        di.fill(0.0);
        fft(&mut sr, &mut si);
        fft(&mut dr, &mut di);
        for (b, &(lo, hi, _, _)) in bins.iter().enumerate() {
            let (mut s, mut e, mut d) = (0.0f64, 0.0f64, 0.0f64);
            for k in lo..hi {
                let (er, ei) = (sr[k] - dr[k], si[k] - di[k]);
                s += sr[k] * sr[k] + si[k] * si[k];
                e += er * er + ei * ei;
                d += dr[k] * dr[k] + di[k] * di[k];
            }
            bands[b].sig += s;
            bands[b].err += e;
            src_e[b].push(s);
            ratio[b].push(if s > 0.0 && d > 0.0 {
                db(d / s)
            } else {
                f64::NAN
            });
        }
        start += HOP;
    }
    Analysis {
        bands,
        src_e,
        ratio,
    }
}

fn main() {
    let mut a: Vec<String> = std::env::args().skip(1).collect();
    let mut flag = |name: &str, default: f64| -> f64 {
        match a.iter().position(|x| x == name) {
            Some(i) => {
                let v = a[i + 1]
                    .parse()
                    .unwrap_or_else(|_| panic!("{name} takes a number"));
                a.drain(i..i + 2);
                v
            }
            None => default,
        }
    };
    let above = flag("--above", 8000.0);
    let top = flag("--top", f64::INFINITY);
    if a.len() < 4 {
        eprintln!(
            "usage: band <src.f32> <rate> <ch> [--above <Hz>] [--top <Hz>] <label>=<dec.f32> ..."
        );
        std::process::exit(2);
    }
    let src_all = read_f32(&a[0]);
    let rate: f64 = a[1].parse().expect("rate");
    let ch: usize = a[2].parse().expect("channels");
    let streams: Vec<(String, Vec<f32>)> = a[3..]
        .iter()
        .map(|s| {
            let (label, path) = s.split_once('=').unwrap_or_else(|| {
                eprintln!("expected <label>=<path>, got {s}");
                std::process::exit(2);
            });
            (label.to_string(), read_f32(path))
        })
        .collect();

    let bins = band_bins(rate);
    let win = window(N);
    // Skip the head: the codec is still warming up and the source is compared
    // against a decoder that has not yet seen enough of it.
    let skip = (rate as usize) / 10;

    println!("source {}, {} channels at {} Hz", a[0], ch, rate as u64);
    let mut totals: Vec<Vec<Band>> = Vec::new();
    for (label, dec_all) in &streams {
        // One lag for the whole stream, from channel 0. The codec delay does not
        // vary between channels or over time.
        let (s0, d0) = (deinterleave(&src_all, ch, 0), deinterleave(dec_all, ch, 0));
        let (lag, corr) = best_lag(
            &d0[skip.min(d0.len())..],
            &s0[skip.min(s0.len())..],
            MAX_LAG,
        );
        println!("  {label}: lag {lag} samples, correlation {corr:.4}");

        let mut acc: Vec<Band> = vec![Band::default(); bins.len()];
        for c in 0..ch {
            let s = deinterleave(&src_all, ch, c);
            let d = deinterleave(dec_all, ch, c);
            let take = s
                .len()
                .saturating_sub(skip)
                .min(d.len().saturating_sub(skip + lag));
            let (s, d) = (&s[skip..skip + take], &d[skip + lag..skip + lag + take]);
            let a = analyse(s, d, &bins, &win);
            for (b, band) in a.bands.iter().enumerate() {
                // Gate this channel's blocks against this channel's own peak.
                let peak = a.src_e[b].iter().copied().fold(0.0f64, f64::max);
                let floor = peak * 10f64.powf(ACTIVE_FLOOR_DB / 10.0);
                acc[b].sig += band.sig;
                acc[b].err += band.err;
                for (i, v) in a.ratio[b].iter().enumerate() {
                    if v.is_finite() && a.src_e[b][i] >= floor {
                        acc[b].ratio.push(*v);
                    }
                }
            }
        }
        totals.push(acc);
    }

    print!("\n{:<5}{:>14}{:>10}", "band", "Hz", "src dB");
    for (label, _) in &streams {
        print!("{:>26}", format!("--- {label} ---"));
    }
    println!();
    print!("{:<5}{:>14}{:>10}", "", "", "");
    for _ in &streams {
        print!("{:>8}{:>9}{:>9}", "snr", "bias", "env");
    }
    println!();
    let width = 29 + 26 * streams.len();
    println!("{}", "-".repeat(width));

    for (b, &(_, _, lo_hz, hi_hz)) in bins.iter().enumerate() {
        print!(
            "{:<5}{:>14}{:>10.1}",
            b,
            format!("{:.0}-{:.0}", lo_hz, hi_hz),
            db(totals[0][b].sig),
        );
        let mut all_below = true;
        for t in &totals {
            let s = &t[b];
            let (bias, env) = bias_env(&s.ratio);
            let snr = db(s.sig / s.err);
            all_below &= snr < 0.0;
            print!("{snr:>8.2}{bias:>9.2}{env:>9.2}");
        }
        // A band where every stream scores below zero is one where emitting
        // silence would have scored better than any of them.
        println!("{}", if all_below { "  *" } else { "" });
    }

    // The aggregate a broadband SNR would report, and the one it hides.
    let coded = |hz: f64| hz.min(top.min(rate / 2.0));
    for (name, lo_keep) in [
        (format!("full band to {:.0}", coded(f64::INFINITY)), 0.0f64),
        (
            format!("{:.0}-{:.0} Hz", above, coded(f64::INFINITY)),
            above,
        ),
    ] {
        print!("{name:<29}");
        for t in &totals {
            let (mut sig, mut err, mut ratio) = (0.0, 0.0, Vec::new());
            for (b, &(_, _, lo_hz, hi_hz)) in bins.iter().enumerate() {
                if lo_hz < lo_keep || hi_hz > top {
                    continue;
                }
                sig += t[b].sig;
                err += t[b].err;
                ratio.extend_from_slice(&t[b].ratio);
            }
            let (bias, env) = bias_env(&ratio);
            print!("{:>8.2}{:>9.2}{:>9.2}", db(sig / err), bias, env);
        }
        println!();
    }
    println!();
    println!("* snr cannot rank these: the band is noise-filled, and emitting silence there");
    println!("  scores exactly 0.00 dB -- better than any stream shown. Read bias and env.");
}
