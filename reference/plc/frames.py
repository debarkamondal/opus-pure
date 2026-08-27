#!/usr/bin/env python3
"""Per-frame diff between two raw f32 dumps: frames that differ, and by how much."""
import struct, sys, math

def load(p):
    d = open(p,'rb').read()
    return struct.unpack('<%df'%(len(d)//4), d)

a,b = load(sys.argv[1]), load(sys.argv[2])
frame = int(sys.argv[3])
n = min(len(a),len(b))//frame
show_all = len(sys.argv) > 4 and sys.argv[4] == 'all'
for f in range(n):
    da = a[f*frame:(f+1)*frame]; db = b[f*frame:(f+1)*frame]
    d = [x-y for x,y in zip(da,db)]
    peak = max(abs(x) for x in d)
    ndiff = sum(1 for x in d if x != 0.0)
    if ndiff or show_all:
        se = sum(x*x for x in d); sig = sum(y*y for y in db)
        snr = 10*math.log10(sig/se) if se>0 else float('inf')
        first = next((i for i,x in enumerate(d) if x != 0.0), None)
        print(f"frame {f:4d}: {ndiff:4d}/{frame} differ  peak={peak:.6g}  snr={snr:7.1f} dB  first={first}")
