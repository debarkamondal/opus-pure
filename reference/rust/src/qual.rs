//! Round-trip quality: encode + decode in-process, report aligned SNR.
#[path = "common.rs"]
mod common;
use common::*;
use opus_pure::{Application, OpusDecoder, OpusEncoder};

fn aligned_snr_db(dec: &[f32], src: &[f32], max_lag: usize) -> f32 {
    let n = dec.len().min(src.len());
    let mut best = (f32::NEG_INFINITY, 0usize);
    for lag in 0..=max_lag {
        let m = n - lag;
        let (mut se, mut sig) = (0.0f64, 0.0f64);
        for i in 0..m {
            let d = (dec[i + lag] - src[i]) as f64;
            se += d * d;
            sig += (src[i] as f64) * (src[i] as f64);
        }
        let snr = if se == 0.0 {
            f32::INFINITY
        } else {
            (10.0 * (sig / se).log10()) as f32
        };
        if snr > best.0 {
            best = (snr, lag);
        }
    }
    best.0
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for &(rate, ch, br, app, kind) in &[
        (48_000i32, 1usize, 64_000i32, "audio", "music"),
        (48_000, 1, 128_000, "audio", "music"),
        (24_000, 1, 48_000, "audio", "music"),
        (48_000, 1, 32_000, "audio", "speech"),
        (48_000, 2, 96_000, "audio", "music"),
        (16_000, 1, 16_000, "voip", "speech"),
    ] {
        let frame = (rate / 50) as usize;
        let n = frame * 100;
        let base = if kind == "music" {
            music_like(rate, n)
        } else {
            speech_like(rate, n)
        };
        let pcm: Vec<f32> = if ch == 1 {
            base.clone()
        } else {
            let t = sine(rate, n, 660.0, 0.25);
            interleave(&[
                base.clone(),
                base.iter().zip(&t).map(|(a, b)| a * 0.75 + b).collect(),
            ])
        };
        let application = if app == "voip" {
            Application::Voip
        } else {
            Application::Audio
        };
        let mut enc = OpusEncoder::new(rate, ch, application)?;
        enc.bitrate_bps = br;
        let mut dec = OpusDecoder::new(rate, ch)?;
        let mut out: Vec<f32> = Vec::new();
        let mut pkt = vec![0u8; 4000];
        let mut buf = vec![0.0f32; 5760 * ch];
        let mut bytes = 0usize;
        for c in pcm.chunks_exact(frame * ch) {
            let l = enc.encode(c, frame, &mut pkt)?;
            bytes += l;
            let m = dec.decode(&pkt[..l], 5760, &mut buf)?;
            out.extend_from_slice(&buf[..m * ch]);
        }
        // Compare channel 0 only, allowing for the codec delay.
        let d0: Vec<f32> = out.iter().step_by(ch).copied().collect();
        let s0: Vec<f32> = pcm.iter().step_by(ch).copied().collect();
        let snr = aligned_snr_db(&d0, &s0, 400);
        println!("{rate}/{ch}ch/{app}/{br}/{kind}: snr {snr:7.3} dB, {bytes} bytes");
    }
    Ok(())
}
