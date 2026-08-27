import array, math, sys
def load(p):
    a = array.array('f')
    with open(p,'rb') as f: a.frombytes(f.read())
    if sys.byteorder != 'little': a.byteswap()
    return a
base = sys.argv[1]; ch = int(sys.argv[2]); blk = int(sys.argv[3]) if len(sys.argv)>3 else 960
pre = int(sys.argv[4]) if len(sys.argv)>4 else 312
R, O = load(base+'.ref.f32'), load(base+'.ours.f32')
n = min(len(R), len(O))//ch
print(f"samples/ch ref={len(R)//ch} ours={len(O)//ch}")
# packet p covers output samples [p*blk-pre, (p+1)*blk-pre)
worst=[]
p = 0
while True:
    s = p*blk - pre; e = s+blk
    s = max(s,0); e = min(e,n)
    if s >= n: break
    m = 0.0
    for i in range(s,e):
        for c in range(ch):
            d = abs(R[i*ch+c]-O[i*ch+c])
            if d>m: m=d
    worst.append((p,m,s,e))
    p += 1
big = [w for w in worst if w[1] > 1e-5]
print(f"{len(worst)} packets, {len(big)} with max|d| > 1e-5")
for (p,m,s,e) in big[:40]:
    print(f"  pkt {p:4}  samples {s}..{e}  max|d| {m:.4e}")
tot = sum((R[i]-O[i])**2 for i in range(n*ch)); sig = sum(R[i]*R[i] for i in range(n*ch))
print(f"overall SNR {10*math.log10(sig/tot) if tot>0 else float('inf'):.2f} dB")
