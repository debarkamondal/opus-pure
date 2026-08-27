import array, math, os, sys, glob

def load(p):
    a = array.array('f')
    with open(p,'rb') as f: a.frombytes(f.read())
    if sys.byteorder != 'little': a.byteswap()
    return a

def stats(x, y):
    """Return (max_abs_diff, snr_db, exact_match_fraction)."""
    n = min(len(x), len(y))
    se = 0.0; sig = 0.0; mx = 0.0; exact = 0
    for i in range(n):
        d = x[i] - y[i]
        if abs(d) > mx: mx = abs(d)
        if d == 0.0: exact += 1
        se += d*d; sig += y[i]*y[i]
    snr = 10*math.log10(sig/se) if se > 0 else float('inf')
    return mx, snr, exact/n if n else 0.0

rows = []
DIR = sys.argv[1] if len(sys.argv) > 1 else 'reference/work/out'
for ref in sorted(glob.glob(f'{DIR}/*.ref.f32')):
    b = os.path.basename(ref)[:-8]
    ours = f'{DIR}/{b}.ours.f32'
    if not os.path.exists(ours): continue
    R, O = load(ref), load(ours)
    if len(R) != len(O):
        rows.append((b, 'LENGTH MISMATCH', len(R), len(O))); continue
    mx, snr, ex = stats(O, R)
    rows.append((b, mx, snr, ex))

print(f"{'file':<24}{'max|Δ|':>12}{'SNR vs libopus':>16}{'bit-exact':>12}")
print('-'*64)
worst = 0.0
for r in rows:
    if r[1] == 'LENGTH MISMATCH':
        print(f"{r[0]:<24}  LENGTH MISMATCH {r[2]} vs {r[3]}"); continue
    b, mx, snr, ex = r
    worst = max(worst, mx)
    s = 'inf' if snr == float('inf') else f'{snr:8.2f} dB'
    print(f"{b:<24}{mx:12.3e}{s:>16}{ex*100:11.2f}%")
print('-'*64)
print(f"worst max|Δ| across all files: {worst:.3e}")
