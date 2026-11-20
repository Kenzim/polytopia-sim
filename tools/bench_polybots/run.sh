#!/usr/bin/env bash
# Random-play speed comparison of polytopia-sim (this repository) and engine-fast (polybots-engine). See docs/speed.md
# for what it measures and how to read the results.
#
# usage: RULES=/path/to/rules.json tools/bench_polybots/run.sh > results.jsonl
#        python3 tools/bench_polybots/summarise.py results.jsonl
#
# Env: RULES    engine-fast's rules file, extracted from your own copy of the game (required, see docs/speed.md)
#      SECS     seconds per measurement (default 10)
#      TRAIN    seconds of profile training per board size and engine (default 15)
#      THREADS  thread counts (default "1 8 48")
#      SIZES    board sizes, 11 and 16 (default "11 16")
#      ROUNDS   rounds; every round measures every build once, in turn (default 3)
#      OUT      build folder (default tools/bench_polybots/target)
#      NICE     niceness of the measured runs (default 10)
#      SKIP_BUILD=1 reuses the binaries of an earlier run.
# Needs the Rust 1.92 toolchain with llvm-tools (`rustup toolchain install 1.92.0 --component llvm-tools`); both
# engines are built with it.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
RULES=${RULES:?set RULES to the engine-fast rules file (see docs/speed.md)}
RULES=$(realpath "$RULES")
SECS=${SECS:-10}
TRAIN=${TRAIN:-15}
THREADS=${THREADS:-1 8 48}
SIZES=${SIZES:-11 16}
ROUNDS=${ROUNDS:-3}
OUT=${OUT:-$HERE/target}
TC=1.92.0
N="nice -n ${NICE:-10}"
PROFDATA=$(rustc +$TC --print sysroot)/lib/rustlib/$(rustc +$TC -vV | sed -n 's/^host: //p')/bin/llvm-profdata
BUILDS="theirs-release ours-release theirs-tuned ours-tuned"

# bench BUILD MODE ARGS...: run one binary; engine-fast's binary takes the rules file first.
bench() {
    local build=$1
    shift
    case $build in
        theirs-*) $N "$OUT/$build/bench-theirs" "$RULES" "$@" ;;
        ours-*) $N "$OUT/$build/bench-ours" "$@" ;;
    esac
}

# tuned ENGINE: full LTO, mimalloc, and profile-guided optimisation trained on one thread of the same random play.
tuned() {
    local engine=$1 data=$OUT/pgo-data-$1
    rm -rf "$data"
    mkdir -p "$data"
    (cd "$HERE/$engine" && CARGO_TARGET_DIR=$OUT/pgo-gen-$engine RUSTFLAGS="-Cprofile-generate=$data" \
        cargo +$TC build --profile tuned --features mimalloc -q)
    for s in $SIZES; do
        case $engine in
            theirs) "$OUT/pgo-gen-$engine/tuned/bench-theirs" "$RULES" run "$s" 1 "$TRAIN" > /dev/null ;;
            ours) "$OUT/pgo-gen-$engine/tuned/bench-ours" run "$s" 1 "$TRAIN" > /dev/null ;;
        esac
    done
    "$PROFDATA" merge -o "$data/merged.profdata" "$data"
    (cd "$HERE/$engine" && CARGO_TARGET_DIR=$OUT/pgo-$engine RUSTFLAGS="-Cprofile-use=$data/merged.profdata" \
        cargo +$TC build --profile tuned --features mimalloc -q)
    mkdir -p "$OUT/$engine-tuned"
    cp "$OUT/pgo-$engine/tuned/bench-$engine" "$OUT/$engine-tuned/"
}

if [ -z "${SKIP_BUILD:-}" ]; then
    for engine in theirs ours; do
        (cd "$HERE/$engine" && CARGO_TARGET_DIR=$OUT/build-$engine cargo +$TC build --release -q)
        mkdir -p "$OUT/$engine-release"
        cp "$OUT/build-$engine/release/bench-$engine" "$OUT/$engine-release/"
        tuned "$engine"
    done
fi

theirs_rev=$(sed -n 's/^source = "git+[^#]*#\([0-9a-f]*\)"$/\1/p' "$HERE/theirs/Cargo.lock" | head -1)
ours_rev=$(git -C "$HERE" rev-parse --short HEAD 2>/dev/null || echo unknown)
cpu=$(sed -n 's/^model name[[:space:]]*: //p' /proc/cpuinfo | head -1)
printf '{"host":{"cpu":"%s","logical_cpus":%s,"load_average":"%s","rustc":"%s","ours":"%s","theirs":"%s","rules_sha256":"%s","secs":%s}}\n' \
    "$cpu" "$(nproc)" "$(cut -d' ' -f1-3 /proc/loadavg)" "$(rustc +$TC --version)" "$ours_rev" "$theirs_rev" \
    "$(sha256sum "$RULES" | cut -c1-64)" "$SECS"

for round in $(seq "$ROUNDS"); do
    for s in $SIZES; do
        for b in $BUILDS; do
            echo "{\"build\":\"$b\",\"round\":$round,\"ops\":$(bench "$b" ops "$s" "$SECS")}"
        done
        for t in $THREADS; do
            for b in $BUILDS; do
                echo "{\"build\":\"$b\",\"round\":$round,\"run\":$(bench "$b" run "$s" "$t" "$SECS")}"
            done
        done
    done
done
