//! Time this crate's encode and decode over one configuration, so `cspeed` can
//! time libopus over the same audio at the same settings and the two can be
//! read side by side.
//!
//! `benches/throughput.rs` already measures this crate; what it cannot say is
//! what the numbers are *worth*, because it has nothing to compare them
//! against. This tool exists to supply the other column. Its timing is
//! deliberately identical to that benchmark's — fastest of N passes, a fresh
//! encoder per pass built outside the timed region, packets collected in a
//! separate untimed pass — and `cspeed.c` transcribes the same shape in C, so
//! what is left between the two columns is the codec.
//!
//! ```text
//! rspeed gen <pcm_out> <rate> <ch> <frame> <seconds> <speech|music>
//! rspeed run <pcm_in> <rate> <ch> <frame> <bitrate> <complexity>
//!            <voip|audio|lowdelay> <auto|nb|mb|wb|swb|fb> <auto|voice|music> <reps>
//! rspeed dec <same arguments as run>
//! ```
//!
//! `run` takes exactly the argument list `cspeed` takes and prints exactly the
//! line it prints, so `run.sh` can call either without knowing which.
//!
//! `dec` takes the same arguments and decodes only, encoding once outside the
//! measurement. It exists for the profiler: in every case the table covers,
//! encoding outweighs decoding by between two and eleven times, so a sampling
//! profiler pointed at `run` reports almost entirely on the encoder. Its timing
//! is `run`'s decode timing unchanged, so the two agree and a profile can be
//! trusted to describe the row it came from.

mod common;
use common::harness::{interleave, music_like, packet_mode, speech_like};
use common::write_f32;

use opus_pure::{Application, Bandwidth, OpusDecoder, OpusEncoder, Signal};
use std::time::Instant;

/// Largest packet the encoder can emit: 1275 bytes per frame, up to 48 frames.
const MAX_PACKET: usize = 1275 * 48 + 2;

fn die(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(2);
}

/// The benchmark's own source material, so a row here and a row there describe
/// the same audio. The right channel lags by half a millisecond and sits about
/// 2 dB down: two identical channels would make mid/side coding free and
/// flatter every stereo row.
fn source(rate: i32, channels: usize, samples: usize, speech: bool) -> Vec<f32> {
    let left = if speech {
        speech_like(rate, samples)
    } else {
        music_like(rate, samples)
    };
    if channels == 1 {
        return left;
    }
    let lag = (rate / 2000).max(1) as usize;
    let mut right = vec![0.0f32; samples];
    for i in lag..samples {
        right[i] = left[i - lag] * 0.79;
    }
    interleave(&[left, right])
}

struct Case {
    rate: i32,
    channels: usize,
    frame: usize,
    bitrate: i32,
    complexity: i32,
    app: Application,
    bandwidth: Option<Bandwidth>,
    signal: Option<Signal>,
}

impl Case {
    fn encoder(&self) -> OpusEncoder {
        let mut enc = OpusEncoder::new(self.rate, self.channels, self.app)
            .unwrap_or_else(|e| die(&format!("encoder: {e:?}")));
        enc.bitrate_bps = self.bitrate;
        enc.complexity = self.complexity;
        enc.force_bandwidth = self.bandwidth;
        enc.signal_type = self.signal;
        enc
    }
}

fn generate(a: &[String]) {
    let rate: i32 = a[3].parse().unwrap_or_else(|_| die("rate"));
    let channels: usize = a[4].parse().unwrap_or_else(|_| die("ch"));
    let frame: usize = a[5].parse().unwrap_or_else(|_| die("frame"));
    let seconds: f64 = a[6].parse().unwrap_or_else(|_| die("seconds"));
    let speech = match a[7].as_str() {
        "speech" => true,
        "music" => false,
        s => die(&format!("content: {s}")),
    };
    // A whole number of frames, so both stacks encode the same frame count and
    // the per-frame numbers divide by the same denominator.
    let mut n = (rate as f64 * seconds) as usize;
    n -= n % frame;
    let pcm = source(rate, channels, n, speech);
    write_f32(&a[2], &pcm).unwrap_or_else(|e| die(&format!("write: {e}")));
    eprintln!("{} samples/ch, {} frames", n, n / frame);
}

fn parse_case(a: &[String]) -> Case {
    Case {
        rate: a[3].parse().unwrap_or_else(|_| die("rate")),
        channels: a[4].parse().unwrap_or_else(|_| die("ch")),
        frame: a[5].parse().unwrap_or_else(|_| die("frame")),
        bitrate: a[6].parse().unwrap_or_else(|_| die("bitrate")),
        complexity: a[7].parse().unwrap_or_else(|_| die("complexity")),
        app: match a[8].as_str() {
            "voip" => Application::Voip,
            "audio" => Application::Audio,
            "lowdelay" => Application::RestrictedLowDelay,
            s => die(&format!("application: {s}")),
        },
        bandwidth: match a[9].as_str() {
            "auto" => None,
            "nb" => Some(Bandwidth::Narrowband),
            "mb" => Some(Bandwidth::Mediumband),
            "wb" => Some(Bandwidth::Wideband),
            "swb" => Some(Bandwidth::Superwideband),
            "fb" => Some(Bandwidth::Fullband),
            s => die(&format!("bandwidth: {s}")),
        },
        signal: match a[10].as_str() {
            "auto" => None,
            "voice" => Some(Signal::Voice),
            "music" => Some(Signal::Music),
            s => die(&format!("signal: {s}")),
        },
    }
}

/// Everything a measurement needs that is not itself measured: the clip, and
/// the packets an encode produced from it.
struct Prepared {
    pcm: Vec<f32>,
    packets: Vec<Vec<u8>>,
    modes: Vec<&'static str>,
    stride: usize,
    frames: usize,
    audio_s: f64,
    coded: usize,
}

fn prepare(case: &Case, a: &[String]) -> Prepared {
    // Read the whole clip up front: file I/O is not part of either measurement.
    let raw = std::fs::read(&a[2]).unwrap_or_else(|e| die(&format!("pcm: {e}")));
    let pcm: Vec<f32> = raw
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();

    let stride = case.frame * case.channels;
    let frames = pcm.len() / stride;
    if frames == 0 {
        die("clip is shorter than one frame");
    }
    let audio_s = (frames * case.frame) as f64 / case.rate as f64;

    // Untimed pass: keep the packets for the decode measurement, and read the
    // modes the encoder actually chose out of their TOC bytes.
    let mut enc = case.encoder();
    let mut scratch = vec![0u8; MAX_PACKET];
    let mut packets: Vec<Vec<u8>> = Vec::with_capacity(frames);
    let mut modes: Vec<&str> = Vec::new();
    for chunk in pcm.chunks_exact(stride) {
        let n = enc
            .encode(chunk, case.frame, &mut scratch)
            .unwrap_or_else(|e| die(&format!("encode: {e:?}")));
        let packet = scratch[..n].to_vec();
        let m = packet_mode(&packet);
        if !modes.contains(&m) {
            modes.push(m);
        }
        packets.push(packet);
    }
    let coded: usize = packets.iter().map(|p| p.len()).sum();

    // Optional 12th argument: dump the packets length-prefixed, for the split probe.
    if let Some(path) = a.get(12) {
        use std::io::Write;
        let mut f = std::io::BufWriter::new(
            std::fs::File::create(path).unwrap_or_else(|e| die(&format!("pkt_out: {e}"))),
        );
        for p in &packets {
            f.write_all(&(p.len() as u32).to_le_bytes()).unwrap();
            f.write_all(p).unwrap();
        }
        f.flush().unwrap();
    }

    Prepared {
        pcm,
        packets,
        modes,
        stride,
        frames,
        audio_s,
        coded,
    }
}

/// The decode half of a measurement, shared so that `run` and `dec` time the
/// same loop rather than two loops that merely look alike.
fn time_decode(case: &Case, p: &Prepared, reps: usize) -> f64 {
    let mut out = vec![0.0f32; p.stride];
    let mut best = f64::INFINITY;
    for _ in 0..reps {
        // A fresh decoder per pass, built outside the timed region: decoder
        // state evolves, and the first packet into a used decoder is not the
        // same work as the first packet into a new one.
        let mut dec = OpusDecoder::new(case.rate, case.channels)
            .unwrap_or_else(|e| die(&format!("decoder: {e:?}")));
        let t = Instant::now();
        for pkt in &p.packets {
            dec.decode(pkt, case.frame, &mut out)
                .unwrap_or_else(|e| die(&format!("decode: {e:?}")));
        }
        best = best.min(t.elapsed().as_secs_f64());
    }
    best
}

fn run(a: &[String]) {
    let case = parse_case(a);
    let reps: usize = a[11].parse().unwrap_or_else(|_| die("reps"));
    let p = prepare(&case, a);
    let (pcm, stride, frames, audio_s, coded) = (&p.pcm, p.stride, p.frames, p.audio_s, p.coded);
    let modes = &p.modes;
    let mut scratch = vec![0u8; MAX_PACKET];

    let mut enc_best = f64::INFINITY;
    for _ in 0..reps {
        let mut enc = case.encoder();
        let t = Instant::now();
        // Checked inside the timed region, as `benches/throughput.rs` checks:
        // an error here would otherwise read as speed.
        for chunk in pcm.chunks_exact(stride) {
            enc.encode(chunk, case.frame, &mut scratch)
                .unwrap_or_else(|e| die(&format!("encode: {e:?}")));
        }
        enc_best = enc_best.min(t.elapsed().as_secs_f64());
    }
    let dec_best = time_decode(&case, &p, reps);

    println!(
        "{:.3}\t{:.1}\t{:.3}\t{:.1}\t{:.1}\t{}",
        enc_best * 1e6 / frames as f64,
        audio_s / enc_best,
        dec_best * 1e6 / frames as f64,
        audio_s / dec_best,
        coded as f64 * 8.0 / audio_s / 1000.0,
        modes.join("+")
    );
}

/// Decode only, for the profiler. Prints `run`'s decode columns and nothing
/// else, so a run of this can be checked against the matching `run` row.
fn dec(a: &[String]) {
    let case = parse_case(a);
    let reps: usize = a[11].parse().unwrap_or_else(|_| die("reps"));
    let p = prepare(&case, a);
    let best = time_decode(&case, &p, reps);
    println!(
        "{:.3}\t{:.1}\t{}",
        best * 1e6 / p.frames as f64,
        p.audio_s / best,
        p.modes.join("+")
    );
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("gen") if a.len() >= 8 => generate(&a),
        Some("run") if a.len() >= 12 => run(&a),
        Some("dec") if a.len() >= 12 => dec(&a),
        _ => die(concat!(
            "usage: rspeed gen <pcm_out> <rate> <ch> <frame> <seconds> <speech|music>\n",
            "       rspeed run <pcm_in> <rate> <ch> <frame> <bitrate> <complexity>",
            " <voip|audio|lowdelay> <auto|nb|mb|wb|swb|fb> <auto|voice|music> <reps>\n",
            "       rspeed dec <same arguments as run>   (decode only, for the profiler)"
        )),
    }
}
