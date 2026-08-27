#!/bin/bash
# Time this crate against libopus 1.6.1 on identical audio at identical settings.
#
#   reference/speed/run.sh              # every case
#   reference/speed/run.sh silk         # only cases whose label contains "silk"
#   reference/speed/run.sh '' 10        # every case, 10 passes instead of 5
#
# Needs reference/build.sh and the Rust tools; see reference/speed/README.md for
# what the numbers mean and why there are two libopus columns.
set -euo pipefail
cd "$(dirname "$0")/../.."

FILTER=${1:-}
REPS=${2:-5}
SECONDS_OF_AUDIO=8

B=reference/work/bin
R=reference/rust/target/release
W=reference/work/speed
mkdir -p "$W"

for t in "$B/cspeed" "$B/cspeed_fixed"; do
    [ -x "$t" ] || { echo "missing $t; run: reference/build.sh" >&2; exit 1; }
done
[ -x "$R/rspeed" ] || {
    echo "missing $R/rspeed; run: cargo build --release --manifest-path reference/rust/Cargo.toml" >&2
    exit 1
}

# label|channels|bitrate|bandwidth|application|signal|content|expected mode
#
# The first ten mirror `benches/throughput.rs`'s mode cases exactly, so a row
# here and a row there describe the same work. The complexity rows follow,
# because complexity is the one knob a caller reaches for when the answer to
# "is it fast enough" is no, and it does not buy the same thing in each mode.
CASES=(
    "silk NB 8 kb/s mono|1|8000|nb|voip|voice|speech|silk"
    "silk MB 24 kb/s mono|1|24000|mb|voip|voice|speech|silk"
    "silk WB 20 kb/s mono|1|20000|wb|voip|voice|speech|silk"
    "silk WB 32 kb/s stereo|2|32000|wb|voip|voice|speech|silk"
    "hybrid SWB 12 kb/s mono|1|12000|swb|voip|voice|speech|hybrid"
    "hybrid FB 20 kb/s mono|1|20000|fb|voip|voice|speech|hybrid"
    "hybrid FB 24 kb/s stereo|2|24000|fb|voip|voice|speech|hybrid"
    "celt FB 64 kb/s mono|1|64000|fb|audio|music|music|celt"
    "celt FB 128 kb/s stereo|2|128000|fb|audio|music|music|celt"
    "celt FB 256 kb/s stereo|2|256000|fb|audio|music|music|celt"
)
COMPLEXITY=(0 5 10)
for c in "${COMPLEXITY[@]}"; do
    CASES+=("silk WB 20 kb/s mono c$c|1|20000|wb|voip|voice|speech|silk|$c")
    CASES+=("celt FB 96 kb/s stereo c$c|2|96000|fb|audio|music|music|celt|$c")
done

RATE=48000
FRAME=960          # 20 ms, the duration the mode cases use

echo "generating source audio..."
for key in 1:speech 2:speech 1:music 2:music; do
    ch=${key%%:*}; content=${key##*:}
    "$R/rspeed" gen "$W/$content$ch.f32" $RATE $ch $FRAME $SECONDS_OF_AUDIO $content 2>/dev/null
done

# Held for the summary tables, one line per case.
OUT=$W/rows.tsv
: > "$OUT"

printf '\n%-27s %-7s %-26s %-26s %s\n' \
    "" "" "     encode x-realtime" "     decode x-realtime" " delivered kb/s"
printf '%-27s %-7s %8s %8s %7s  %8s %8s %7s  %7s %8s\n' \
    "case" "mode" "ours" "libopus" "ratio" "ours" "libopus" "ratio" "ours" "libopus"
printf -- '%.0s-' {1..112}; echo

for spec in "${CASES[@]}"; do
    IFS='|' read -r label ch bitrate bw app signal content expect complexity <<< "$spec"
    complexity=${complexity:-9}
    case "$label" in *"$FILTER"*) ;; *) continue ;; esac

    pcm=$W/$content$ch.f32
    args=("$pcm" $RATE $ch $FRAME $bitrate $complexity $app $bw $signal $REPS)
    read -r r_enc r_ext r_dec r_dxt r_kbps r_mode < <("$R/rspeed" run "${args[@]}")
    read -r c_enc c_ext c_dec c_dxt c_kbps c_mode < <("$B/cspeed" "${args[@]}")
    read -r f_enc f_ext f_dec f_dxt f_kbps f_mode < <("$B/cspeed_fixed" "${args[@]}")

    # A row where the two stacks chose different modes is not comparing the
    # same work at all; one where they spent materially different bitrates is
    # comparing the same work on different amounts of it. Both are marked
    # rather than left for the reader to spot in the kb/s columns.
    flag=""
    [ "$r_mode" = "$c_mode" ] || flag="$flag  <- modes differ: ours $r_mode, libopus $c_mode"
    [ "$r_mode" = "$expect" ] || flag="$flag  <- expected $expect"
    flag="$flag$(awk -v a="$r_kbps" -v b="$c_kbps" \
        'BEGIN{ d=(b>0)?(a/b-1)*100:0; if (d>5 || d<-5) printf "  <- %+.0f%% bits vs libopus", d }')"

    er=$(awk -v a="$r_ext" -v b="$c_ext" 'BEGIN{printf "%.2f", a/b}')
    dr=$(awk -v a="$r_dxt" -v b="$c_dxt" 'BEGIN{printf "%.2f", a/b}')
    printf '%-27s %-7s %8.1f %8.1f %6sx  %8.1f %8.1f %6sx  %7s %8s%s\n' \
        "$label" "$r_mode" "$r_ext" "$c_ext" "$er" "$r_dxt" "$c_dxt" "$dr" "$r_kbps" "$c_kbps" "$flag"
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$label" "$r_mode" "$r_ext" "$c_ext" "$er" "$r_dxt" "$c_dxt" "$dr" "$f_ext" "$r_kbps" >> "$OUT"
done

# The fixed-point build, where it is the like-for-like reference rather than
# the library a caller would link: this crate implements SILK's fixed-point
# encoder, so a SILK or hybrid encode row against a float libopus is comparing
# two different algorithms as well as two languages.
echo
echo "SILK encoder path, against the fixed-point build (the same algorithm ours implements)"
printf '%-28s %9s %9s %7s\n' "case" "ours" "libopus" "ratio"
printf -- '%.0s-' {1..56}; echo
while IFS=$'\t' read -r label mode r_ext c_ext er r_dxt c_dxt dr f_ext kbps; do
    case "$mode" in silk|hybrid|*silk*|*hybrid*) ;; *) continue ;; esac
    fr=$(awk -v a="$r_ext" -v b="$f_ext" 'BEGIN{printf "%.2f", a/b}')
    printf '%-28s %9.1f %9.1f %6sx\n' "$label" "$r_ext" "$f_ext" "$fr"
done < "$OUT"

echo
echo "ratios above 1.00x mean this crate is faster; $REPS passes, fastest kept,"
echo "${SECONDS_OF_AUDIO}s of audio per case, 20 ms frames at $RATE Hz."
echo "a marked row codes a different number of bits than libopus does at the same"
echo "target, so its ratio is speed on a different amount of work, not speed alone."
