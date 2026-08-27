//! Decode an official RFC 6716 / RFC 8251 test vector with this crate.
//!
//! Dev tool. Lives in `reference/vectors/`; copy into `examples/` for a run.
//!
//!   vector_check <testvectorNN.bit> <out.s16> <rate> <channels>
//!
//! The `.bit` container is opus_demo's: per packet, a 4-byte big-endian length,
//! a 4-byte big-endian copy of the *encoder's* final range-coder state, then the
//! payload. That stored range state is the strongest conformance signal in the
//! file: if our range decoder ends a packet in a different state than the
//! encoder ended it, the entropy decode diverged, whatever the audio sounds
//! like. Report it per packet, then write S16LE for opus_compare.

use opus_pure::OpusDecoder;
use std::io::Write;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (path, out_path) = (&a[1], &a[2]);
    let rate: i32 = a[3].parse().unwrap();
    let channels: usize = a[4].parse().unwrap();

    let raw = std::fs::read(path).expect("read bitstream");
    let mut dec = OpusDecoder::new(rate, channels).expect("decoder");

    // 120 ms at 48 kHz, the largest an Opus packet can decode to.
    let max_frame = (rate as usize / 1000) * 120;
    let mut pcm = vec![0.0f32; max_frame * channels];
    let mut out: Vec<u8> = Vec::with_capacity(raw.len() * 8);

    let (mut pos, mut packets, mut range_bad, mut errors) = (0usize, 0usize, 0usize, 0usize);
    let mut first_bad: Option<(usize, u32, u32)> = None;
    let mut first_err: Option<(usize, String)> = None;

    while pos + 8 <= raw.len() {
        let len = u32::from_be_bytes(raw[pos..pos + 4].try_into().unwrap()) as usize;
        let enc_range = u32::from_be_bytes(raw[pos + 4..pos + 8].try_into().unwrap());
        pos += 8;
        if pos + len > raw.len() {
            eprintln!("truncated packet {packets} at {pos}");
            break;
        }
        let payload = &raw[pos..pos + len];
        pos += len;
        packets += 1;

        match dec.decode(payload, max_frame, &mut pcm) {
            Ok(n) => {
                if dec.final_range() != enc_range {
                    range_bad += 1;
                    first_bad.get_or_insert((packets - 1, enc_range, dec.final_range()));
                }
                for &s in &pcm[..n * channels] {
                    // libopus's FLOAT2INT16: round-half-to-even, saturating.
                    let v = (s * 32768.0).round_ties_even().clamp(-32768.0, 32767.0) as i16;
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }
            Err(e) => {
                errors += 1;
                first_err.get_or_insert((packets - 1, format!("{e:?}")));
            }
        }
    }

    std::fs::File::create(out_path)
        .and_then(|mut f| f.write_all(&out))
        .expect("write pcm");

    println!(
        "  packets={packets} samples={} errors={errors} range_mismatch={range_bad}",
        out.len() / 2 / channels
    );
    if let Some((i, want, got)) = first_bad {
        println!("  first range mismatch: packet {i} expected {want:#010x} got {got:#010x}");
    }
    if let Some((i, e)) = first_err {
        println!("  first decode error: packet {i}: {e}");
    }
}
