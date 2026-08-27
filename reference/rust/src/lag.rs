//! Measure the delay of a decoded signal against its source, per frequency band.
//!
//! A hybrid stream carries its low band through SILK and its high band through
//! CELT, so a single lag search over the full-band signal reports an
//! energy-weighted blend of whatever delays those two layers have. Splitting the
//! band first says which layer is late.
//!
//! The same FIR runs over both signals, so the filter's own group delay cancels
//! and only the codec's delay is left.
#[path = "common.rs"]
mod common;
use common::{convolve, lowpass, read_f32, write_f32};

/// Best integer lag of `dec` behind `src`, with a parabolic sub-sample refinement.
fn best_lag(dec: &[f32], src: &[f32], max_lag: usize) -> (f64, f64) {
    let n = dec.len().saturating_sub(max_lag);
    let score = |lag: usize| -> f64 {
        let (mut num, mut a, mut b) = (0.0f64, 0.0f64, 0.0f64);
        for i in 0..n {
            let d = dec[i + lag] as f64;
            let s = src[i] as f64;
            num += d * s;
            a += d * d;
            b += s * s;
        }
        if a == 0.0 || b == 0.0 {
            0.0
        } else {
            num / (a * b).sqrt()
        }
    };
    let mut best = (0usize, f64::MIN);
    for lag in 0..=max_lag {
        let c = score(lag);
        if c > best.1 {
            best = (lag, c);
        }
    }
    let (l, c) = best;
    // Parabolic interpolation: the true peak of a blended delay is not integral.
    let refined = if l > 0 && l < max_lag {
        let (ym, yp) = (score(l - 1), score(l + 1));
        let denom = ym - 2.0 * c + yp;
        if denom.abs() > 1e-12 {
            l as f64 - 0.5 * (yp - ym) / denom
        } else {
            l as f64
        }
    } else {
        l as f64
    };
    (refined, c)
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() < 4 {
        eprintln!("usage: lag <src.f32> <dec.f32> <rate> <cutoff_hz> [max_lag] [--dump prefix]");
        std::process::exit(2);
    }
    let (src, dec) = (read_f32(&a[0]), read_f32(&a[1]));
    let rate: f64 = a[2].parse().unwrap();
    let cutoff: f64 = a[3].parse().unwrap();
    let max_lag: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(1000);

    // Skip the head so codec warm-up does not weight the correlation.
    let skip = (rate as usize) / 10;
    let take = src.len().min(dec.len()) - skip;
    let (s, d) = (&src[skip..skip + take], &dec[skip..skip + take]);

    let h = lowpass(511, cutoff / rate);
    let (s_lo, d_lo) = (convolve(s, &h), convolve(d, &h));
    let hi = |x: &[f32], lo: &[f32]| -> Vec<f32> { x.iter().zip(lo).map(|(a, b)| a - b).collect() };
    let (s_hi, d_hi) = (hi(s, &s_lo), hi(d, &d_lo));

    let e = |v: &[f32]| v.iter().map(|x| (*x as f64) * (*x as f64)).sum::<f64>() / v.len() as f64;
    println!(
        "{:<10}{:>12}{:>10}{:>16}",
        "band", "lag", "corr", "source energy"
    );
    for (name, sb, db) in [
        ("full", s, d),
        ("low", &s_lo[..], &d_lo[..]),
        ("high", &s_hi[..], &d_hi[..]),
    ] {
        let (lag, corr) = best_lag(db, sb, max_lag);
        println!("{name:<10}{lag:>12.2}{corr:>10.4}{:>16.3e}", e(sb));
    }

    if let Some(i) = a.iter().position(|x| x == "--dump") {
        let p = &a[i + 1];
        write_f32(&format!("{p}.src_lo.f32"), &s_lo).unwrap();
        write_f32(&format!("{p}.dec_lo.f32"), &d_lo).unwrap();
        write_f32(&format!("{p}.src_hi.f32"), &s_hi).unwrap();
        write_f32(&format!("{p}.dec_hi.f32"), &d_hi).unwrap();
    }
}
