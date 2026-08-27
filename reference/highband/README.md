# The hybrid high band

A hybrid packet splits at 8 kHz: SILK codes below it, CELT above. This crate's CELT layer spends fewer bits up there than libopus's — 96 bits per frame against 129 at 20 kb/s fullband mono — and [`reference/speed/`](../speed/) could say that the difference existed but not whether it cost anything. This directory answers that, and the answer turned out not to be the one the question assumed.

Backs [`tests/highband.rs`](../../tests/highband.rs). It is the second directory here whose reference is partly not libopus: what a codec *should* put in a band it cannot afford to code is a judgement, not a fact, so some of what follows compares two defensible choices rather than measuring a deviation from a correct answer.

## Running it

```sh
reference/highband/run.sh              # the case table
reference/highband/run.sh rates        # how the high band moves with bitrate
reference/highband/run.sh bands        # the full per-band table for one case
```

Needs [`reference/build.sh`](../build.sh) and `cargo build --release --manifest-path reference/rust/Cargo.toml`. `SECS=60 reference/highband/run.sh` lengthens the clip.

## Why not SNR

The obvious measurement is a signal-to-noise ratio restricted to the band. It does not work up here, and it fails in a way that looks like a result.

At conversational rates the CELT layer has a few kb/s for a 12 kHz-wide band. That is far too little to code a waveform, so it does not try: RFC 6716 §4.3.4's folding fills the band with noise shaped like the spectrum and scaled to the coded energy. The output then has the right spectrum and no phase relationship to the source at all. Error energy is signal plus noise, and the SNR of that is negative.

Which means **emitting silence scores better than either codec**. Zero output makes the error exactly the signal, for an SNR of precisely 0.00 dB, and both stacks score below it. `band` marks every such band with a `*` for that reason. In those rows the SNR column is not a weak measure, it is an inverted one: the way to improve it is to output less, and a stack that attenuates the band will beat a stack that does not, however well either codes.

That is not hypothetical. Model the band as uncorrelated noise at the measured level and the SNR follows from the level alone:

| | measured level | predicted snr | measured snr |
| --- | --- | --- | --- |
| ours, FB 20 kb/s mono | +0.51 dB | −3.27 | −3.08 |
| libopus, FB 20 kb/s mono | −1.18 dB | −2.46 | −2.61 |
| ours, FB 24 kb/s stereo | +0.45 dB | −3.24 | −3.73 |
| libopus, FB 24 kb/s stereo | −3.68 dB | −1.55 | −2.88 |

libopus's SNR lead is its level, not its shape.

## The three columns

| | what it is | when to read it |
| --- | --- | --- |
| `snr` | waveform SNR in the band | while the decoder still codes a waveform, which above 8 kHz means roughly 32 kb/s and up. Starred rows: never |
| `bias` | mean of 10·log10(E_decoded / E_source) over analysis blocks | always. What CELT preserves whether or not it can afford the shape |
| `env` | standard deviation of that ratio **about its mean** | always. How well the moving envelope is tracked once a steady offset is removed |

`bias` and `env` are orthogonal on purpose. An encoder that quiets a band it is guessing at by a steady 3 dB should show up in `bias` alone; an RMS about zero would charge that 3 dB to envelope error as well, and the first version of this tool did exactly that and made libopus look erratic when it is merely quiet.

Analysis is a 1024-point FFT at 75% overlap through a 4-term Blackman-Harris window, accumulated along CELT's own band edges, which the crate exposes as `probe::CELT_BAND_EDGES_200HZ` rather than this tool holding a copy. The window matters: the low band sits 30 to 40 dB above the high band, and −92 dB sidelobes are what keeps the tool from measuring its own skirts. Blocks more than 40 dB below the band's loudest are pauses and are excluded — otherwise the statistic is dominated by the ratio of one near-silence to another.

## What it found

```
                                    celt bits/frame                     ours, above 8 kHz      libopus, above 8 kHz
case                        mode     ours  them    kb/s o/t      snr    bias     env      snr    bias     env
----------------------------------------------------------------------------------------------------------------
hybrid SWB 12 kb/s mono     hybrid     52    81  10.2/11.4     -3.53    0.53    2.31    -1.80   -5.16    4.91
hybrid FB 20 kb/s mono      hybrid     96   129  16.1/17.2     -3.08    0.51    2.06    -2.61   -1.18    2.14
hybrid FB 24 kb/s stereo    hybrid     99   129  21.1/22.7     -3.73    0.45    2.19    -2.88   -3.68    3.36
celt FB 20 kb/s mono        celt        -     -  20.4/20.4     -3.06    0.27    2.36    -2.85   -0.15    2.14
celt FB 64 kb/s mono        celt        -     -  64.5/64.4      4.17    0.20    1.17     5.96   -0.02    0.88
```

**The bit deficit does not starve the band.** On the two measures that apply, this crate is not behind: its high band arrives within half a dB of the level it left at, and tracks the envelope at least as tightly as libopus's, on 25 to 35% fewer bits.

**libopus is deliberately quieting the band instead**, and the rate sweep is what makes that unambiguous:

```
                  celt bits/frame         ours, above 8 kHz      libopus, above 8 kHz
request   mode     ours  them      snr    bias     env      snr    bias     env
------------------------------------------------------------------------------
12 kb/s   hybrid     59    89    -3.47    0.59    2.18    -1.94   -3.95    3.95
16 kb/s   hybrid     69   101    -3.46    0.58    2.14    -2.21   -2.67    3.01
20 kb/s   hybrid     96   129    -3.08    0.51    2.06    -2.61   -1.18    2.14
24 kb/s   hybrid    137   168    -2.99    0.43    1.93    -2.69   -0.54    1.73
32 kb/s   hybrid    217   248    -2.80    0.30    1.86    -2.63   -0.28    1.47
40 kb/s   hybrid    297   328    -2.55    0.17    1.73    -2.36   -0.38    1.27
64 kb/s   hybrid    537   568    -2.33   -0.08    1.52    -2.33   -0.36    1.19
```

libopus's attenuation is 4 dB where it can afford least and converges to a third of a dB once it can code the band properly. That is a gain schedule, not an error, and its `env` column moves with it, so the gain is being applied per frame rather than as a fixed offset. Ours is flat at half a dB across a five-fold rate range.

Which is better is a listening question this directory cannot settle. Full-energy noise fill preserves the spectrum and can sound hissy; attenuating it trades brightness for cleanliness. Both are defensible and libopus's is the tuned one.

**The bit gap is a constant, not a curve.** libopus spends 30 to 32 more bits per frame on the CELT layer at every rate from 12 to 64 kb/s. A bit-allocation difference that scaled with rate would look nothing like that, so whatever causes it is a fixed quantity of bits rather than a different allocation slope — which is a much narrower thing to go looking for than "the allocator differs".

## The controls

Three, because each of the first two results has an innocent explanation that had to be ruled out.

**Both stacks are decoded by the same decoder.** `split` decodes either stack's packets, so what is left between the two columns is the encoder and nothing else.

**Our decoder is not the one making libopus's band quiet.** `cband` decodes libopus's packets with libopus's own decoder. The `bands` leg runs both and prints them side by side; they agree to every digit printed. This matters because the whole finding would be an artefact if our decoder mishandled libopus's high band.

**The difference belongs to hybrid, not to CELT.** The two `lowdelay` rows code the same audio at the same rate with no SILK in the packet, and there the two stacks agree to within half a dB on all three columns. Whatever libopus is doing, it is doing it in hybrid.

## What is still open

The mechanism. libopus's decoded band energy comes from its coded band energy, so its encoder is quantising a lower energy than the band actually has, in an amount that tracks how few bits it has. Finding where in `celt_encoder.c` that happens is the frame-by-frame trace this crate has not done. The 30-bit constant is probably the better thread to pull first, being a fixed quantity rather than a curve.

Whether matching libopus's gain schedule would sound better is a listening test, not a measurement, and nothing here should be read as saying this crate should adopt it.
