#!/usr/bin/env python3
"""FNV-1a of raw f32 PCM hashed as little-endian i16, matching `pcm_hash` in
`tests/decoder_conformance.rs`.

Rust's `f32::round` breaks ties away from zero; Python's built-in `round` breaks
them to even. That difference is invisible on most audio and would silently
produce a wrong constant on the sample that lands exactly on .5, so the rounding
is spelled out here rather than borrowed.
"""
import math, struct, sys


def round_half_away(x: float) -> int:
    return math.floor(x + 0.5) if x >= 0.0 else math.ceil(x - 0.5)


d = open(sys.argv[1], 'rb').read()
v = struct.unpack('<%df' % (len(d) // 4), d)
h = 0xcbf29ce484222325
for s in v:
    # `as i32 as i16` in Rust: truncate to 32 bits then to 16, both wrapping.
    x = round_half_away(s * 32768.0) & 0xFFFF
    for b in (x & 0xFF, (x >> 8) & 0xFF):
        h ^= b
        h = (h * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
print("0x%016x" % h)
