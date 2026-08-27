# Interop sweep against libopus 1.6.1 (direction A)

Encodes a matrix of configurations with this crate, decodes every stream twice — once with `opusdec` from opus-tools, once with this crate's own decoder — and compares the two decodes sample for sample. It answers one question: *does a real libopus decoder get from our bitstream what we get from it?*

Direction B (encode with `opusenc`, decode with us) is covered by [`../vectors/`](../vectors/); the decoder is pinned bit-exactly by [`../plc/`](../plc/) and `tests/decoder_conformance.rs`.

## Requirements

`opus-tools` 0.2 or later on `PATH` (`opusdec`, `opusinfo`) and `python3`. No libopus build is needed: this sweep goes through the installed `opusdec` rather than linking the library.

```sh
brew install opus-tools     # or the distro equivalent
```

## Running it

```sh
reference/interop/run_sweep.sh
```

About four minutes. It regenerates every `.opus`, runs `opusinfo` over all of them, decodes each with both stacks, and prints the two tables that `docs/interop-validation.md` quotes. Nothing is cached, so the numbers in that document can always be re-derived from a clean tree.

## The matrices

The generators are `--bin`s of [`../rust/`](../rust/); `run_sweep.sh` builds them.

The generators are `--bin`s of [`../rust/`](../rust/), which `run_sweep.sh` builds.

| binary | files | matrix |
| --- | --- | --- |
| `sweep` | 120 | 20 ms, 5 rates x mono/stereo x 6 forced bandwidths x 2 applications, 48 kb/s, music |
| `focus` | 160 | 20 ms, 5 rates x mono/stereo x 4 bitrates x speech/music x 2 applications, bandwidth auto |
| `sixty` | 160 | 60 ms, 5 rates x mono/stereo x 4 bandwidths x 2 applications x 2 bitrates, music |

Signals come from `tests/common/mod.rs`, included by path rather than copied, so the sweep encodes exactly what `cargo test` encodes. That file's generators are built from correctly-rounded arithmetic, so these results do not depend on the host's libm — an earlier copy of them lived here and went stale, which is why it is a path include now.

## What the report measures

`report.py` prints, for each matrix:

- **bit-identical** — every sample equal. This tracks pure-SILK streams exactly, and the report asserts that it does: SILK is fixed-point on both sides, so it either matches to the bit or something is wrong. CELT and hybrid are float and land at float32 rounding instead.
- **better than 100 dB SNR** — the working threshold for "two independent implementations of the same transform".
- **widest window of differing samples** and **streams differing away from a mode switch**. The second is the load-bearing one. A stream that changes coding mode cross-fades over 5 ms, and our concealment is not bit-exact, so disagreement there is expected; disagreement anywhere else is a defect. The report locates every differing sample and checks it against the stream's own switch points, so "only at the seam" is measured rather than assumed.

`TOL` is 1e-5, about two orders above the float32 rounding these signals produce (~2e-7).

## Other tools here

- `report.py <out_dir> <src_dir> <samples_per_packet>` — the summary above for any decoded matrix.
- `oggmode.py '<glob>'` — the mode sequence of each file's packets, from their TOC bytes.
- `seam.py <config_name> <channels>` — every run of differing samples in one stream, with its offset from the nearest mode switch. This is the tool to reach for when `report.py` reports a stray.
- `compare.py [dir]` — the older flat per-file table (max diff, SNR, bit-exact %). Still useful for eyeballing one directory.
- `dec` — decode one `.opus` to raw f32le at 48 kHz through our reader and decoder, applying pre-skip and end trimming the way `opusdec` does.
- `plc <pkt_out> <pcm_out> <rate> <ch> <bitrate> <bw> <app> <signal> <frame_ms> <frames> [lost,...]` — encode a stream, write its packets, and decode them with the given ones dropped. Paired with `cplc`, which does the same through libopus, it is how concealment is held against the reference; [`../plc/run.sh`](../plc/run.sh) drives both.
- `one`, `ms`, `qual`, `stereoprobe`, `stereoqual`, `gen` — single-config probes kept from earlier investigations.

## Reading a failure

A stray (a stream differing away from a switch) is the signal that matters. Run `seam.py` on it: if the differing run sits at a constant offset from the start, suspect a delay difference and check `tests/encoder_delay.rs`; if it is spread through the stream, suspect the coder itself. `blockdiff.py` and `diffmap.py` break a difference down per packet.

A length mismatch means the granule arithmetic disagrees, not the audio — check the final page's granule against the sum of the packets' own durations.
