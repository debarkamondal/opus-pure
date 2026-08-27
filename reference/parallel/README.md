# Chunk-parallel encoding, against the serial encode

The one harness here whose reference is not libopus. Chunked encoding is this crate's own addition — libopus has nothing to compare it against — so the thing it has to be held to is the encode *one continuous encoder* would have produced from the same audio. `encode_parallel` splits a clip into contiguous frame ranges, encodes them on separate threads, and primes each worker by re-encoding the audio before its chunk. This measures what survives that priming.

It backs `tests/parallel.rs`.

## Run

```sh
cargo build --release --manifest-path reference/rust/Cargo.toml

reference/parallel/run.sh                    # the standard matrix
W=160 reference/parallel/run.sh              # the same matrix at a different warm-up
reference/parallel/run.sh 48000 1 16000 20 120 speech 2000 4   # one configuration, in full
```

Arguments to a single run are `<rate> <ch> <bitrate> <frame_ms> <seconds> <speech|music> <warmup_ms> <threads> [auto|voice|music]`. The standard matrix encodes every configuration three times over — once serially for the anchor, twice in parallel to check determinism — so it takes a few minutes; `SECS` shortens the clip and `THREADS` changes the split.

A single run prints the plan, whether the output is deterministic, delivered bitrate on both sides, the coding modes each chunk used against the ones the serial encoder used, and the worst per-frame SNR difference.

## Why per chunk and per frame, and not per clip

Averages cannot see either of the things that go wrong.

A chunk boundary costs a few frames: at the cut, one packet was produced by an encoder that did not produce the packet before it, and the rate controllers on either side hold different state. Three bad frames in six thousand move a whole-clip SNR by hundredths of a dB.

The larger failure is not at the boundary at all. The choice between SILK, hybrid and CELT comes from a content analysis whose history is two seconds deep, so a worker primed on less than that decides for itself — and then holds that decision for its **entire chunk**. Averaged over the clip it is a fraction of a dB. To a listener it is a coding mode that has no business being there.

## What this found: the warm-up covered the wrong time constant

`warmup` was 8 frames, and its documentation named what it was sized for: "the deepest inter-frame memory (SILK LTP lag + NSQ delay + CELT overlap)". That is the *signal* path, and 8 frames does settle it — those are filters, and a stable filter forgets its initial conditions in a few frames.

The mode decision is not a filter. `src/analysis.rs` keeps a hundred-entry ring of 20 ms observations (`DETECT_SIZE`) and averages the music/speech probability across it, with exponential averages of the same length underneath; the encoder then applies hysteresis on top. Nothing about that converges in 160 ms, and the result is not a seam.

At 16 kb/s on speech, where the analysis sits closest to its decision threshold, over 120 s split four ways:

| warm-up | worst chunk, frames coded in a mode the serial encoder did not use | worst frame vs serial |
| --- | --- | --- |
| 160 ms (the old default) | 36 of 1500 | −14.82 dB |
| 500 ms | 16 of 1500 | −14.53 dB |
| 1000 ms | 0 of 1500 | −4.10 dB |
| 2000 ms (the default now) | 0 of 1500 | −4.10 dB |
| 3000 ms | 0 of 1500 | −3.94 dB |

Zero is not a guarantee, only what a converged analysis usually produces: the mode decision is hysteretic, and near its threshold a few frames can still go either way. On a 64 s clip of the same audio, where the chunks fall differently against the content, 2000 ms left 16 frames of one chunk in hybrid and 3000 ms cleared them. Going further is not free — the warm-up caps the worker count — so the default is where the systematic error goes away rather than where the last frame does.

Delivered bitrate barely moves across that whole range — 16.34 kb/s at 160 ms against 16.37 at 2000, on a serial 16.37 — which is why nothing had caught it. The old test compared whole-clip SNR against a serial encode with a 3 dB tolerance, and the defect it was watching for costs 0.03 dB by that measure.

### Where it bites

Only where the mode decision is genuinely close. At 120 s, four workers, warm-up 160 ms:

| content | 16 kb/s | 24 kb/s | 32 kb/s | 64 kb/s |
| --- | --- | --- | --- | --- |
| speech | 36 | 21 | 16 | 0 |
| music | 0 | 0 | 0 | 0 |

(frames per chunk of 1500 in a mode the serial encoder did not use)

Music is never ambiguous — it is CELT at every rate here — and speech at 64 kb/s is not either. The damage is confined to the range where a codec actually has to decide, which is the range most speech is carried in.

Pinning the signal type removes it outright: with `signal_type` set to voice or music, a 160 ms warm-up disagrees on nothing, because the analysis is no longer in the decision. That is the cheap way to buy a short warm-up when the content is known.

### What the fix costs

Priming is redundant encoding: every worker but the first re-encodes audio it will not emit. Going from 160 ms to 2000 ms took that from 0.4% of the work to 5.0% at four workers, and it caps the worker count, since a chunk is never allowed to be shorter than four warm-ups. With the default that is one worker per 8 seconds of audio. `ParallelConfig::plan` reports both before any encoding happens.

## What is left, and is not going away

Fully primed, the worst frame still lands about 4 dB below the serial encode's, at a boundary. That is the rate controllers: CELT's VBR reservoir and drift and SILK's bit reservoir are deliberately long-memory integrators, and a worker's is not the continuous encoder's for some frames after its cut. Constant bitrate removes nearly all of it.

This part is inherent. At a chunk boundary one packet was produced by an encoder that did not produce the packet before it, and priming cannot change that — it can only make the two encoders agree about everything except where they started. It is why chunk-parallel encoding is an opt-in path rather than what `OpusEncoder` does on its own.
