//! Mode-switch reproducer: encode a stream whose content forces SILK<->CELT
//! transitions, dump the per-packet mode, and report where our decoder differs
//! from libopus.
#[path = "common.rs"]
mod common;
use common::*;
use opus_pure::{Application, OggOpusWriter, OpusEncoder, OpusHead};

fn mode_of(toc: u8) -> &'static str {
    match toc >> 3 {
        0..=11 => "SILK",
        12..=15 => "HYBRID",
        _ => "CELT",
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut a = std::env::args().skip(1);
    let rate: i32 = a.next().unwrap_or("48000".into()).parse()?;
    let ch: usize = a.next().unwrap_or("1".into()).parse()?;
    let br: i32 = a.next().unwrap_or("32000".into()).parse()?;
    let app = if a.next().as_deref() == Some("voip") {
        Application::Voip
    } else {
        Application::Audio
    };

    let dir = "reference/work/ms";
    std::fs::create_dir_all(dir)?;
    let frame = (rate / 50) as usize;
    let seg = rate as usize; // 1 s per segment
    // speech | music | speech -> at least two mode transitions
    let mut mono = speech_like(rate, seg);
    mono.extend(music_like(rate, seg));
    mono.extend(speech_like(rate, seg));
    let pcm: Vec<f32> = if ch == 1 {
        mono
    } else {
        let t = sine(rate, mono.len(), 660.0, 0.25);
        let r: Vec<f32> = mono.iter().zip(&t).map(|(x, y)| x * 0.75 + y).collect();
        interleave(&[mono, r])
    };

    let name = format!("ms_{rate}_{ch}ch_{br}");
    let mut enc = OpusEncoder::new(rate, ch, app)?;
    enc.bitrate_bps = br;
    // The writer derives each packet's granule itself; this is still needed to
    // convert frame indices to output sample offsets below.
    let s48 = (frame as u64 * 48000 / rate as u64) as u32;
    let head = OpusHead::new(ch as u8, rate as u32)?;
    let mut w = OggOpusWriter::new(std::fs::File::create(format!("{dir}/{name}.opus"))?, head)?;
    let mut pkt = vec![0u8; 4000];
    let mut modes = Vec::new();
    for c in pcm.chunks_exact(frame * ch) {
        let l = enc.encode(c, frame, &mut pkt)?;
        modes.push(mode_of(pkt[0]));
        w.write_packet(&pkt[..l])?;
    }
    w.finish()?;

    // Packet index -> first 48 kHz output sample (post-pre-skip).
    let pre_skip = 312usize;
    let mut switches = Vec::new();
    for i in 1..modes.len() {
        if modes[i] != modes[i - 1] {
            switches.push((i, modes[i - 1], modes[i]));
        }
    }
    println!(
        "{name}: {} packets, {} transitions",
        modes.len(),
        switches.len()
    );
    for (i, from, to) in &switches {
        let start = (*i * s48 as usize) as i64 - pre_skip as i64;
        println!("  pkt {i:4} {from} -> {to}   output sample {start}");
    }
    std::fs::write(
        format!("{dir}/{name}.switches"),
        switches
            .iter()
            .map(|(i, f, t)| {
                format!(
                    "{i} {f} {t} {}\n",
                    (*i * s48 as usize) as i64 - pre_skip as i64
                )
            })
            .collect::<String>(),
    )?;
    std::fs::write(format!("{dir}/{name}.s48"), format!("{s48}\n"))?;
    Ok(())
}
