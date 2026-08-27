#!/bin/bash
# Direction A: encode with this crate, decode with both stacks, compare.
#
# Two matrices, 440 configurations, all at the 48 kHz comparison rate:
#   20 ms  120 bandwidth/application configs + 160 bitrate/content configs
#   60 ms  160 bandwidth/application/bitrate configs
#
# Needs opus-tools (`opusdec`, `opusinfo`) on PATH and python3.
set -euo pipefail
cd "$(dirname "$0")/../.."          # repository root; every path below is relative to it
W=reference/work                    # generated files, gitignored
B=reference/rust/target/release

cargo build --release --manifest-path reference/rust/Cargo.toml \
    --bin sweep --bin focus --bin sixty --bin dec

echo "== generating =="
rm -rf $W/sweep $W/focus $W/sixty
$B/sweep  | grep -c OK | xargs echo "  20 ms bandwidth sweep:"
$B/focus  | wc -l      | xargs echo "  20 ms bitrate sweep:  "
$B/sixty  | grep -c OK | xargs echo "  60 ms sweep:          "

decode_dir() {  # <src_dir> <out_dir>  (appends; caller creates the dir)
    for f in "$1"/*.opus; do
        b=$(basename "$f" .opus)
        opusdec --quiet --float --rate 48000 "$f" "$2/$b.ref.f32" 2>/dev/null
        $B/dec "$f" "$2/$b.ours.f32" >/dev/null
    done
}

echo "== validating containers =="
warn=0
for f in $W/sweep/*.opus $W/focus/*.opus $W/sixty/*.opus; do
    opusinfo "$f" 2>&1 | grep -qiE 'warning|error' && { echo "  BAD $f"; warn=$((warn+1)); }
done
echo "  opusinfo warnings/errors: $warn"

echo "== decoding 20 ms =="
rm -rf $W/out && mkdir -p $W/out
decode_dir $W/sweep $W/out
decode_dir $W/focus $W/out
echo "== 20 ms =="
python3 reference/interop/report.py

echo
echo "== decoding 60 ms =="
rm -rf $W/sixtyout && mkdir -p $W/sixtyout
decode_dir $W/sixty $W/sixtyout
echo "== 60 ms =="
python3 reference/interop/report.py sixtyout sixty 2880
