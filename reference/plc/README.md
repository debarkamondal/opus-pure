# Decoder cross-check against libopus 1.6.1

Holds this crate's decoder against the real C decoder on identical packets. `tests/reference_vectors.rs` pins the *encoder* byte-exactly; this is what generates the frozen values in `tests/decoder_conformance.rs`, for both the clean path and concealment.

## Run

```sh
reference/build.sh                                                    # once
cargo build --release --manifest-path reference/rust/Cargo.toml

reference/plc/run.sh 16000 1 24000 wb voip speech 20 100 60             # SILK: expect 100% bit-identical
CPLC=cplcf reference/plc/run.sh 48000 1 64000 fb audio music 20 100 60  # CELT: expect >100 dB
reference/plc/run.sh 16000 1 24000 wb voip speech 20 100 40,41,42 frames  # per-frame breakdown
```

Arguments are `<rate> <ch> <bitrate> <bw> <app> <signal> <frame_ms> <frames> <lost,...> [frames]`. `run.sh` encodes with this crate, decodes the packets with the given ones dropped, runs libopus over the same file with the same drops, and compares.

**Two libopus builds, because the layers differ.** SILK decodes in fixed point in *both* builds, so `cplc` (fixed) is the reference for anything SILK; CELT is float in a float build and fixed in a fixed one, so `cplcf` is the reference for anything CELT or hybrid. Comparing our float CELT against a fixed-point libopus measures the wrong thing.

## Regenerating `tests/decoder_conformance.rs`

All three frozen tables come from the same packets, and the test writes them for you:

```sh
cargo test --release --test decoder_conformance -- --ignored --nocapture
```

That prints one file per configuration under `reference/work/plc/`, plus the loss list each `FROZEN_PLC` row needs. Then, per row:

```sh
B=reference/work/bin W=reference/work/plc

# FROZEN.pcm — libopus's clean decode
$B/cpcm $W/wb_mono.pkt $W/out.pcm 16000 1 320
python3 reference/plc/pcmhash.py $W/out.pcm

# FROZEN_PLC.pcm — the same packets, same drops, through libopus
$B/cplc $W/wb_mono.pkt $W/out.pcm 16000 1 320 10,11,12
python3 reference/plc/pcmhash.py $W/out.pcm

# FROZEN_DOWNMIX.pcm — a stereo stream, one output channel. `cpcm` and `cplc`
# take the decoder's channel count, which libopus lets differ from the stream's,
# so this needs no separate tool: pass 1 against a `*_stereo.pkt`.
$B/cpcm $W/wb_stereo.pkt $W/out.pcm 16000 1 320        # clean
$B/cplc $W/wb_stereo.pkt $W/out.pcm 16000 1 320 20     # with a packet concealed
python3 reference/plc/pcmhash.py $W/out.pcm
```

Frame size is `rate / 50`. `pcmhash.py` is FNV-1a over the samples as little-endian i16, matching `pcm_hash` in the test file — including Rust's round-half-away-from-zero, which Python's built-in `round` does not do.

Only regenerate when the *encoder* changed deliberately; a decoder change that moves these hashes is the thing that file exists to catch. The `packets` hash in `FROZEN` guards that the inputs are still the right ones, so it moves first and the failure says so.

All 21 frozen values — 6 clean, 9 concealed, 6 downmixed to mono — reproduce from a fresh `reference/build.sh` as of 2026-08-23.

## The tools

Built into `reference/work/bin` by `reference/build.sh`.

- `cpcm <pkt_in> <pcm_out> <rate> <ch> <frame>` — decode with libopus, dump raw f32.
- `cplc <pkt_in> <pcm_out> <rate> <ch> <frame> [lost,...]` — the same, concealing the given packets. `cplcf` is the float-build twin.
- `cdec <pkt_in> <rate> <frame> <lose_from> <lose_count>` — per-frame energy rather than PCM. Enough to say concealment is *producing* something, not enough to say it is producing the right thing, which is why `cplc` exists.
- `plc` (a `--bin` of `reference/rust`) — the crate's side: encode a stream, write the packets, decode them with the given ones dropped.

Python, all reading raw f32:

- `cmp.py a.pcm b.pcm <frame>` — SNR, peak difference, percentage bit-identical, and where the first difference is.
- `frames.py a.pcm b.pcm <frame> [all]` — the same per frame, which is what tells a concealed frame apart from the recovery after it.
- `lag.py a.pcm b.pcm <max_lag>` — constant lag and sign-flip search.
- `pcmhash.py a.pcm` — the frozen hash.

`lag.py` is what turned an apparent catastrophe into a one-line diagnosis: raw SNR was -1.2 dB, but correlation at lag -12 was +1.0000 with infinite SNR, so the two decoders agreed bit for bit and differed only in delay.

## Findings

**One decoder defect, since fixed.** `render_silk_frame` skipped the SILK resampler whenever the API rate already equalled SILK's internal rate, and three of the four `silk_resampler.init` sites were guarded on the same condition. libopus's `USE_silk_resampler_copy` path is not a plain copy: it routes through `delayBuf` and applies `delay_matrix_dec`, whose diagonal is 4, 9 and 12 samples at 8, 12 and 16 kHz. Every SILK-only stream therefore came out that many samples early. Now bit-identical (100%, infinite SNR) at 8/12/16 kHz, mono and stereo.

**A second, found by the vector suite and pinned here.** Decoding a stereo stream to a mono output rendered the channel count after synthesis — a second complete decoder, its two output channels averaged — where libopus merges inside each layer ahead of synthesis, keeping one decoder. The `FROZEN_DOWNMIX` rows hold the steady-state half of that against libopus's own downmix, bit for bit, clean and through a concealed packet, because the vector suite needs the vectors themselves and CI does not have them. The other half, two decoders resuming from stale history at each mono/stereo switch, needs a switching stream and stays with `reference/vectors/run.sh`.

CELT at 48 kHz and hybrid at 24 kHz were already right, agreeing to 139-157 dB — float rounding in the f32 output path, not drift.

**Why no test caught it.** Every fidelity test measured the decoder against the *encoder's input* through `aligned_correlation`, which searches lags and reports the best one. A constant output delay is exactly what that search removes.

**Concealment** matched *in energy* long before it matched in samples — concealed frames land within 0.7-1.1x of libopus at every position across SILK (8/16 kHz), hybrid (24 kHz) and CELT (48 kHz), and SILK's fast fade to silence is libopus's design rather than a defect here. Measuring the samples themselves later found four SILK defects and a CELT one, none of which moved a concealed frame's energy: they all showed up in the frames *after* it. See `docs/interop-validation.md`, "Packet-loss concealment".
