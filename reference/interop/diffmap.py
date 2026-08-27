import array, math, sys
def load(p):
    a = array.array('f')
    with open(p,'rb') as f: a.frombytes(f.read())
    if sys.byteorder != 'little': a.byteswap()
    return a
base = sys.argv[1]; ch = int(sys.argv[2]) if len(sys.argv)>2 else 1
R, O = load(base+'.ref.f32'), load(base+'.ours.f32')
n = min(len(R), len(O))
print(f"len ref={len(R)} ours={len(O)}")
# contiguous runs of nonzero difference
runs = []; start = None
for i in range(n):
    d = R[i]-O[i]
    if d != 0.0:
        if start is None: start = i
    else:
        if start is not None: runs.append((start, i)); start = None
if start is not None: runs.append((start, n))
print(f"{len(runs)} differing runs")
for (s,e) in runs[:40]:
    seg = [abs(R[i]-O[i]) for i in range(s,e)]
    print(f"  samples {s//ch}..{e//ch}  len {(e-s)//ch}  max|d| {max(seg):.4e}")
tot = sum((R[i]-O[i])**2 for i in range(n)); sig = sum(R[i]*R[i] for i in range(n))
print(f"overall SNR {10*math.log10(sig/tot) if tot>0 else float('inf'):.2f} dB, exact {sum(1 for i in range(n) if R[i]==O[i])*100.0/n:.2f}%")
