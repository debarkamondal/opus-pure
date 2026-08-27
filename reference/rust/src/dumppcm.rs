//! Write the test suite's synthetic signals as raw interleaved f32, so the C
//! reference tools can be fed byte-for-byte the audio `cargo test` encodes.
//!
//!   dumppcm <out.f32> <rate> <samples> [channels] [music|speech]
//!
//! The generators come from `tests/common/mod.rs` through `common.rs`. They
//! used to be copied into this file, and the copy had drifted: it was still the
//! four-harmonic tremolo signal from an earlier revision of the suite, so every
//! comparison fed from here was measuring audio no test had encoded in months.
#[path = "common.rs"]
mod common;
use common::*;

use std::io::Write;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 4 {
        eprintln!("usage: dumppcm <out.f32> <rate> <samples> [channels] [music|speech]");
        std::process::exit(2);
    }
    let path = &a[1];
    let rate: i32 = a[2].parse().expect("rate");
    let n: usize = a[3].parse().expect("samples");
    let ch: usize = a.get(4).map(|s| s.parse().expect("channels")).unwrap_or(1);
    let kind = a.get(5).map(String::as_str).unwrap_or("music");

    let signal = |seed_shift: usize| -> Vec<f32> {
        let v = match kind {
            "speech" => speech_like(rate, n + seed_shift),
            _ => music_like(rate, n + seed_shift),
        };
        v[seed_shift..].to_vec()
    };

    // A stereo pair the two channels of which are not identical, so a codec
    // that throws the side channel away is visible. Offsetting the right
    // channel by a few samples decorrelates it without changing its spectrum.
    let pcm = match ch {
        1 => signal(0),
        2 => interleave(&[signal(0), signal(17)]),
        other => panic!("channels must be 1 or 2, got {other}"),
    };

    let mut f = std::io::BufWriter::new(std::fs::File::create(path).expect("create"));
    for s in &pcm {
        f.write_all(&s.to_le_bytes()).expect("write");
    }
    f.flush().expect("flush");
    eprintln!(
        "{path}: {} samples x {ch} ch @ {rate} Hz ({kind})",
        pcm.len() / ch
    );
}
