#!/bin/bash
# What a hybrid packet's CELT high band is worth, ours against libopus's.
#
#   reference/highband/run.sh              # the case table
#   reference/highband/run.sh rates        # how the high band moves with bitrate
#   reference/highband/run.sh bands <case> # the full per-band table for one case
#
# Needs reference/build.sh and the Rust tools; see reference/highband/README.md
# for what the columns mean and why `snr` is not the answer up here.
set -euo pipefail
cd "$(dirname "$0")/../.."

B=reference/work/bin
R=reference/rust/target/release
W=reference/work/highband
mkdir -p "$W"

for t in "$B/cspeed" "$B/cband"; do
    [ -x "$t" ] || { echo "missing $t; run: reference/build.sh" >&2; exit 1; }
done
for t in "$R/rspeed" "$R/split" "$R/band"; do
    [ -x "$t" ] || {
        echo "missing $t; run: cargo build --release --manifest-path reference/rust/Cargo.toml" >&2
        exit 1
    }
done

RATE=48000
FRAME=960          # 20 ms
SECS=${SECS:-20}

# label|channels|bitrate|bandwidth|application|signal|content|top Hz|expected mode
#
# `top` is the bandwidth the case asks for, and the summary row stops there: a
# superwideband stream is silent above 12 kHz on purpose. The expected mode is
# checked against the mode the encoders actually chose, so a row cannot quietly
# stop measuring the thing it is named for.
#
# The three hybrid rows of `reference/speed/run.sh`, which is where the bit
# split that prompted this was measured, plus a CELT-only control at the same
# rate: the question is whether a difference belongs to hybrid or to CELT, and
# only a row with no SILK in it can say.
CASES=(
    "hybrid SWB 12 kb/s mono|1|12000|swb|voip|voice|speech|12000|hybrid"
    "hybrid FB 20 kb/s mono|1|20000|fb|voip|voice|speech|20000|hybrid"
    "hybrid FB 24 kb/s stereo|2|24000|fb|voip|voice|speech|20000|hybrid"
    "celt FB 20 kb/s mono|1|20000|fb|lowdelay|voice|speech|20000|celt"
    "celt FB 64 kb/s mono|1|64000|fb|lowdelay|voice|speech|20000|celt"
)

gen() { # channels content -> path
    local ch=$1 content=$2 p="$W/$2$1.f32"
    [ -f "$p" ] || "$R/rspeed" gen "$p" $RATE "$ch" $FRAME "$SECS" "$content" >/dev/null 2>&1
    echo "$p"
}

# Encode one configuration with both stacks and decode both with *our* decoder,
# so what is left between the two is the encoder and nothing else. `cband`
# decodes libopus's packets with libopus's own decoder as a check on that.
# Sets O_KBPS/O_MODE and T_KBPS/T_MODE from the encoders' own reports.
encode_both() { # tag ch bitrate bw app signal pcm
    local tag=$1 ch=$2 br=$3 bw=$4 app=$5 sig=$6 pcm=$7
    read -r _ _ _ _ O_KBPS O_MODE < <(
        "$R/rspeed" run "$pcm" $RATE "$ch" $FRAME "$br" 9 "$app" "$bw" "$sig" 1 "$W/$tag.ours.pkt")
    read -r _ _ _ _ T_KBPS T_MODE < <(
        "$B/cspeed"     "$pcm" $RATE "$ch" $FRAME "$br" 9 "$app" "$bw" "$sig" 1 1 "$W/$tag.theirs.pkt")
    "$R/split" "$W/$tag.ours.pkt"   $RATE "$ch" $FRAME "$W/$tag.ours.f32"   > "$W/$tag.ours.split"
    "$R/split" "$W/$tag.theirs.pkt" $RATE "$ch" $FRAME "$W/$tag.theirs.f32" > "$W/$tag.theirs.split"
}

# Bits the CELT layer spent, per frame. A dash on a row with no SILK in it: the
# split is only observable where there are two layers to divide.
celt_bits() { sed -n 's/.*celt \([0-9]*\) .*/\1/p' "$1" | grep -x '[0-9]*' || echo -; }

# The "above" row of a `band` run: snr bias env for each stream in turn.
above() { "$R/band" "$@" | sed -n 's/^[0-9]*-[0-9]* Hz//p'; }

case "${1:-cases}" in
cases)
    printf '%-27s %-7s %11s %11s  %-24s %-24s\n' \
        "" "" "celt bits/frame" "" "       ours, above 8 kHz" "     libopus, above 8 kHz"
    printf '%-27s %-7s %5s %5s %11s  %7s %7s %7s  %7s %7s %7s\n' \
        "case" "mode" "ours" "them" "kb/s o/t" "snr" "bias" "env" "snr" "bias" "env"
    printf -- '-%.0s' {1..112}; echo
    for spec in "${CASES[@]}"; do
        IFS='|' read -r label ch br bw app sig content top expect <<< "$spec"
        pcm=$(gen "$ch" "$content")
        tag=$(echo "$label" | tr -c 'a-zA-Z0-9' '_')
        encode_both "$tag" "$ch" "$br" "$bw" "$app" "$sig" "$pcm"
        ob=$(celt_bits "$W/$tag.ours.split"); tb=$(celt_bits "$W/$tag.theirs.split")
        read -r osnr obias oenv tsnr tbias tenv < <(above "$pcm" $RATE "$ch" --top "$top" \
            "ours=$W/$tag.ours.f32" "theirs=$W/$tag.theirs.f32")
        printf '%-27s %-7s %5s %5s %5s/%-5s  %7s %7s %7s  %7s %7s %7s\n' \
            "$label" "$O_MODE" "$ob" "$tb" "$O_KBPS" "$T_KBPS" \
            "$osnr" "$obias" "$oenv" "$tsnr" "$tbias" "$tenv"
        [ "$O_MODE" = "$expect" ] && [ "$T_MODE" = "$expect" ] || {
            echo "  ! expected $expect, got ours=$O_MODE libopus=$T_MODE" >&2; }
    done
    ;;
rates)
    # Does libopus attenuate the high band exactly when it cannot afford to code
    # it? If so the gap closes as the rate rises; if it is a fixed offset it
    # does not.
    pcm=$(gen 1 speech)
    printf '%-9s %-7s %11s  %-24s %-24s\n' \
        "" "" "celt bits/frame" "       ours, above 8 kHz" "     libopus, above 8 kHz"
    printf '%-9s %-7s %5s %5s  %7s %7s %7s  %7s %7s %7s\n' \
        "request" "mode" "ours" "them" "snr" "bias" "env" "snr" "bias" "env"
    printf -- '-%.0s' {1..78}; echo
    for br in 12000 16000 20000 24000 32000 40000 64000; do
        encode_both "r$br" 1 "$br" fb voip voice "$pcm"
        ob=$(celt_bits "$W/r$br.ours.split"); tb=$(celt_bits "$W/r$br.theirs.split")
        read -r osnr obias oenv tsnr tbias tenv < <(above "$pcm" $RATE 1 \
            "ours=$W/r$br.ours.f32" "theirs=$W/r$br.theirs.f32")
        printf '%-9s %-7s %5s %5s  %7s %7s %7s  %7s %7s %7s\n' \
            "$(( br / 1000 )) kb/s" "$O_MODE" "$ob" "$tb" "$osnr" "$obias" "$oenv" "$tsnr" "$tbias" "$tenv"
    done
    ;;
bands)
    want=${2:-"hybrid FB 20 kb/s mono"}
    for spec in "${CASES[@]}"; do
        IFS='|' read -r label ch br bw app sig content top expect <<< "$spec"
        [ "$label" = "$want" ] || continue
        pcm=$(gen "$ch" "$content")
        tag=$(echo "$label" | tr -c 'a-zA-Z0-9' '_')
        encode_both "$tag" "$ch" "$br" "$bw" "$app" "$sig" "$pcm"
        # libopus's packets through libopus's own decoder, as a check that the
        # per-band answer is the encoder's and not our decoder's.
        "$B/cband" "$W/$tag.theirs.pkt" $RATE "$ch" $FRAME "$W/$tag.theirs.cdec.f32" >/dev/null
        echo "$label"
        "$R/band" "$pcm" $RATE "$ch" --top "$top" \
            "ours=$W/$tag.ours.f32" "theirs=$W/$tag.theirs.f32" "theirs-cdec=$W/$tag.theirs.cdec.f32"
        exit 0
    done
    echo "no case named \"$want\"; one of:" >&2
    printf '  %s\n' "${CASES[@]%%|*}" >&2
    exit 2
    ;;
*)
    echo "usage: $0 [cases|rates|bands [<case>]]" >&2
    exit 2
    ;;
esac
