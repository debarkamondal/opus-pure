//! Generate a matrix of .opus files with our encoder + muxer, alongside the
//! source PCM, so a reference decoder can be measured against both.

#[path = "common.rs"]
mod common;

use common::*;
use opus_pure::{
    Application, Bandwidth, ChannelLayout, OggOpusWriter, OpusEncoder, OpusHead, OpusMSEncoder,
    OpusTags, Signal,
};

struct Case {
    name: &'static str,
    rate: i32,
    channels: usize,
    frame: usize,
    app: Application,
    bitrate: i32,
    signal: Option<Signal>,
    bandwidth: Option<Bandwidth>,
    content: &'static str,
    secs: f32,
}

fn content(kind: &str, rate: i32, n: usize, ch: usize) -> Vec<f32> {
    let per = match kind {
        "speech" => speech_like(rate, n),
        "music" => music_like(rate, n),
        _ => sine(rate, n, 440.0, 0.5),
    };
    if ch == 1 {
        per
    } else {
        // Decorrelate the right channel a little so stereo coding is exercised
        // rather than collapsing to a pure mono downmix.
        let tone = sine(rate, n, 660.0, 0.25);
        let right: Vec<f32> = per.iter().zip(&tone).map(|(s, t)| s * 0.75 + t).collect();
        let mut chans = vec![per, right];
        chans.truncate(2);
        interleave(&chans)
    }
}

fn toc_mode(toc: u8) -> &'static str {
    match toc >> 3 {
        0..=11 => "silk",
        12..=15 => "hybrid",
        _ => "celt",
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "reference/work/out".into());
    std::fs::create_dir_all(&dir)?;

    let cases = vec![
        Case {
            name: "mono48_20ms_music",
            rate: 48000,
            channels: 1,
            frame: 960,
            app: Application::Audio,
            bitrate: 96000,
            signal: Some(Signal::Music),
            bandwidth: None,
            content: "music",
            secs: 2.0,
        },
        Case {
            name: "stereo48_20ms_music",
            rate: 48000,
            channels: 2,
            frame: 960,
            app: Application::Audio,
            bitrate: 128000,
            signal: Some(Signal::Music),
            bandwidth: None,
            content: "music",
            secs: 2.0,
        },
        Case {
            name: "mono48_10ms_speech",
            rate: 48000,
            channels: 1,
            frame: 480,
            app: Application::Voip,
            bitrate: 32000,
            signal: Some(Signal::Voice),
            bandwidth: None,
            content: "speech",
            secs: 2.0,
        },
        Case {
            name: "stereo48_40ms_speech",
            rate: 48000,
            channels: 2,
            frame: 1920,
            app: Application::Voip,
            bitrate: 48000,
            signal: Some(Signal::Voice),
            bandwidth: None,
            content: "speech",
            secs: 2.0,
        },
        Case {
            name: "mono48_2p5ms_celt",
            rate: 48000,
            channels: 1,
            frame: 120,
            app: Application::RestrictedLowDelay,
            bitrate: 96000,
            signal: None,
            bandwidth: None,
            content: "music",
            secs: 1.0,
        },
        Case {
            name: "stereo48_5ms_celt",
            rate: 48000,
            channels: 2,
            frame: 240,
            app: Application::RestrictedLowDelay,
            bitrate: 128000,
            signal: None,
            bandwidth: None,
            content: "music",
            secs: 1.0,
        },
        Case {
            name: "mono16_20ms_silk",
            rate: 16000,
            channels: 1,
            frame: 320,
            app: Application::Voip,
            bitrate: 24000,
            signal: Some(Signal::Voice),
            bandwidth: Some(Bandwidth::Wideband),
            content: "speech",
            secs: 2.0,
        },
        Case {
            name: "stereo16_20ms_silk",
            rate: 16000,
            channels: 2,
            frame: 320,
            app: Application::Voip,
            bitrate: 32000,
            signal: Some(Signal::Voice),
            bandwidth: Some(Bandwidth::Wideband),
            content: "speech",
            secs: 2.0,
        },
        Case {
            name: "mono8_20ms_nb",
            rate: 8000,
            channels: 1,
            frame: 160,
            app: Application::Voip,
            bitrate: 16000,
            signal: Some(Signal::Voice),
            bandwidth: Some(Bandwidth::Narrowband),
            content: "speech",
            secs: 2.0,
        },
        Case {
            name: "mono24_20ms_hybrid",
            rate: 24000,
            channels: 1,
            frame: 480,
            app: Application::Audio,
            bitrate: 48000,
            signal: None,
            bandwidth: Some(Bandwidth::Superwideband),
            content: "music",
            secs: 2.0,
        },
        Case {
            name: "stereo48_20ms_hybrid",
            rate: 48000,
            channels: 2,
            frame: 960,
            app: Application::Audio,
            bitrate: 40000,
            signal: Some(Signal::Voice),
            bandwidth: Some(Bandwidth::Superwideband),
            content: "speech",
            secs: 2.0,
        },
        Case {
            name: "stereo48_20ms_sine",
            rate: 48000,
            channels: 2,
            frame: 960,
            app: Application::Audio,
            bitrate: 96000,
            signal: Some(Signal::Music),
            bandwidth: None,
            content: "sine",
            secs: 2.0,
        },
    ];

    for c in cases {
        let n = (c.rate as f32 * c.secs) as usize;
        let n = n - (n % c.frame);
        let pcm = content(c.content, c.rate, n, c.channels);

        let mut enc = OpusEncoder::new(c.rate, c.channels, c.app)?;
        enc.bitrate_bps = c.bitrate;
        enc.signal_type = c.signal;
        if let Some(bw) = c.bandwidth {
            enc.force_bandwidth = Some(bw);
        }

        let mut head = OpusHead::new(c.channels as u8, c.rate as u32)?;
        head.pre_skip = OpusHead::RECOMMENDED_PRE_SKIP;
        let mut tags = OpusTags::new();
        tags.push("ENCODER", "opus-pure interop harness")?;
        let path = format!("{dir}/{}.opus", c.name);
        let mut w = OggOpusWriter::with_tags(std::fs::File::create(&path)?, head, tags)?;

        let mut pkt = vec![0u8; 4000];
        let mut modes = std::collections::BTreeMap::<&str, usize>::new();
        for chunk in pcm.chunks_exact(c.frame * c.channels) {
            let len = enc.encode(chunk, c.frame, &mut pkt)?;
            *modes.entry(toc_mode(pkt[0])).or_default() += 1;
            w.write_packet(&pkt[..len])?;
        }
        w.finish()?;

        write_f32(&format!("{dir}/{}.src.f32", c.name), &pcm)?;
        let modes: Vec<String> = modes.iter().map(|(k, v)| format!("{k}={v}")).collect();
        println!(
            "{:<22} rate={:<5} ch={} frame={:<4} bytes={:<7} modes[{}]",
            c.name,
            c.rate,
            c.channels,
            c.frame,
            std::fs::metadata(&path)?.len(),
            modes.join(",")
        );
    }

    // --- Multistream 5.1, mapping family 1 -------------------------------
    let rate = 48000i32;
    let frame = 960usize;
    let ch = 6usize;
    let n = rate as usize * 2;
    let layout = ChannelLayout::surround(ch, 1)?;
    // Each channel gets a distinct tone so a channel swap is unmistakable.
    let tones = [110.0f32, 220.0, 330.0, 440.0, 550.0, 660.0];
    let per: Vec<Vec<f32>> = tones.iter().map(|&f| sine(rate, n, f, 0.4)).collect();
    let pcm = interleave(&per);

    let mut enc = OpusMSEncoder::new(rate, ch, 1, Application::Audio)?;
    enc.set_bitrate(320_000);

    // The header comes from the encoder, so the stream count, coupled count
    // and channel mapping cannot disagree with what is actually in the packets.
    let head = OpusHead::for_ms_encoder(&enc, rate as u32);
    let path = format!("{dir}/surround51.opus");
    let mut w = OggOpusWriter::new(std::fs::File::create(&path)?, head)?;
    let mut packet = vec![0u8; 8000];
    for chunk in pcm.chunks_exact(frame * ch) {
        let len = enc.encode(chunk, frame, &mut packet)?;
        w.write_packet(&packet[..len])?;
    }
    w.finish()?;
    write_f32(&format!("{dir}/surround51.src.f32"), &pcm)?;
    println!(
        "{:<22} rate={:<5} ch={} frame={:<4} bytes={:<7} streams={} coupled={}",
        "surround51",
        rate,
        ch,
        frame,
        std::fs::metadata(&path)?.len(),
        layout.nb_streams,
        layout.nb_coupled_streams
    );
    Ok(())
}
