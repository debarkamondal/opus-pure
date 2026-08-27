#!/bin/bash
# Decode the official RFC 6716 / RFC 8251 test vectors with this crate and hold
# the result against the IETF reference decodes.
#
#   reference/vectors/run.sh [mono|stereo|both]     (default: both)
#
# The pass rule is libopus's own, from tests/run_vectors.sh: each vector ships
# two reference decodes, `.dec` from a float build and `m.dec` from a fixed-point
# one, and a decode passes if `opus_compare` accepts *either*. Comparing against
# only one of them fails vectors that are perfectly conformant.
#
# Needs the vectors fetched (see README.md) and reference/build.sh run.
set -uo pipefail
cd "$(dirname "$0")/../.."

W=reference/work/vectors
V=$W/opus_newvectors
B=reference/work/bin
R=reference/rust/target/release

[ -d "$V" ] || { echo "test vectors not found in $V; see reference/vectors/README.md" >&2; exit 1; }
[ -x "$B/opus_compare" ] || { echo "run reference/build.sh first" >&2; exit 1; }
[ -x "$R/vector_check" ] || cargo build --release --manifest-path reference/rust/Cargo.toml --bin vector_check

leg() {  # <channels> <label>
    local ch=$1 label=$2 pass=0 fail=0 pkts=0 rng=0 errs=0
    echo "== $label =="
    for f in 01 02 03 04 05 06 07 08 09 10 11 12; do
        local out=$W/rs$f.$ch.s16
        local stats
        stats=$($R/vector_check "$V/testvector$f.bit" "$out" 48000 "$ch" 2>&1 | tail -3)
        pkts=$((pkts + $(sed -n 's/.*packets=\([0-9]*\).*/\1/p' <<<"$stats")))
        rng=$((rng + $(sed -n 's/.*range_mismatch=\([0-9]*\).*/\1/p' <<<"$stats")))
        errs=$((errs + $(sed -n 's/.*errors=\([0-9]*\).*/\1/p' <<<"$stats")))

        local s=""; [ "$ch" = 2 ] && s="-s"
        $B/opus_compare $s -r 48000 "$V/testvector$f.dec"  "$out" >"$W/a.log" 2>&1; local ra=$?
        $B/opus_compare $s -r 48000 "$V/testvector${f}m.dec" "$out" >"$W/b.log" 2>&1; local rb=$?
        local qa qb verdict
        qa=$(sed -n 's/.*metric: \([0-9.]*\).*/\1/p' "$W/a.log")
        qb=$(sed -n 's/.*metric: \([0-9.]*\).*/\1/p' "$W/b.log")
        if [ $ra -eq 0 ] || [ $rb -eq 0 ]; then verdict=PASS; pass=$((pass+1)); else verdict=FAIL; fail=$((fail+1)); fi
        printf "  %s  %-4s  float=%-6s fixed=%-6s\n" "$f" "$verdict" "${qa:-fail}" "${qb:-fail}"
    done
    printf "  -> %d pass, %d fail | %d packets, %d decode errors, %d range mismatches\n\n" \
        "$pass" "$fail" "$pkts" "$errs" "$rng"
    [ "$fail" -eq 0 ]
}

rc=0
case "${1:-both}" in
    mono)   leg 1 "mono"   || rc=1 ;;
    stereo) leg 2 "stereo" || rc=1 ;;
    both)   leg 2 "stereo" || rc=1; leg 1 "mono" || rc=1 ;;
    *)      echo "usage: $0 [mono|stereo|both]" >&2; exit 2 ;;
esac
exit $rc
