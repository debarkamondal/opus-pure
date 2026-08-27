//! Concealment cross-check: encode a stream with this crate, write the packets,
//! then decode them with a given set dropped and dump the PCM. `cplc`
//! decodes the same packets with the same drops through libopus, so the two
//! dumps can be compared sample for sample.
//!
//! usage: plc <pkt_out> <pcm_out> <rate> <ch> <bitrate> <auto|nb|mb|wb|swb|fb>
//!            <voip|audio> <speech|music> <frame_ms> <frames> [lost,...]
#![allow(dead_code)]

mod common;
use common::{music_like, speech_like, write_f32};

use opus_pure::{Application, Bandwidth, OpusDecoder, OpusEncoder, Signal};
use std::io::Write;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 11 {
        eprintln!(
            "usage: {} <pkt_out> <pcm_out> <rate> <ch> <bitrate> \
             <auto|nb|mb|wb|swb|fb> <voip|audio> <speech|music> <frame_ms> <frames> [lost,...]",
            a[0]
        );
        std::process::exit(2);
    }
    let (pkt_out, pcm_out) = (&a[1], &a[2]);
    let rate: i32 = a[3].parse().unwrap();
    let channels: usize = a[4].parse().unwrap();
    let bitrate: i32 = a[5].parse().unwrap();
    let bandwidth = match a[6].as_str() {
        "auto" => None,
        "nb" => Some(Bandwidth::Narrowband),
        "mb" => Some(Bandwidth::Mediumband),
        "wb" => Some(Bandwidth::Wideband),
        "swb" => Some(Bandwidth::Superwideband),
        "fb" => Some(Bandwidth::Fullband),
        s => panic!("bandwidth {s}"),
    };
    let app = match a[7].as_str() {
        "voip" => Application::Voip,
        "audio" => Application::Audio,
        s => panic!("application {s}"),
    };
    let speech = a[8] == "speech";
    let frame_ms: i32 = a[9].parse().unwrap();
    let frames: usize = a[10].parse().unwrap();
    let lost: Vec<usize> = a
        .get(11)
        .filter(|s| !s.is_empty())
        .map(|s| s.split(',').map(|t| t.parse().unwrap()).collect())
        .unwrap_or_default();

    let frame = (rate as usize * frame_ms as usize) / 1000;
    let mono = if speech {
        speech_like(rate, frame * frames)
    } else {
        music_like(rate, frame * frames)
    };
    let src: Vec<f32> = if channels == 1 {
        mono
    } else {
        let shift = rate as usize / 2;
        (0..mono.len())
            .flat_map(|i| [mono[i], mono[(i + shift) % mono.len()] * 0.9])
            .collect()
    };

    let mut enc = OpusEncoder::new(rate, channels, app).expect("encoder");
    enc.bitrate_bps = bitrate;
    enc.force_bandwidth = bandwidth;
    enc.signal_type = Some(if speech { Signal::Voice } else { Signal::Music });

    let mut packets: Vec<Vec<u8>> = Vec::with_capacity(frames);
    let mut buf = vec![0u8; 4000];
    for i in 0..frames {
        let n = enc
            .encode(
                &src[i * frame * channels..(i + 1) * frame * channels],
                frame,
                &mut buf,
            )
            .expect("encode");
        packets.push(buf[..n].to_vec());
    }

    let mut f = std::io::BufWriter::new(std::fs::File::create(pkt_out).expect("pkt_out"));
    for p in &packets {
        f.write_all(&(p.len() as u32).to_le_bytes()).unwrap();
        f.write_all(p).unwrap();
    }
    f.flush().unwrap();

    let mut dec = OpusDecoder::new(rate, channels).expect("decoder");
    let mut out = vec![0.0f32; frame * channels];
    let mut pcm = Vec::with_capacity(frame * channels * frames);
    let mut modes = String::new();
    for (i, p) in packets.iter().enumerate() {
        let drop = lost.contains(&i);
        let n = if drop {
            dec.decode(&[], frame, &mut out).expect("plc")
        } else {
            dec.decode(p, frame, &mut out).expect("decode")
        };
        pcm.extend_from_slice(&out[..n * channels]);
        if i < 40 {
            let toc = p[0] >> 3;
            modes.push_str(&format!("{}{} ", if drop { "*" } else { "" }, toc));
        }
    }
    write_f32(pcm_out, &pcm).expect("pcm_out");
    eprintln!("{} packets, {} concealed", packets.len(), lost.len());
    eprintln!("toc configs: {modes}");
}
