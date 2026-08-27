# Reference values for `tests/integer_pcm.rs`

The integer-PCM API is pinned against C libopus 1.6.1. This is the recipe that produced the constants in that test file.

Two of the three builds `reference/build.sh` makes are needed here, and neither is the stock one — see [the top-level README](../README.md#the-three-libopus-builds) for why the encoder comparison needs a fixed-point libopus and the decoder and soft-clip comparisons need a float one with contraction disabled.

## Generate

From the repository root, write the inputs and this crate's own results:

```sh
reference/build.sh                                                    # once
cargo test --release --test integer_pcm -- --ignored --nocapture
```

Then run the reference over the same inputs. The three cases are `CASES` in the test file: 8 kHz mono, VOIP, CBR, 20 frames of 20 ms.

```sh
B=reference/work/bin S=reference/work/s16

# ENCODE_EXPECTED — libopus opus_encode over the 440 Hz reference sine
$B/cs16_fixed enc $S/sine.s16 $S/a.pkt 8000 1 160 10000 5  0 0
$B/cs16_fixed enc $S/sine.s16 $S/b.pkt 8000 1 160 10000 10 0 0
$B/cs16_fixed enc $S/sine.s16 $S/c.pkt 8000 1 160  6000 5  0 0

# DECODE_EXPECTED — libopus opus_decode over *this crate's* packets, so the
# comparison isolates the decoder from any difference in what was encoded
for i in 0 1 2; do $B/cs16_nofma dec $S/case$i.pkt $S/case$i.s16 8000 1 160; done

# SOFT_CLIP_EXPECTED — libopus opus_pcm_soft_clip in 960-sample blocks
$B/cs16_nofma clip $S/over_unity.f32 $S/clipped.f32 2 960
```

Each prints an FNV-1a hash of its output. Those are the constants. All seven currently match what the crate produces, so a mismatch means something moved.

`probe` is the standalone experiment behind the contraction finding: it evaluates `x + a*x*x` five ways — as written, both associations, and `fmaf` — and prints them, which is how the one-ULP difference was pinned on the FMA rather than on the arithmetic.

## Two things worth knowing about the comparison

**20 frames, not more.** Past roughly packet 27 the two encoders' CBR rate-control state drifts apart on a steady tone and the packets stop matching. `tests/reference_vectors.rs` pins 20 frames and does not reach it either. Whatever that is, it predates the integer API.

**Depth 16 and depth 24 code identically on ordinary material.** Both this crate and libopus produce the same packets either way — the two land on the same side of every dynamic-allocation threshold. The declared depth does reach the coder; it takes 12 bits or fewer to see it, which is what `a_declared_depth_below_16_is_honoured` checks.
