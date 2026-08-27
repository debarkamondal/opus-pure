# Speed: this crate against libopus 1.6.1

Every other directory here asks whether this crate is *correct* against the reference. This one asks what it costs. `benches/throughput.rs` already measures encode and decode throughput, but an absolute number is hard to read: 200x realtime is only good or bad relative to what the C library does on the same machine with the same audio. This supplies the other column.

## Run

```sh
reference/build.sh                                                    # once
cargo build --release --manifest-path reference/rust/Cargo.toml
reference/speed/run.sh                    # every case
reference/speed/run.sh silk               # only labels containing "silk"
reference/speed/run.sh '' 12              # 12 passes instead of 5
```

`rspeed` and `cspeed` take the same argument list and print the same six fields, so `run.sh` calls either without knowing which:

```text
<pcm.f32> <rate> <ch> <frame> <bitrate> <complexity> <voip|audio|lowdelay>
          <auto|nb|mb|wb|swb|fb> <auto|voice|music> <reps>
-> enc_us_per_frame  enc_xrt  dec_us_per_frame  dec_xrt  kb/s  modes
```

`rspeed gen` writes the source audio, and both stacks read that one file, so they provably encode identical samples rather than two generators that agree today.

Both also decode without encoding, which is what a profiler needs. Encoding outweighs decoding by between two and eleven times in every case here, so a sampling profiler pointed at a full run reports almost entirely on the encoder:

```sh
$R/rspeed dec $W/music2.f32 48000 2 960 128000 9 audio fb music 3000 &
sample $! 15 1 -f prof.txt          # macOS; perf record on Linux
$B/cspeed  -d $W/music2.f32 48000 2 960 128000 9 audio fb music 4000 &
```

`rspeed dec` takes exactly `run`'s arguments and `cspeed -d` takes a leading flag so every positional index is unchanged. Both print only the decode columns, and both reuse the same timed loop the full run uses, so a profile can be trusted to describe the row it came from: measured against the matching `run` row, the two agree to 0.2%.

Both also take an optional trailing path and dump their packets there, length-prefixed, which is what `split` reads:

```sh
B=reference/work/bin R=reference/rust/target/release W=reference/work/speed
$R/rspeed run $W/speech1.f32 48000 1 960 20000 9 voip fb voice 1 $W/ours.pkt
$B/cspeed     $W/speech1.f32 48000 1 960 20000 9 voip fb voice 1 1 $W/theirs.pkt
$R/split $W/ours.pkt   48000 1 960          # -> silk bits/frame, celt bits/frame
$R/split $W/theirs.pkt 48000 1 960
```

`cspeed` takes the VBR-constraint argument before the path; `rspeed` has no such setting, so its path comes straight after the pass count. `split` reports how a hybrid packet divides between its two layers, which is not otherwise observable: the two share one range-coded stream. It reads the boundary out of this crate's decoder, under the crate's non-default `probe` feature, and because that decoder matches libopus bit for bit on hybrid it measures the reference's split just as well as our own.

## What makes it a fair comparison

A speed number is easy to get and easy to get wrong, so the things that would quietly bias it are pinned:

**The methodology is transcribed, not reimplemented.** `cspeed.c` uses the same shape `benches/throughput.rs` uses: the fastest of N passes rather than the mean, since benchmark noise on a general-purpose machine can only ever make a pass slower; a fresh encoder for every pass, built outside the timed region, because encoder state evolves and a warmed encoder measures something the first pass never sees; packets collected in a separate untimed pass, so no allocator work lands in the measurement; and the return value checked inside the timed loop, because an error there would otherwise read as speed.

**`rspeed` is checked against the benchmark it mirrors.** Running `cargo bench -- --filter "celt FB"` and the matching `run.sh` rows agree to about 1%, inside the 0.7%-average, 3%-worst run-to-run noise that benchmark documents. If the two ever diverge, the harness has drifted from the instrument it is supposed to reproduce.

**Both stacks get their SIMD.** libopus is built `CMAKE_BUILD_TYPE=Release` with `OPUS_USE_NEON=ON` and runtime dispatch, and `celt/arm/armcpu.c` has an `__APPLE__` branch that reports NEON, so its kernels are compiled *and* reached. This crate builds at cargo's default release settings with no `RUSTFLAGS` and no `.cargo/config.toml`, where NEON is baseline for aarch64. Neither side gets `-march=native`. This is the matchup a caller actually faces: `cargo add opus-pure` against a stock libopus.

**Settings are matched, including the ones that are easy to miss.** `OPUS_SET_LSB_DEPTH(24)` is the float-API default on both sides. Complexity, bitrate, application, forced bandwidth and signal type come from the case. `OPUS_SET_VBR_CONSTRAINT` defaults to 1 in libopus and this crate has no equivalent, so `cspeed` takes it as an optional 11th argument; measured, it changes nothing in SILK or hybrid at these rates and moves CELT by 4% at 96 kb/s and higher.

## The two libopus columns

There is no single fair opponent, because this crate implements SILK's **fixed-point** encoder while a float libopus uses SILK's float encoder. So the harness builds `cspeed` twice and reports both:

| column | build | answers |
| --- | --- | --- |
| `libopus` | `build`, float | *what does choosing this crate over the C library cost me?* This is what a caller links: distributions ship the float build. |
| the second table | `build-fixed` | *is our port of this algorithm slower than the same algorithm in C?* Only meaningful for SILK and hybrid **encode**; SILK decode is fixed point in both builds, and CELT is float in the one that matters. |

The two coincide for CELT, where float is both what a caller links and the same algorithm. Hybrid is the awkward case: it is our fixed-point SILK plus our float CELT, which no single libopus build matches, so neither column is exactly like-for-like there.

## Results, 2026-08-23

Apple M1 Pro, 48 kHz, 20 ms frames, complexity 9 unless the label says otherwise, 8 s of audio per case, fastest of 12 passes. Ratios above 1.00x mean this crate is faster.

| case | mode | encode ours | libopus | ratio | decode ours | libopus | ratio |
| --- | --- | --- | --- | --- | --- | --- | --- |
| silk NB 8 kb/s mono | silk | 218x | 195x | **1.12x** | 3432x | 3958x | 0.87x |
| silk MB 24 kb/s mono | silk | 162x | 148x | **1.09x** | 2194x | 2568x | 0.85x |
| silk WB 20 kb/s mono | silk | 129x | 116x | **1.12x** | 1902x | 2201x | 0.86x |
| silk WB 32 kb/s stereo | silk | 67x | 61x | **1.11x** | 985x | 1084x | 0.91x |
| hybrid SWB 12 kb/s mono | hybrid | 121x | 105x | **1.15x** | 1209x | 1315x | 0.92x |
| hybrid FB 20 kb/s mono | hybrid | 119x | 104x | **1.15x** | 1056x | 1175x | 0.90x |
| hybrid FB 24 kb/s stereo | hybrid | 62x | 55x | **1.14x** | 639x | 629x | **1.02x** |
| celt FB 64 kb/s mono | celt | 311x | 232x | **1.34x** | 996x | 1187x | 0.84x |
| celt FB 128 kb/s stereo | celt | 204x | 116x | **1.76x** | 532x | 615x | 0.86x |
| celt FB 256 kb/s stereo | celt | 178x | 105x | **1.70x** | 386x | 452x | 0.86x |

Against the fixed-point build, where the SILK encoder is the same algorithm on both sides: 0.97x, 1.04x, 1.08x, 1.11x for the four SILK rows and 1.09x, 1.09x, 1.13x for the hybrid ones. Our SILK encoder is within a few percent of C's either way, which is about what a faithful port with equivalent kernels should look like.

### Encode is faster, decode is slower

Both directions are consistent enough across modes to be structural rather than incidental.

The **encode** advantage is largest in CELT (1.34–1.76x) and small in SILK (1.09–1.12x). That ordering is what the SIMD work predicts: CELT's hot kernels are the ones this crate has hand-written NEON for, while SILK's encoder is dominated by the fixed-point delayed-decision quantiser, where there is less to win.

**Decode** runs at 0.84–1.02x, and has been 0.61–0.72x and then 0.70–0.87x on the way here. Two rounds of profiling closed it, both below. The deficit used to be largest in SILK and smallest in CELT, the reverse of the encode ordering; it is now flat across the three modes, and stereo hybrid is the first decode row to pass the reference.

### Complexity behaves differently in the two stacks

| | complexity 0 | 5 | 10 |
| --- | --- | --- | --- |
| silk WB 20 kb/s mono | 0.97x | 0.95x | 1.15x |
| celt FB 96 kb/s stereo | 1.41x | 1.28x | 1.65x |

In SILK this crate is slightly *slower* than libopus below the default and faster at 10, so the port's cost curve across the complexity range is not quite the reference's. In CELT it leads throughout.

## What this found: the decoder was doing work the reference does not

The flat decode deficit was the other thing the harness surfaced, and it stayed unexplained for as long as there was no way to look at decode on its own. `rspeed dec` and `cspeed -d` supply that, and profiling both stacks over the same audio decomposes the gap rather than merely confirming it. On `celt FB 128 kb/s stereo`, where the whole gap was 15.3 us per frame:

| | ours | libopus | gap | share |
| --- | --- | --- | --- | --- |
| de-emphasis and the range decoder | 10.9 us | 5.6 us | +5.3 | 35% |
| PVQ pulse decode | 13.4 | 9.4 | +3.9 | 26% |
| zeroing buffers | 2.5 | 0.2 | +2.3 | 15% |
| band machinery | 7.7 | 6.1 | +1.5 | 10% |
| the decoder's own output plumbing | 1.1 | 0.1 | +1.0 | 7% |
| `memmove` | 1.0 | 0.2 | +0.8 | 5% |
| MDCT, comb filter, `exp_rotation`, energy, allocation | 10.2 | 10.3 | −0.1 | −1% |
| FFT | 2.7 | 3.0 | −0.2 | −1% |

The last two rows are the useful ones: the DSP kernels are at or better than parity, and the FFT is faster. The gap was not in the signal processing. It was in four things around it.

**The de-emphasis recursion, the single largest item.** Three separate faults in one loop. It carried `i % downsample` and `i / downsample` on a divisor the compiler cannot see, so every output sample paid two integer divisions; the reference decides which samples survive downsampling *outside* the recursion for exactly that reason. It clamped each sum to `SIG_SAT`, transcribed from the reference's `SATURATE(x, SIG_SAT)` — which `arch.h:323` defines as the identity in a float build, so the clamp could never change a sample, and it sat between the add and the multiply on a serial dependency chain, roughly doubling its length. Dropping it alone took 3.5 us off a 47.9 us frame. And it ran one channel at a time, where the reference keeps `deemphasis_stereo_simple` for the common case because the two channels are independent and interleaving them lets one fill the other's stalls.

**Zeroed buffers that were overwritten in full.** `alg_unquant` built a fresh `[0i32; MAX_PVQ_N]` on every call — 1.4 KB, memset once per PVQ partition per band per channel per frame, and immediately overwritten by the pulse decoder. The reference takes the same buffer from its stack allocator, which costs nothing. It is now owned by `quant_all_bands` and threaded through `BandCtx`, so it is created once per frame. Two more in the synthesis path: a `fill(0.0)` immediately followed by a full `copy_from_slice`, and a zeroed temporary the postfilter history was staged through, now an in-place `copy_within`.

**A bounds test inside the per-sample loops.** The SILK output path tested `idx < output.len()` on every sample of every conversion loop, which cost the branch and any chance of vectorising the i16-to-float conversion. It is one check now, taken before the loop.

**Two output conventions in one decoder.** `decode_from_range_coder_with_band_range` wrote planar while `conceal_lost_bands` wrote interleaved, so every CELT consumer staged through a scratch buffer and repacked: the CELT-only stereo path, the hybrid sum, the redundant frame and its two cross-fades. The reference writes interleaved straight out of de-emphasis. Now so does this, and the staging buffer, the repack loops and the CELT-only path's mono/stereo split are gone.

The result, on the rows above: 0.61–0.72x becomes 0.70–0.87x. Fullband stereo decode went from 420x realtime to 532x and stereo hybrid from 424x to 543x.

### What is left of it

CELT is the closer half at 0.84–0.86x, and the remaining gap there is mostly PVQ pulse decode — a tight `cwrsi` loop in both stacks, where we are 1.4x. SILK was further out at 0.70–0.76x, half of it in `silk_decode_core` at 1.7x, which resisted the obvious fix: the reference writes its short-term prediction out tap by tap and asserts the order is 10 or 16, and matching that with a const-generic unroll bought almost nothing, because the disassembly was already two instructions per tap with no bounds checks. That one is the next section.

None of this changed a decoded sample. The RFC vectors pass 12 of 12 over 20,075 packets with no range mismatches, and the 440-configuration interop sweep is identical to its recorded baseline, down to the same seven configurations below 100 dB at the same SNRs.

## What this found: `silk_decode_core` was queueing behind its own output

The unrolled tap loop was the right thing to look at and the wrong thing to blame. What settled it was attribution *inside* the function: temporarily marking its four phases `#[inline(never)]` gives the sampler four symbols where it had one.

| | us/frame |
| --- | --- |
| short-term LPC synthesis | 4.60 |
| excitation | 0.36 |
| the rest of the function | 0.28 |
| long-term prediction | 0.21 |

(Forcing the phases out of line makes the whole decode about 3% faster, so these sum to 5.45 where the function as normally compiled measures 5.98. The proportions are the point: one loop is 84% of it.)

At 320 output samples in a 20 ms wideband frame, 4.60 us is 46 cycles per sample for sixteen multiply-accumulates. The multiplies are not the problem and neither are the loads. The additions are.

This filter is recursive: every sample it writes becomes the newest history sample for the next one. The reference lists its taps newest first and this crate's loop followed that order, so the addition consuming the previous output came *first* and the other fifteen queued up behind it. All sixteen sat on the loop-carried dependency, each waiting on the one before, while the products they were adding had been ready for cycles.

Reversing the loop, so the newest tap goes in last, is the entire fix. Each product depends only on the history and the coefficients and never on the running total, and two's complement addition is associative, so the sum is unchanged to the bit; but the fifteen older taps now run ahead of the recursion instead of behind it. `silk_decode_core` went from 5.98 us/frame to **3.23**, against the reference's 3.47.

libopus does not pay this because clang reassociates. Its taps are written out one per line, so they reach LLVM as straight-line code and the pass rebuilds them into a summation tree — visible in its disassembly as partial sums merging rather than one register threaded through sixteen adds. A `for` loop is unrolled after that pass has run, so the order the source gives is the order that reaches code generation. Writing the tree out by hand in Rust also works and is 4% *slower* than simply reordering: LLVM flattens it back into a chain either way, and only the position of the recursive tap ever mattered.

Every SILK and hybrid decode row moved:

| | before | after |
| --- | --- | --- |
| silk NB 8 kb/s mono | 0.75x | 0.87x |
| silk MB 24 kb/s mono | 0.76x | 0.85x |
| silk WB 20 kb/s mono | 0.70x | 0.86x |
| silk WB 32 kb/s stereo | 0.71x | 0.91x |
| hybrid SWB 12 kb/s mono | 0.79x | 0.92x |
| hybrid FB 20 kb/s mono | 0.79x | 0.90x |
| hybrid FB 24 kb/s stereo | 0.86x | **1.02x** |

Nothing decoded differently: the RFC vectors pass 12 of 12 over 20,075 packets with every printed SNR unchanged, and the interop sweep matches its baseline exactly.

The same shape is in three other places — `silk/cng.rs`, `silk/plc.rs`, and the encoder's `silk/nsq_del_dec.rs`. No case in this harness reaches the first two, which run only on a lost packet or in DTX, and the encoder is already ahead of the reference; none of the three has been touched.

### What is left of it now

SILK decode is now limited by the resampler. From the same profile of `silk WB 20 kb/s mono`:

| | ours | libopus |
| --- | --- | --- |
| `silk_resampler_private_up2_hq` | 1.97 us/frame | 1.26 |
| `silk_resampler_private_IIR_FIR` | 1.40 | 1.09 |

That is 1.0 us of the 1.4 us still between the two stacks on that row. Neither has been looked at.

## What this found: the hybrid rate split was wrong

The harness reports delivered bitrate for both stacks and marks any row where they differ by more than 5%, because a row coding more bits is being timed on more work. That mark was the more interesting result. As first measured:

| mode | ours vs target | libopus vs target |
| --- | --- | --- |
| CELT | +0 to +1% | +0 to +1% |
| SILK | −10 to −24% | −15 to −30% |
| hybrid | **+12 to +15%** | **−5 to −14%** |

CELT landed on target on both sides. SILK undershot on both. Hybrid was the outlier, overshooting where the reference undershot, about 20 percentage points apart.

Chasing it turned up five separate divergences from `opus_encoder.c`, none of them visible from the bitstream, because a wrong split still decodes correctly:

1. `compute_silk_rate_for_hybrid` had no FEC columns. The reference's table carries a wider SILK share for FEC frames, since the redundant copy costs real bits.
2. It had no channel handling at all. libopus allocates **per channel** — divide the total, read the table at the single-channel rate, scale back up, then trim 1000 for stereo. Reading the table at the full stereo rate lands a row too high and hands SILK far more than its share.
3. The SILK base rate came from the configured bitrate rather than the frame's own budget (`bits_target`: capped by the caller's buffer, less the TOC byte). At 20 kb/s and 20 ms that is 19600 rather than 20000, a whole table row's worth of interpolation.
4. CELT's target was the whole packet's rate, where libopus gives it `bitrate_bps - silk_mode.bitRate`. Since CELT then adds the SILK bits back via `target += tell`, the low band was being counted twice.
5. Constrained VBR was left on for the hybrid high band, where libopus explicitly turns it off. Its reservoir is sized for the whole packet, which in hybrid is mostly SILK's bits, so it capped the little rate the high band had.

A sixth thing was missing underneath them: `SILKInfo`, which libopus hands CELT before every hybrid frame (`CELT_SET_SILK_INFO`) and which three separate behaviours key off — the tonal/noisy target nudge, `allow_weak_transients`, and the low-bitrate temporal-resolution floor. Without it `enable_tf_analysis` also lost its `!hybrid` term, so this crate ran TF analysis on hybrid frames where the reference does not.

### Where the bits were going

The split between the two layers is not visible from outside the decoder, because a hybrid packet is one range-coded stream. `split` reads it out of the decoder's own position at the point SILK finishes. Since that decoder matches libopus bit for bit on hybrid, pointing it at libopus's packets measures the reference's split too. At 20 kb/s fullband mono:

| | SILK low band | CELT high band | total |
| --- | --- | --- | --- |
| before | 227 bits/frame | **54** | 14.0 kb/s* |
| after | 227 bits/frame | **97** | 16.2 kb/s |
| libopus | 215 bits/frame | 129 | 17.2 kb/s |

(*with only fix 4 applied, which alone overcorrects — the original 22.3 kb/s came from CELT being handed the whole packet's rate.)

The high band was the starved half throughout. SILK was never far off.

### What it was worth

Round-trip through libopus's decoder, against the source:

| | before | after | libopus |
| --- | --- | --- | --- |
| hybrid SWB 12 kb/s mono | 13.8 kb/s, 5.52 dB | 10.3 kb/s, 5.46 dB | 11.4 kb/s, 5.38 dB |
| hybrid FB 20 kb/s mono | 22.3 kb/s, 5.56 dB | 16.2 kb/s, 5.55 dB | 17.2 kb/s, 5.49 dB |
| hybrid FB 24 kb/s stereo | 26.8 kb/s, **2.61 dB** | 21.1 kb/s, **4.27 dB** | 22.6 kb/s, 4.28 dB |

Mono holds its quality on a quarter fewer bits. Stereo gains **1.66 dB while also spending 21% less**, landing on libopus's 4.28 dB: the per-channel allocation was not merely mis-spending, it was degrading stereo hybrid.

The speed table shows the same thing from the other side. `hybrid FB 24 kb/s stereo` used to read 1.82x where every other hybrid row read about 1.12x, and this README used to say the number to distrust there was ours. It was: libopus pays about a 1.9x penalty going from mono to stereo in hybrid and this crate was paying 1.16x, because it was not doing the work. That row now reads 1.14x, in line with its neighbours.

### What is left

Hybrid now undershoots libopus by 6–10% rather than overshooting by 12–15%, and the residual splits in two. Our SILK spends about 6% more than libopus's in hybrid, which matches what the SILK-only rows show (+6 to +11%), so it is not a hybrid problem. Our CELT high band still spends 97 bits/frame against 129, which is.

What those bits are worth has since been measured, in [`highband/`](../highband/): they do not buy high-band quality. The band is noise-filled at these rates, and on the two measures that apply to a noise fill — the level it comes back at, and how tightly its envelope is tracked — this crate is not behind libopus on 25% fewer bits. libopus spends them quieting the band instead, by 4 dB where it can afford least, converging to nothing once it can code the band properly. The gap is also a constant 30 to 32 bits per frame from 12 to 64 kb/s rather than a curve, which is a narrower thing to look for than a different allocation slope. Where in `celt_encoder.c` either behaviour comes from is still the frame-by-frame trace nobody has run.

None of this is a conformance question. libopus decodes the new hybrid bitstream at 152 dB, the 440-configuration interop sweep is unchanged, and the RFC vectors still pass 12 of 12 on both legs.
