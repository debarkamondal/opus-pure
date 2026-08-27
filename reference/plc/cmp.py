#!/usr/bin/env python3
"""Sample-level comparison of two raw f32 PCM dumps."""
import struct, sys, math

def load(p):
    d = open(p, 'rb').read()
    return struct.unpack('<%df' % (len(d)//4), d)

a, b = load(sys.argv[1]), load(sys.argv[2])
frame = int(sys.argv[3]) if len(sys.argv) > 3 else 0
n = min(len(a), len(b))
if len(a) != len(b):
    print(f"  LENGTH MISMATCH rust={len(a)} c={len(b)}")
diff = [a[i]-b[i] for i in range(n)]
se = sum(d*d for d in diff)
sig = sum(x*x for x in b[:n])
snr = 10*math.log10(sig/se) if se > 0 else float('inf')
peak = max(abs(d) for d in diff)
exact = sum(1 for d in diff if d == 0.0)
print(f"  {n} samples  SNR={snr:.1f} dB  peak|diff|={peak:.6f}  bit-identical={100.0*exact/n:.1f}%")
if frame and se > 0:
    worst, wf = 0.0, -1
    for f in range(n // frame):
        s = sum(d*d for d in diff[f*frame:(f+1)*frame])
        if s > worst: worst, wf = s, f
    first = next((i for i, d in enumerate(diff) if d != 0.0), None)
    print(f"  first differing sample: {first} (frame {first//frame if first is not None else '-'}), "
          f"worst frame: {wf}")
