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

use opus_pure::{MAX_PACKET_SAMPLES, OggOpusReader, OpusDecoder, OpusMSDecoder, Trim};

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
///
/// The trimming goes through the crate's own [`Trim`] rather than a copy of the
/// arithmetic. This harness held that copy for a while, and it was the only
/// place in the repository that got it right: the README, the crate docs and
/// `examples/decode.rs` all dropped the end-trim, and nothing compared the two.
pub fn decode_ours(path: &str) -> Result<(Vec<f32>, usize), Box<dyn std::error::Error>> {
    let file = std::fs::File::open(path)?;
    let mut rd = OggOpusReader::new(std::io::BufReader::new(file))?;
    let head = rd.head().clone();
    let channels = head.channel_count as usize;

    let mut mono: Option<OpusDecoder> = None;
    let mut multi: Option<OpusMSDecoder> = None;
    if head.mapping_family == 0 {
        mono = Some(head.decoder(48_000)?);
    } else {
        let mut d = OpusMSDecoder::new(48_000, channels, head.mapping_family)?;
        // `OpusHead::decoder` carries the gain on the family-0 path; a surround
        // stream has to be told, and `opusdec` applies it either way.
        for stream in d.streams_mut() {
            stream.gain_q8 = i32::from(head.output_gain_q8);
        }
        multi = Some(d);
    }

    let mut trim = Trim::new(&head, 48_000, channels)?;
    let mut pcm: Vec<f32> = Vec::new();
    let mut buf = vec![0f32; MAX_PACKET_SAMPLES * channels];
    while let Some(pkt) = rd.read_packet()? {
        // `frame_size` is buffer capacity, as in libopus: hand it the whole
        // scratch buffer and let it report what the packet actually held.
        let n = match (mono.as_mut(), multi.as_mut()) {
            (Some(d), _) => d.decode(&pkt.data, MAX_PACKET_SAMPLES, &mut buf)?,
            (_, Some(d)) => d.decode(&pkt.data, packet_samples_48k(&pkt.data), &mut buf)?,
            _ => unreachable!(),
        };
        pcm.extend_from_slice(trim.keep(&pkt, &buf[..n * channels]));
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
