//! one <rate> <ch> <frame> <bitrate> <audio|voip> <music|speech> <scale> <name>
#[path = "common.rs"]
mod common;
use common::*;
use opus_pure::{Application, OggOpusWriter, OpusEncoder, OpusHead};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (rate, ch, frame): (i32, usize, usize) = (a[0].parse()?, a[1].parse()?, a[2].parse()?);
    let br: i32 = a[3].parse()?;
    let app = match a[4].as_str() {
        "voip" => Application::Voip,
        "rld" => Application::RestrictedLowDelay,
        _ => Application::Audio,
    };
    let scale: f32 = a[6].parse()?;
    let name = &a[7];
    let n = rate as usize * 2;
    let base: Vec<f32> = (match a[5].as_str() {
        "speech" => speech_like(rate, n),
        "sine" => sine(rate, n, 1000.0, 0.5),
        _ => music_like(rate, n),
    })
    .iter()
    .map(|v| v * scale)
    .collect();
    let pcm: Vec<f32> = if ch == 1 {
        base
    } else {
        let t: Vec<f32> = sine(rate, n, 660.0, 0.25 * scale);
        let r: Vec<f32> = base.iter().zip(&t).map(|(x, y)| x * 0.75 + y).collect();
        interleave(&[base, r])
    };
    let mut enc = OpusEncoder::new(rate, ch, app)?;
    enc.bitrate_bps = br;
    let head = OpusHead::new(ch as u8, rate as u32)?;
    let mut w = OggOpusWriter::new(
        std::fs::File::create(format!("reference/work/one/{name}.opus"))?,
        head,
    )?;
    let mut pkt = vec![0u8; 4000];
    for c in pcm.chunks_exact(frame * ch) {
        let l = enc.encode(c, frame, &mut pkt)?;
        w.write_packet(&pkt[..l])?;
    }
    w.finish()?;
    println!(
        "{name} src_peak={:.4}",
        pcm.iter().fold(0f32, |m, v| m.max(v.abs()))
    );
    Ok(())
}
