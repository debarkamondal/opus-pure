#!/bin/bash
# Fetch libopus 1.6.1, build the variants the cross-checks need, and compile
# every C harness in this directory against them.
#
#   reference/build.sh            # everything (first run: ~5 minutes)
#   reference/build.sh libopus    # just the libopus builds
#   reference/build.sh tools      # just the harnesses (libopus must exist)
#
# Everything produced lands in reference/work/, which is gitignored. Nothing
# here writes into a tracked directory.
set -euo pipefail
cd "$(dirname "$0")/.."             # repository root

VERSION=1.6.1
SHA256=6ffcb593207be92584df15b32466ed64bbec99109f007c82205f0194572411a1
URL=https://downloads.xiph.org/releases/opus/opus-$VERSION.tar.gz

W=reference/work
SRC=$W/libopus/opus-$VERSION
BIN=$W/bin
JOBS=$( (command -v nproc >/dev/null && nproc) || sysctl -n hw.ncpu || echo 4)

want=${1:-all}

# --- libopus ----------------------------------------------------------------
#
# Three builds, because which one is the reference depends on what is being
# compared, and using the wrong one measures the wrong thing:
#
#   build        float. The default. CELT is float here, so this is the
#                reference for anything CELT or hybrid, and for opus_demo.
#   build-fixed  OPUS_FIXED_POINT. SILK is fixed point in *both* builds, but
#                only this one takes the same SILK *encoder* path this crate
#                implements; a float libopus uses SILK's float encoder and will
#                not match byte for byte.
#   build-nofma  float, with -ffp-contract=off. The soft-clip curve is
#                `x + a*x*x`, which clang at -O2 on arm64 fuses into an FMA.
#                Rust never contracts, so the unfused form is what this crate
#                computes and what the C source literally says. Measured: with
#                contraction on, 367 of 7680 samples differ by exactly one ULP;
#                with it off, zero do.
build_libopus() {
    mkdir -p "$W/libopus"
    if [ ! -d "$SRC" ]; then
        if [ ! -f "$W/libopus/opus-$VERSION.tar.gz" ]; then
            echo "== fetching libopus $VERSION =="
            curl -sSL -o "$W/libopus/opus-$VERSION.tar.gz" "$URL"
        fi
        echo "$SHA256  $W/libopus/opus-$VERSION.tar.gz" | shasum -a 256 -c -
        tar xzf "$W/libopus/opus-$VERSION.tar.gz" -C "$W/libopus"
    fi

    local common=(-DCMAKE_BUILD_TYPE=Release -DOPUS_BUILD_SHARED_LIBRARY=OFF
                  -DOPUS_BUILD_TESTING=OFF -DOPUS_BUILD_PROGRAMS=OFF)
    for variant in build build-fixed build-nofma; do
        [ -f "$SRC/$variant/libopus.a" ] && { echo "== $variant: already built =="; continue; }
        echo "== building $variant =="
        # One array rather than a base plus an optional extra: bash 3.2, which
        # is what macOS ships, treats an empty array as unset under `set -u`.
        local args=("${common[@]}")
        case $variant in
            build-fixed) args+=(-DOPUS_FIXED_POINT=ON) ;;
            build-nofma) args+=(-DCMAKE_C_FLAGS="-ffp-contract=off") ;;
        esac
        # libopus's CMakeLists.txt:568 calls `message(ERROR "Runtime cpu
        # capability detection needed for MAY_HAVE_NEON")`. ERROR is not a CMake
        # message mode, so the word is printed as part of the text and configure
        # succeeds normally. Dropping that one line keeps a clean run reading as
        # clean; everything else cmake says still comes through.
        local log=$SRC/$variant-configure.log
        if ! cmake -S "$SRC" -B "$SRC/$variant" "${args[@]}" >/dev/null 2>"$log"; then
            cat "$log" >&2
            exit 1
        fi
        grep -v 'Runtime cpu capability detection needed for MAY_HAVE_NEON' "$log" >&2 || true
        cmake --build "$SRC/$variant" -j"$JOBS" >/dev/null
    done
}

# --- the harnesses ----------------------------------------------------------
build_tools() {
    [ -f "$SRC/build/libopus.a" ] || { echo "libopus is not built; run: $0 libopus" >&2; exit 1; }
    mkdir -p "$BIN"

    # `cc <source> <variant>` -> $BIN/<basename>, or `cc <source> <variant> <name>`
    cc_tool() {
        local src=$1 variant=$2 out=${3:-$(basename "${1%.c}")}
        cc -O2 -ffp-contract=off -o "$BIN/$out" "$src" \
           -I"$SRC/include" "$SRC/$variant/libopus.a" -lm
        echo "  $out ($variant)"
    }

    echo "== building harnesses =="
    cc_tool reference/vectors/crange.c    build
    cc_tool reference/vectors/cenc.c      build
    cc_tool reference/vectors/cenc_app.c  build
    cc_tool reference/vectors/cbw.c       build
    cc_tool reference/vectors/cvec.c      build-fixed          # SILK encoder path
    cc_tool reference/plc/cpcm.c          build
    cc_tool reference/plc/cdec.c          build
    cc_tool reference/plc/cplc.c          build-fixed  cplc    # SILK reference
    cc_tool reference/plc/cplc.c          build        cplcf   # CELT/hybrid reference
    cc_tool reference/multiframe/cmf.c    build
    cc_tool reference/speed/cspeed.c      build        cspeed        # what a caller links
    cc_tool reference/speed/cspeed.c      build-fixed  cspeed_fixed  # our SILK encoder's algorithm
    cc_tool reference/highband/cband.c    build
    cc_tool reference/s16/cs16.c          build-fixed  cs16_fixed
    cc_tool reference/s16/cs16.c          build-nofma  cs16_nofma
    cc -O2 -o "$BIN/probe" reference/s16/probe.c -lm && echo "  probe (no libopus)"

    # opus_demo and opus_compare ship as programs rather than library clients,
    # so they need the internal headers as well.
    for t in opus_demo opus_compare; do
        cc -O2 -o "$BIN/$t" "$SRC/src/$t.c" \
           -I"$SRC/include" -I"$SRC/celt" -I"$SRC/silk" -I"$SRC" \
           "$SRC/build/libopus.a" -lm
        echo "  $t (build)"
    done
}

case $want in
    all)     build_libopus; build_tools ;;
    libopus) build_libopus ;;
    tools)   build_tools ;;
    *)       echo "usage: $0 [all|libopus|tools]" >&2; exit 2 ;;
esac
echo "done -> $BIN"
