#!/bin/sh
# Compare this crate's concealment against libopus on identical packets.
#
#   run.sh <rate> <ch> <bitrate> <bw> <app> <signal> <frame_ms> <frames> <lost,...> [frames|cmp]
#
# `cplc` is the fixed-point reference and is right for anything SILK; set
# CPLC=cplcf for the float build, which is right for anything CELT or hybrid.
# See README.md.
set -e
cd "$(dirname "$0")/../.."          # repository root
W=reference/work
rate=$1 ch=$2 br=$3 bw=$4 app=$5 sig=$6 ms=$7 n=$8 lost=$9 mode=${10:-cmp}
fr=$((rate * ms / 1000))
mkdir -p "$W/plc"

reference/rust/target/release/plc "$W/plc/t.pkt" "$W/plc/t.rs.pcm" \
    "$rate" "$ch" "$br" "$bw" "$app" "$sig" "$ms" "$n" "$lost" 2>/dev/null
"$W/bin/${CPLC:-cplc}" "$W/plc/t.pkt" "$W/plc/t.c.pcm" "$rate" "$ch" "$fr" "$lost" 2>/dev/null
python3 reference/plc/cmp.py "$W/plc/t.rs.pcm" "$W/plc/t.c.pcm" $((fr * ch))
if [ "$mode" = frames ]; then
  python3 reference/plc/frames.py "$W/plc/t.rs.pcm" "$W/plc/t.c.pcm" $((fr * ch))
fi
