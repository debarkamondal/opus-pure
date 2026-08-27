//! Dump a pure-hybrid stream three ways: source, our decode, and the packets in
//! `cpcm` framing so libopus can decode the same bitstream.
//!
//! Written to chase the ~0.6 ms by which this crate's hybrid output trails
//! libopus's, which `tests/encoder_delay.rs` pins but does not explain.
#[path = "common.rs"]
mod common;
use common::*;
use opus_pure::{Application, Bandwidth, OpusDecoder, OpusEncoder, Signal};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut a = std::env::args().skip(1);
    let rate: i32 = a
        .next()
        .ok_or("usage: hybrid <rate> <bitrate> <tag>")?
        .parse()?;
    let br: i32 = a
        .next()
        .ok_or("usage: hybrid <rate> <bitrate> <tag>")?
        .parse()?;
    let tag = a.next().unwrap_or_else(|| "h".into());
    let kind = a.next().unwrap_or_else(|| "music".into());
    let bw = a.next().unwrap_or_else(|| "auto".into());

    let frame = (rate / 50) as usize;
    let n = frame * 200;
    let src = if kind == "speech" {
        speech_like(rate, n)
    } else {
        music_like(rate, n)
    };

    let mut enc = OpusEncoder::new(rate, 1, Application::Voip)?;
    enc.bitrate_bps = br;
    enc.force_bandwidth = match bw.as_str() {
        "nb" => Some(Bandwidth::Narrowband),
        "mb" => Some(Bandwidth::Mediumband),
        "wb" => Some(Bandwidth::Wideband),
        "swb" => Some(Bandwidth::Superwideband),
        "fb" => Some(Bandwidth::Fullband),
        _ => None,
    };
    if kind == "speech" {
        enc.signal_type = Some(Signal::Voice);
    }
    let mut dec = OpusDecoder::new(rate, 1)?;

    let mut pkts: Vec<u8> = Vec::new();
    let mut decoded: Vec<f32> = Vec::new();
    let mut buf = vec![0f32; frame];
    let mut pkt = vec![0u8; 4000];
    let mut modes = std::collections::BTreeMap::new();
    for c in src.chunks_exact(frame) {
        let l = enc.encode(c, frame, &mut pkt)?;
        let cfg = pkt[0] >> 3;
        let m = if cfg < 12 {
            "silk"
        } else if cfg < 16 {
            "hybrid"
        } else {
            "celt"
        };
        *modes.entry(m).or_insert(0usize) += 1;
        pkts.extend_from_slice(&(l as u32).to_le_bytes());
        pkts.extend_from_slice(&pkt[..l]);
        let got = dec.decode(&pkt[..l], frame, &mut buf)?;
        decoded.extend_from_slice(&buf[..got]);
    }

    let d = format!("reference/work/hybrid/{tag}");
    std::fs::create_dir_all("reference/work/hybrid")?;
    write_f32(&format!("{d}.src.f32"), &src)?;
    write_f32(&format!("{d}.ours.f32"), &decoded)?;
    std::fs::write(format!("{d}.pkt"), &pkts)?;
    println!(
        "{tag}: {rate} Hz {br} bps, {} frames, modes {modes:?}",
        n / frame
    );
    Ok(())
}
