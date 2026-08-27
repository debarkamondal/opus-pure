#!/usr/bin/env python3
"""Search for a constant lag or sign flip between two f32 PCM dumps."""
import struct, sys, math
def load(p):
    d = open(p,'rb').read(); return list(struct.unpack('<%df' % (len(d)//4), d))
a, b = load(sys.argv[1]), load(sys.argv[2])
maxlag = int(sys.argv[3]) if len(sys.argv) > 3 else 200
# skip the start-up transient
s = min(len(a), len(b)) // 4
best = None
for lag in range(-maxlag, maxlag+1):
    ia, ib = (s+lag, s) if lag >= 0 else (s, s-lag)
    n = min(len(a)-ia, len(b)-ib)
    if n < 1000: continue
    x, y = a[ia:ia+n], b[ib:ib+n]
    num = sum(p*q for p, q in zip(x, y))
    ea = math.sqrt(sum(p*p for p in x)); eb = math.sqrt(sum(q*q for q in y))
    if ea == 0 or eb == 0: continue
    c = num/(ea*eb)
    if best is None or abs(c) > abs(best[1]): best = (lag, c)
lag, c = best
print(f"  best lag={lag:+d} samples  correlation={c:+.4f}")
ia, ib = (s+lag, s) if lag >= 0 else (s, s-lag)
n = min(len(a)-ia, len(b)-ib)
x, y = a[ia:ia+n], b[ib:ib+n]
se = sum((p-q)**2 for p, q in zip(x, y)); sig = sum(q*q for q in y)
print(f"  aligned SNR={10*math.log10(sig/se) if se>0 else float('inf'):.1f} dB")
