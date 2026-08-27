//! 60 ms encoder sweep, written as .opus for reference decoding by libopus.
//!
//! When this sweep was first written the encoder forced every duration above
//! 20 ms to SILK, so a 60 ms packet was always a single SILK frame and the
//! guard below could check for SILK config 3, 7 or 11. The encoder now chooses
//! the mode freely and *frames* the result to suit, so 60 ms of music comes
//! back as three 20 ms CELT frames behind one TOC byte. The guard therefore
//! checks the thing that actually matters — that the packet really carries
//! 60 ms — and the framing is recorded rather than dictated.
#[path = "common.rs"]
mod common;
use common::*;
use opus_pure::{Application, Bandwidth, OggOpusWriter, OpusEncoder, OpusHead};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = "reference/work/sixty";
    std::fs::create_dir_all(dir)?;
    let bws = [
        ("auto", None),
        ("nb", Some(Bandwidth::Narrowband)),
        ("mb", Some(Bandwidth::Mediumband)),
        ("wb", Some(Bandwidth::Wideband)),
    ];
    let apps = [("audio", Application::Audio), ("voip", Application::Voip)];
    for &rate in &[8000i32, 12000, 16000, 24000, 48000] {
        for &ch in &[1usize, 2] {
            for (bn, bw) in bws {
                for (an, app) in apps {
                    for &br in &[16000i32, 48000] {
                        let frame = (rate as usize * 60) / 1000;
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
                        let name = format!("{rate}_{ch}ch_{bn}_{an}_{br}");
                        let path = format!("{dir}/{name}.opus");
                        let run = || -> Result<(), Box<dyn std::error::Error>> {
                            let mut enc = OpusEncoder::new(rate, ch, app)?;
                            enc.bitrate_bps = br;
                            enc.force_bandwidth = bw;
                            let head = OpusHead::new(ch as u8, rate as u32)?;
                            let mut w = OggOpusWriter::new(std::fs::File::create(&path)?, head)?;
                            let mut pkt = vec![0u8; 4000];
                            for c in pcm.chunks_exact(frame * ch) {
                                let l = enc.encode(c, frame, &mut pkt)?;
                                assert_eq!(
                                    packet_samples_48k(&pkt[..l]),
                                    2880,
                                    "{name}: TOC {:02X} does not carry 60 ms",
                                    pkt[0]
                                );
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
    }
    Ok(())
}
