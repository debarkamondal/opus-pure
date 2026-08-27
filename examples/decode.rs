//! Decode a `.opus` file back to WAV.
//!
//!   cargo run --release --example decode -- input.opus output.wav [rate]
//!
//! Opus always decodes at a rate you choose, not one stored in the file, so the
//! output rate is an argument. `OpusHead::input_sample_rate` records what the
//! original was, and is used as the default.

#[path = "common/wav.rs"]
mod wav;

use opus_pure::{OggOpusReader, OpusDecoder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: {} <input.opus> <output.wav> [output_rate]", args[0]);
        std::process::exit(2);
    }

    let file = std::fs::File::open(&args[1])?;
    let mut reader = OggOpusReader::new(std::io::BufReader::new(file))?;
    let head = reader.head().clone();
    let channels = head.channel_count as usize;

    // Opus decodes to 8/12/16/24/48 kHz only; fall back to 48 kHz when the file
    // records something else (or nothing).
    let requested: i32 = args
        .get(3)
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(head.input_sample_rate as i32);
    let rate = match requested {
        8_000 | 12_000 | 16_000 | 24_000 | 48_000 => requested,
        other => {
            eprintln!("note: {other} Hz is not an Opus decode rate; using 48000 Hz");
            48_000
        }
    };

    println!(
        "{}: {channels} ch, pre-skip {} samples, gain {:+.2} dB, vendor {:?}",
        args[1],
        head.pre_skip,
        head.output_gain_db(),
        reader.tags().vendor,
    );
    for comment in &reader.tags().comments {
        println!("  {comment}");
    }

    let frame = (rate / 50) as usize;
    let mut decoder = OpusDecoder::new(rate, channels)?;
    // RFC 7845 §5.1: the header carries a gain and players SHOULD apply it.
    // The decoder applies it in the same pass, ahead of any clipping.
    decoder.gain_q8 = head.output_gain_q8 as i32;
    let mut samples = Vec::new();
    let mut out = vec![0.0f32; frame * channels];
    let mut packets = 0usize;

    for packet in reader.packets() {
        let n = decoder.decode(&packet?.data, frame, &mut out)?;
        samples.extend_from_slice(&out[..n * channels]);
        packets += 1;
    }

    // The first `pre_skip` samples are the encoder's algorithmic delay, expressed
    // at 48 kHz — scale it to the decode rate before trimming.
    let skip = (head.pre_skip as usize * rate as usize / 48_000) * channels;
    let trimmed = samples.split_off(skip.min(samples.len()));
    let per_channel = trimmed.len() / channels;

    wav::write(
        &args[2],
        &wav::Wav {
            sample_rate: rate as u32,
            channels: head.channel_count as u16,
            samples: trimmed,
        },
    )?;
    println!(
        "  {packets} packets -> {} ({rate} Hz, {per_channel} samples/ch = {:.2} s after pre-skip)",
        args[2],
        per_channel as f64 / rate as f64,
    );
    Ok(())
}
