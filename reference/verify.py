#!/usr/bin/env python3
"""Re-derive every frozen libopus constant in the test suite, from libopus.

    reference/verify.py [--skip-dump]

Reads the expected values out of the test sources rather than carrying its own
copy, so this script cannot drift from what the tests assert. A mismatch means
either the encoder changed (regenerate, deliberately) or something is wrong.

Covers the 28 values whose inputs the `#[ignore]`d dumpers write:
`tests/integer_pcm.rs` (7) and `tests/decoder_conformance.rs` (21). The 22
configurations in `tests/reference_vectors.rs` are checked by `cvec`, one
configuration per run; see `reference/vectors/README.md`.

Needs `reference/build.sh` to have been run.
"""
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "reference/work/bin"
S16 = ROOT / "reference/work/s16"
PLC = ROOT / "reference/work/plc"


def die(msg):
    print(f"error: {msg}", file=sys.stderr)
    sys.exit(1)


def run(*args, **kw):
    return subprocess.run([str(a) for a in args], capture_output=True, text=True, **kw)


def hexes(text, name, count):
    """The `count` hex literals of `const <name>` — Rust's `_` separators removed."""
    m = re.search(rf"const {name}[^=]*=\s*\[(.*?)\];", text, re.S)
    if not m:
        m = re.search(rf"const {name}[^=]*=\s*(0x[0-9a-f_]+);", text, re.S)
        if not m:
            die(f"could not find {name}")
        vals = [m.group(1)]
    else:
        vals = re.findall(r"0x[0-9a-f_]+", m.group(1))
    vals = [v.replace("_", "") for v in vals]
    if len(vals) != count:
        die(f"{name}: expected {count} values, found {len(vals)}")
    return vals


def check(results, label, got, want):
    """Compare two hex strings. The C tools print bare hex and `pcmhash.py`
    prints an `0x` prefix, so normalise rather than depending on the caller."""
    got, want = got.lower().removeprefix("0x"), want.lower().removeprefix("0x")
    ok = got == want
    results.append(ok)
    mark = "ok  " if ok else "FAIL"
    extra = "" if ok else f"   got {got}, want {want}"
    print(f"  {mark}  {label}{extra}")


def integer_pcm(results):
    src = (ROOT / "tests/integer_pcm.rs").read_text()
    enc = hexes(src, "ENCODE_EXPECTED", 3)
    dec = hexes(src, "DECODE_EXPECTED", 3)
    clip = hexes(src, "SOFT_CLIP_EXPECTED", 1)[0]
    cases = re.findall(r'\("([^"]+)",\s*([0-9_]+),\s*([0-9_]+)\)', src)
    cases = [(n, int(b.replace("_", "")), int(c)) for n, b, c in cases][:3]
    if len(cases) != 3:
        die("could not parse CASES from tests/integer_pcm.rs")

    print("tests/integer_pcm.rs")
    for i, ((name, bitrate, complexity), want) in enumerate(zip(cases, enc)):
        r = run(BIN / "cs16_fixed", "enc", S16 / "sine.s16", S16 / f"v{i}.pkt",
                8000, 1, 160, bitrate, complexity, 0, 0)
        got = re.search(r"hash=([0-9a-f]+)", r.stdout)
        check(results, f"ENCODE_EXPECTED[{i}]  {name}", got.group(1) if got else r.stderr.strip(), want)

    for i, want in enumerate(dec):
        r = run(BIN / "cs16_nofma", "dec", S16 / f"case{i}.pkt", S16 / f"case{i}.s16", 8000, 1, 160)
        got = re.search(r"hash=([0-9a-f]+)", r.stdout)
        check(results, f"DECODE_EXPECTED[{i}]  {cases[i][0]}", got.group(1) if got else r.stderr.strip(), want)

    r = run(BIN / "cs16_nofma", "clip", S16 / "over_unity.f32", S16 / "clipped.f32", 2, 960)
    got = re.search(r"hash=([0-9a-f]+)", r.stdout)
    check(results, "SOFT_CLIP_EXPECTED", got.group(1) if got else r.stderr.strip(), clip)


def decoder_conformance(results):
    src = (ROOT / "tests/decoder_conformance.rs").read_text()

    def table(name, pattern):
        block = re.search(rf"const {name}: &\[\w+\] = &\[(.*?)\n\];", src, re.S)
        if not block:
            die(f"could not find {name}")
        return re.findall(pattern, block.group(1), re.S)

    frozen = table("FROZEN",
                   r'rate:\s*([0-9_]+),\s*channels:\s*(\d+),.*?label:\s*"([^"]+)".*?pcm:\s*(0x[0-9a-f_]+)')
    plc = table("FROZEN_PLC",
                r'rate:\s*([0-9_]+),\s*channels:\s*(\d+),.*?label:\s*"([^"]+)",\s*'
                r'lost:\s*&\[([0-9,\s]*)\],\s*pcm:\s*(0x[0-9a-f_]+)')
    downmix = table("FROZEN_DOWNMIX",
                    r'label:\s*"([^"]+)",\s*lost:\s*&\[([0-9,\s]*)\],\s*pcm:\s*(0x[0-9a-f_]+)')
    if not frozen or not plc or not downmix:
        die("could not parse the frozen tables from tests/decoder_conformance.rs")

    def pcm_hash(path):
        return run(sys.executable, ROOT / "reference/plc/pcmhash.py", path).stdout.strip()

    print("\ntests/decoder_conformance.rs — clean decode")
    for rate, ch, label, want in frozen:
        rate, ch = int(rate.replace("_", "")), int(ch)
        stem = label.replace(" ", "_")
        run(BIN / "cpcm", PLC / f"{stem}.pkt", PLC / "v.pcm", rate, ch, rate // 50)
        check(results, f"FROZEN {label}", pcm_hash(PLC / "v.pcm"), want.replace("_", ""))

    print("\ntests/decoder_conformance.rs — concealment")
    for rate, ch, label, lost, want in plc:
        rate, ch = int(rate.replace("_", "")), int(ch)
        # FROZEN_PLC reuses FROZEN's packets; the label carries a suffix.
        stem = next(f[2].replace(" ", "_") for f in frozen
                    if label.startswith(f[2]) and int(f[0].replace("_", "")) == rate
                    and int(f[1]) == ch)
        losses = ",".join(t.strip() for t in lost.split(",") if t.strip())
        run(BIN / "cplc", PLC / f"{stem}.pkt", PLC / "v.pcm", rate, ch, rate // 50, losses)
        check(results, f"FROZEN_PLC {label} [{losses}]", pcm_hash(PLC / "v.pcm"), want.replace("_", ""))

    print("\ntests/decoder_conformance.rs — stereo stream, mono output")
    for label, lost, want in downmix:
        rate = next(int(f[0].replace("_", "")) for f in frozen if f[2] == label)
        stem = label.replace(" ", "_")
        losses = ",".join(t.strip() for t in lost.split(",") if t.strip())
        # The same stereo packets, one output channel: a libopus decoder's
        # channel count is independent of the stream's, so `cpcm`/`cplc` need no
        # flag for this beyond the count they already take.
        tool = "cplc" if losses else "cpcm"
        args = [PLC / f"{stem}.pkt", PLC / "v.pcm", rate, 1, rate // 50]
        run(BIN / tool, *args, *([losses] if losses else []))
        suffix = f" [{losses}]" if losses else ""
        check(results, f"FROZEN_DOWNMIX {label}{suffix}", pcm_hash(PLC / "v.pcm"), want.replace("_", ""))


def main():
    if not (BIN / "cs16_fixed").exists():
        die("harnesses not built; run reference/build.sh")

    if "--skip-dump" not in sys.argv:
        print("regenerating inputs...")
        for t in ("integer_pcm", "decoder_conformance"):
            r = run("cargo", "test", "--release", "--test", t, "--", "--ignored", "--quiet", cwd=ROOT)
            if r.returncode != 0:
                die(f"dumper for {t} failed:\n{r.stdout}\n{r.stderr}")
        print()

    results = []
    integer_pcm(results)
    decoder_conformance(results)

    bad = results.count(False)
    print(f"\n{len(results) - bad} of {len(results)} frozen values reproduce from libopus 1.6.1")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
