"""Report which coding modes an Ogg Opus file's packets use, from their TOC bytes."""
import sys, os, glob

def packets(path):
    data = open(path, 'rb').read()
    i, pending = 0, b''
    while i < len(data):
        assert data[i:i+4] == b'OggS', f'{path}: not a page at {i}'
        nseg = data[i+26]
        segs = data[i+27:i+27+nseg]
        body = i + 27 + nseg
        for s in segs:
            pending += data[body:body+s]
            body += s
            if s < 255:
                yield pending
                pending = b''
        i = body

def mode(pkt):
    cfg = pkt[0] >> 3
    if cfg < 12: return 'silk'
    if cfg < 16: return 'hybrid'
    return 'celt'

def summarize(path):
    ms = [mode(p) for p in packets(path)][2:]  # skip OpusHead/OpusTags
    order, switches = [], 0
    for m in ms:
        if not order or order[-1] != m:
            order.append(m)
            switches += 1
    return '+'.join(order), switches - 1, len(ms)

if __name__ == '__main__':
    for pat in sys.argv[1:]:
        for f in sorted(glob.glob(pat)):
            seq, sw, n = summarize(f)
            print(f'{os.path.basename(f)[:-5]:<30}{seq:<28}switches={sw:<3}packets={n}')
