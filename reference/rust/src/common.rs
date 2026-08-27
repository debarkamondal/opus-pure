//! Shared helpers for the interop harness: signal generation, and a "decode
//! with our stack" routine that matches what `opusdec` produces.
//!
//! The generators are the integration suite's own, included by path rather than
//! copied. A sweep that encodes different audio than `cargo test` does cannot be
//! used to explain a test result, and a copy goes stale silently: this file held
//! one, and it missed the change that made the generators architecture-
//! independent, so the sweep was still measuring libm-dependent audio after the
//! tests had stopped.
#![allow(dead_code, unused_imports)]

use std::io::Write;

use opus_pure::{OggOpusReader, OpusDecoder, OpusMSDecoder};

#[path = "../../../tests/common/mod.rs"]
pub mod harness;
pub use harness::{
    Lcg, band_limit, convolve, highpass, interleave, lowpass, music_like, packet_samples_48k, sine,
    speech_like,
};

pub fn write_f32(path: &str, pcm: &[f32]) -> std::io::Result<()> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    for s in pcm {
        f.write_all(&s.to_le_bytes())?;
    }
    f.flush()
}

/// Decode an Ogg Opus file with our reader + decoder, at 48 kHz, applying the
/// same pre-skip and end trimming `opusdec` does, so output is sample-aligned
/// with the reference tool.
pub fn decode_ours(path: &str) -> Result<(Vec<f32>, usize), Box<dyn std::error::Error>> {
    let file = std::fs::File::open(path)?;
    let mut rd = OggOpusReader::new(std::io::BufReader::new(file))?;
    let head = rd.head().clone();
    let channels = head.channel_count as usize;
    let pre_skip = head.pre_skip as u64;

    let mut mono: Option<OpusDecoder> = None;
    let mut multi: Option<OpusMSDecoder> = None;
    if head.mapping_family == 0 {
        mono = Some(OpusDecoder::new(48_000, channels)?);
    } else {
        multi = Some(OpusMSDecoder::new(48_000, channels, head.mapping_family)?);
    }

    let mut pcm: Vec<f32> = Vec::new();
    let mut buf = vec![0f32; 5760 * channels];
    let mut last_granule: i64 = -1;
    while let Some(pkt) = rd.read_packet()? {
        // `frame_size` is buffer capacity, as in libopus: hand it the whole
        // scratch buffer and let it report what the packet actually held.
        let n = match (mono.as_mut(), multi.as_mut()) {
            (Some(d), _) => d.decode(&pkt.data, 5760, &mut buf)?,
            (_, Some(d)) => d.decode(&pkt.data, packet_samples_48k(&pkt.data), &mut buf)?,
            _ => unreachable!(),
        };
        pcm.extend_from_slice(&buf[..n * channels]);
        if pkt.page_granule >= 0 {
            last_granule = pkt.page_granule;
        }
    }

    // Trim the encoder delay from the front.
    let decoded = pcm.len() / channels;
    let skip = (pre_skip as usize).min(decoded);
    pcm.drain(..skip * channels);
    // Honour end-trimming: the final granule may claim fewer samples than the
    // packets carry. Never extend past what was actually decoded.
    if last_granule >= 0 {
        let playable = (last_granule as u64).saturating_sub(pre_skip) as usize;
        let have = pcm.len() / channels;
        if playable < have {
            pcm.truncate(playable * channels);
        }
    }
    Ok((pcm, channels))
}

/// Read a little-endian `f32` file, the format every tool here passes PCM in.
pub fn read_f32(path: &str) -> Vec<f32> {
    let b = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    b.as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect()
}
