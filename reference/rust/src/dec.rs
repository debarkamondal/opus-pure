//! Decode an Ogg Opus file with our reader + decoder to raw f32le at 48 kHz.
#[path = "common.rs"]
mod common;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let inp = args.next().ok_or("usage: dec <in.opus> <out.f32>")?;
    let out = args.next().ok_or("usage: dec <in.opus> <out.f32>")?;
    let (pcm, ch) = common::decode_ours(&inp)?;
    common::write_f32(&out, &pcm)?;
    println!("{inp}: {} samples/ch, {} channels", pcm.len() / ch, ch);
    Ok(())
}
