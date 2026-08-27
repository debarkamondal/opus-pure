#!/usr/bin/env bash
# Hold encode_parallel against the serial encode across the cases where they can
# differ. See README.md for what each line is showing.
set -euo pipefail
cd "$(dirname "$0")/../.."
B=./reference/rust/target/release/parchunk
[ -x "$B" ] || { echo "build first: cargo build --release --manifest-path reference/rust/Cargo.toml" >&2; exit 1; }

W=${W:-2000}          # warm-up under test, milliseconds
SECS=${SECS:-120}     # clip length
THREADS=${THREADS:-4}

if [ $# -gt 0 ]; then exec "$B" "$@"; fi

echo "== warm-up sweep: speech at 16 kb/s, where the mode decision is closest =="
for w in 160 500 1000 2000 3000; do
  printf '  %5s ms  ' "$w"
  "$B" 48000 1 16000 20 "$SECS" speech "$w" "$THREADS" \
    | grep -E 'worst chunk|SNR vs serial' | sed 's/^ *//' | tr '\n' '|' | sed 's/|$//'
  echo
done

echo
echo "== across content and rate, at warm-up ${W} ms =="
for c in speech music; do
  for br in 16000 24000 32000 64000; do
    printf '  %-6s %3s kb/s  ' "$c" "$((br / 1000))"
    "$B" 48000 1 "$br" 20 "$SECS" "$c" "$W" "$THREADS" | grep -E 'worst chunk' | sed 's/^ *//'
  done
done

echo
echo "== pinning the signal type takes the analysis out of the decision =="
for s in auto voice music; do
  printf '  %-5s  ' "$s"
  "$B" 48000 1 16000 20 "$SECS" speech 160 "$THREADS" "$s" | grep -E 'worst chunk' | sed 's/^ *//'
done
