# Reference harnesses

Every claim this crate makes about matching libopus was produced by a program in this directory. They are here so those claims can be re-derived rather than taken on trust: each builds the real C library, runs it over the same input this crate is given, and compares. Two are not straight comparisons: [`parallel/`](parallel/) measures something libopus does not do, and [`highband/`](highband/) reaches a band where libopus and this crate make different but equally defensible choices, so it reports both rather than scoring one against the other.

Two of them were themselves wrong when they were collected here, and had been measuring the wrong thing for months without failing; both are described in [What the harnesses got wrong](#what-the-harnesses-got-wrong).

## Quick start

```sh
reference/build.sh                    # fetch libopus 1.6.1, build it three ways, build the harnesses
cargo build --release --manifest-path reference/rust/Cargo.toml
```

About five minutes the first time, seconds after that. Everything produced lands in `reference/work/`, which is gitignored; nothing here writes into a tracked directory. Run every command from the repository root.

## The three libopus builds

Which build is the reference depends on what is being compared, and reaching for the wrong one measures the wrong thing. `build.sh` produces all three.

| build | what it is | when it is the reference |
| --- | --- | --- |
| `build` | float, stock | anything CELT or hybrid; `opus_demo` and `opus_compare` |
| `build-fixed` | `OPUS_FIXED_POINT=ON` | anything SILK. SILK is fixed point in both builds, but only this one takes the SILK **encoder** path this crate implements |
| `build-nofma` | float, `-ffp-contract=off` | the soft-clip curve |

The last one is not fussiness. `opus_pcm_soft_clip` computes `x + a*x*x`, which clang at `-O2` on arm64 fuses into an FMA and Rust never does. Measured against a stock build, 367 of 7680 samples differ by exactly one ULP; against this one, zero do. The unfused form is what the C source literally says.

## What is here

| directory | holds against the reference | backs |
| --- | --- | --- |
| [`vectors/`](vectors/) | the official RFC 6716 / RFC 8251 decoder vectors | `tests/bitstream_conformance.rs` |
| [`plc/`](plc/) | the decoder, and concealment, sample for sample | `tests/decoder_conformance.rs` |
| [`interop/`](interop/) | a 440-configuration encode sweep, decoded by both stacks | `docs/interop-validation.md` |
| [`multiframe/`](multiframe/) | multi-frame packet framing, packet for packet | `tests/multiframe_packets.rs` |
| [`s16/`](s16/) | the integer PCM API and soft clip | `tests/integer_pcm.rs` |
| [`speed/`](speed/) | encode and decode throughput, on identical audio | `benches/throughput.rs` |
| [`highband/`](highband/) | what a hybrid packet's CELT layer delivers above 8 kHz | `tests/highband.rs` |
| [`parallel/`](parallel/) | chunk-parallel encoding, against the serial encode | `tests/parallel.rs` |
| [`rust/`](rust/) | every Rust-side tool, in one cargo package | — |

`cvec`, in [`vectors/`](vectors/), also regenerates the expected packets in `tests/reference_vectors.rs`.

[`parallel/`](parallel/) is the one directory whose reference is not libopus at all. Chunked encoding is this crate's own addition and the C library has nothing to compare it against, so what it is held to is the encode one continuous encoder would have produced from the same audio. That is what caught the warm-up described there being sized for the wrong time constant.

[`highband/`](highband/) does compare against libopus, but it is the one place where a difference is not automatically a defect. Above 8 kHz a hybrid packet has too few bits to code a waveform, so the band is filled with shaped noise and what remains to choose is how loud to make it. libopus quiets it in proportion to how little it could afford; this crate holds the source's level. Both are defensible, so that directory reports the two side by side rather than treating libopus's as the answer. It also documents why the obvious measurement, a band-limited SNR, ranks silence above both codecs and cannot be used.

[`speed/`](speed/) is the one directory here that does not ask whether this crate is correct. It asks what it costs, which is the question `benches/throughput.rs` cannot answer on its own: an absolute throughput number means little without the C library's number beside it on the same machine and the same audio. It reports delivered bitrate alongside the timings, which is how the hybrid rate split described in that directory's README was found: encode speed alone would not have shown it. Both stacks also decode without encoding, which is what makes a decoder profile readable, and comparing the two profiles over the same packets is how the decode overheads described there were found.

The Rust tools live together rather than one per topic directory so that a single `cargo build --manifest-path reference/rust/Cargo.toml` compiles all of them against the current crate: a tool that stops building is then a build failure rather than a surprise. They share `common.rs`, which pulls the signal generators from `tests/common/mod.rs` by path — so a harness encodes exactly what `cargo test` encodes.

## Checking every frozen value at once

```sh
reference/verify.py
```

Re-derives all 28 frozen libopus constants in the test suite — the seven in `tests/integer_pcm.rs` and the twenty-one in `tests/decoder_conformance.rs` — by running the C library over the same inputs and comparing. It reads the expected values out of the test sources rather than carrying a copy, so it cannot drift from what the tests assert. All 28 reproduce as of 2026-08-23.

`tests/reference_vectors.rs`'s 22 configurations are checked with `cvec`, one configuration per run, since its expected values are the packet bytes themselves; see [`vectors/`](vectors/).

## Regenerating a frozen value

The tests that hold frozen hashes each carry an `#[ignore]`d dumper that writes their inputs where these tools can reach them:

```sh
cargo test --release --test integer_pcm         -- --ignored --nocapture
cargo test --release --test decoder_conformance -- --ignored --nocapture
```

Then follow the recipe in the matching directory. Only regenerate when the **encoder** changed on purpose: a decoder change that moves these hashes is the thing those tests exist to catch.

## What the harnesses got wrong

Two defects in the harnesses themselves, both of which had been quietly producing misleading results:

**Three tools carried stale copies of the signal generators.** `dumppcm`, `mfprobe` and `dec_compare` each had a private `music_like` that was an older revision of the suite's — a four-harmonic tremolo where the suite now generates a six-partial chord — and used `f32::sin`, which the suite had already abandoned because it is libm-dependent and therefore differs between architectures. So the multi-frame comparison was fed audio no test had encoded in months, and its two sides were not even fed the same audio: `dumppcm` built stereo as `(s, s*0.8)` and `mfprobe` as `(s, s)`. They now share `common.rs`, and `mfprobe` reads the same PCM file `cmf` does instead of regenerating it.

**The RFC vector mono leg had never actually been run.** Each vector ships two reference decodes, `.dec` from a float libopus and `m.dec` from a fixed-point one, and libopus's own `run_vectors.sh` passes a decode that matches *either*. The recipe here compared against only `.dec`, which fails vectors that are perfectly conformant. Running it correctly, with `reference/vectors/run.sh`, surfaced a real defect in the crate: a packet whose channel count differed from the output's was decoded by a second, parallel decoder, so no decoder saw the whole stream and both resumed from stale history at every mono/stereo switch. It is fixed, and both legs now pass 12 of 12 — see that directory's README.
