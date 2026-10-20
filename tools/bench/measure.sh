#!/usr/bin/env bash
# Measure jsons_to_avro on generated schemas, for one avrotize version.
#
#   tools/bench/measure.sh <label> <avrotize dependency> [<shape> <size>]...
#
# <avrotize dependency> is a Cargo dependency value, e.g. '"=0.1.2"' for a release or
# '{ path = "/path/to/checkout" }'. Each version is built as its own crate from
# examples/bench_convert.rs, so versions are compared with identical harness code.
# Without shapes, it measures the standard set below.
#
# Per shape it prints: the median wall time of 7 runs; the conversion's own
# instructions (callgrind), allocated bytes and blocks (dhat), each a one-run count
# minus a generate-only count; peak heap (dhat t-gmax, including the input schema);
# and the hash of the output, which must match between versions unless the output
# changed. Needs valgrind.
set -euo pipefail
label=$1; dep=$2; shift 2
repo=$(cd "$(dirname "$0")/../.." && pwd)
crate=$repo/target/bench-rig/$label
mkdir -p "$crate/src"
cat > "$crate/Cargo.toml" <<TOML
[package]
name = "bench_convert"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
avrotize = $dep
serde_json = { version = "1", features = ["preserve_order"] }

[profile.release]
debug = "line-tables-only"

[workspace]
TOML
cp "$repo/examples/bench_convert.rs" "$crate/src/main.rs"
(cd "$crate" && cargo build -q --release)
bin=$crate/target/release/bench_convert

count() { # <shape> <size> <runs> -> "instructions bytes blocks peak"
  local ir dh
  ir=$(BENCH_NO_HASH=1 valgrind --tool=callgrind --callgrind-out-file=/dev/null "$bin" "$@" 2>&1 |
    sed -n 's/.*Collected : \([0-9]*\).*/\1/p')
  dh=$(BENCH_NO_HASH=1 valgrind --tool=dhat --dhat-out-file=/dev/null "$bin" "$@" 2>&1)
  echo "$ir" \
    "$(echo "$dh" | sed -n 's/.*Total: *\([0-9,]*\) bytes in \([0-9,]*\) blocks.*/\1 \2/p' | tr -d ,)" \
    "$(echo "$dh" | sed -n 's/.*At t-gmax: *\([0-9,]*\) bytes.*/\1/p' | tr -d ,)"
}

[ $# -gt 0 ] || set -- wide 2000 narrow 200 deep 30 collide 200 genson 1000
while [ $# -ge 2 ]; do
  shape=$1; size=$2; shift 2
  wall=$("$bin" "$shape" "$size" 7)
  read -r ir1 b1 k1 peak <<< "$(count "$shape" "$size" 1)"
  read -r ir0 b0 k0 _ <<< "$(count "$shape" "$size" 0)"
  printf "%-8s %-8s %5s  %9s ms  %12d Ir  %11d B  %8d blocks  peak %10d B  hash %s\n" \
    "$label" "$shape" "$size" "$(echo "$wall" | sed -n 's/.*median \([0-9.]*\)s.*/\1/p' | awk '{printf "%.2f", $1 * 1000}')" \
    $((ir1 - ir0)) $((b1 - b0)) $((k1 - k0)) "$peak" "$(echo "$wall" | sed -n 's/.*hash \([0-9a-f]*\).*/\1/p')"
done
