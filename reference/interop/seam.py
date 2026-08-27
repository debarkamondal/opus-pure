"""Locate where two decodes of the same stream disagree, relative to its mode switches.

Reports every run of samples whose difference exceeds float32 rounding, and the
packet boundary each run sits on, so "they differ only at the mode switch" is a
measurement rather than an assertion.
"""
import array, sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from oggmode import packets, mode

TOL = 1e-5  # two orders above float32 rounding (~2e-7) on these signals

def load(p):
    a = array.array('f')
    with open(p, 'rb') as f: a.frombytes(f.read())
    if sys.byteorder != 'little': a.byteswap()
    return a

def runs(ref, ours, ch):
    n = min(len(ref), len(ours)) // ch
    out, start = [], None
    for i in range(n):
        bad = any(abs(ref[i*ch+c] - ours[i*ch+c]) > TOL for c in range(ch))
        if bad and start is None: start = i
        elif not bad and start is not None:
            out.append((start, i)); start = None
    if start is not None: out.append((start, n))
    return out

def switch_packets(opus):
    ms = [mode(p) for p in packets(opus)][2:]
    return [i for i in range(1, len(ms)) if ms[i] != ms[i-1]], ms

if __name__ == '__main__':
    name, ch = sys.argv[1], int(sys.argv[2])
    d = 'reference/work/out'
    opus = f'reference/work/sweep/{name}.opus'
    if not os.path.exists(opus): opus = f'reference/work/focus/{name}.opus'
    sw, ms = switch_packets(opus)
    spp = 960  # 20 ms at the 48 kHz output rate
    r = runs(load(f'{d}/{name}.ref.f32'), load(f'{d}/{name}.ours.f32'), ch)
    print(f'{name}: {len(ms)} packets, switches at {sw} '
          f'(samples {[s*spp for s in sw]})')
    print(f'  {len(r)} run(s) above {TOL:g}:')
    for a, b in r:
        near = min(sw, key=lambda s: abs(a - s*spp)) if sw else None
        off = a - near*spp if near is not None else 0
        print(f'    samples {a}-{b} ({b-a} long, {(b-a)/48:.2f} ms) '
              f'starts {off:+d} from the switch at packet {near}')
