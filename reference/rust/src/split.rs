//! Where a hybrid packet's bits go: SILK low band against CELT high band.
//!
//! A hybrid packet is one range-coded stream, so the boundary between the two
//! layers is not visible from outside the decoder. This reads it out of the
//! decoder's own position at the point SILK finishes, which the crate exposes
//! under its `probe` feature. Because that decoder matches libopus bit for bit
//! on hybrid, pointing this at libopus's packets measures *its* split too, which
//! is how the hybrid rate-control divergence was localised to the high band.
//!
//! usage: split <pkt> <rate> <ch> <frame> [pcm_out]
mod common;
use opus_pure::OpusDecoder;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (rate, ch, frame): (i32, usize, usize) = (
        a[2].parse().unwrap(),
        a[3].parse().unwrap(),
        a[4].parse().unwrap(),
    );
    let raw = std::fs::read(&a[1]).expect("pkt");
    let mut dec = OpusDecoder::new(rate, ch).expect("decoder");
    let mut out = vec![0.0f32; frame * ch];
    let (mut silk, mut total, mut n) = (0i64, 0i64, 0i64);
    let mut pcm: Vec<f32> = Vec::new();
    let mut i = 0usize;
    while i + 4 <= raw.len() {
        let len = u32::from_le_bytes([raw[i], raw[i + 1], raw[i + 2], raw[i + 3]]) as usize;
        i += 4;
        if i + len > raw.len() {
            break;
        }
        let got = dec
            .decode(&raw[i..i + len], frame, &mut out)
            .expect("decode");
        pcm.extend_from_slice(&out[..got * ch]);
        i += len;
        if dec.probe_silk_bits > 0 {
            silk += dec.probe_silk_bits as i64;
            total += (len * 8) as i64;
            n += 1;
        }
    }
    if let Some(path) = a.get(5) {
        common::write_f32(path, &pcm).expect("pcm out");
    }
    let fps = rate as f64 / frame as f64;
    println!(
        "{:.0} hybrid packets: silk {:.0} bits/frame ({:.1} kb/s), celt {:.0} ({:.1} kb/s), total {:.1} kb/s",
        n as f64,
        silk as f64 / n as f64,
        silk as f64 / n as f64 * fps / 1000.0,
        (total - silk) as f64 / n as f64,
        (total - silk) as f64 / n as f64 * fps / 1000.0,
        total as f64 / n as f64 * fps / 1000.0,
    );
}
