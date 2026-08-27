//! Focused check: default (auto) bandwidth across bitrates, content, rates.
#[path = "common.rs"]
mod common;
use common::*;
use opus_pure::{Application, OggOpusWriter, OpusEncoder, OpusHead};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = "reference/work/focus";
    std::fs::create_dir_all(dir)?;
    for &rate in &[8000i32, 12000, 16000, 24000, 48000] {
        for &ch in &[1usize, 2] {
            for &br in &[16000i32, 32000, 64000, 96000] {
                for kind in ["music", "speech"] {
                    for (an, app) in [("audio", Application::Audio), ("voip", Application::Voip)] {
                        let frame = (rate / 50) as usize;
                        let n = rate as usize * 2;
                        let base = if kind == "music" {
                            music_like(rate, n)
                        } else {
                            speech_like(rate, n)
                        };
                        let pcm: Vec<f32> = if ch == 1 {
                            base
                        } else {
                            let t = sine(rate, n, 660.0, 0.25);
                            let r: Vec<f32> =
                                base.iter().zip(&t).map(|(a, b)| a * 0.75 + b).collect();
                            interleave(&[base, r])
                        };
                        let src_peak = pcm.iter().fold(0f32, |m, v| m.max(v.abs()));
                        let name = format!("{rate}_{ch}ch_{br}_{kind}_{an}");
                        let mut enc = OpusEncoder::new(rate, ch, app)?;
                        enc.bitrate_bps = br; // force_bandwidth left at default (auto)
                        let head = OpusHead::new(ch as u8, rate as u32)?;
                        let mut w = OggOpusWriter::new(
                            std::fs::File::create(format!("{dir}/{name}.opus"))?,
                            head,
                        )?;
                        let mut pkt = vec![0u8; 4000];
                        for c in pcm.chunks_exact(frame * ch) {
                            let l = enc.encode(c, frame, &mut pkt)?;
                            w.write_packet(&pkt[..l])?;
                        }
                        w.finish()?;
                        println!("{name} src_peak={src_peak:.4}");
                    }
                }
            }
        }
    }
    Ok(())
}
