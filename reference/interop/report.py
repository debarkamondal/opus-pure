"""Summarize a direction-A sweep: our encoder's streams, decoded by both stacks.

Prints the table `docs/interop-validation.md` quotes. Every claim in that table
is computed here, including the "differences only at a mode switch" one, which
is a measurement of where the differing samples are and not an assertion.
"""
import array, glob, math, os, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from oggmode import packets, mode

TOL = 1e-5            # ~two orders above float32 rounding on these signals
SAMPLES_PER_PKT = 960  # 20 ms at the 48 kHz comparison rate
PRE_SKIP = 312         # trimmed from the front of both decodes

def load(p):
    a = array.array('f')
    with open(p, 'rb') as f: a.frombytes(f.read())
    if sys.byteorder != 'little': a.byteswap()
    return a

def stats(ours, ref):
    n = min(len(ours), len(ref))
    se = sig = mx = 0.0; exact = 0
    for i in range(n):
        d = ours[i] - ref[i]
        if abs(d) > mx: mx = abs(d)
        if d == 0.0: exact += 1
        se += d * d; sig += ref[i] * ref[i]
    snr = 10 * math.log10(sig / se) if se > 0 else float('inf')
    return mx, snr, (exact / n if n else 0.0)

def diff_span(ours, ref, ch):
    """First and last frame index whose channels differ by more than TOL."""
    n = min(len(ours), len(ref)) // ch
    lo = hi = None
    for i in range(n):
        if any(abs(ours[i*ch+c] - ref[i*ch+c]) > TOL for c in range(ch)):
            if lo is None: lo = i
            hi = i
    return lo, hi

def opus_path(name, src_dirs):
    for d in src_dirs:
        p = f'reference/work/{d}/{name}.opus'
        if os.path.exists(p): return p
    raise FileNotFoundError(name)

def main(out_dir='reference/work/out', src_dirs=('sweep', 'focus'),
         spp=SAMPLES_PER_PKT):
    rows = []
    for ref_path in sorted(glob.glob(f'{out_dir}/*.ref.f32')):
        name = os.path.basename(ref_path)[:-8]
        ours_path = f'{out_dir}/{name}.ours.f32'
        if not os.path.exists(ours_path): continue
        ref, ours = load(ref_path), load(ours_path)
        ms = [mode(p) for p in packets(opus_path(name, src_dirs))][2:]
        seq, switches = [], []
        for i, m in enumerate(ms):
            if not seq or seq[-1] != m:
                seq.append(m)
                if i: switches.append(i)
        ch = 2 if '_2ch_' in name else 1
        if len(ref) != len(ours):
            rows.append(dict(name=name, bad=f'LENGTH {len(ref)} vs {len(ours)}'))
            continue
        mx, snr, exact = stats(ours, ref)
        lo, hi = diff_span(ours, ref, ch)
        peak = max(abs(v) for v in ours) if ours else 0.0
        rows.append(dict(name=name, mx=mx, snr=snr, exact=exact, ch=ch, peak=peak,
                         seq='+'.join(seq), switches=switches, lo=lo, hi=hi,
                         packets=len(ms), bad=None))

    good = [r for r in rows if not r['bad']]
    bad = [r for r in rows if r['bad']]
    identical = [r for r in good if r['exact'] == 1.0]
    pure_silk = [r for r in good if r['seq'] == 'silk']
    over = [r for r in good if r['snr'] >= 100.0]
    under = sorted((r for r in good if r['snr'] < 100.0), key=lambda r: r['snr'])
    switching = [r for r in good if r['switches']]

    print(f'configurations                                  {len(rows)}')
    if bad:
        print(f'LENGTH MISMATCHES                               {len(bad)}')
        for r in bad: print(f'   {r["name"]}: {r["bad"]}')
    print(f'decode bit-identically to libopus               {len(identical)}')
    print(f'   ... and the pure-SILK streams number         {len(pure_silk)}'
          f'   (same set: {sorted(r["name"] for r in identical) == sorted(r["name"] for r in pure_silk)})')
    print(f'agree to better than 100 dB SNR                 {len(over)}')
    print(f'below 100 dB                                    {len(under)}')
    print(f'streams that change coding mode                 {len(switching)}')
    print(f'worst max|delta| across all files               {max(r["mx"] for r in good):.3e}')
    # libopus's float API does not clamp, so ordinary ringing carries past 1.0.
    # A peak far above that is a broken bitstream, not a tuning difference.
    print(f'worst decoded peak                              {max(r["peak"] for r in good):.2f}')

    # Every differing sample must sit within one packet of a mode switch.
    worst_ms, strays = 0.0, []
    for r in good:
        if r['lo'] is None: continue
        span = (r['hi'] - r['lo'] + 1) / 48.0
        worst_ms = max(worst_ms, span)
        near = any(abs(r['lo'] + PRE_SKIP - s * spp) <= spp and
                   abs(r['hi'] + PRE_SKIP - s * spp) <= spp for s in r['switches'])
        if not near: strays.append(r)
    print(f'widest window of differing samples              {worst_ms:.2f} ms')
    print(f'streams differing away from a mode switch       {len(strays)}')
    for r in strays:
        print(f'   {r["name"]}: samples {r["lo"]}-{r["hi"]}, '
              f'switches at {[s*spp - PRE_SKIP for s in r["switches"]]}')

    if under:
        print('\nbelow 100 dB, worst first:')
        print(f'   {"configuration":<32}{"SNR":>10}  {"max|delta|":>10}  modes')
        for r in under:
            print(f'   {r["name"]:<32}{r["snr"]:7.2f} dB  {r["mx"]:10.3e}  {r["seq"]}')

if __name__ == '__main__':
    # report.py                     -> the 20 ms sweep
    # report.py sixtyout sixty 2880 -> the 60 ms sweep
    if len(sys.argv) > 1:
        main(f'reference/work/{sys.argv[1]}', (sys.argv[2],), int(sys.argv[3]))
    else:
        main()
