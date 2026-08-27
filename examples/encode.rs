//! Encode a WAV file to a playable `.opus` file.
//!
//!   cargo run --release --example encode -- input.wav output.opus [bitrate]
//!
//! The output is a standard Ogg Opus stream: `ffplay`, VLC and `opusdec` will
//! all open it.

#[path = "common/wav.rs"]
mod wav;

use opus_pure::{Application, MAX_PACKET_BYTES, OggOpusWriter, OpusEncoder, OpusHead, OpusTags};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: {} <input.wav> <output.opus> [bitrate_bps]", args[0]);
        std::process::exit(2);
    }
    let bitrate: i32 = args
        .get(3)
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(64_000);

    let input = wav::read(&args[1])?;
    let channels = input.channels as usize;
    let rate = input.sample_rate as i32;

    // Opus codes 20 ms frames at the encoder's own rate. The container counts
    // everything at 48 kHz regardless, and `write_packet` reads that out of the
    // packet rather than making us convert it.
    let frame = (rate / 50) as usize;

    let mut encoder = OpusEncoder::new(rate, channels, Application::Audio)?;
    encoder.bitrate_bps = bitrate;

    let mut tags = OpusTags::new();
    tags.push("ENCODER", concat!("opus-pure ", env!("CARGO_PKG_VERSION")))?;
    // Built from the encoder, so the pre-skip is that encoder's real delay
    // rather than the conventional constant.
    let head = OpusHead::for_encoder(&encoder, input.sample_rate);
    let file = std::fs::File::create(&args[2])?;
    let mut writer = OggOpusWriter::with_tags(std::io::BufWriter::new(file), head, tags)?;

    let mut packet = vec![0u8; MAX_PACKET_BYTES];
    let mut payload_bytes = 0usize;
    let mut frames = 0usize;

    // Whole frames only; a trailing partial frame is padded with silence so the
    // last of the audio is not dropped.
    let per_frame = frame * channels;
    let mut pos = 0;
    while pos < input.samples.len() {
        let end = (pos + per_frame).min(input.samples.len());
        let mut block = input.samples[pos..end].to_vec();
        block.resize(per_frame, 0.0);
        let n = encoder.encode(&block, frame, &mut packet)?;
        writer.write_packet(&packet[..n])?;
        payload_bytes += n;
        frames += 1;
        pos = end;
    }
    writer.finish()?;

    let secs = frames as f64 * frame as f64 / rate as f64;
    println!(
        "{} -> {}\n  {channels} ch @ {rate} Hz, {frames} frames ({secs:.2} s)\n  \
         {payload_bytes} payload bytes = {:.1} kb/s (target {:.1} kb/s)",
        args[1],
        args[2],
        payload_bytes as f64 * 8.0 / secs / 1000.0,
        bitrate as f64 / 1000.0,
    );
    Ok(())
}
