//! Sweep encoder configs, writing one .opus per config for reference decoding.
#[path = "common.rs"]
mod common;
use common::*;
use opus_pure::{Application, Bandwidth, OggOpusWriter, OpusEncoder, OpusHead};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = "reference/work/sweep";
    std::fs::create_dir_all(dir)?;
    let bws = [
        ("auto", None),
        ("nb", Some(Bandwidth::Narrowband)),
        ("mb", Some(Bandwidth::Mediumband)),
        ("wb", Some(Bandwidth::Wideband)),
        ("swb", Some(Bandwidth::Superwideband)),
        ("fb", Some(Bandwidth::Fullband)),
    ];
    let apps = [("audio", Application::Audio), ("voip", Application::Voip)];
    for &rate in &[8000i32, 12000, 16000, 24000, 48000] {
        for &ch in &[1usize, 2] {
            for (bn, bw) in bws {
                for (an, app) in apps {
                    let frame = (rate / 50) as usize; // 20 ms
                    let n = rate as usize * 2;
                    let base = music_like(rate, n);
                    let pcm: Vec<f32> = if ch == 1 {
                        base
                    } else {
                        let t = sine(rate, n, 660.0, 0.25);
                        interleave(&[
                            base.clone(),
                            base.iter().zip(&t).map(|(a, b)| a * 0.75 + b).collect(),
                        ])
                    };
                    let name = format!("{rate}_{ch}ch_{bn}_{an}");
                    let path = format!("{dir}/{name}.opus");
                    let run = || -> Result<(), Box<dyn std::error::Error>> {
                        let mut enc = OpusEncoder::new(rate, ch, app)?;
                        enc.bitrate_bps = 48000;
                        enc.force_bandwidth = bw;
                        let head = OpusHead::new(ch as u8, rate as u32)?;
                        let mut w = OggOpusWriter::new(std::fs::File::create(&path)?, head)?;
                        let mut pkt = vec![0u8; 4000];
                        for c in pcm.chunks_exact(frame * ch) {
                            let l = enc.encode(c, frame, &mut pkt)?;
                            w.write_packet(&pkt[..l])?;
                        }
                        w.finish()?;
                        Ok(())
                    };
                    match run() {
                        Ok(()) => println!("{name} OK"),
                        Err(e) => {
                            let _ = std::fs::remove_file(&path);
                            println!("{name} ERR {e}");
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
