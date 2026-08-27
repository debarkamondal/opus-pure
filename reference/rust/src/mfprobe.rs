//! What mode and framing does *this crate* pick for a given duration? Prints
//! one line per packet in exactly the form `cmf` (reference/multiframe/cmf.c) prints it
//! for libopus, so the two runs can be compared with `diff`.
//!
//!   mfprobe <pcm.f32> <rate> <ch> <frame> <bitrate> <voip|audio> <vbr:0|1>
//!
//! Reads the PCM rather than generating it, for the same reason `cmf` does:
//! both sides then provably see identical bytes. `dumppcm` writes the file.
//!
//! Each packet is also decoded and its range state checked against the
//! encoder's, so a framing difference that also broke the entropy coder fails
//! here rather than printing a plausible line.
use opus_pure::{Application, OpusDecoder, OpusEncoder, RateControl, packet};
use std::io::Read;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 8 {
        eprintln!("usage: mfprobe <pcm.f32> <rate> <ch> <frame> <bitrate> <voip|audio> <vbr:0|1>");
        std::process::exit(2);
    }
    let rate: i32 = a[2].parse().expect("rate");
    let ch: usize = a[3].parse().expect("ch");
    let fs: usize = a[4].parse().expect("frame");
    let br: i32 = a[5].parse().expect("bitrate");
    let app = if a[6] == "audio" {
        Application::Audio
    } else {
        Application::Voip
    };
    let vbr = a[7] != "0";

    let mut raw = Vec::new();
    std::fs::File::open(&a[1])
        .expect("open pcm")
        .read_to_end(&mut raw)
        .expect("read pcm");
    let pcm: Vec<f32> = raw
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();

    let mut enc = OpusEncoder::new(rate, ch, app).expect("encoder");
    enc.bitrate_bps = br;
    enc.rate_control = if vbr {
        RateControl::ConstrainedVbr
    } else {
        RateControl::Cbr
    };
    let mut dec = OpusDecoder::new(rate, ch).expect("decoder");
    let mut buf = vec![0u8; 8000];
    let mut out = vec![0.0f32; fs * ch];

    for (i, frame) in pcm.chunks_exact(fs * ch).enumerate() {
        let n = enc.encode(frame, fs, &mut buf).expect("encode");
        let toc = buf[0];
        let mode = if toc >> 3 >= 16 {
            "celt"
        } else if toc >> 3 >= 12 {
            "hybrid"
        } else {
            "silk"
        };
        let frames = packet::frame_count(&buf[..n]).expect("frame_count");
        let got = dec.decode(&buf[..n], fs, &mut out).expect("decode");
        assert_eq!(got, fs, "packet {i} decoded {got} of {fs}");
        assert_eq!(
            dec.final_range(),
            enc.final_range(),
            "packet {i} range mismatch"
        );
        println!(
            "{i:3} len={n:5} toc={toc:02x} mode={mode:<6} code={} frames={frames}",
            toc & 3
        );
    }
}
