# Multi-frame packets: cross-check against libopus 1.6.1

Opus reaches durations no single frame can code by packing several frames behind one TOC byte (RFC 6716 §3.2), and libopus decides the split in `opus_encode_native`. This holds that decision against the reference packet for packet. The CI gates live in `tests/multiframe_packets.rs`, `tests/bitstream_conformance.rs` and `tests/bitstream_stability.rs`.

## Run

Both sides read the *same* PCM file, so they provably see identical bytes — `mfprobe` prints its lines in exactly the form `cmf` prints them, and the comparison is a `diff`.

```sh
reference/build.sh                                                    # once
cargo build --release --manifest-path reference/rust/Cargo.toml
B=reference/work/bin R=reference/rust/target/release M=reference/work/multiframe
mkdir -p $M

$R/dumppcm $M/music48.f32   48000 960000 1
$R/dumppcm $M/music48st.f32 48000 960000 2

$R/mfprobe $M/music48.f32 48000 1 2880 64000 audio 1 > $M/ours.txt
$B/cmf     $M/music48.f32 48000 1 2880 64000 audio 1 > $M/theirs.txt
diff $M/ours.txt $M/theirs.txt && echo identical
```

Arguments are the same for both: `<pcm.f32> <rate> <ch> <frame samples> <bitrate> <voip|audio> <vbr 0|1>`. Each prints one line per packet — length, TOC, mode, packing code, frame count. `mfprobe` also decodes every packet and asserts its length and range state against the encoder's, so a framing difference that also broke the entropy coder fails rather than printing a plausible line.

`dumppcm <out.f32> <rate> <samples> [channels] [music|speech]` writes the suite's own generators, taken from `tests/common/mod.rs` by path. It used to hold a private copy that had gone stale — see [the top-level README](../README.md#what-the-harnesses-got-wrong).

## Results, 2026-08-23

48 kHz, `OPUS_APPLICATION_AUDIO`, VBR, on the suite's `music_like` signal. **Every packet identical**, not just the first: 1,615 packets across the six configurations, matching on length, TOC, mode, packing code and frame count alike.

| duration | channels | bitrate | packets | first packet |
| --- | --- | --- | --- | --- |
| 40 ms | mono | 64 kb/s | 500 | 319 bytes, TOC `f9`, code 1, 2 frames |
| 60 ms | mono | 64 kb/s | 333 | 479 bytes, TOC `fb`, code 3, 3 frames |
| 80 ms | mono | 64 kb/s | 250 | 638 bytes, TOC `fb`, code 3, 4 frames |
| 100 ms | mono | 64 kb/s | 200 | 797 bytes, TOC `fb`, code 3, 5 frames |
| 120 ms | mono | 64 kb/s | 166 | 956 bytes, TOC `fb`, code 3, 6 frames |
| 120 ms | stereo | 96 kb/s | 166 | 1436 bytes, TOC `ff`, code 3, 6 frames |

At 16 kHz through `OPUS_APPLICATION_VOIP` the two once picked different *modes* on this input. That was visible at 20 ms too, where no split is involved, so it was a mode-decision difference rather than a framing one; since fixed, see `docs/interop-validation.md`.
