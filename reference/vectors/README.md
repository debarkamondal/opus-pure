# Official RFC 6716 / RFC 8251 test vectors

The conformance check for the *decoder*. `tests/bitstream_conformance.rs` keeps the strongest signal from it running in CI without the files.

The vectors are ~75 MB and the IETF distributes them separately from libopus for that reason, so they are not in this repository and CI does not fetch them. Re-run this whenever the decoder changes structurally.

## Fetch

```sh
mkdir -p reference/work/vectors && cd reference/work/vectors
curl -sSLO https://opus-codec.org/docs/opus_testvectors-rfc8251.tar.gz
tar xzf opus_testvectors-rfc8251.tar.gz     # -> opus_newvectors/
```

12 streams. `testvectorNN.bit` is the bitstream; `NN.dec` is the reference decode from a **float** libopus and `NNm.dec` from a **fixed-point** one. Both are 48 kHz stereo S16LE.

## Run

```sh
reference/build.sh                       # once
reference/vectors/run.sh                 # both legs; or `mono` / `stereo`
```

**The pass rule takes both references.** A decode passes if `opus_compare` accepts `.dec` *or* `m.dec`. This is libopus's own rule, from its `tests/run_vectors.sh`, and it is not optional: comparing against only the float reference fails vectors that are perfectly conformant — libopus's own decoder fails four of them that way. An earlier version of this recipe made exactly that mistake, and the mono leg it produced was wrong in both directions: it reported failures that were not real, and it had never been trusted enough to expose the one that was.

## Results, 2026-08-23

Against libopus 1.6.1, both references, all 12 vectors, 20,075 packets per leg.

| | stereo | mono |
| --- | --- | --- |
| vectors passing | **12 of 12** | **12 of 12** |
| decode errors | 0 | 0 |
| range-coder mismatches | **0** | **0** |
| `opus_compare` quality, passing vectors | 99.7 – 100.0 % | 96.2 – 99.9 % |

**Stereo is exact where it counts.** Zero range mismatches over every packet means the entropy layer is bit-exact with the reference across the whole mode space the vectors cover, including transitions this crate's own encoder never emits.

For scale, libopus 1.6.1's own `opus_demo` scores 99.6-99.7 % on the stereo comparison. That is not this crate decoding better than libopus: 1.6.1's demo decodes to 24-bit and then rounds with `(s+128)>>8`, a step the 2016 reference files did not have, while `vector_check` converts f32 to S16 the way `opus_decode` does.

### What the mono leg found

Run correctly, the mono leg failed 6 of 12 where libopus's own decoder passed all 12, with zero range mismatches, so the entropy decode was right and the divergence was downstream of it.

This crate rendered the output channel count *after* synthesis: a packet whose channel count differed from the output's was decoded by a second, complete `OpusDecoder` and its channels averaged. libopus keeps one decoder and merges inside each layer instead — `celt_synthesis` sums the two spectra before a single inverse MDCT, and `silk/dec_API.c` emits the mid, which is `(L+R)/2` by construction, without reconstructing L/R at all.

The in-layer merge is close to arithmetically neutral on its own; what failed the vectors was having two decoders. Each saw only the packets that matched it, so both resumed from stale history at every mono/stereo switch. Comparing the old and new mono decodes vector by vector, the error tracks the switch count and nothing else:

| vectors | stereo packets | channel switches | old vs. new |
| --- | --- | --- | --- |
| 12 | 0 % | 0 | identical |
| 01, 11 | 100 % | 0 | 106 dB |
| 02–07 | ~49 % | 1 | 30–63 dB |
| 08, 09, 10 | 52–71 % | 32–64 | 13–16 dB |

One decoder now carries the stream whatever its channel count, and the leg passes 12 of 12.

Nothing had ever exercised this. `tests/bitstream_conformance.rs` runs at the stream's own channel count, and the mono leg here was being run with the wrong pass rule. `tests/decoder_conformance.rs` now pins three SILK configurations against libopus's own downmix, bit for bit, clean and through a concealed packet — that is the steady-state half, and it runs without the vectors; this leg remains the only check on the switching.

## The `.bit` container

Per packet: a 4-byte big-endian length, a 4-byte big-endian copy of the **encoder's** final range-coder state, then the payload. That stored state is the strongest thing in the file. `opus_demo` compares it against the decoder's `OPUS_GET_FINAL_RANGE` for exactly this reason: if the two differ, the entropy decode diverged, whatever the audio sounds like. `vector_check` reports it per packet.

## The tools

Built by `reference/build.sh` into `reference/work/bin`, except `vector_check`, which is a `--bin` of `reference/rust`.

- `vector_check <bit> <out.s16> <rate> <ch>` — decode a `.bit` with **this crate**, reporting per-packet range agreement and writing S16LE for `opus_compare`.
- `crange <bit> <rate> <ch>` — decode a `.bit` with **libopus** and print its final range beside the stored one. This is how you settle whose range state is right when ours disagrees.
- `cenc <pcm.f32> <out.bit> <rate> <ch> <frame> <bitrate>` — encode raw interleaved f32 with libopus into the same container, for comparing encoder behaviour. `cenc_app` is the same with a selectable application.
- `cbw <pcm.f32> <rate> <ch> <frame> <bitrate> <voip|audio> <bandwidth>` — encode with a forced bandwidth and print each packet's TOC config.
- `cvec <rate> <bitrate> <complexity> <auto|nb|mb|wb|swb|fb> <frames>` — regenerate the expected packets in `tests/reference_vectors.rs`, printed as the hex string literals that table holds. It generates the 440 Hz sine itself, mirroring `gen_pcm`, and mirrors `run_forced`'s settings: mono, VOIP, CBR. The TOC configs it saw go to stderr, so a run that silently stopped being SILK is visible. **Built against the fixed-point libopus**, for the reason in [the top-level README](../README.md#the-three-libopus-builds).

## What this run found, historically

An **encoder** defect, since fixed. `celt_encoder.c` codes the digital-silence flag only when CELT owns a fresh range coder (`tell == 1`) and forces `silence = 0` otherwise; `celt_decoder.c` reads it under the same condition. This port gated the write on `tell == 1` but left `silence` set when the branch was skipped, so a digitally silent **hybrid** frame took the silence shortcut, shrank the coder, marked the budget spent, and coded no CELT layer — while every decoder still read one. `crange` settled it: libopus's decoder reported the same range ours did, and both disagreed with what our encoder claimed.

The vectors themselves did not contain that case (they exercise the decoder, and the defect was in the encoder). It surfaced from the *technique* the vectors taught: comparing encoder and decoder range state per packet. `tests/bitstream_conformance.rs` now does that in CI on runtime-generated audio.
