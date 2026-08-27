//! Hold `encode_parallel` against the serial encode it claims to approximate.
//!
//! Serial is the reference here, not libopus: chunked encoding is this crate's
//! own addition, and the question it has to answer is whether splitting a clip
//! across threads produces the stream one encoder would have produced. So this
//! encodes the same audio both ways and reports the three things that can
//! differ, in the order they matter.
//!
//! **Coding mode.** The largest decision an Opus encoder makes is whether a
//! frame is SILK, hybrid or CELT, and it makes it from a content analysis whose
//! history is two seconds deep. A worker primed for less than that decides for
//! itself, and the result is not a seam but an entire chunk in the wrong mode,
//! so this counts disagreeing frames per chunk rather than looking near
//! boundaries.
//!
//! **Delivered bitrate.** The rate controllers are long-memory integrators, and
//! a worker's is wrong for a while after its boundary.
//!
//! **Per-frame SNR against the source.** What is left once the modes agree:
//! a few dB over a few frames at each boundary.
//!
//! ```text
//! parchunk <rate> <ch> <bitrate> <frame_ms> <seconds> <speech|music>
//!          <warmup_ms> <threads> [auto|voice|music]
//! ```
#[path = "common.rs"]
mod common;
use common::harness::{aligned_correlation, music_like, packet_mode, speech_like};
use opus_pure::{Application, OpusDecoder, ParallelConfig, Signal, encode_parallel};

fn die(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(2);
}

/// The same source the speed and interop harnesses use, so a row here describes
/// the same audio as a row there. The right channel lags by half a millisecond
/// and sits about 2 dB down; two identical channels would make mid/side coding
/// free and flatter every stereo result.
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
    let mut out = vec![0.0f32; samples * 2];
    for i in 0..samples {
        out[2 * i] = left[i];
        out[2 * i + 1] = if i >= lag { left[i - lag] * 0.79 } else { 0.0 };
    }
    out
}

fn decode(packets: &[Vec<u8>], rate: i32, channels: usize, frame: usize) -> Vec<f32> {
    let mut dec = OpusDecoder::new(rate, channels).unwrap_or_else(|e| die(&format!("{e:?}")));
    let mut out = Vec::with_capacity(packets.len() * frame * channels);
    let mut buf = vec![0.0f32; frame * channels];
    for p in packets {
        let n = dec
            .decode(p, frame, &mut buf)
            .unwrap_or_else(|e| die(&format!("decode: {e:?}")));
        out.extend_from_slice(&buf[..n * channels]);
    }
    out
}

/// Per-frame SNR against the source, at a single alignment lag measured once for
/// the whole stream. Both streams are aligned by the same lag so their rows are
/// comparable frame by frame.
fn per_frame_snr(dec: &[f32], src: &[f32], channels: usize, frame: usize, lag: usize) -> Vec<f32> {
    let step = frame * channels;
    let off = lag * channels;
    let n = (dec.len().saturating_sub(off) / step).min(src.len() / step);
    let mut out = Vec::with_capacity(n);
    for f in 0..n {
        let d = &dec[off + f * step..off + (f + 1) * step];
        let s = &src[f * step..(f + 1) * step];
        let (mut sig, mut err) = (0.0f64, 0.0f64);
        for i in 0..step {
            sig += f64::from(s[i]) * f64::from(s[i]);
            let e = f64::from(d[i] - s[i]);
            err += e * e;
        }
        out.push(if err > 0.0 {
            (10.0 * (sig / err).log10()) as f32
        } else {
            f32::INFINITY
        });
    }
    out
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 9 {
        die(concat!(
            "usage: parchunk <rate> <ch> <bitrate> <frame_ms> <seconds> <speech|music>\n",
            "                <warmup_ms> <threads> [auto|voice|music]"
        ));
    }
    let rate: i32 = a[1].parse().unwrap_or_else(|_| die("rate"));
    let channels: usize = a[2].parse().unwrap_or_else(|_| die("ch"));
    let bitrate: i32 = a[3].parse().unwrap_or_else(|_| die("bitrate"));
    let frame_ms: f64 = a[4].parse().unwrap_or_else(|_| die("frame_ms"));
    let seconds: f64 = a[5].parse().unwrap_or_else(|_| die("seconds"));
    let speech = match a[6].as_str() {
        "speech" => true,
        "music" => false,
        s => die(&format!("content: {s}")),
    };
    let warmup_ms: u32 = a[7].parse().unwrap_or_else(|_| die("warmup_ms"));
    let threads: usize = a[8].parse().unwrap_or_else(|_| die("threads"));
    let signal = match a.get(9).map(String::as_str) {
        None | Some("auto") => None,
        Some("voice") => Some(Signal::Voice),
        Some("music") => Some(Signal::Music),
        Some(s) => die(&format!("signal: {s}")),
    };

    let frame = (rate as f64 * frame_ms / 1000.0).round() as usize;
    // A whole number of frames, so both encodes see the same denominator.
    let mut samples = (rate as f64 * seconds) as usize;
    samples -= samples % frame;
    let pcm = source(rate, channels, samples, speech);

    let mut cfg = ParallelConfig::new(rate, channels, Application::Audio);
    cfg.bitrate_bps = bitrate;
    cfg.warmup_ms = warmup_ms;
    cfg.threads = threads;
    cfg.signal_type = signal;

    let total_frames = samples / frame;
    let plan = cfg.plan(total_frames, frame);

    // The serial anchor: one worker is by construction one continuous encoder.
    let mut serial_cfg = cfg;
    serial_cfg.threads = 1;
    serial_cfg.warmup_ms = 0;
    let ser = encode_parallel(&serial_cfg, &pcm, frame).unwrap();
    let par = encode_parallel(&cfg, &pcm, frame).unwrap();
    let par_again = encode_parallel(&cfg, &pcm, frame).unwrap();

    let audio_s = samples as f64 / rate as f64;
    let kbps =
        |p: &[Vec<u8>]| p.iter().map(Vec::len).sum::<usize>() as f64 * 8.0 / audio_s / 1000.0;

    println!(
        "{rate} Hz {channels}ch {} kb/s {frame_ms} ms {} {seconds:.0} s  warmup={warmup_ms} ms signal={}",
        bitrate / 1000,
        if speech { "speech" } else { "music" },
        a.get(9).map_or("auto", String::as_str),
    );
    println!(
        "  plan: {} worker(s), {} warm-up frames, {} redundant of {total_frames} ({:.1}% overhead)",
        plan.workers,
        plan.warmup_frames,
        plan.redundant_frames,
        100.0 * plan.overhead(),
    );
    println!(
        "  packets: {} serial, {} parallel   deterministic={}   byte-identical={}",
        ser.len(),
        par.len(),
        par == par_again,
        par == ser,
    );
    println!(
        "  bitrate: {:.2} kb/s serial, {:.2} kb/s parallel ({:+.2}%)",
        kbps(&ser),
        kbps(&par),
        100.0 * (kbps(&par) - kbps(&ser)) / kbps(&ser),
    );

    // Mode agreement, per chunk. A worker that primed on too little analysis
    // codes its whole range in a mode the continuous encoder never chose, so the
    // count that matters is over the chunk, not near its head.
    let mut worst_chunk = 0usize;
    for (w, &(lo, hi)) in plan.ranges.iter().enumerate() {
        let differ = (lo..hi)
            .filter(|&i| packet_mode(&ser[i]) != packet_mode(&par[i]))
            .count();
        worst_chunk = worst_chunk.max(differ);
        let modes = |p: &[Vec<u8>]| {
            let mut c: Vec<(&str, usize)> = Vec::new();
            for m in p[lo..hi].iter().map(|x| packet_mode(x)) {
                match c.iter_mut().find(|e| e.0 == m) {
                    Some(e) => e.1 += 1,
                    None => c.push((m, 1)),
                }
            }
            c.sort_unstable();
            c.iter()
                .map(|(k, v)| format!("{k}:{v}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        println!(
            "  chunk {w} [{lo}..{hi}): mode differs on {differ}/{} frames | serial {{{}}} | parallel {{{}}}",
            hi - lo,
            modes(&ser),
            modes(&par),
        );
    }
    println!("  worst chunk: {worst_chunk} frames in a mode the serial encoder did not use");

    // What is left once the modes agree: the boundary itself.
    let ds = decode(&ser, rate, channels, frame);
    let dp = decode(&par, rate, channels, frame);
    let (_, lag) = aligned_correlation(&ds, &pcm, frame);
    let lag = lag / channels;
    let ss = per_frame_snr(&ds, &pcm, channels, frame, lag);
    let sp = per_frame_snr(&dp, &pcm, channels, frame, lag);
    let boundaries: Vec<usize> = plan.ranges.iter().skip(1).map(|&(lo, _)| lo).collect();
    let near = |i: usize| boundaries.iter().any(|&b| i + 2 >= b && i < b + 4);

    let mut worst = (0usize, 0.0f32);
    let mut worst_away = (0usize, 0.0f32);
    for i in 0..ss.len().min(sp.len()) {
        let drop = ss[i] - sp[i];
        if drop > worst.1 {
            worst = (i, drop);
        }
        if !near(i) && drop > worst_away.1 {
            worst_away = (i, drop);
        }
    }
    println!(
        "  SNR vs serial: worst frame drops {:.2} dB at {} ({}); worst away from a boundary {:.2} dB at {}",
        worst.1,
        worst.0,
        if near(worst.0) {
            "at a boundary"
        } else {
            "not at a boundary"
        },
        worst_away.1,
        worst_away.0,
    );
}
